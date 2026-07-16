'use server';

import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';
import { query as _query } from '@/db';
import { recordEvent } from '@/waybill/events';
import { loadWaybill } from '@/waybill/queries';
import { matchPerm } from '@/perm';
import type { ActorWithScope } from '@/server/guard';
import { upsertProcurementDraftAccrual, finalizeProcurementDraft } from '@/finance/postProcurementToGL';
import { actorForWaybill, canConfirmGl, canSaveProcurementAccrual, type WbForCheck } from './_helpers';

function canPostGlAccrual(actor: ActorWithScope, wb: WbForCheck): boolean {
  return wb.current_stage === 'accounting_authorization'
    && (matchPerm(actor.permissions, 'finance:gl:post::allow')
      || (actor.role_name === 'accounting_manager'
        && matchPerm(actor.permissions, 'finance:gl:post::allow')));
}

function canPostGlSettlement(actor: ActorWithScope, wb: WbForCheck): boolean {
  return wb.current_stage === 'disbursed'
    && matchPerm(actor.permissions, 'finance:gl:post::allow')
    && ['finance', 'account_officer', 'account_supervisor', 'accounting_manager'].includes(actor.role_name);
}

const SaveProcurementAccrualForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
});

export async function saveProcurementAccrualAction(formData: FormData): Promise<void> {
  const parsed = SaveProcurementAccrualForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
  });
  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'pr' && wb.origin !== 'po') {
    throw new Error(`accrual only for procurement origin (got ${wb.origin})`);
  }

  const actor = await actorForWaybill();
  if (!canSaveProcurementAccrual(actor)) {
    throw new Error('cannot save procurement accrual');
  }

  let vendorName = '';
  if (wb.origin === 'pr') {
    const r = await _query<{ vendor_name: string | null }>(
      `SELECT vendor_name FROM purchase_requisitions WHERE id = $1`,
      [wb.origin_id],
    );
    vendorName = r.rows[0]?.vendor_name ?? '';
  } else {
    const r = await _query<{ vendor_name: string | null }>(
      `SELECT vendor_name FROM purchase_orders WHERE id = $1`,
      [wb.origin_id],
    );
    vendorName = r.rows[0]?.vendor_name ?? '';
  }

  await upsertProcurementDraftAccrual({
    origin: wb.origin,
    originId: wb.origin_id,
    vendorName,
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const PostProcurementAccrualForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  journalId: z.coerce.number().int().positive(),
});

export async function postProcurementAccrualAction(formData: FormData): Promise<void> {
  const parsed = PostProcurementAccrualForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    journalId: String(formData.get('journalId') ?? '0'),
  });
  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'pr' && wb.origin !== 'po') {
    throw new Error(`accrual only for procurement origin (got ${wb.origin})`);
  }

  const actor = await actorForWaybill();
  if (!canPostGlAccrual(actor, wb)) {
    throw new Error('cannot post GL accrual at this stage');
  }

  const fin = await finalizeProcurementDraft({
    journalId: parsed.journalId,
    actorId: actor.id,
  });
  if (!fin) throw new Error('draft journal not found');

  await recordEvent({
    waybillId: wb.id,
    kind: 'posted-to-gl-accrual',
    stageFrom: 'accounting_authorization',
    stageTo: 'accounting_authorization',
    actorId: actor.id,
    actorRole: actor.role_name,
    payload: { journalId: fin.journalId, step: 'accrual' },
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const PostProcurementSettlementForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  journalId: z.coerce.number().int().positive(),
});

export async function postProcurementSettlementAction(formData: FormData): Promise<void> {
  const parsed = PostProcurementSettlementForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    journalId: String(formData.get('journalId') ?? '0'),
  });
  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'pr' && wb.origin !== 'po') {
    throw new Error(`settlement only for procurement origin (got ${wb.origin})`);
  }

  const actor = await actorForWaybill();
  if (!canPostGlSettlement(actor, wb)) {
    throw new Error('cannot post GL settlement at this stage');
  }

  const fin = await finalizeProcurementDraft({
    journalId: parsed.journalId,
    actorId: actor.id,
  });
  if (!fin) throw new Error('draft journal not found');

  await recordEvent({
    waybillId: wb.id,
    kind: 'posted-to-gl-settlement',
    stageFrom: 'disbursed',
    stageTo: 'disbursed',
    actorId: actor.id,
    actorRole: actor.role_name,
    payload: { journalId: fin.journalId, step: 'settlement' },
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const ConfirmProcurementGlForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  step: z.enum(['accrual', 'settlement']),
});

export async function confirmProcurementGlAction(formData: FormData): Promise<void> {
  const parsed = ConfirmProcurementGlForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    step: String(formData.get('step') ?? ''),
  });
  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'pr' && wb.origin !== 'po') {
    throw new Error(`confirm-gl only for procurement origin (got ${wb.origin})`);
  }

  const actor = await actorForWaybill();
  if (!canConfirmGl(actor, wb)) {
    throw new Error('cannot confirm GL at this stage');
  }

  const kind = parsed.step === 'accrual' ? 'gl-confirmed-accrual' : 'gl-confirmed-settlement';

  const exists = await _query<{ exists: boolean }>(
    `SELECT EXISTS (
       SELECT 1 FROM waybill_events
        WHERE waybill_id = $1 AND kind = $2
     ) AS exists`,
    [wb.id, kind === 'gl-confirmed-accrual' ? 'posted-to-gl-accrual' : 'posted-to-gl-settlement'],
  );
  if (!exists.rows[0]?.exists) {
    throw new Error(`No posted-to-gl-${parsed.step} event on this waybill yet`);
  }

  const already = await _query<{ exists: boolean }>(
    `SELECT EXISTS (
       SELECT 1 FROM waybill_events
        WHERE waybill_id = $1 AND kind = $2
     ) AS exists`,
    [wb.id, kind],
  );
  if (already.rows[0]?.exists) {
    throw new Error(`GL ${parsed.step} already confirmed`);
  }

  await recordEvent({
    waybillId: wb.id,
    kind,
    stageFrom: wb.current_stage,
    stageTo: wb.current_stage,
    actorId: actor.id,
    actorRole: actor.role_name,
    payload: { step: parsed.step },
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}