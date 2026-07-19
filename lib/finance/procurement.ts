import 'server-only';
import { query, withTransaction } from '@/db';
import { receiveStockWithHook } from '@/inventory';
import type { PostingActor } from './types';
import { postJournalInTransaction, PostingError } from './journals';

export async function receivePurchaseOrder(args: {
  poId: number;
  poLineId: number;
  warehouseId: number;
  quantity: number;
  lotId?: number | null;
  receiptDate: string;
  sourceEventKey: string;
  actor: PostingActor;
}) {
  const found = await query<{
    product_id: string | null;
    qty: string;
    received_qty: string;
    unit_price: string;
    unit_code: string;
    currency: string;
    fx_rate: string;
    branch_id: string;
  }>(
    `SELECT i.product_id::text, i.qty::text, i.received_qty::text, i.unit_price::text,
            i.unit_code, po.currency, po.fx_rate::text,
            COALESCE(po.branch_id, (SELECT id FROM finance.branches WHERE active ORDER BY id LIMIT 1))::text AS branch_id
       FROM po_items i JOIN purchase_orders po ON po.id = i.po_id
      WHERE i.id = $1 AND po.id = $2`,
    [args.poLineId, args.poId],
  );
  const line = found.rows[0];
  if (!line?.product_id) throw new PostingError('PO line must be mapped to an inventory product');
  const quantity = Math.round(args.quantity * 1_000_000) / 1_000_000;
  if (quantity <= 0 || quantity > Number(line.qty) - Number(line.received_qty)) throw new PostingError('Receipt quantity exceeds the open PO quantity');
  const fx = line.currency.trim() === 'THB' ? 1 : Number(line.fx_rate);
  return receiveStockWithHook({
    kind: 'receipt',
    movementDate: args.receiptDate,
    sourceType: 'purchase_order_line',
    sourceId: String(args.poLineId),
    sourceEventKey: args.sourceEventKey,
    branchId: Number(line.branch_id),
    lines: [{ productId: Number(line.product_id), quantity, unitCode: line.unit_code, unitCostThb: Number(line.unit_price) * fx, toWarehouseId: args.warehouseId, lotId: args.lotId ?? null }],
  }, args.actor, '210700', async (q, movement) => {
    const locked = await q<{ qty: string; received_qty: string }>(`SELECT qty::text, received_qty::text FROM po_items WHERE id = $1 AND po_id = $2 FOR UPDATE`, [args.poLineId, args.poId]);
    if (!locked.rows[0] || quantity > Number(locked.rows[0].qty) - Number(locked.rows[0].received_qty)) throw new PostingError('Receipt quantity exceeds the current open PO quantity');
    const receiptNo = (await q<{ document_no: string }>(`SELECT finance.next_document_number('GR', $1, $2::date) AS document_no`, [Number(line.branch_id), args.receiptDate])).rows[0].document_no;
    const receipt = await q<{ id: string }>(`INSERT INTO inventory.purchase_receipts(receipt_no, po_id, warehouse_id, receipt_date, status, movement_id, received_by) VALUES ($1,$2,$3,$4,'posted',$5,$6) RETURNING id::text`, [receiptNo, args.poId, args.warehouseId, args.receiptDate, movement.movementId, args.actor.id]);
    await q(`INSERT INTO inventory.purchase_receipt_lines(receipt_id, po_line_id, product_id, quantity, unit_cost_thb, lot_id) VALUES ($1,$2,$3,$4,$5,$6)`, [Number(receipt.rows[0].id), args.poLineId, Number(line.product_id), quantity, Number(line.unit_price) * fx, args.lotId ?? null]);
    await q(`UPDATE po_items SET received_qty = received_qty + $2 WHERE id = $1`, [args.poLineId, quantity]);
  });
}

