import 'server-only';
import { z } from 'zod';
import { redirect } from 'next/navigation';
import { revalidatePath } from 'next/cache';
import { withTransaction, query } from '@/db';
import { recordEvent } from '@/waybill/events';
import { appendWaybillEvent } from '@/waybill/append';
import { loadWaybill } from '@/waybill/queries';
import { loadActor, type ActorWithScope } from '@/server/guard';
import { hasPermission } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { STAGE_TO_PERM } from '@/perm';
import {
  upsertSalesDraftVat,
  upsertSalesDraftAccrual,
  upsertSalesDraftSettlement,
  finalizeSalesDraft,
} from '@/finance/postSalesToGL';

const IdSchema = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
});

async function loadSalesOrderContext(salesOrderId: number) {
  const r = await query<{
    id: number;
    so_number: string;
    customer_id: number;
    sales_rep_id: number;
    status: string;
    total_amount: string;
    payment_terms: string;
    waybill_id: string | null;
  }>(
    `SELECT so.id, so.so_number, so.customer_id, so.sales_rep_id, so.status,
            so.total_amount::text, so.payment_terms,
            w.id AS waybill_id
       FROM sales_orders so
       LEFT JOIN waybills w ON w.origin = 'so' AND w.origin_id = so.id
      WHERE so.id = $1`,
    [salesOrderId],
  );
  return r.rows[0] ?? null;
}

async function requireActor(): Promise<ActorWithScope> {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  return actor;
}

function canActOnSalesStage(actor: ActorWithScope, stage: string): boolean {
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  const stagePerm = STAGE_TO_PERM[stage];
  return !!stagePerm && hasPermission(actor, stagePerm);
}

export async function submitSalesOrderAction(formData: FormData): Promise<void> {
  const parsed = IdSchema.parse({ waybillId: String(formData.get('waybillId') ?? '') });
  const wb = await loadWaybill(parsed.waybillId);
  if (!wb || wb.origin !== 'so') throw new Error('Sales waybill not found');

  const actor = await requireActor();
  if (!hasPermission(actor, 'finance:sales:submit::allow')) {
    throw new Error('forbidden');
  }
  if (!canActOnSalesStage(actor, wb.current_stage)) {
    throw new Error('cannot act on current stage');
  }

  const amountTHB = wb.total_amount != null ? Number(wb.total_amount) : 0;
  const nextStage = amountTHB < 5000 ? 'so_credit_check' : 'so_sales_review';
  const eventKind = amountTHB < 5000 ? 'so-auto-approved' : 'so-submitted';

  await withTransaction(async (q) => {
    await q(
      `UPDATE sales_orders SET status = $1, updated_at = now() WHERE id = $2`,
      [nextStage, wb.origin_id],
    );
    await q(
      `UPDATE waybills SET current_stage = $1, updated_at = now() WHERE id = $2`,
      [nextStage, wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: eventKind,
      stageFrom: wb.current_stage,
      stageTo: nextStage,
      actorId: actor.id,
      actorRole: actor.role_name ?? 'sales_rep',
      payload: { decision: 'submit', auto_approved: amountTHB < 5000, total_amount_thb: amountTHB },
      client: q as never,
    });
  });

  revalidatePath(`/waybill/${parsed.waybillId}`);
  revalidatePath('/sales');
  redirect(`/waybill/${parsed.waybillId}`);
}

export async function approveSalesReviewAction(formData: FormData): Promise<void> {
  const parsed = IdSchema.parse({ waybillId: String(formData.get('waybillId') ?? '') });
  await advanceSalesOrderAction({ waybillId: parsed.waybillId, target: 'so_credit_check', event: 'so-reviewed' });
}

export async function approveSalesCreditAction(formData: FormData): Promise<void> {
  const parsed = IdSchema.parse({ waybillId: String(formData.get('waybillId') ?? '') });
  await advanceSalesOrderAction({ waybillId: parsed.waybillId, target: 'so_invoiced', event: 'so-credit-checked' });
}

