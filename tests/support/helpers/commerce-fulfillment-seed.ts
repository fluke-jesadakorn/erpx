import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { isTransactionalSqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { sha256 } from '@erpx/server-runtime/public/crypto';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import { assertion } from '@erpx/server-workflow/public/command-assertions';
import { executeCommand, replayCommand } from '@erpx/server-workflow/public/commands';
import { fulfillmentSchema, roles } from '@erpx/server-commerce/domain/commerce';
import { commerceFinanceJoinedFacts } from '@erpx/server-finance/public/commerce-joined-facts';
import { CommerceFinanceFacts } from '@erpx/server-finance/public/commerce-facts';
import { CommerceInventoryFacts } from '@erpx/server-inventory/public/commerce-facts';
import { CommerceFilesFacts } from '@erpx/server-files/public/commerce-facts';
import { commerceCommerceStockEffects } from '@erpx/server-inventory/public/commerce-stock-owner-effects';
import { prepareBusinessEvidence } from '@erpx/server-records/public/business-evidence';
import { confirmedFunctionalFx, prepareJournal } from '@erpx/server-finance/public/journal-planning';
import { snapshotMoney } from '@erpx/server-finance/public/company-money';
import { receiptQualityHoldSteps } from '@erpx/server-quality/public/quality';
import { quantityMilli, lineAmounts, scaledAmount } from '@erpx/feature-commerce/lib/commerce-math';
import { convertUnits } from '@erpx/feature-finance/lib/company-money';
import { minorToDecimal } from '@erpx/feature-finance/lib/finance-money';
import type { z } from 'zod';

/**
 * Test-only SQL oracle for Commerce order fulfillment.
 *
 * The production sales goods/mixed delivery writer is the native Rust use case
 * (`erpx-server-commerce`); the TypeScript fulfillment command was retired.
 * Retained TS fixtures and integration tests still need an order to carry
 * deliveries/receipts, so this helper reproduces the same durable owner effects
 * directly against PostgreSQL: order state/version, fulfillment and line rows,
 * stock/lot movements, reservation consumption, delivery evidence bindings,
 * money snapshots, draft goods journals and goods valuations.
 *
 * It is not production code and must not run in a request path. Callers seed a
 * fulfillment through {@link commerceFulfillmentSeed}, then assert on the same
 * downstream owner documents they always did. Validation mirrors the retired
 * command so fixture misuse fails loudly; native transport/package enforcement
 * is intentionally not reproduced here.
 */
export type CommerceFulfillmentInput = z.input<typeof fulfillmentSchema>;
export type CommerceFulfillmentReceipt = { id: string; fulfillmentId: string; version: number };
type Row = Record<string, string | number | null>;
type Statement = ReturnType<SqlDatabase['prepare']>;

const error = (status: number, code: string, message: string) => new ApiError(status, code, message);

function allowed(actor: WorkContext, kind: keyof typeof roles) {
  return requirePermission(actor, roles[kind]);
}

/** A fixture binds its database once and keeps the retained call shape. */
export function commerceFulfillmentSeed(database: SqlDatabase) {
  return (actor: WorkContext, id: string, input: CommerceFulfillmentInput, key: string) =>
    seedCommerceFulfillment(database, actor, id, input, key);
}

export async function seedCommerceFulfillment(
  database: SqlDatabase,
  actor: WorkContext,
  id: string,
  raw: CommerceFulfillmentInput,
  key: string,
): Promise<CommerceFulfillmentReceipt> {
  if (!isTransactionalSqlDatabase(database))
    throw error(503, 'TRANSACTION_REQUIRED', 'Commerce fulfillment seeding requires a PostgreSQL transaction.');
  return database.transaction((tx) => fulfill(tx, actor, id, raw, key));
}

async function fulfill(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  raw: CommerceFulfillmentInput,
  key: string,
): Promise<CommerceFulfillmentReceipt> {
  const input = fulfillmentSchema.parse(raw);
  actor = allowed(actor, input.completionMode === 'service' ? 'sales' : 'stock');
  if (input.completionMode === 'mixed') actor = allowed(actor, 'sales');
  const previous = await replayCommand<CommerceFulfillmentReceipt>(db, actor, key, 'commerce.order.fulfill', id, input);
  if (previous) return previous;
  const order = await orderRow(db, actor, id);
  if (order.kind === 'quote') throw error(422, 'ORDER_REQUIRED', 'Convert the quotation to a sales order first.');
  const joined = commerceFinanceJoinedFacts(db);
  const config = await joined.readCommerceSettings1({ companyId: actor.companyId });
  if (!config) throw error(409, 'COMMERCE_SETUP_REQUIRED', 'Configure the company posting accounts before processing orders.');
  const periods = await CommerceFinanceFacts(db).readFinancePeriods1({ companyId: actor.companyId, date: input.date, date2: input.date, fact3: null, fact4: null });
  if (periods.results.length !== 1) throw error(409, 'PERIOD_UNAVAILABLE', 'Choose the accounting period when several open periods contain this date.');
  const periodId = String(periods.results[0].id);
  if (input.completionMode !== 'service' && (!config.inventory_account_id || !config.cogs_account_id || !config.clearing_account_id))
    throw error(409, 'INVENTORY_POSTING_SETUP_REQUIRED', 'Ask Finance to configure inventory, cost of sales and clearing accounts before delivering goods.');
  const fulfillment = `delivery-${key}`;
  const journal = `journal-${key}`;
  const incoming = order.kind === 'purchase';
  if (incoming && input.completionMode !== 'goods')
    throw error(422, 'SERVICE_SUPPLIER_BILL_REQUIRED', 'Purchased services use supplier bills.');
  if (input.date < String(order.order_date))
    throw error(422, 'INVALID_COMPLETION_DATE', 'Completion cannot precede the order date.');
  const stockCurrency = String(config.base_currency);
  const valuationFx = await confirmedFunctionalFx(db, actor, incoming ? String(order.currency) : stockCurrency, stockCurrency, input.date);
  const convertedStock = (units: number) => {
    const value = convertUnits(String(units), 2, 2, '1', minorToDecimal(valuationFx.micros, 6));
    if (value > BigInt(Number.MAX_SAFE_INTEGER))
      throw error(422, 'MONEY_OVERFLOW', 'The functional inventory amount exceeds supported ledger precision.');
    return Number(value);
  };
  if (!incoming && input.completionMode === 'service' && input.evidenceNote.length < 3)
    throw error(422, 'SERVICE_COMPLETION_NOTE_REQUIRED', 'Describe the service completed and who confirmed it.');
  if (!incoming && input.completionMode !== 'service' && ((!input.evidenceIds.length && !input.attachmentSetIds.length) || input.evidenceNote.length < 3))
    throw error(422, 'DELIVERY_EVIDENCE_REQUIRED', 'Attach proof of delivery and record who or what confirmed receipt.');
  if (new Set(input.evidenceIds).size !== input.evidenceIds.length)
    throw error(422, 'DELIVERY_EVIDENCE_DUPLICATE', 'Use each delivery evidence file once.');
  if (!incoming)
    for (const attachmentId of input.evidenceIds) {
      const valid = await CommerceFilesFacts(db).readAttachments2({ attachmentId, id });
      if (!valid)
        throw error(422, 'DELIVERY_EVIDENCE_INVALID', 'The selected proof is missing, revoked, belongs to another order, or is already linked to a delivery.');
    }
  if (new Set(input.lines.map((line) => line.lineId)).size !== input.lines.length)
    throw error(422, 'DUPLICATE_LINE', 'Each order line may appear once.');
  const attachmentBinding = await prepareBusinessEvidence(db, actor, {
    attachmentSetIds: input.attachmentSetIds,
    targetTypes: incoming ? ['purchase_order', 'purchase_receipt', 'commerce_order'] : ['commerce_order_delivery'],
    bindingType: incoming ? 'purchase_receipt' : 'commerce_fulfillment',
    contexts: incoming
      ? [...['delivery_note', 'packing_list', 'damage_photo'].map((purposeKey) => ({ purposeKey, componentKind: 'action' as const, componentRef: 'goods-receipt' })), { purposeKey: 'purchase_receipt', componentKind: 'action' as const, componentRef: 'goods-receipt' }]
      : [{ purposeKey: 'proof_of_delivery', componentKind: 'action' as const, componentRef: 'delivery' }],
    targetId: id,
    expectedTargetVersion: input.expectedVersion,
    commandId: key,
    bindingId: fulfillment,
  });
  const steps: Statement[] = [
    ...(attachmentBinding?.guards ?? []),
    assertion(db, key, 'version-commerce_orders', `EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND version=? AND state IN (?,?))`, [id, input.expectedVersion, 'confirmed', 'partially_fulfilled']),
    db.prepare('INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,evidence_note,completion_kind,completion_note) VALUES(?,?,?,?,?,?,?,?,?)')
      .bind(fulfillment, id, `${incoming ? 'GR' : 'DN'}-${(await sha256(key)).slice(0, 24).toUpperCase()}`, input.date, actor.userId, Date.now(), incoming ? null : input.evidenceNote, input.completionMode, input.evidenceNote),
  ];
  if (!incoming)
    for (const [index, attachmentId] of input.evidenceIds.entries())
      steps.push(CommerceFilesFacts(db).guardRule23({ key, guardId: `delivery-evidence-${index}`, values: [attachmentId, id] }));
  const stock = commerceCommerceStockEffects(db);
  const inventory = CommerceInventoryFacts(db);
  let value = 0, documentSubtotal = 0, goodsCount = 0;
  for (const [i, entry] of input.lines.entries()) {
    const line = await db.prepare('SELECT l.*,p.tracking_mode,p.unit FROM commerce_order_lines l JOIN commerce_products p ON p.id=l.product_id WHERE l.id=? AND l.order_id=?').bind(entry.lineId, id).first<Row>();
    if (!line) throw error(422, 'LINE_INVALID', 'Order line not found.');
    const qty = quantityMilli(entry.quantity);
    const remaining = Number(line.quantity_milli) - Number(line.fulfilled_milli);
    if (qty > remaining) throw error(422, 'OVER_FULFILLMENT', 'Quantity exceeds the remaining order quantity.');
    if (line.item_type === 'service') {
      if (incoming || input.completionMode === 'goods')
        throw error(422, 'SERVICE_COMPLETION_REQUIRED', 'Complete service lines through the service action.');
      if (entry.allocations?.length)
        throw error(422, 'SERVICE_STOCK_ALLOCATION_FORBIDDEN', 'Services have no lot or serial allocations.');
      const prior = await fulfillmentLines(db, line.id);
      const amounts = qty === remaining
        ? { subtotal: Number(line.subtotal_minor) - Number(prior?.subtotal ?? 0), tax: Number(line.tax_minor) - Number(prior?.tax ?? 0) }
        : lineAmounts(qty, Number(line.price_minor), Number(line.tax_bps));
      steps.push(assertion(db, key, `service-line-${i}`, 'EXISTS(SELECT 1 FROM commerce_order_lines WHERE id=? AND fulfilled_milli=? AND quantity_milli-fulfilled_milli>=?)', [line.id, line.fulfilled_milli, qty]));
      for (const [amountKind, amountUnits] of [['subtotal', amounts.subtotal], ['tax', amounts.tax], ['total', amounts.subtotal + amounts.tax]] as const)
        steps.push(await money(db, actor, { sourceType: 'commerce_fulfillment_line', sourceId: `${fulfillment}-${i}`, amountKind, transactionDate: input.date, currency: String(order.currency), amountUnits }));
      steps.push(db.prepare('INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor,stock_currency) VALUES(?,?,?,?,?,?,?,?)')
        .bind(`${fulfillment}-${i}`, fulfillment, line.id, qty, 0, amounts.subtotal, amounts.tax, stockCurrency));
      steps.push(db.prepare('UPDATE commerce_order_lines SET fulfilled_milli=fulfilled_milli+? WHERE id=?').bind(qty, line.id));
      continue;
    }
    if (input.completionMode === 'service' || !order.warehouse_id)
      throw error(422, 'STOCK_DELIVERY_REQUIRED', 'Stock items require warehouse delivery authority.');
    goodsCount += 1;
    const available = await inventory.readInventoryActiveReservations3({ warehouse_id: order.warehouse_id, product_id: line.product_id });
    const ownReservations = incoming ? [] : (await inventory.readInventoryOrderReservations3({ id: line.id })).results;
    const ownHeld = ownReservations.reduce((sum, reservation) => sum + Number(reservation.quantity_milli) - Number(reservation.consumed_milli) - Number(reservation.released_milli), 0);
    if (!incoming && Number(available?.quantity_milli ?? 0) - Number(available?.reserved_milli ?? 0) + ownHeld < qty)
      throw error(409, 'INSUFFICIENT_STOCK', 'There is not enough stock in this warehouse. Receive the purchase first.');
    const prior = await fulfillmentLines(db, line.id);
    const amounts = qty === remaining
      ? { subtotal: Number(line.subtotal_minor) - Number(prior?.subtotal ?? 0), tax: Number(line.tax_minor) - Number(prior?.tax ?? 0) }
      : lineAmounts(qty, Number(line.price_minor), Number(line.tax_bps));
    const stockSubtotal = incoming ? convertedStock(documentSubtotal + amounts.subtotal) - convertedStock(documentSubtotal) : 0;
    documentSubtotal += amounts.subtotal;
    const tracking = String(line.tracking_mode ?? 'none'), allocations = entry.allocations ?? [];
    if (tracking === 'none' && allocations.length)
      throw error(422, 'LOT_ALLOCATION_UNEXPECTED', 'This product does not use lot or serial tracking.');
    if (tracking !== 'none' && allocations.reduce((sum, allocation) => sum + quantityMilli(allocation.quantity), 0) !== qty)
      throw error(422, 'LOT_ALLOCATION_REQUIRED', 'Allocate the full movement quantity to lots or serial numbers.');
    if (tracking === 'serial' && allocations.some((allocation) => quantityMilli(allocation.quantity) !== 1000))
      throw error(422, 'SERIAL_QUANTITY_INVALID', 'Each serial allocation must be exactly one unit.');
    const lotPrepared: Array<{
      lotId: string;
      quantityMilli: number;
      valueMinor: number;
      disposition: 'available' | 'quarantine';
      reason: string;
      dueDate: string;
      existing: Row | null;
      lotCode: string | null;
      serialNumber: string | null;
      manufacturedDate: string | null;
      expiryDate: string | null;
    }> = [];
    let cost = tracking === 'none' ? (incoming ? stockSubtotal : qty === Number(available?.quantity_milli) ? Number(available?.value_minor) : scaledAmount(Number(available?.value_minor ?? 0), qty, Number(available?.quantity_milli ?? 1))) : 0;
    if (tracking !== 'none') {
      const identities = new Set<string>();
      let allocatedValue = 0;
      for (const [allocationIndex, allocation] of allocations.entries()) {
        const allocationQty = quantityMilli(allocation.quantity), identity = incoming ? (tracking === 'serial' ? allocation.serialNumber : allocation.lotCode) : allocation.lotId;
        if (!identity || identities.has(identity))
          throw error(422, 'LOT_ALLOCATION_DUPLICATE', 'Use each lot or serial once per movement line.');
        identities.add(identity);
        let existing: Row | null;
        if (incoming)
          existing = await inventory.readInventoryLots1({ warehouse_id: order.warehouse_id, companyId: actor.companyId, product_id: line.product_id, identity, tracking }) ?? null;
        else
          existing = await inventory.readInventoryQualityHolds3({ fact0: allocation.lotId ?? '', companyId: actor.companyId, product_id: line.product_id, warehouse_id: order.warehouse_id }) ?? null;
        if (!incoming && (!existing || Number(existing.quantity_milli) < allocationQty || Number(existing.quality_held)))
          throw error(409, 'LOT_UNAVAILABLE', 'The selected lot or serial is unavailable, quarantined, or has insufficient quantity.');
        if (incoming && tracking === 'serial' && existing)
          throw error(409, 'SERIAL_ALREADY_RECEIVED', 'This serial number already exists for the product.');
        if (incoming && allocation.disposition === 'quarantine' && (!allocation.quarantineReason || !allocation.quarantineDueDate))
          throw error(422, 'QUARANTINE_EVIDENCE_REQUIRED', 'Quarantined receipt allocations require a reason and review due date.');
        const lotId = String(existing?.id ?? `lot-${(await sha256(`${actor.companyId}:${line.product_id}:${tracking}:${identity}`)).slice(0, 48)}`);
        const allocationValue = incoming
          ? (allocationIndex === allocations.length - 1 ? stockSubtotal - allocatedValue : scaledAmount(stockSubtotal, allocationQty, qty))
          : (allocationQty === Number(existing!.quantity_milli) ? Number(existing!.value_minor) : scaledAmount(Number(existing!.value_minor), allocationQty, Number(existing!.quantity_milli)));
        allocatedValue += allocationValue;
        cost += incoming ? 0 : allocationValue;
        lotPrepared.push({ lotId, quantityMilli: allocationQty, valueMinor: allocationValue, disposition: allocation.disposition, reason: allocation.quarantineReason, dueDate: allocation.quarantineDueDate, existing, lotCode: tracking === 'lot' ? String(allocation.lotCode) : null, serialNumber: tracking === 'serial' ? String(allocation.serialNumber) : null, manufacturedDate: allocation.manufacturedDate || String(existing?.manufactured_date ?? '') || null, expiryDate: allocation.expiryDate || String(existing?.expiry_date ?? '') || null });
      }
      if (incoming) cost = allocatedValue;
    }
    value += cost;
    steps.push(inventory.guardRule24({ key, guardId: `movement-date-${i}`, values: [order.warehouse_id, line.product_id, input.date] }));
    steps.push(assertion(db, key, `line-${i}`, 'EXISTS(SELECT 1 FROM commerce_order_lines WHERE id=? AND fulfilled_milli=? AND quantity_milli-fulfilled_milli>=?)', [line.id, line.fulfilled_milli, qty]));
    if (available)
      steps.push(assertion(db, key, `stock-${i}`, 'EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND product_id=? AND version=?)', [order.warehouse_id, line.product_id, available.version]));
    else
      steps.push(stock.prepareCommerceStock2({ warehouse_id: order.warehouse_id, product_id: line.product_id }));
    let consumeRemaining = qty;
    for (const reservation of ownReservations) {
      const n = Math.min(consumeRemaining, Number(reservation.quantity_milli) - Number(reservation.consumed_milli) - Number(reservation.released_milli));
      if (!n) break;
      steps.push(inventory.prepareInventoryOrderReservations4({ n, id: reservation.id }));
      consumeRemaining -= n;
    }
    steps.push(stock.prepareCommerceStock3({ fact0: incoming ? qty : -qty, fact1: incoming ? cost : -cost, warehouse_id: order.warehouse_id, product_id: line.product_id }));
    for (const [amountKind, amountUnits] of [['stock_value', cost], ['subtotal', amounts.subtotal], ['tax', amounts.tax], ['total', amounts.subtotal + amounts.tax]] as const)
      steps.push(await money(db, actor, { sourceType: 'commerce_fulfillment_line', sourceId: `${fulfillment}-${i}`, amountKind, transactionDate: input.date, currency: amountKind === 'stock_value' ? stockCurrency : String(order.currency), amountUnits }));
    steps.push(db.prepare('INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor,stock_currency) VALUES(?,?,?,?,?,?,?,?)')
      .bind(`${fulfillment}-${i}`, fulfillment, line.id, qty, cost, amounts.subtotal, amounts.tax, stockCurrency));
    for (const [allocationIndex, allocation] of lotPrepared.entries()) {
      const allocationId = `${fulfillment}-${i}-${allocationIndex}`, lineId = `${fulfillment}-${i}`;
      if (incoming) {
        if (!allocation.existing)
          steps.push(inventory.prepareInventoryLots3({ lotId: allocation.lotId, companyId: actor.companyId, product_id: line.product_id, tracking, lotCode: allocation.lotCode, serialNumber: allocation.serialNumber, manufacturedDate: allocation.manufacturedDate, expiryDate: allocation.expiryDate, userId: actor.userId, fact9: Date.now() }));
        else
          steps.push(inventory.guardRule27({ key, guardId: `lot-${i}-${allocationIndex}`, values: [allocation.lotId, actor.companyId, line.product_id, tracking, allocation.lotCode, allocation.serialNumber, allocation.manufacturedDate, allocation.expiryDate] }), inventory.prepareInventoryLots4({ lotId: allocation.lotId }));
        if (allocation.existing?.balance_version)
          steps.push(inventory.guardRule28({ key, guardId: `lot-balance-${i}-${allocationIndex}`, values: [allocation.lotId, order.warehouse_id, allocation.existing.balance_version] }));
        steps.push(inventory.prepareInventoryLotBalances3({ lotId: allocation.lotId, warehouse_id: order.warehouse_id, quantityMilli: allocation.quantityMilli, valueMinor: allocation.valueMinor }));
      } else {
        steps.push(inventory.guardRule29({ key, guardId: `lot-balance-${i}-${allocationIndex}`, values: [allocation.lotId, order.warehouse_id, allocation.existing!.balance_version, allocation.quantityMilli] }), inventory.prepareInventoryLotBalances4({ quantityMilli: allocation.quantityMilli, valueMinor: allocation.valueMinor, lotId: allocation.lotId, warehouse_id: order.warehouse_id }), inventory.prepareInventoryLots5({ lotId: allocation.lotId, lotId2: allocation.lotId }));
      }
      const qualityHoldId = incoming && allocation.disposition === 'quarantine' ? `quality-${key}-${i}-${allocationIndex}` : null;
      if (qualityHoldId)
        steps.push(...await receiptQualityHoldSteps(db, actor, { id: qualityHoldId, key: `${key}-${i}-${allocationIndex}`, branchId: String(order.branch_id), warehouseId: String(order.warehouse_id), productId: String(line.product_id), lotId: allocation.lotId, quantityMilli: allocation.quantityMilli, reason: allocation.reason, dueDate: allocation.dueDate, sourceId: allocationId }));
      steps.push(inventory.prepareInventoryLotMoves2({ fact0: `lot-move-${allocationId}`, lotId: allocation.lotId, warehouse_id: order.warehouse_id, fact3: incoming ? 'goods_receipt' : 'delivery', lineId, fact5: incoming ? allocation.quantityMilli : -allocation.quantityMilli, fact6: incoming ? allocation.valueMinor : -allocation.valueMinor, date: input.date, userId: actor.userId, fact9: Date.now() }), inventory.prepareInventoryFulfillmentLotAllocations1({ fact0: `lot-allocation-${allocationId}`, lineId, lotId: allocation.lotId, quantityMilli: allocation.quantityMilli, valueMinor: allocation.valueMinor, disposition: allocation.disposition, qualityHoldId }));
    }
    steps.push(db.prepare('UPDATE commerce_order_lines SET fulfilled_milli=fulfilled_milli+? WHERE id=?').bind(qty, line.id));
    steps.push(stock.prepareCommerceStockMoves1({ fact0: `${fulfillment}-${i}`, warehouse_id: order.warehouse_id, product_id: line.product_id, fulfillment, fact4: incoming ? qty : -qty, fact5: incoming ? cost : -cost, fact6: Date.now() }));
  }
  if (goodsCount) {
    const goods = await prepareJournal(db, actor, {
      chartId: String(config.chart_id), periodId, entryDate: input.date,
      currency: incoming ? String(order.currency) : stockCurrency, fxRate: minorToDecimal(valuationFx.micros, 6),
      sourceType: incoming ? 'goods_receipt' : 'delivery', sourceId: fulfillment, memo: `${incoming ? 'Goods receipt' : 'Delivery'} ${fulfillment}`, isOpening: false,
      lines: [
        { accountId: String(incoming ? config.inventory_account_id : config.cogs_account_id), debit: minorToDecimal(incoming ? documentSubtotal : value), credit: '0' },
        { accountId: String(incoming ? config.clearing_account_id : config.inventory_account_id), debit: '0', credit: minorToDecimal(incoming ? documentSubtotal : value) },
      ],
    }, key, 'draft');
    if (goods.preparedLines.reduce((sum, line) => sum + line.baseDebit, 0) !== value)
      throw error(409, 'GOODS_VALUATION_RECONCILIATION_REQUIRED', 'The frozen functional stock value must match the exact goods journal.');
    steps.push(...goods.steps, db.prepare(`INSERT INTO commerce_goods_valuations(fulfillment_id,company_id,chart_id,journal_id,document_currency,journal_currency,stock_currency,
    valuation_date,fx_rate_micros,source_rate_id,target_rate_id,used_latest_fallback,document_subtotal_minor,stock_value_minor,captured_by,captured_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`)
      .bind(fulfillment, actor.companyId, config.chart_id, journal, String(order.currency), incoming ? String(order.currency) : stockCurrency, stockCurrency, input.date, valuationFx.micros, valuationFx.sourceRateId, valuationFx.targetRateId, valuationFx.usedLatestFallback, documentSubtotal, value, actor.userId, Date.now()));
    steps.push(db.prepare('UPDATE commerce_fulfillments SET journal_id=? WHERE id=?').bind(journal, fulfillment));
  }
  if (!incoming)
    for (const attachmentId of input.evidenceIds)
      steps.push(db.prepare('INSERT INTO commerce_fulfillment_evidence(fulfillment_id,attachment_id,linked_by,linked_at) VALUES(?,?,?,?)').bind(fulfillment, attachmentId, actor.userId, Date.now()));
  if (attachmentBinding) steps.push(...attachmentBinding.steps);
  steps.push(db.prepare("UPDATE commerce_orders SET state=CASE WHEN EXISTS(SELECT 1 FROM commerce_order_lines WHERE order_id=? AND fulfilled_milli<quantity_milli) THEN 'partially_fulfilled' ELSE 'fulfilled' END,version=version+1,updated_at=? WHERE id=?").bind(id, Date.now(), id));
  return executeCommand(db, actor, key, 'commerce.order.fulfill', input, id, input.expectedVersion + 1, { id, fulfillmentId: fulfillment, version: input.expectedVersion + 1 }, steps);
}

async function fulfillmentLines(db: SqlDatabase, orderLineId: string | number | null) {
  return db.prepare('SELECT COALESCE(SUM(subtotal_minor),0) subtotal,COALESCE(SUM(tax_minor),0) tax FROM commerce_fulfillment_lines WHERE order_line_id=?').bind(orderLineId).first<{ subtotal: number; tax: number }>();
}

async function money(db: SqlDatabase, actor: WorkContext, input: { sourceType: string; sourceId: string; amountKind: string; transactionDate: string; currency: string; amountUnits: number }) {
  if (!Number.isSafeInteger(input.amountUnits))
    throw error(422, 'MONEY_OVERFLOW', 'The commerce amount exceeds supported ledger precision.');
  return (await snapshotMoney(db, actor, { ...input, amountUnits: String(input.amountUnits), currencyScale: 2 })).statement;
}

async function orderRow(db: SqlDatabase, actor: WorkContext, id: string) {
  const args = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
  const row = await commerceFinanceJoinedFacts(db).readCommerceOrders1({ id, fact1: args });
  if (!row) throw error(404, 'ORDER_NOT_FOUND', 'Order not found in this workspace.');
  return row;
}
