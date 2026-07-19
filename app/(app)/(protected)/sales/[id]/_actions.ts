'use server';
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
  upsertSalesDraftSettlement,
  finalizeSalesDraft,
} from '@/finance/postSalesToGL';
import { allocateReceipt, refundArCredit } from '@/finance';
import { postJournalInTransaction } from '@/finance/journals';
import { returnStockWithHook, shipStockWithHook } from '@/inventory';

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
    due_date: string;
    waybill_id: string | null;
    branch_id: string;
    currency: string;
    fx_rate: string;
    ar_account_code: string;
    vat_account_code: string;
    revenue_account_code: string;
  }>(
    `SELECT so.id, so.so_number, so.customer_id, so.sales_rep_id, so.status,
            so.total_amount::text, so.payment_terms, so.due_date::text,
            so.branch_id::text, so.currency, so.fx_rate::text,
            so.ar_account_code, so.vat_account_code, so.revenue_account_code,
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
  await advanceSalesOrderAction({ waybillId: parsed.waybillId, target: 'so_dept_approval', event: 'so-reviewed' });
}

export async function approveSalesDepartmentAction(formData: FormData): Promise<void> {
  const parsed = IdSchema.parse({ waybillId: String(formData.get('waybillId') ?? '') });
  await advanceSalesOrderAction({ waybillId: parsed.waybillId, target: 'so_credit_check', event: 'so-dept-approved' });
}

export async function approveSalesCreditAction(formData: FormData): Promise<void> {
  const parsed = IdSchema.parse({ waybillId: String(formData.get('waybillId') ?? '') });
  await advanceSalesOrderAction({ waybillId: parsed.waybillId, target: 'so_invoiced', event: 'so-credit-checked' });
}

export async function issueSalesInvoiceAction(formData: FormData): Promise<void> {
  const parsed = IdSchema.parse({ waybillId: String(formData.get('waybillId') ?? '') });
  void formData.get('invoiceNumber');

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
    const rows = await q<{
      id: number;
      description: string;
      qty: string;
      shipped_qty: string;
      invoiced_qty: string;
      product_id: string | null;
      unit_price: string;
      vat_amount: string;
      mapped_revenue_account_code: string | null;
    }>(
      `SELECT id, description, qty::text, shipped_qty::text, invoiced_qty::text,
              product_id::text, unit_price::text, vat_amount::text,
              mapped_revenue_account_code
         FROM so_items
        WHERE sales_order_id = $1
        ORDER BY id FOR UPDATE`,
      [so.id],
    );
    const items = rows.rows.map((line) => {
      const ordered = Number(line.qty);
      const eligible = line.product_id ? Number(line.shipped_qty) : ordered;
      const quantity = Math.round((eligible - Number(line.invoiced_qty)) * 1_000_000) / 1_000_000;
      const subtotal = Math.round(quantity * Number(line.unit_price) * 100) / 100;
      const vat = ordered > 0
        ? Math.round((Number(line.vat_amount) * quantity / ordered) * 100) / 100
        : 0;
      return { ...line, quantity, subtotal, vat, total: Math.round((subtotal + vat) * 100) / 100 };
    }).filter((line) => line.quantity > 0);
    if (!items.length) throw new Error('No shipped or service quantity is available to invoice');
    const subtotal = Math.round(items.reduce((sum, line) => sum + line.subtotal, 0) * 100) / 100;
    const vat = Math.round(items.reduce((sum, line) => sum + line.vat, 0) * 100) / 100;
    const total = Math.round((subtotal + vat) * 100) / 100;
    const sequence = await q<{ document_no: string }>(
      `SELECT finance.next_document_number('INV', $1, current_date) AS document_no`,
      [Number(so.branch_id)],
    );
    const invoiceNumber = sequence.rows[0].document_no;
    const revenue = new Map<string, number>();
    for (const line of items) {
      const code = line.mapped_revenue_account_code ?? so.revenue_account_code;
      revenue.set(code, Math.round(((revenue.get(code) ?? 0) + line.subtotal) * 100) / 100);
    }
    const fx = so.currency.trim() === 'THB' ? 1 : Number(so.fx_rate);
    const journal = await postJournalInTransaction(q, {
      postingDate: new Date().toISOString().slice(0, 10),
      description: `Sales invoice ${invoiceNumber}`,
      currencyCode: so.currency.trim(),
      fxRate: fx,
      sourceType: 'sales_invoice',
      sourceId: String(so.id),
      sourceEventKey: `sales:${so.id}:invoice:${invoiceNumber}`,
      branchId: Number(so.branch_id),
      waybillId: wb.id,
      lines: [
        {
          accountCode: so.ar_account_code,
          description: `Accounts receivable ${invoiceNumber}`,
          debitThb: Math.round(total * fx * 100) / 100,
          foreignAmount: total,
          currencyCode: so.currency.trim(),
          branchId: Number(so.branch_id),
          customerId: so.customer_id,
          waybillId: wb.id,
          sourceDocumentType: 'invoice',
          sourceDocumentId: invoiceNumber,
        },
        ...Array.from(revenue, ([accountCode, amount]) => ({
          accountCode,
          description: `Revenue ${invoiceNumber}`,
          creditThb: Math.round(amount * fx * 100) / 100,
          foreignAmount: -amount,
          currencyCode: so.currency.trim(),
          branchId: Number(so.branch_id),
          customerId: so.customer_id,
          waybillId: wb.id,
          sourceDocumentType: 'invoice',
          sourceDocumentId: invoiceNumber,
        })),
        ...(vat > 0 ? [{
          accountCode: so.vat_account_code,
          description: `Output VAT ${invoiceNumber}`,
          creditThb: Math.round(vat * fx * 100) / 100,
          foreignAmount: -vat,
          currencyCode: so.currency.trim(),
          branchId: Number(so.branch_id),
          customerId: so.customer_id,
          waybillId: wb.id,
          sourceDocumentType: 'invoice',
          sourceDocumentId: invoiceNumber,
        }] : []),
      ],
    }, { id: actor.id, permissions: actor.permissions });
    const document = await q<{ id: string }>(
      `INSERT INTO finance.commercial_documents
         (document_type, document_no, branch_id, customer_id, source_type, source_id,
          issue_date, currency_code, fx_rate, subtotal, tax_amount, total_amount,
          status, issued_by, issued_at, journal_id, payload)
       VALUES ('invoice',$1,$2,$3,'sales_order',$4,current_date,$5,$6,$7,$8,$9,
               'issued',$10,now(),$11,$12::jsonb)
       RETURNING id::text`,
      [invoiceNumber, Number(so.branch_id), so.customer_id, String(so.id), so.currency.trim(), fx,
        subtotal, vat, total, actor.id, journal.id, JSON.stringify({ items: items.map((line) => ({
          salesOrderLineId: line.id,
          description: line.description,
          quantity: line.quantity,
          unitPrice: Number(line.unit_price),
          subtotal: line.subtotal,
          vat: line.vat,
          total: line.total,
        })) })],
    );
    await q(
      `INSERT INTO finance.ar_documents
         (document_id, customer_id, branch_id, document_no, document_type,
          document_date, due_date, currency_code, fx_rate, original_foreign,
          open_foreign, original_thb, open_thb, journal_id)
       VALUES ($1,$2,$3,$4,'invoice',current_date,$5,$6,$7,$8,$8,$9,$9,$10)`,
      [Number(document.rows[0].id), so.customer_id, Number(so.branch_id), invoiceNumber,
        so.due_date, so.currency.trim(), fx, total, Math.round(total * fx * 100) / 100, journal.id],
    );
    for (const line of items) {
      await q(`UPDATE so_items SET invoiced_qty = invoiced_qty + $2 WHERE id = $1`, [line.id, line.quantity]);
    }
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
      payload: { invoice_number: invoiceNumber, subtotal, vat, total, journalId: journal.id, itemCount: items.length },
      client: q as never,
    });
    await recordEvent({
      waybillId: wb.id,
      kind: 'posted-to-gl-sales-accrual',
      stageFrom: 'so_invoiced',
      stageTo: 'so_invoiced',
      actorId: actor.id,
      actorRole: actor.role_name ?? 'accounting_manager',
      payload: { invoice_number: invoiceNumber, journalId: journal.id, combined_invoice_revenue_vat: true },
      client: q as never,
    });
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

export async function mapSalesProductAction(formData: FormData): Promise<void> {
  const actor = await requireActor();
  if (!hasPermission(actor, 'inventory:stock:ship::allow')) throw new Error('forbidden');
  await query(`UPDATE so_items SET product_id = $2 WHERE id = $1`, [Number(formData.get('lineId')), Number(formData.get('productId'))]);
  revalidatePath(`/sales/${String(formData.get('salesOrderId'))}`);
}

export async function reserveSalesStockAction(formData: FormData): Promise<void> {
  const actor = await requireActor();
  if (!hasPermission(actor, 'inventory:stock:ship::allow')) throw new Error('forbidden');
  const lineId = Number(formData.get('lineId'));
  const warehouseId = Number(formData.get('warehouseId'));
  const quantity = Number(formData.get('quantity'));
  const lotId = Number(formData.get('lotId')) || null;
  await withTransaction(async (q) => {
    const line = await q<{ product_id: string | null; qty: string; reserved_qty: string; sales_order_id: number }>(`SELECT product_id::text, qty::text, reserved_qty::text, sales_order_id FROM so_items WHERE id = $1 FOR UPDATE`, [lineId]);
    const item = line.rows[0];
    if (!item?.product_id) throw new Error('Map this sales line to a product first');
    if (quantity <= 0 || Number(item.reserved_qty) + quantity > Number(item.qty)) throw new Error('Reservation exceeds the open sales quantity');
    await q(`SELECT product_id FROM inventory.stock_balances WHERE product_id = $1 AND warehouse_id = $2 AND lot_id IS NOT DISTINCT FROM $3::bigint FOR UPDATE`, [Number(item.product_id), warehouseId, lotId]);
    const available = await q<{ available: string }>(`SELECT coalesce(sum(quantity),0) - coalesce((SELECT sum(quantity) FROM inventory.reservations WHERE product_id = $1 AND warehouse_id = $2 AND lot_id IS NOT DISTINCT FROM $3::bigint AND status = 'active'),0) AS available FROM inventory.stock_balances WHERE product_id = $1 AND warehouse_id = $2 AND lot_id IS NOT DISTINCT FROM $3::bigint`, [Number(item.product_id), warehouseId, lotId]);
    if (Number(available.rows[0]?.available ?? 0) < quantity) throw new Error('Insufficient available stock');
    await q(`INSERT INTO inventory.reservations(product_id, warehouse_id, lot_id, source_type, source_id, quantity) VALUES ($1,$2,$3,'sales_order_line',$4,$5)`, [Number(item.product_id), warehouseId, lotId, String(lineId), quantity]);
    await q(`UPDATE so_items SET reserved_qty = reserved_qty + $2 WHERE id = $1`, [lineId, quantity]);
  });
  revalidatePath(`/sales/${String(formData.get('salesOrderId'))}`);
  revalidatePath('/inventory');
}

export async function shipSalesStockAction(formData: FormData): Promise<void> {
  const actor = await requireActor();
  const lineId = Number(formData.get('lineId'));
  const warehouseId = Number(formData.get('warehouseId'));
  const quantity = Number(formData.get('quantity'));
  const lotId = Number(formData.get('lotId')) || null;
  const movementDate = String(formData.get('movementDate'));
  const line = await query<{ product_id: string | null; sales_order_id: number; base_unit: string; branch_id: string }>(`SELECT i.product_id::text, i.sales_order_id, p.base_unit, so.branch_id::text FROM so_items i JOIN sales_orders so ON so.id = i.sales_order_id LEFT JOIN inventory.products p ON p.id = i.product_id WHERE i.id = $1`, [lineId]);
  const item = line.rows[0];
  if (!item?.product_id || !item.base_unit) throw new Error('Map this sales line to a product first');
  if (quantity <= 0) throw new Error('Shipment quantity must be positive');
  const requestKey = String(formData.get('requestKey') || crypto.randomUUID());
  await shipStockWithHook({ kind: 'shipment', movementDate, sourceType: 'sales_order_line', sourceId: String(lineId), sourceEventKey: `sales-shipment:${requestKey}`, branchId: Number(item.branch_id), lines: [{ productId: Number(item.product_id), quantity, unitCode: item.base_unit, fromWarehouseId: warehouseId, lotId }] }, { id: actor.id, permissions: actor.permissions }, async (q, movement) => {
    const locked = await q<{ qty: string; shipped_qty: string }>(`SELECT qty::text, shipped_qty::text FROM so_items WHERE id = $1 FOR UPDATE`, [lineId]);
    const current = locked.rows[0];
    if (!current || Number(current.shipped_qty) + quantity > Number(current.qty)) throw new Error('Shipment exceeds the open sales quantity');
    const inserted = await q<{ id: string }>(`INSERT INTO inventory.sales_shipments(shipment_no, sales_order_id, warehouse_id, shipment_date, status, movement_id, shipped_by) VALUES ($1,$2,$3,$4,'posted',$5,$6) RETURNING id::text`, [movement.movementNo, item.sales_order_id, warehouseId, movementDate, movement.movementId, actor.id]);
    await q(`INSERT INTO inventory.sales_shipment_lines(shipment_id, sales_order_line_id, product_id, quantity, lot_id) VALUES ($1,$2,$3,$4,$5)`, [Number(inserted.rows[0].id), lineId, Number(item.product_id), quantity, lotId]);
    await q(`UPDATE so_items SET shipped_qty = shipped_qty + $2, reserved_qty = greatest(0, reserved_qty - $2) WHERE id = $1`, [lineId, quantity]);
    await q(`UPDATE inventory.reservations SET status = 'fulfilled' WHERE source_type = 'sales_order_line' AND source_id = $1 AND status = 'active'`, [String(lineId)]);
  });
  revalidatePath(`/sales/${item.sales_order_id}`);
  revalidatePath('/inventory');
  revalidatePath('/ledger');
}

export async function allocateSalesReceiptAction(formData: FormData): Promise<void> {
  const actor = await requireActor();
  const salesOrderId = Number(formData.get('salesOrderId'));
  const ar = await query<{ id: string }>(`SELECT d.id::text FROM finance.ar_documents d JOIN finance.commercial_documents c ON c.id = d.document_id WHERE c.source_type = 'sales_order' AND c.source_id = $1 AND d.status IN ('open','partially_paid') ORDER BY d.id DESC LIMIT 1`, [String(salesOrderId)]);
  if (!ar.rows[0]) throw new Error('Open AR invoice not found');
  await allocateReceipt({ arDocumentId: Number(ar.rows[0].id), allocationDate: String(formData.get('allocationDate')), foreignAmount: Number(formData.get('foreignAmount')), whtAmountThb: Number(formData.get('whtAmountThb') || 0), fxRate: Number(formData.get('fxRate') || 0) || undefined, sourceEventKey: `sales-receipt:${String(formData.get('requestKey') || crypto.randomUUID())}`, actor: { id: actor.id, permissions: actor.permissions } });
  revalidatePath(`/sales/${salesOrderId}`);
  revalidatePath('/executive');
  revalidatePath('/reports');
}

export async function returnSalesStockAction(formData: FormData): Promise<void> {
  const actor = await requireActor();
  const lineId = Number(formData.get('lineId'));
  const warehouseId = Number(formData.get('warehouseId'));
  const quantity = Number(formData.get('quantity'));
  const lotId = Number(formData.get('lotId')) || null;
  const movementDate = String(formData.get('movementDate'));
  const row = await query<{ product_id: string | null; shipped_qty: string; returned_qty: string; sales_order_id: number; base_unit: string; branch_id: string }>(`SELECT i.product_id::text, i.shipped_qty::text, i.returned_qty::text, i.sales_order_id, p.base_unit, so.branch_id::text FROM so_items i JOIN sales_orders so ON so.id = i.sales_order_id LEFT JOIN inventory.products p ON p.id = i.product_id WHERE i.id = $1`, [lineId]);
  const item = row.rows[0];
  if (!item?.product_id || !item.base_unit) throw new Error('Mapped product not found');
  if (quantity <= 0 || Number(item.returned_qty) + quantity > Number(item.shipped_qty)) throw new Error('Return exceeds shipped quantity');
  const priorCost = await query<{ unit_cost_thb: string }>(`SELECT l.unit_cost_thb::text FROM inventory.stock_movement_lines l JOIN inventory.stock_movements m ON m.id = l.movement_id WHERE m.kind = 'shipment' AND m.source_type = 'sales_order_line' AND m.source_id = $1 AND l.product_id = $2 ORDER BY m.posted_at DESC LIMIT 1`, [String(lineId), Number(item.product_id)]);
  if (!priorCost.rows[0]) throw new Error('Original shipment cost not found');
  const requestKey = String(formData.get('requestKey') || crypto.randomUUID());
  await returnStockWithHook({ kind: 'customer_return', movementDate, sourceType: 'sales_return_line', sourceId: String(lineId), sourceEventKey: `sales-return:${requestKey}`, branchId: Number(item.branch_id), lines: [{ productId: Number(item.product_id), quantity, unitCode: item.base_unit, unitCostThb: Number(priorCost.rows[0].unit_cost_thb), toWarehouseId: warehouseId, lotId }] }, { id: actor.id, permissions: actor.permissions }, 'customer', async (q) => {
    const locked = await q<{
      qty: string;
      shipped_qty: string;
      returned_qty: string;
      line_total: string;
      vat_amount: string;
      mapped_revenue_account_code: string | null;
      sales_order_id: number;
      customer_id: number;
      branch_id: string;
      currency: string;
      fx_rate: string;
      ar_account_code: string;
      vat_account_code: string;
      revenue_account_code: string;
    }>(
      `SELECT i.qty::text, i.shipped_qty::text, i.returned_qty::text,
              i.line_total::text, i.vat_amount::text, i.mapped_revenue_account_code,
              i.sales_order_id, so.customer_id, so.branch_id::text, so.currency,
              so.fx_rate::text, so.ar_account_code, so.vat_account_code,
              so.revenue_account_code
         FROM so_items i
         JOIN sales_orders so ON so.id = i.sales_order_id
        WHERE i.id = $1 FOR UPDATE OF i`,
      [lineId],
    );
    const current = locked.rows[0];
    if (!current || Number(current.returned_qty) + quantity > Number(current.shipped_qty)) throw new Error('Return exceeds the current shipped quantity');
    const invoice = await q<{ id: string }>(
      `SELECT id::text FROM finance.ar_documents
        WHERE customer_id = $1 AND document_type = 'invoice'
          AND document_no = (SELECT invoice_number FROM sales_orders WHERE id = $2)
        ORDER BY id DESC LIMIT 1`,
      [current.customer_id, current.sales_order_id],
    );
    if (invoice.rows[0]) {
      const fx = current.currency.trim() === 'THB' ? 1 : Number(current.fx_rate);
      const netForeign = Math.round(((Number(current.line_total) - Number(current.vat_amount)) / Number(current.qty)) * quantity * 100) / 100;
      const vatForeign = Math.round((Number(current.vat_amount) / Number(current.qty)) * quantity * 100) / 100;
      const grossForeign = Math.round((netForeign + vatForeign) * 100) / 100;
      const netThb = Math.round(netForeign * fx * 100) / 100;
      const vatThb = Math.round(vatForeign * fx * 100) / 100;
      const grossThb = Math.round((netThb + vatThb) * 100) / 100;
      const creditNo = (await q<{ document_no: string }>(`SELECT finance.next_document_number('CN', $1, $2::date) AS document_no`, [Number(current.branch_id), movementDate])).rows[0].document_no;
      const journal = await postJournalInTransaction(q, {
        postingDate: movementDate,
        description: `Sales credit note ${creditNo}`,
        currencyCode: current.currency.trim(),
        fxRate: fx,
        sourceType: 'sales_credit_note',
        sourceId: String(current.sales_order_id),
        sourceEventKey: `sales-credit:${requestKey}`,
        branchId: Number(current.branch_id),
        lines: [
          { accountCode: current.mapped_revenue_account_code ?? current.revenue_account_code, description: `Revenue return ${creditNo}`, debitThb: netThb, foreignAmount: netForeign, currencyCode: current.currency.trim(), branchId: Number(current.branch_id), customerId: current.customer_id, productId: Number(item.product_id), warehouseId },
          ...(vatThb > 0 ? [{ accountCode: current.vat_account_code, description: `Output VAT return ${creditNo}`, debitThb: vatThb, foreignAmount: vatForeign, currencyCode: current.currency.trim(), branchId: Number(current.branch_id), customerId: current.customer_id }] : []),
          { accountCode: current.ar_account_code, description: `Customer credit ${creditNo}`, creditThb: grossThb, foreignAmount: -grossForeign, currencyCode: current.currency.trim(), branchId: Number(current.branch_id), customerId: current.customer_id },
        ],
      }, { id: actor.id, permissions: actor.permissions });
      const document = await q<{ id: string }>(
        `INSERT INTO finance.commercial_documents
           (document_type, document_no, branch_id, customer_id, source_type,
            source_id, issue_date, currency_code, fx_rate, subtotal, tax_amount,
            total_amount, status, issued_by, issued_at, journal_id, payload)
         VALUES ('credit_note',$1,$2,$3,'sales_return',$4,$5,$6,$7,$8,$9,$10,
                 'issued',$11,now(),$12,$13)
         RETURNING id::text`,
        [creditNo, Number(current.branch_id), current.customer_id, `${current.sales_order_id}:${lineId}`, movementDate, current.currency.trim(), fx, -netForeign, -vatForeign, -grossForeign, actor.id, journal.id, { salesOrderId: current.sales_order_id, lineId, quantity }],
      );
      await q(
        `INSERT INTO finance.ar_documents
           (document_id, customer_id, branch_id, document_no, document_type,
            document_date, due_date, currency_code, fx_rate, original_foreign,
            open_foreign, original_thb, open_thb, journal_id)
         VALUES ($1,$2,$3,$4,'credit_note',$5,$5,$6,$7,$8,$8,$9,$9,$10)`,
        [Number(document.rows[0].id), current.customer_id, Number(current.branch_id), creditNo, movementDate, current.currency.trim(), fx, -grossForeign, -grossThb, journal.id],
      );
    }
    await q(`UPDATE so_items SET returned_qty = returned_qty + $2 WHERE id = $1`, [lineId, quantity]);
  });
  revalidatePath(`/sales/${item.sales_order_id}`);
  revalidatePath('/inventory');
  revalidatePath('/ledger');
  revalidatePath('/reports');
  revalidatePath('/executive');
}

export async function refundSalesCreditAction(formData: FormData): Promise<void> {
  const actor = await requireActor();
  const salesOrderId = Number(formData.get('salesOrderId'));
  const arDocumentId = Number(formData.get('arDocumentId'));
  const owned = await query<{ present: boolean }>(
    `SELECT EXISTS (
       SELECT 1 FROM finance.ar_documents d
       JOIN finance.commercial_documents c ON c.id = d.document_id
       WHERE d.id = $1 AND d.document_type = 'credit_note'
         AND c.source_type = 'sales_return' AND c.source_id LIKE $2 || ':%'
     ) AS present`,
    [arDocumentId, String(salesOrderId)],
  );
  if (!owned.rows[0]?.present) throw new Error('Credit note does not belong to this sales order');
  await refundArCredit({
    arDocumentId,
    refundDate: String(formData.get('refundDate')),
    foreignAmount: Number(formData.get('foreignAmount')),
    fxRate: Number(formData.get('fxRate') || 0) || undefined,
    sourceEventKey: `sales-refund:${String(formData.get('requestKey') || crypto.randomUUID())}`,
    actor: { id: actor.id, permissions: actor.permissions },
  });
  revalidatePath(`/sales/${salesOrderId}`);
  revalidatePath('/ledger');
  revalidatePath('/reports');
  revalidatePath('/executive');
}
