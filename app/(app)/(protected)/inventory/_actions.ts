'use server';

import { revalidatePath } from 'next/cache';
import { query, withTransaction } from '@/db';
import { requireActor, requireAction } from '@/server/guard';
import { adjustStock, countStock, processRecostJob, receiveStock, reverseWriteDown, shipStock, transferStock, writeDownStock } from '@/inventory';
import { matchVendorInvoice, receivePurchaseOrder } from '@/finance';

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();
const number = (form: FormData, key: string) => Number(text(form, key));

function purchaseOrderRef(form: FormData) {
  const [poId, poLineId] = text(form, 'po_line_ref').split(':').map(Number);
  if (!Number.isInteger(poId) || !Number.isInteger(poLineId) || poId <= 0 || poLineId <= 0) {
    throw new Error('Select a valid PO line');
  }
  return { poId, poLineId };
}

export async function createProductAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'inventory_product_manage', { perm: 'inventory:stock:adjust::allow' });
  await query(
    `INSERT INTO inventory.products
       (sku, name, name_th, base_unit, lot_tracked, expiry_tracked,
        inventory_account_code, revenue_account_code, cogs_account_code)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)`,
    [
      text(form, 'sku'), text(form, 'name'), text(form, 'name_th') || null, text(form, 'unit_code'),
      form.get('lot_tracked') === 'on', form.get('expiry_tracked') === 'on',
      text(form, 'inventory_account'), text(form, 'revenue_account'), text(form, 'cogs_account'),
    ],
  );
  revalidatePath('/inventory');
}

async function stockContext(productId: number, warehouseId: number) {
  const result = await query<{
    unit_code: string;
    branch_id: string;
    lot_tracked: boolean;
  }>(
    `SELECT p.base_unit AS unit_code, w.branch_id::text, p.lot_tracked
       FROM inventory.products p CROSS JOIN inventory.warehouses w
      WHERE p.id = $1 AND w.id = $2 AND p.active AND w.active`,
    [productId, warehouseId],
  );
  if (!result.rows[0]) throw new Error('Invalid product or warehouse');
  return result.rows[0];
}

async function resolveLot(form: FormData, productId: number) {
  const lotId = number(form, 'lot_id');
  if (lotId > 0) return lotId;
  const lotNo = text(form, 'lot_no');
  if (!lotNo) return null;
  const result = await query<{ id: string }>(
    `INSERT INTO inventory.lots(product_id, lot_no, expires_on)
     VALUES ($1,$2,$3::date)
     ON CONFLICT (product_id, lot_no) DO UPDATE SET expires_on = coalesce(excluded.expires_on, inventory.lots.expires_on)
     RETURNING id::text`,
    [productId, lotNo, text(form, 'expires_on') || null],
  );
  return Number(result.rows[0].id);
}

export async function receiveStockAction(form: FormData) {
  const actor = await requireActor();
  const productId = number(form, 'product_id');
  const warehouseId = number(form, 'warehouse_id');
  const ctx = await stockContext(productId, warehouseId);
  const lotId = await resolveLot(form, productId);
  if (ctx.lot_tracked && !lotId) throw new Error('This product requires a lot number');
  await receiveStock({
    kind: 'receipt',
    movementDate: text(form, 'movement_date'),
    sourceType: text(form, 'source_type') || 'manual_receipt',
    sourceId: text(form, 'source_id') || crypto.randomUUID(),
    sourceEventKey: `receipt:${text(form, 'request_key') || crypto.randomUUID()}`,
    branchId: Number(ctx.branch_id),
    lines: [{
      productId,
      quantity: number(form, 'quantity'),
      unitCode: ctx.unit_code,
      unitCostThb: number(form, 'unit_cost_thb'),
      toWarehouseId: warehouseId,
      lotId,
    }],
  }, { id: actor.id, permissions: actor.permissions });
  revalidatePath('/inventory');
  revalidatePath('/ledger');
  revalidatePath('/executive');
}

export async function shipStockAction(form: FormData) {
  const actor = await requireActor();
  const productId = number(form, 'product_id');
  const warehouseId = number(form, 'warehouse_id');
  const ctx = await stockContext(productId, warehouseId);
  const lotId = number(form, 'lot_id') || null;
  await shipStock({
    kind: 'shipment',
    movementDate: text(form, 'movement_date'),
    sourceType: text(form, 'source_type') || 'manual_shipment',
    sourceId: text(form, 'source_id') || crypto.randomUUID(),
    sourceEventKey: `shipment:${text(form, 'request_key') || crypto.randomUUID()}`,
    branchId: Number(ctx.branch_id),
    lines: [{
      productId,
      quantity: number(form, 'quantity'),
      unitCode: ctx.unit_code,
      fromWarehouseId: warehouseId,
      lotId,
    }],
  }, { id: actor.id, permissions: actor.permissions });
  revalidatePath('/inventory');
  revalidatePath('/ledger');
  revalidatePath('/executive');
}

