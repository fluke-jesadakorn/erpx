'use server';

import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';
import { query as _query } from '@folio-lib/db';
import { recordEvent } from '@folio-lib/waybill/events';
import { loadWaybill } from '@folio-lib/waybill/queries';
import { matchPerm } from '@folio-lib/perm';
import type { ActorWithScope } from '@folio-lib/server/guard';
import { finalizeSalesDraft } from '@folio-lib/finance/postSalesToGL';
import { actorForWaybill, type WbForCheck } from './_helpers';

function canPostSalesGlStep(actor: ActorWithScope, wb: WbForCheck, stage: string): boolean {
  if (wb.current_stage !== stage) return false;
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return true;
  if (matchPerm(actor.permissions, 'finance:gl:post::allow')) {
    return ['finance', 'account_officer', 'account_supervisor', 'accounting_manager'].includes(actor.role_name);
  }
  return false;
}

function canConfirmSalesGl(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (wb.origin !== 'so') return false;
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return true;
  if (matchPerm(actor.permissions, 'finance:gl:confirm::allow')) {
    return ['finance', 'account_officer', 'account_supervisor', 'accounting_manager', 'cfo', 'ceo'].includes(actor.role_name);
  }
  return false;
}

const PostSalesGlForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  journalId: z.coerce.number().int().positive(),
});

async function postSalesGlStep(args: {
  formData: FormData;
  expectedOrigin: 'so';
  stage: 'so_invoiced' | 'so_paid';
  postedKind: 'posted-to-gl-sales-vat' | 'posted-to-gl-sales-accrual' | 'posted-to-gl-sales-settlement';
  stepLabel: 'vat' | 'accrual' | 'settlement';
}): Promise<void> {
  const parsed = PostSalesGlForm.parse({
    waybillId: String(args.formData.get('waybillId') ?? ''),
    journalId: String(args.formData.get('journalId') ?? '0'),
  });
  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== args.expectedOrigin) {
    throw new Error(`sales GL only for sales origin (got ${wb.origin})`);
  }
  if (wb.current_stage !== args.stage) {
    throw new Error(
      `sales GL ${args.stepLabel} only at ${args.stage} (current: ${wb.current_stage})`,
    );
  }

  const actor = await actorForWaybill();
  if (!canPostSalesGlStep(actor, wb, args.stage)) {
    throw new Error(`cannot post sales GL ${args.stepLabel} at this stage`);
  }

  const fin = await finalizeSalesDraft({
    journalId: parsed.journalId,
    actorId: actor.id,
  });
  if (!fin) throw new Error('draft journal not found');

  await recordEvent({
    waybillId: wb.id,
    kind: args.postedKind,
    stageFrom: args.stage,
    stageTo: args.stage,
    actorId: actor.id,
    actorRole: actor.role_name,
    payload: { journalId: fin.journalId, step: args.stepLabel },
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

export async function postSalesGlVatAction(formData: FormData): Promise<void> {
  await postSalesGlStep({
    formData,
    expectedOrigin: 'so',
    stage: 'so_invoiced',
    postedKind: 'posted-to-gl-sales-vat',
    stepLabel: 'vat',
  });
}

export async function postSalesGlAccrualAction(formData: FormData): Promise<void> {
  await postSalesGlStep({
    formData,
    expectedOrigin: 'so',
    stage: 'so_invoiced',
    postedKind: 'posted-to-gl-sales-accrual',
    stepLabel: 'accrual',
  });
}

export async function postSalesGlSettlementAction(formData: FormData): Promise<void> {
  await postSalesGlStep({
    formData,
    expectedOrigin: 'so',
    stage: 'so_paid',
    postedKind: 'posted-to-gl-sales-settlement',
    stepLabel: 'settlement',
  });
}

const ConfirmSalesGlForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  step: z.enum(['vat', 'accrual', 'settlement']),
});

export async function confirmSalesGlAction(formData: FormData): Promise<void> {
  const parsed = ConfirmSalesGlForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    step: String(formData.get('step') ?? ''),
  });
  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'so') {
    throw new Error(`confirm-sales-gl only for sales origin (got ${wb.origin})`);
  }

  const actor = await actorForWaybill();
  if (!canConfirmSalesGl(actor, wb)) {
    throw new Error('cannot confirm sales GL at this stage');
  }

  const kind =
    parsed.step === 'vat'
      ? 'gl-confirmed-sales-vat'
      : parsed.step === 'accrual'
        ? 'gl-confirmed-sales-accrual'
        : 'gl-confirmed-sales-settlement';

  const postedKind =
    parsed.step === 'vat'
      ? 'posted-to-gl-sales-vat'
      : parsed.step === 'accrual'
        ? 'posted-to-gl-sales-accrual'
        : 'posted-to-gl-sales-settlement';

  const postedExists = await _query<{ exists: boolean }>(
    `SELECT EXISTS (
       SELECT 1 FROM waybill_events
        WHERE waybill_id = $1 AND kind = $2
     ) AS exists`,
    [wb.id, postedKind],
  );
  if (!postedExists.rows[0]?.exists) {
    throw new Error(`No ${postedKind} event on this waybill yet`);
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