export async function issueSalesInvoiceAction(formData: FormData): Promise<void> {
  const parsed = IdSchema.parse({ waybillId: String(formData.get('waybillId') ?? '') });
  const invoiceNumber = String(formData.get('invoiceNumber') ?? '').trim();
  if (!invoiceNumber) throw new Error('invoiceNumber required');

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb || wb.origin !== 'so') throw new Error('Sales waybill not found');

  const actor = await requireActor();
  if (!hasPermission(actor, 'finance:sales:invoice::allow')) {
    throw new Error('forbidden');
  }
  if (!canActOnSalesStage(actor, wb.current_stage)) {
    throw new Error('cannot act on current stage');
  }

  const so = await loadSalesOrderContext(wb.origin_id);
  if (!so) throw new Error('Sales order not found');

  await withTransaction(async (q) => {
    await q(
      `UPDATE sales_orders SET status = 'so_invoiced', invoice_number = $1, invoice_issued_at = now(), invoice_issuer_id = $2, updated_at = now() WHERE id = $3`,
      [invoiceNumber, actor.id, so.id],
    );
    await q(
      `UPDATE waybills SET current_stage = 'so_invoiced', updated_at = now() WHERE id = $1`,
      [wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'so-invoiced',
      stageFrom: wb.current_stage,
      stageTo: 'so_invoiced',
      actorId: actor.id,
      actorRole: actor.role_name ?? 'accounting_manager',
      payload: { invoice_number: invoiceNumber },
      client: q as never,
    });
  });

  await upsertSalesDraftVat({ salesOrderId: so.id, vendorName: so.so_number });
  await upsertSalesDraftAccrual({ salesOrderId: so.id, vendorName: so.so_number });
  const vat = await (await import('@/finance/postSalesToGL')).loadDraftSalesJournal({ salesOrderId: so.id, step: 'sales_vat' });
  const accr = await (await import('@/finance/postSalesToGL')).loadDraftSalesJournal({ salesOrderId: so.id, step: 'sales_accrual' });
  if (vat) await finalizeSalesDraft({ journalId: vat.journalId, actorId: actor.id });
  if (accr) await finalizeSalesDraft({ journalId: accr.journalId, actorId: actor.id });

  await appendWaybillEvent({
    origin: 'so',
    originId: so.id,
    kind: 'posted-to-gl-sales-vat',
    stageFrom: 'so_invoiced',
    stageTo: 'so_invoiced',
    actorId: actor.id,
    actorRole: actor.role_name ?? 'accounting_manager',
    payload: { final: true },
  });

  revalidatePath(`/waybill/${parsed.waybillId}`);
  redirect(`/waybill/${parsed.waybillId}`);
}

async function advanceSalesOrderAction(args: { waybillId: string; target: string; event: import('@/waybill/events').WaybillEventKind }): Promise<void> {
  const wb = await loadWaybill(args.waybillId);
  if (!wb || wb.origin !== 'so') throw new Error('Sales waybill not found');

  const actor = await requireActor();
  if (!canActOnSalesStage(actor, wb.current_stage)) {
    throw new Error('cannot act on current stage');
  }

  await withTransaction(async (q) => {
    await q(`UPDATE sales_orders SET status = $1, updated_at = now() WHERE id = $2`, [args.target, wb.origin_id]);
    await q(`UPDATE waybills SET current_stage = $1, updated_at = now() WHERE id = $2`, [args.target, wb.id]);
    await recordEvent({
      waybillId: wb.id,
      kind: args.event,
      stageFrom: wb.current_stage,
      stageTo: args.target,
      actorId: actor.id,
      actorRole: actor.role_name ?? 'staff',
      payload: { decision: 'advance' },
      client: q as never,
    });
  });

  revalidatePath(`/waybill/${args.waybillId}`);
  redirect(`/waybill/${args.waybillId}`);
}