export async function transferStockAction(form: FormData) {
  const actor = await requireActor();
  const productId = number(form, 'product_id');
  const fromWarehouseId = number(form, 'from_warehouse_id');
  const toWarehouseId = number(form, 'to_warehouse_id');
  if (fromWarehouseId === toWarehouseId) throw new Error('Source and destination warehouses must differ');
  const ctx = await stockContext(productId, fromWarehouseId);
  await transferStock({
    kind: 'transfer',
    movementDate: text(form, 'movement_date'),
    sourceType: 'warehouse_transfer',
    sourceId: crypto.randomUUID(),
    sourceEventKey: `transfer:${text(form, 'request_key') || crypto.randomUUID()}`,
    branchId: Number(ctx.branch_id),
    lines: [{
      productId,
      quantity: number(form, 'quantity'),
      unitCode: ctx.unit_code,
      fromWarehouseId,
      toWarehouseId,
      lotId: number(form, 'lot_id') || null,
    }],
  }, { id: actor.id, permissions: actor.permissions });
  revalidatePath('/inventory');
  revalidatePath('/ledger');
}

async function postVariance(form: FormData, kind: 'adjustment' | 'count') {
  const actor = await requireActor();
  const productId = number(form, 'product_id');
  const warehouseId = number(form, 'warehouse_id');
  const direction = text(form, 'direction');
  if (direction !== 'increase' && direction !== 'decrease') throw new Error('Select an increase or decrease');
  const ctx = await stockContext(productId, warehouseId);
  const unitCost = number(form, 'unit_cost_thb');
  if (direction === 'increase' && unitCost <= 0) throw new Error('A positive unit cost is required for stock increases');
  const line = {
    productId,
    quantity: number(form, 'quantity'),
    unitCode: ctx.unit_code,
    unitCostThb: direction === 'increase' ? unitCost : undefined,
    fromWarehouseId: direction === 'decrease' ? warehouseId : undefined,
    toWarehouseId: direction === 'increase' ? warehouseId : undefined,
    lotId: number(form, 'lot_id') || null,
  };
  const move = {
    kind,
    movementDate: text(form, 'movement_date'),
    sourceType: kind === 'count' ? 'stock_count_variance' : 'stock_adjustment',
    sourceId: text(form, 'reason'),
    sourceEventKey: `${kind}:${text(form, 'request_key') || crypto.randomUUID()}`,
    branchId: Number(ctx.branch_id),
    metadata: { reason: text(form, 'reason') },
    lines: [line],
  };
  if (kind === 'count') await countStock(move, { id: actor.id, permissions: actor.permissions }, direction);
  else await adjustStock(move, { id: actor.id, permissions: actor.permissions }, direction);
  revalidatePath('/inventory');
  revalidatePath('/ledger');
  revalidatePath('/executive');
}

export async function adjustStockAction(form: FormData) {
  await postVariance(form, 'adjustment');
}

export async function countStockAction(form: FormData) {
  await postVariance(form, 'count');
}

export async function writeDownStockAction(form: FormData) {
  const actor = await requireActor();
  const productId = number(form, 'product_id');
  const warehouseId = number(form, 'warehouse_id');
  const ctx = await stockContext(productId, warehouseId);
  await writeDownStock({
    productId,
    warehouseId,
    lotId: number(form, 'lot_id') || null,
    movementDate: text(form, 'movement_date'),
    amountThb: number(form, 'amount_thb'),
    reason: text(form, 'reason'),
    sourceEventKey: `write-down:${text(form, 'request_key') || crypto.randomUUID()}`,
    branchId: Number(ctx.branch_id),
  }, { id: actor.id, permissions: actor.permissions });
  revalidatePath('/inventory');
  revalidatePath('/ledger');
  revalidatePath('/reports');
  revalidatePath('/executive');
}