export async function matchVendorInvoice(args: {
  poId: number;
  invoiceNo: string;
  invoiceDate: string;
  dueDate: string;
  taxCode?: string | null;
  landedCostThb?: number;
  lines: Array<{ poLineId: number; quantity: number; unitPrice: number }>;
  overrideReason?: string | null;
  sourceEventKey: string;
  actor: PostingActor;
}) {
  if (!args.lines.length) throw new PostingError('Vendor invoice requires at least one matched PO line');
  return withTransaction(async (q) => {
    const po = await q<{ branch_id: string; vendor_id: string | null; vendor_name: string | null; currency: string; fx_rate: string }>(`SELECT COALESCE(branch_id, (SELECT id FROM finance.branches WHERE active ORDER BY id LIMIT 1))::text AS branch_id, vendor_id::text, vendor_name, currency, fx_rate::text FROM purchase_orders WHERE id = $1 FOR UPDATE`, [args.poId]);
    const header = po.rows[0];
    if (!header) throw new PostingError('Purchase order not found');
    let vendorId = header.vendor_id ? Number(header.vendor_id) : null;
    if (!vendorId) {
      const vendor = await q<{ id: string }>(`INSERT INTO finance.vendors(code, name) VALUES ($1,$2) ON CONFLICT (code) DO UPDATE SET name = excluded.name RETURNING id::text`, [`PO-${args.poId}`, header.vendor_name ?? `Vendor PO-${args.poId}`]);
      vendorId = Number(vendor.rows[0].id);
      await q(`UPDATE purchase_orders SET vendor_id = $2 WHERE id = $1`, [args.poId, vendorId]);
    }
    const fx = header.currency.trim() === 'THB' ? 1 : Number(header.fx_rate);
    const journalLines: Array<{ accountCode: string; description: string; debitThb?: number; creditThb?: number; branchId: number; vendorId: number; productId?: number }> = [];
    const matched: Array<{ poLineId: number; productId: number; quantity: number; expectedThb: number }> = [];
    let netForeign = 0;
    let netThb = 0;
    for (const input of args.lines) {
      const line = await q<{ qty: string; received_qty: string; invoiced_qty: string; unit_price: string; product_id: string | null }>(`SELECT qty::text, received_qty::text, invoiced_qty::text, unit_price::text, product_id::text FROM po_items WHERE id = $1 AND po_id = $2 FOR UPDATE`, [input.poLineId, args.poId]);
      const item = line.rows[0];
      if (!item?.product_id) throw new PostingError(`PO line ${input.poLineId} has no product`);
      const openReceived = Number(item.received_qty) - Number(item.invoiced_qty);
      if (input.quantity <= 0 || input.quantity > openReceived) throw new PostingError(`Invoice quantity exceeds received quantity for PO line ${input.poLineId}`);
      const expected = Number(item.unit_price);
      const variancePct = expected === 0 ? 1 : Math.abs(input.unitPrice - expected) / expected;
      if (variancePct > 0.05 && !args.overrideReason?.trim()) throw new PostingError(`Price variance on PO line ${input.poLineId} exceeds 5%; accounting-manager override reason required`);
      const expectedThb = Math.round(input.quantity * expected * fx * 100) / 100;
      const invoicedForeign = Math.round(input.quantity * input.unitPrice * 100) / 100;
      const invoicedThb = Math.round(invoicedForeign * fx * 100) / 100;
      const variance = Math.round((invoicedThb - expectedThb) * 100) / 100;
      journalLines.push({ accountCode: '210700', description: `Clear GRNI PO-${args.poId} line ${input.poLineId}`, debitThb: expectedThb, branchId: Number(header.branch_id), vendorId, productId: Number(item.product_id) });
      if (variance > 0) journalLines.push({ accountCode: '120200', description: `Purchase price variance PO-${args.poId}`, debitThb: variance, branchId: Number(header.branch_id), vendorId, productId: Number(item.product_id) });
      if (variance < 0) journalLines.push({ accountCode: '120200', description: `Purchase price variance PO-${args.poId}`, creditThb: -variance, branchId: Number(header.branch_id), vendorId, productId: Number(item.product_id) });
      matched.push({ poLineId: input.poLineId, productId: Number(item.product_id), quantity: input.quantity, expectedThb });
      await q(`UPDATE po_items SET invoiced_qty = invoiced_qty + $2, variance_override_by = CASE WHEN $3::text IS NULL THEN variance_override_by ELSE $4 END, variance_override_reason = coalesce($3, variance_override_reason) WHERE id = $1`, [input.poLineId, input.quantity, args.overrideReason?.trim() || null, args.actor.id]);
      netForeign += invoicedForeign;
      netThb += invoicedThb;
    }
    const landed = Math.round(Number(args.landedCostThb ?? 0) * 100) / 100;
    if (landed > 0) {
      journalLines.push({ accountCode: '120200', description: `Landed cost PO-${args.poId}`, debitThb: landed, branchId: Number(header.branch_id), vendorId });
      netThb += landed;
      netForeign += header.currency.trim() === 'THB' ? landed : landed / fx;
    }
    let vatThb = 0;
    if (args.taxCode) {
      const tax = await q<{ rate: string; account_code: string | null }>(`SELECT rate::text, account_code FROM finance.tax_codes WHERE code = $1 AND active`, [args.taxCode]);
      if (!tax.rows[0]) throw new PostingError('Invalid tax code');
      vatThb = Math.round(netThb * Number(tax.rows[0].rate) * 100) / 100;
      if (vatThb > 0) journalLines.push({ accountCode: tax.rows[0].account_code ?? '110500', description: `Input VAT ${args.invoiceNo}`, debitThb: vatThb, branchId: Number(header.branch_id), vendorId });
    }
    const grossThb = Math.round((netThb + vatThb) * 100) / 100;
    const grossForeign = Math.round((netForeign + (header.currency.trim() === 'THB' ? vatThb : vatThb / fx)) * 100) / 100;
    journalLines.push({ accountCode: '210100', description: `Vendor invoice ${args.invoiceNo}`, creditThb: grossThb, branchId: Number(header.branch_id), vendorId });
    const journal = await postJournalInTransaction(q, { postingDate: args.invoiceDate, description: `Three-way match ${args.invoiceNo} / PO-${args.poId}`, currencyCode: header.currency.trim(), fxRate: fx, sourceType: 'vendor_invoice', sourceId: args.invoiceNo, sourceEventKey: args.sourceEventKey, branchId: Number(header.branch_id), metadata: { poId: args.poId, overrideReason: args.overrideReason ?? null }, lines: journalLines }, args.actor);
    const document = await q<{ id: string }>(`INSERT INTO finance.ap_documents(vendor_id, branch_id, document_no, document_type, source_type, source_id, document_date, due_date, currency_code, fx_rate, original_foreign, open_foreign, original_thb, open_thb, journal_id) VALUES ($1,$2,$3,'vendor_invoice','po',$4,$5,$6,$7,$8,$9,$9,$10,$10,$11) RETURNING id::text`, [vendorId, Number(header.branch_id), args.invoiceNo, String(args.poId), args.invoiceDate, args.dueDate, header.currency.trim(), fx, grossForeign, grossThb, journal.id]);
    if (landed > 0) {
      const basis = matched.reduce((sum, line) => sum + line.expectedThb, 0);
      let remaining = landed;
      for (let index = 0; index < matched.length; index += 1) {
        const line = matched[index];
        const lineAmount = index === matched.length - 1
          ? remaining
          : Math.round(landed * (basis > 0 ? line.expectedThb / basis : line.quantity / matched.reduce((sum, item) => sum + item.quantity, 0)) * 100) / 100;
        remaining = Math.round((remaining - lineAmount) * 100) / 100;
        const receipts = await q<{ id: string; warehouse_id: string; lot_id: string | null; quantity: string }>(
          `SELECT rl.id::text, r.warehouse_id::text, rl.lot_id::text, rl.quantity::text
             FROM inventory.purchase_receipt_lines rl
             JOIN inventory.purchase_receipts r ON r.id = rl.receipt_id
            WHERE rl.po_line_id = $1 AND r.status = 'posted'
            ORDER BY r.receipt_date, rl.id`,
          [line.poLineId],
        );
        if (!receipts.rows.length) throw new PostingError(`Landed cost has no posted receipt for PO line ${line.poLineId}`);
        const received = receipts.rows.reduce((sum, receipt) => sum + Number(receipt.quantity), 0);
        let receiptRemaining = lineAmount;
        for (let receiptIndex = 0; receiptIndex < receipts.rows.length; receiptIndex += 1) {
          const receipt = receipts.rows[receiptIndex];
          const amount = receiptIndex === receipts.rows.length - 1
            ? receiptRemaining
            : Math.round(lineAmount * Number(receipt.quantity) / received * 100) / 100;
          receiptRemaining = Math.round((receiptRemaining - amount) * 100) / 100;
          const updated = await q(
            `UPDATE inventory.stock_balances
                SET avg_cost_thb = round((quantity * avg_cost_thb + $4) / quantity, 6),
                    updated_at = now()
              WHERE product_id = $1 AND warehouse_id = $2
                AND bin_id IS NULL AND lot_id IS NOT DISTINCT FROM $3::bigint
                AND quantity > 0`,
            [line.productId, Number(receipt.warehouse_id), receipt.lot_id ? Number(receipt.lot_id) : null, amount],
          );
          if (!updated.rowCount) throw new PostingError(`Landed cost cannot be allocated to zero on-hand stock for PO line ${line.poLineId}`);
          await q(
            `INSERT INTO inventory.landed_cost_allocations
               (ap_document_id, receipt_line_id, allocation_basis, amount_thb, journal_id)
             VALUES ($1,$2,'value',$3,$4)`,
            [Number(document.rows[0].id), Number(receipt.id), amount, journal.id],
          );
        }
      }
    }
    return { apDocumentId: Number(document.rows[0].id), journalId: journal.id };
  });
}