export async function rejectSalesOrderAction(formData: FormData): Promise<void> {
  const parsed = z.object({
    waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
    reason: z.string().min(5).max(2000),
  }).parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    reason: String(formData.get('reason') ?? '').trim(),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb || wb.origin !== 'so') throw new Error('Sales waybill not found');

  const actor = await requireActor();
  if (!['disbursed', 'rejected'].includes(wb.current_stage)) {
    if (!canActOnSalesStage(actor, wb.current_stage)) {
      throw new Error('cannot reject at this stage');
    }
  }

  await withTransaction(async (q) => {
    await q(
      `UPDATE sales_orders SET status = 'rejected', rejection_reason = $2, rejection_actor_id = $3, rejected_at = now(), updated_at = now() WHERE id = $1`,
      [wb.origin_id, parsed.reason, actor.id],
    );
    await q(
      `UPDATE waybills SET current_stage = 'rejected', status = 'rejected', updated_at = now() WHERE id = $1`,
      [wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'so-rejected',
      stageFrom: wb.current_stage,
      stageTo: 'rejected',
      actorId: actor.id,
      actorRole: 'staff',
      payload: { reason: parsed.reason },
      client: q as never,
    });
  });

  revalidatePath(`/waybill/${parsed.waybillId}`);
  redirect(`/waybill/${parsed.waybillId}`);
}

export async function attachArReceiptAction(formData: FormData): Promise<void> {
  const parsed = z.object({
    waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
    slipId: z.coerce.number().int().positive(),
  }).parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    slipId: String(formData.get('slipId') ?? ''),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb || wb.origin !== 'so') throw new Error('Sales waybill not found');

  const actor = await requireActor();
  if (!hasPermission(actor, 'finance:sales:settle::allow')) {
    throw new Error('forbidden');
  }
  if (wb.current_stage === 'so_paid') {
    if (!canActOnSalesStage(actor, wb.current_stage)) {
      throw new Error('cannot act on current stage');
    }
  }

  const so = await loadSalesOrderContext(wb.origin_id);
  if (!so) throw new Error('Sales order not found');

  await withTransaction(async (q) => {
    await q(
      `UPDATE sales_orders SET status = 'so_paid', ar_slip_id = $2, paid_by = $3, paid_at = now(), updated_at = now() WHERE id = $1`,
      [so.id, parsed.slipId, actor.id],
    );
    await q(`UPDATE waybills SET current_stage = 'so_paid', status = 'completed', updated_at = now() WHERE id = $1`, [wb.id]);
    await recordEvent({
      waybillId: wb.id,
      kind: 'so-paid',
      stageFrom: wb.current_stage,
      stageTo: 'so_paid',
      actorId: actor.id,
      actorRole: actor.role_name ?? 'finance',
      payload: { slip_id: parsed.slipId },
      client: q as never,
    });
  });

  await upsertSalesDraftSettlement({ salesOrderId: so.id, vendorName: so.so_number });
  const sett = await (await import('@/finance/postSalesToGL')).loadDraftSalesJournal({ salesOrderId: so.id, step: 'sales_settlement' });
  if (sett) await finalizeSalesDraft({ journalId: sett.journalId, actorId: actor.id });

  await appendWaybillEvent({
    origin: 'so',
    originId: so.id,
    kind: 'posted-to-gl-sales-settlement',
    stageFrom: 'so_paid',
    stageTo: 'so_paid',
    actorId: actor.id,
    actorRole: actor.role_name ?? 'finance',
    payload: { final: true, slip_id: parsed.slipId },
  });

  revalidatePath(`/waybill/${parsed.waybillId}`);
  redirect(`/waybill/${parsed.waybillId}`);
}

export async function startSalesDraftAction(_formData: FormData): Promise<{ waybillId: string; salesOrderId: number } | null> {
  return null;
}

export async function saveSalesDraftAction(_formData: FormData): Promise<{ savedAt: string | null; error?: string }> {
  return { savedAt: new Date().toISOString() };
}

export async function discardSalesDraftAction(_formData: FormData): Promise<{ ok: boolean }> {
  return { ok: true };
}