export async function reverseWriteDownAction(form: FormData) {
  const actor = await requireActor();
  await reverseWriteDown({ movementId: number(form, 'movement_id'), movementDate: text(form, 'movement_date'), sourceEventKey: `write-down-reversal:${text(form, 'request_key') || crypto.randomUUID()}` }, { id: actor.id, permissions: actor.permissions });
  revalidatePath('/inventory');
  revalidatePath('/ledger');
  revalidatePath('/reports');
  revalidatePath('/executive');
}

export async function processRecostJobAction(form: FormData) {
  const actor = await requireActor();
  await processRecostJob(number(form, 'job_id'), text(form, 'posting_date'), { id: actor.id, permissions: actor.permissions });
  revalidatePath('/inventory');
  revalidatePath('/ledger');
  revalidatePath('/reports');
  revalidatePath('/executive');
}

export async function reserveStockAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'inventory_reserve', { perm: 'inventory:stock:ship::allow' });
  const productId = number(form, 'product_id');
  const warehouseId = number(form, 'warehouse_id');
  const quantity = number(form, 'quantity');
  const lotId = number(form, 'lot_id') || null;
  await withTransaction(async (q) => {
    await q(
      `SELECT product_id FROM inventory.stock_balances
        WHERE product_id = $1 AND warehouse_id = $2
          AND lot_id IS NOT DISTINCT FROM $3::bigint
        FOR UPDATE`,
      [productId, warehouseId, lotId],
    );
    const availability = await q<{ available: string }>(
      `SELECT coalesce(sum(b.quantity), 0) - coalesce((
         SELECT sum(r.quantity) FROM inventory.reservations r
          WHERE r.product_id = $1 AND r.warehouse_id = $2
            AND r.lot_id IS NOT DISTINCT FROM $3::bigint AND r.status = 'active'
       ), 0) AS available
         FROM inventory.stock_balances b
        WHERE b.product_id = $1 AND b.warehouse_id = $2
          AND b.lot_id IS NOT DISTINCT FROM $3::bigint
       `,
      [productId, warehouseId, lotId],
    );
    if (Number(availability.rows[0]?.available ?? 0) < quantity) throw new Error('Reservation exceeds available stock');
    await q(
      `INSERT INTO inventory.reservations(product_id, warehouse_id, lot_id, source_type, source_id, quantity)
       VALUES ($1,$2,$3,$4,$5,$6)`,
      [productId, warehouseId, lotId, text(form, 'source_type'), text(form, 'source_id'), quantity],
    );
  });
  revalidatePath('/inventory');
}

export async function receivePurchaseOrderAction(form: FormData) {
  const actor = await requireActor();
  const { poId, poLineId } = purchaseOrderRef(form);
  await receivePurchaseOrder({ poId, poLineId, warehouseId: number(form, 'warehouse_id'), quantity: number(form, 'quantity'), lotId: number(form, 'lot_id') || null, receiptDate: text(form, 'receipt_date'), sourceEventKey: `po-receipt:${text(form, 'request_key') || crypto.randomUUID()}`, actor: { id: actor.id, permissions: actor.permissions } });
  revalidatePath('/inventory');
  revalidatePath('/ledger');
}

export async function mapPurchaseOrderProductAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'po_product_map', { perm: 'inventory:stock:receive::allow' });
  const product = await query<{ base_unit: string }>(`SELECT base_unit FROM inventory.products WHERE id = $1 AND active`, [number(form, 'product_id')]);
  if (!product.rows[0]) throw new Error('Product not found');
  await query(`UPDATE po_items SET product_id = $2, unit_code = $3 WHERE id = $1`, [number(form, 'po_line_id'), number(form, 'product_id'), product.rows[0].base_unit]);
  revalidatePath('/inventory');
}

export async function matchVendorInvoiceAction(form: FormData) {
  const actor = await requireActor();
  const { poId, poLineId } = purchaseOrderRef(form);
  await matchVendorInvoice({ poId, invoiceNo: text(form, 'invoice_no'), invoiceDate: text(form, 'invoice_date'), dueDate: text(form, 'due_date'), taxCode: text(form, 'tax_code') || null, landedCostThb: number(form, 'landed_cost_thb') || 0, lines: [{ poLineId, quantity: number(form, 'quantity'), unitPrice: number(form, 'unit_price') }], overrideReason: text(form, 'override_reason') || null, sourceEventKey: `vendor-invoice:${text(form, 'request_key') || crypto.randomUUID()}`, actor: { id: actor.id, permissions: actor.permissions } });
  revalidatePath('/inventory');
  revalidatePath('/ledger');
  revalidatePath('/reports');
}
