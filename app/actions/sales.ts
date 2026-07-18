'use server';

import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';
import { query as _query } from '@/db';
import { recordEvent } from '@/waybill/events';
import { loadWaybill } from '@/waybill/queries';
import { hasPermission } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import type { ActorWithScope } from '@/server/guard';
import { finalizeSalesDraft, upsertSalesDraftSettlement, loadDraftSalesJournal } from '@/finance/postSalesToGL';
import { canActAtSalesRecording } from '@folio-lib/sales/coa';
import { actorForWaybill, type WbForCheck } from './_helpers';

function canPostSalesGlStep(actor: ActorWithScope, wb: WbForCheck, stage: string): boolean {
  if (wb.current_stage !== stage) return false;
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  if (hasPermission(actor, 'finance:gl:post::allow')) {
    return ['finance', 'account_officer', 'account_supervisor', 'accounting_manager'].includes(actor.role_name);
  }
  return false;
}

function canConfirmSalesGl(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (wb.origin !== 'so') return false;
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  if (hasPermission(actor, 'finance:gl:confirm::allow')) {
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

const RecordSalesPaymentForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
});

export async function recordSalesPaymentAction(formData: FormData): Promise<void> {
  const parsed = RecordSalesPaymentForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb || wb.origin !== 'so') throw new Error('Not a sales waybill');
  if (wb.current_stage !== 'so_paid') {
    throw new Error(`Cannot record from ${wb.current_stage}`);
  }

  const actor = await actorForWaybill();
  if (!(await canActAtSalesRecording(new Set(actor.permissions ?? []), actor.role_name))) {
    throw new Error('Forbidden');
  }

  const soRes = await _query<{ ar_slip_id: number | null; total_amount: string; so_number: string | null }>(
    `SELECT ar_slip_id, total_amount::text, so_number FROM sales_orders WHERE id = $1`,
    [wb.origin_id],
  );
  const so = soRes.rows[0];
  if (!so) throw new Error('Sales order not found');
  if (!so.ar_slip_id) throw new Error('Payment slip required before record');

  await upsertSalesDraftSettlement({
    salesOrderId: wb.origin_id,
    vendorName: so.so_number ?? '',
  });
  const sett = await loadDraftSalesJournal({
    salesOrderId: wb.origin_id,
    step: 'sales_settlement',
  });
  if (sett) await finalizeSalesDraft({ journalId: sett.journalId, actorId: actor.id });

  await recordEvent({
    waybillId: wb.id,
    kind: 'posted-to-gl-sales-settlement',
    stageFrom: 'so_paid',
    stageTo: 'so_paid',
    actorId: actor.id,
    actorRole: actor.role_name,
    payload: { origin: 'so', totalAmount: so.total_amount, slipId: so.ar_slip_id },
  });

  await _query(
    `UPDATE waybills SET status = 'completed', current_stage = 'so_paid', updated_at = now() WHERE id = $1`,
    [wb.id],
  );
  await _query(
    `UPDATE sales_orders SET status = 'so_paid', updated_at = now() WHERE id = $1`,
    [wb.origin_id],
  );

  revalidatePath(`/waybill/${wb.id}`);
  revalidatePath(`/sales/${wb.origin_id}`);
  redirect(`/waybill/${wb.id}`);
}

export interface AttachSalesPaymentSlipFields {
  payerBankName?: string;
  payerAccountNumber?: string;
  payerAccountName?: string;
  receiverBankName?: string;
  receiverAccountNumber?: string;
  receiverAccountName?: string;
  amount?: number;
  transactionDate?: string;
}

export interface AttachSalesPaymentSlipInput {
  waybillId: string;
  soId: number;
  slipId: number;
  fields: AttachSalesPaymentSlipFields;
}

export async function attachSalesPaymentSlipAction(
  input: AttachSalesPaymentSlipInput,
): Promise<void> {
  if (!/^WB-\d{4}-\d{6}$/.test(input.waybillId)) {
    throw new Error('Invalid waybillId');
  }
  if (!Number.isInteger(input.soId) || input.soId <= 0) {
    throw new Error('Invalid soId');
  }
  if (!Number.isInteger(input.slipId) || input.slipId <= 0) {
    throw new Error('Invalid slipId');
  }

  const actor = await actorForWaybill();
  if (!actor) throw new Error('unauthorized');

  const wb = await loadWaybill(input.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'so') throw new Error(`Sales origin required (got ${wb.origin})`);
  if (wb.current_stage !== 'so_paid') {
    throw new Error(`Slip can only attach at so_paid (current: ${wb.current_stage})`);
  }
  if (wb.origin_id !== input.soId) {
    throw new Error(`Waybill origin_id ${wb.origin_id} ≠ input soId ${input.soId}`);
  }

  const permOk =
    hasPermission(actor, PERM.stage.so_paid.act) ||
    hasPermission(actor, PERM.admin.system.bypass) ||
    ['finance', 'admin', 'cfo', 'account_officer', 'account_supervisor', 'accounting_manager'].includes(actor.role_name);
  if (!permOk) throw new Error('Forbidden at so_paid');

  const slipRes = await _query<{ id: number; status: string; uploaded_by: number }>(
    `SELECT id, status, uploaded_by FROM slips WHERE id = $1`,
    [input.slipId],
  );
  if (slipRes.rows.length === 0) throw new Error('Slip not found');
  if (slipRes.rows[0].status !== 'pending') {
    throw new Error(`Slip must be pending (current: ${slipRes.rows[0].status})`);
  }

  await _query(
    `UPDATE slips
        SET status = 'confirmed',
            confirmed_at = now()
      WHERE id = $1 AND status = 'pending'`,
    [input.slipId],
  );

  await _query(
    `UPDATE sales_orders
        SET ar_slip_id = $1,
            paid_at = COALESCE(paid_at, now()),
            paid_by_id = $2,
            updated_at = now()
      WHERE id = $3`,
    [input.slipId, actor.id, input.soId],
  );

  await recordEvent({
    waybillId: input.waybillId,
    kind: 'so-paid',
    stageFrom: 'so_paid',
    stageTo: 'so_paid',
    actorId: actor.id,
    actorRole: actor.role_name,
    payload: { slipId: input.slipId, fields: input.fields },
  });

  revalidatePath(`/waybill/${input.waybillId}`);
  revalidatePath(`/sales/${input.soId}`);
  redirect(`/waybill/${input.waybillId}`);
}