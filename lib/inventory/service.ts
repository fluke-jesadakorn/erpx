import 'server-only';
import { withTransaction } from '@/db';
import { matchPerm } from '@/perm/grammar';
import { postJournalInTransaction, type FinanceQuery, PostingError } from '@/finance/journals';
import type { JournalLine, PostingActor } from '@/finance/types';
import type { StockMove, StockMoveLine, StockMoveResult, StockWriteDown } from './types';

interface ProductRow {
  id: string;
  base_unit: string;
  lot_tracked: boolean;
  expiry_tracked: boolean;
  inventory_account_code: string;
  cogs_account_code: string;
}

interface BalanceRow {
  quantity: string;
  avg_cost_thb: string;
  bin_id: string | null;
  lot_id: string | null;
}

interface MoveRow {
  id: string;
  movement_no: string | null;
  journal_id: string | null;
  status: string;
}

function can(actor: PostingActor, permission: string) {
  return matchPerm(actor.permissions, permission) || matchPerm(actor.permissions, 'admin:system:bypass::allow');
}

function requirePerm(actor: PostingActor, permission: string) {
  if (!can(actor, permission)) throw new PostingError(`Missing permission: ${permission}`);
}

function qty(value: number) {
  if (!Number.isFinite(value) || value <= 0) throw new PostingError('Stock quantities must be positive');
  return Math.round(value * 1_000_000) / 1_000_000;
}

function cost(value: number) {
  if (!Number.isFinite(value) || value < 0) throw new PostingError('Unit cost cannot be negative');
  return Math.round(value * 1_000_000) / 1_000_000;
}

function money(value: number) {
  return Math.round(value * 100) / 100;
}

async function product(q: FinanceQuery, id: number) {
  const result = await q<ProductRow>(
    `SELECT id::text, base_unit, lot_tracked, expiry_tracked, inventory_account_code, cogs_account_code
       FROM inventory.products WHERE id = $1 AND active FOR SHARE`,
    [id],
  );
  if (!result.rows[0]) throw new PostingError(`Product ${id} is missing or inactive`);
  return result.rows[0];
}

async function validateLot(q: FinanceQuery, p: ProductRow, line: StockMoveLine) {
  if (p.lot_tracked && !line.lotId) throw new PostingError(`Product ${p.id} requires a lot`);
  if (!line.lotId) return;
  const lot = await q<{ product_id: string; expires_on: string | null }>(
    `SELECT product_id::text, expires_on::text FROM inventory.lots WHERE id = $1`,
    [line.lotId],
  );
  if (!lot.rows[0] || Number(lot.rows[0].product_id) !== Number(p.id)) throw new PostingError('Lot does not belong to the product');
  if (p.expiry_tracked && !lot.rows[0].expires_on) throw new PostingError('An expiry date is required for this product lot');
}

async function lockBalance(q: FinanceQuery, args: {
  productId: number;
  warehouseId: number;
  binId?: number | null;
  lotId?: number | null;
}) {
  await q(
    `INSERT INTO inventory.stock_balances(product_id, warehouse_id, bin_id, lot_id)
     VALUES ($1,$2,$3,$4)
     ON CONFLICT (product_id, warehouse_id, bin_id, lot_id) DO NOTHING`,
    [args.productId, args.warehouseId, args.binId ?? null, args.lotId ?? null],
  );
  const result = await q<BalanceRow>(
    `SELECT quantity::text, avg_cost_thb::text, bin_id::text, lot_id::text
       FROM inventory.stock_balances
      WHERE product_id = $1 AND warehouse_id = $2
      FOR UPDATE`,
    [args.productId, args.warehouseId],
  );
  const target = result.rows.find((row) =>
    (row.bin_id === null ? args.binId == null : Number(row.bin_id) === Number(args.binId))
    && (row.lot_id === null ? args.lotId == null : Number(row.lot_id) === Number(args.lotId)),
  );
  if (!target) throw new PostingError('Unable to lock the stock balance');
  const totalQuantity = result.rows.reduce((sum, row) => sum + Number(row.quantity), 0);
  const totalValue = result.rows.reduce((sum, row) => sum + Number(row.quantity) * Number(row.avg_cost_thb), 0);
  return { quantity: Number(target.quantity), totalQuantity, avgCost: totalQuantity > 0 ? totalValue / totalQuantity : 0 };
}

async function addStock(q: FinanceQuery, line: StockMoveLine, unitCost?: number) {
  if (!line.toWarehouseId) throw new PostingError('A destination warehouse is required');
  const amount = qty(line.quantity);
  const before = await lockBalance(q, {
    productId: line.productId,
    warehouseId: line.toWarehouseId,
    binId: line.toBinId,
    lotId: line.lotId,
  });
  const incomingCost = unitCost === undefined ? before.avgCost : cost(unitCost);
  const afterQty = before.quantity + amount;
  const afterTotalQuantity = before.totalQuantity + amount;
  const incomingValue = money(amount * incomingCost);
  const afterCost = afterTotalQuantity === 0 ? 0 : (money(before.totalQuantity * before.avgCost) + incomingValue) / afterTotalQuantity;
  await q(
    `UPDATE inventory.stock_balances
        SET quantity = $5, avg_cost_thb = $6, updated_at = now()
      WHERE product_id = $1 AND warehouse_id = $2
        AND bin_id IS NOT DISTINCT FROM $3::bigint
        AND lot_id IS NOT DISTINCT FROM $4::bigint`,
    [line.productId, line.toWarehouseId, line.toBinId ?? null, line.lotId ?? null, afterQty, cost(afterCost)],
  );
  await q(
    `UPDATE inventory.stock_balances SET avg_cost_thb = $3, updated_at = now()
      WHERE product_id = $1 AND warehouse_id = $2`,
    [line.productId, line.toWarehouseId, cost(afterCost)],
  );
  return { before, quantity: amount, unitCost: incomingCost, afterCost: cost(afterCost), value: incomingValue };
}

async function removeStock(q: FinanceQuery, line: StockMoveLine) {
  if (!line.fromWarehouseId) throw new PostingError('A source warehouse is required');
  const amount = qty(line.quantity);
  const before = await lockBalance(q, {
    productId: line.productId,
    warehouseId: line.fromWarehouseId,
    binId: line.fromBinId,
    lotId: line.lotId,
  });
  if (before.quantity < amount) throw new PostingError(`Negative stock blocked for product ${line.productId}`);
  const afterQty = before.quantity - amount;
  const afterTotalQuantity = before.totalQuantity - amount;
  const removedValue = money(amount * before.avgCost);
  const afterCost = afterTotalQuantity === 0 ? 0 : cost((money(before.totalQuantity * before.avgCost) - removedValue) / afterTotalQuantity);
  await q(
    `UPDATE inventory.stock_balances
        SET quantity = $5, avg_cost_thb = $6, updated_at = now()
      WHERE product_id = $1 AND warehouse_id = $2
        AND bin_id IS NOT DISTINCT FROM $3::bigint
        AND lot_id IS NOT DISTINCT FROM $4::bigint`,
    [line.productId, line.fromWarehouseId, line.fromBinId ?? null, line.lotId ?? null, afterQty, afterCost],
  );
  await q(
    `UPDATE inventory.stock_balances SET avg_cost_thb = $3, updated_at = now()
      WHERE product_id = $1 AND warehouse_id = $2`,
    [line.productId, line.fromWarehouseId, afterCost],
  );
  return { before, quantity: amount, unitCost: before.avgCost, afterCost, value: removedValue };
}

async function insertMovement(q: FinanceQuery, move: StockMove, actorId: number) {
  const existing = await q<MoveRow>(
    `SELECT id::text, movement_no, journal_id::text, status
       FROM inventory.stock_movements WHERE source_event_key = $1 FOR UPDATE`,
    [move.sourceEventKey],
  );
  if (existing.rows[0]) return { existing: true, row: existing.rows[0] };
  const inserted = await q<MoveRow>(
    `INSERT INTO inventory.stock_movements
       (kind, movement_date, source_type, source_id, source_event_key, branch_id, metadata, created_by)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
     RETURNING id::text, movement_no, journal_id::text, status`,
    [move.kind, move.movementDate, move.sourceType, move.sourceId, move.sourceEventKey, move.branchId, move.metadata ?? {}, actorId],
  );
  return { existing: false, row: inserted.rows[0] };
}

function result(row: MoveRow): StockMoveResult {
  if (row.status !== 'posted' || !row.movement_no) throw new PostingError('Stock event exists but is not posted');
  return {
    movementId: Number(row.id),
    movementNo: row.movement_no,
    journalId: row.journal_id === null ? null : Number(row.journal_id),
    status: 'posted',
  };
}

async function addMovementLine(q: FinanceQuery, args: {
  movementId: number;
  lineNo: number;
  line: StockMoveLine;
  unitCost: number;
  value: number;
}) {
  const inserted = await q<{ id: string }>(
    `INSERT INTO inventory.stock_movement_lines
       (movement_id, line_no, product_id, quantity, unit_code, unit_cost_thb,
        extended_cost_thb, from_warehouse_id, from_bin_id, to_warehouse_id,
        to_bin_id, lot_id, source_line_id, metadata)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
     RETURNING id::text`,
    [
      args.movementId,
      args.lineNo,
      args.line.productId,
      args.line.quantity,
      args.line.unitCode,
      args.unitCost,
      args.value,
      args.line.fromWarehouseId ?? null,
      args.line.fromBinId ?? null,
      args.line.toWarehouseId ?? null,
      args.line.toBinId ?? null,
      args.line.lotId ?? null,
      args.line.sourceLineId ?? null,
      args.line.metadata ?? {},
    ],
  );
  return Number(inserted.rows[0].id);
}

async function recordCost(q: FinanceQuery, args: {
  movementLineId: number;
  productId: number;
  warehouseId: number;
  eventDate: string;
  quantityDelta: number;
  beforeCost: number;
  afterCost: number;
  valueDelta: number;
}) {
  await q(
    `INSERT INTO inventory.costing_events
       (movement_line_id, product_id, warehouse_id, event_date, quantity_delta,
        unit_cost_before, unit_cost_after, value_delta_thb)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)`,
    [args.movementLineId, args.productId, args.warehouseId, args.eventDate, args.quantityDelta, args.beforeCost, args.afterCost, args.valueDelta],
  );
}

async function queueBackdatedRecost(q: FinanceQuery, move: StockMove, actorId: number) {
  for (const line of move.lines) {
    const warehouses = [line.fromWarehouseId, line.toWarehouseId].filter((id): id is number => Boolean(id));
    for (const warehouseId of new Set(warehouses)) {
      const later = await q<{ present: boolean }>(
        `SELECT EXISTS (
           SELECT 1 FROM inventory.costing_events
            WHERE product_id = $1 AND warehouse_id = $2 AND event_date > $3
         ) AS present`,
        [line.productId, warehouseId, move.movementDate],
      );
      if (later.rows[0]?.present) {
        await q(
          `INSERT INTO inventory.recost_jobs(product_id, warehouse_id, from_date, requested_by)
           SELECT $1,$2,$3,$4
            WHERE NOT EXISTS (
              SELECT 1 FROM inventory.recost_jobs
               WHERE product_id = $1 AND warehouse_id = $2 AND from_date <= $3 AND status IN ('pending','running')
            )`,
          [line.productId, warehouseId, move.movementDate, actorId],
        );
      }
    }
  }
}

async function finish(q: FinanceQuery, movementId: number, actorId: number, journalId: number | null) {
  const movementNo = await q<{ movement_no: string }>(
    `UPDATE inventory.stock_movements m
        SET status = 'posted', posted_by = $2, posted_at = now(), journal_id = $3,
            movement_no = coalesce(movement_no, finance.next_document_number('SM', branch_id, movement_date))
      WHERE id = $1
      RETURNING movement_no`,
    [movementId, actorId, journalId],
  );
  return movementNo.rows[0].movement_no;
}

async function execute(
  move: StockMove,
  actor: PostingActor,
  permission: string,
  mode: 'add' | 'remove' | 'transfer',
  offsetAccount: string,
  hook?: (q: FinanceQuery, result: StockMoveResult) => Promise<void>,
) {
  requirePerm(actor, permission);
  requirePerm(actor, 'finance:journal:prepare::allow');
  requirePerm(actor, 'finance:journal:approve::allow');
  if (!/^\d{4}-\d{2}-\d{2}$/.test(move.movementDate)) throw new PostingError('Movement date must be YYYY-MM-DD');
  if (!move.lines.length) throw new PostingError('A stock movement requires at least one line');
  return withTransaction(async (q) => {
    const inserted = await insertMovement(q, move, actor.id);
    if (inserted.existing) return result(inserted.row);
    const movementId = Number(inserted.row.id);
    const journalLines: JournalLine[] = [];
    for (const [index, line] of move.lines.entries()) {
      const p = await product(q, line.productId);
      await validateLot(q, p, line);
      if (line.unitCode !== p.base_unit) throw new PostingError(`Product ${line.productId} must be moved in base unit ${p.base_unit}`);
      if (mode === 'add') {
        const added = await addStock(q, line, line.unitCostThb);
        const movementLineId = await addMovementLine(q, { movementId, lineNo: index + 1, line, unitCost: added.unitCost, value: added.value });
        await recordCost(q, {
          movementLineId,
          productId: line.productId,
          warehouseId: line.toWarehouseId as number,
          eventDate: move.movementDate,
          quantityDelta: added.quantity,
          beforeCost: added.before.avgCost,
          afterCost: added.afterCost,
          valueDelta: added.value,
        });
        journalLines.push(
          { accountCode: p.inventory_account_code, description: `${move.kind} inventory`, debitThb: added.value, branchId: move.branchId, productId: line.productId, warehouseId: line.toWarehouseId },
          { accountCode: offsetAccount, description: `${move.kind} offset`, creditThb: added.value, branchId: move.branchId, productId: line.productId, warehouseId: line.toWarehouseId },
        );
      } else if (mode === 'remove') {
        const removed = await removeStock(q, line);
        const movementLineId = await addMovementLine(q, { movementId, lineNo: index + 1, line, unitCost: removed.unitCost, value: removed.value });
        await recordCost(q, {
          movementLineId,
          productId: line.productId,
          warehouseId: line.fromWarehouseId as number,
          eventDate: move.movementDate,
          quantityDelta: -removed.quantity,
          beforeCost: removed.before.avgCost,
          afterCost: removed.afterCost,
          valueDelta: -removed.value,
        });
        journalLines.push(
          { accountCode: offsetAccount === 'auto:cogs' ? p.cogs_account_code : offsetAccount, description: `${move.kind} offset`, debitThb: removed.value, branchId: move.branchId, productId: line.productId, warehouseId: line.fromWarehouseId },
          { accountCode: p.inventory_account_code, description: `${move.kind} inventory`, creditThb: removed.value, branchId: move.branchId, productId: line.productId, warehouseId: line.fromWarehouseId },
        );
      } else {
        const removed = await removeStock(q, line);
        const added = await addStock(q, line, removed.unitCost);
        const movementLineId = await addMovementLine(q, { movementId, lineNo: index + 1, line, unitCost: removed.unitCost, value: removed.value });
        await recordCost(q, {
          movementLineId,
          productId: line.productId,
          warehouseId: line.fromWarehouseId as number,
          eventDate: move.movementDate,
          quantityDelta: -removed.quantity,
          beforeCost: removed.before.avgCost,
          afterCost: removed.afterCost,
          valueDelta: -removed.value,
        });
        await recordCost(q, {
          movementLineId,
          productId: line.productId,
          warehouseId: line.toWarehouseId as number,
          eventDate: move.movementDate,
          quantityDelta: added.quantity,
          beforeCost: added.before.avgCost,
          afterCost: added.afterCost,
          valueDelta: added.value,
        });
        journalLines.push(
          { accountCode: p.inventory_account_code, description: 'Inventory transfer in', debitThb: removed.value, branchId: move.branchId, productId: line.productId, warehouseId: line.toWarehouseId },
          { accountCode: p.inventory_account_code, description: 'Inventory transfer out', creditThb: removed.value, branchId: move.branchId, productId: line.productId, warehouseId: line.fromWarehouseId },
        );
      }
    }
    const journal = await postJournalInTransaction(q, {
      postingDate: move.movementDate,
      description: `${move.kind} ${move.sourceType} ${move.sourceId}`,
      sourceType: 'inventory_movement',
      sourceId: String(movementId),
      sourceEventKey: `inventory-journal:${move.sourceEventKey}`,
      branchId: move.branchId,
      metadata: { movementId, businessSourceType: move.sourceType, businessSourceId: move.sourceId },
      lines: journalLines,
    }, actor);
    const movementNo = await finish(q, movementId, actor.id, journal.id);
    await queueBackdatedRecost(q, move, actor.id);
    const posted = { movementId, movementNo, journalId: journal.id, status: 'posted' as const };
    if (hook) await hook(q, posted);
    return posted;
  });
}

export function receiveStock(move: StockMove, actor: PostingActor, offsetAccount = '210700') {
  return execute({ ...move, kind: 'receipt' }, actor, 'inventory:stock:receive::allow', 'add', offsetAccount);
}

export function receiveStockWithHook(
  move: StockMove,
  actor: PostingActor,
  offsetAccount: string,
  hook: (q: FinanceQuery, result: StockMoveResult) => Promise<void>,
) {
  return execute({ ...move, kind: 'receipt' }, actor, 'inventory:stock:receive::allow', 'add', offsetAccount, hook);
}

export function shipStock(move: StockMove, actor: PostingActor) {
  return execute({ ...move, kind: 'shipment' }, actor, 'inventory:stock:ship::allow', 'remove', 'auto:cogs');
}

export function shipStockWithHook(
  move: StockMove,
  actor: PostingActor,
  hook: (q: FinanceQuery, result: StockMoveResult) => Promise<void>,
) {
  return execute({ ...move, kind: 'shipment' }, actor, 'inventory:stock:ship::allow', 'remove', 'auto:cogs', hook);
}

export function returnStock(move: StockMove, actor: PostingActor, direction: 'customer' | 'vendor') {
  return direction === 'customer'
    ? execute({ ...move, kind: 'customer_return' }, actor, 'inventory:stock:receive::allow', 'add', '510100')
    : execute({ ...move, kind: 'vendor_return' }, actor, 'inventory:stock:ship::allow', 'remove', '210700');
}

export function returnStockWithHook(
  move: StockMove,
  actor: PostingActor,
  direction: 'customer' | 'vendor',
  hook: (q: FinanceQuery, result: StockMoveResult) => Promise<void>,
) {
  return direction === 'customer'
    ? execute({ ...move, kind: 'customer_return' }, actor, 'inventory:stock:receive::allow', 'add', '510100', hook)
    : execute({ ...move, kind: 'vendor_return' }, actor, 'inventory:stock:ship::allow', 'remove', '210700', hook);
}

export function transferStock(move: StockMove, actor: PostingActor) {
  return execute({ ...move, kind: 'transfer' }, actor, 'inventory:stock:transfer::allow', 'transfer', '510500');
}

export function adjustStock(move: StockMove, actor: PostingActor, direction: 'increase' | 'decrease', offsetAccount = '510500') {
  return execute({ ...move, kind: 'adjustment' }, actor, 'inventory:stock:adjust::allow', direction === 'increase' ? 'add' : 'remove', offsetAccount);
}

export function countStock(move: StockMove, actor: PostingActor, direction: 'increase' | 'decrease') {
  return execute({ ...move, kind: 'count' }, actor, 'inventory:stock:count::allow', direction === 'increase' ? 'add' : 'remove', '510500');
}

export async function writeDownStock(args: StockWriteDown, actor: PostingActor) {
  requirePerm(actor, 'inventory:stock:adjust::allow');
  requirePerm(actor, 'finance:journal:prepare::allow');
  requirePerm(actor, 'finance:journal:approve::allow');
  if (!/^\d{4}-\d{2}-\d{2}$/.test(args.movementDate)) throw new PostingError('Movement date must be YYYY-MM-DD');
  const amount = money(args.amountThb);
  if (amount <= 0) throw new PostingError('Write-down amount must be positive');
  return withTransaction(async (q) => {
    const p = await product(q, args.productId);
    const move: StockMove = {
      kind: 'write_down',
      movementDate: args.movementDate,
      sourceType: 'inventory_nrv',
      sourceId: args.reason,
      sourceEventKey: args.sourceEventKey,
      branchId: args.branchId,
      metadata: { reason: args.reason, amountThb: amount },
      lines: [],
    };
    const inserted = await insertMovement(q, move, actor.id);
    if (inserted.existing) return result(inserted.row);
    const before = await lockBalance(q, { productId: args.productId, warehouseId: args.warehouseId, lotId: args.lotId });
    if (before.totalQuantity <= 0) throw new PostingError('Write-down requires stock on hand');
    const currentValue = money(before.totalQuantity * before.avgCost);
    if (amount > currentValue) throw new PostingError('Write-down cannot exceed the current stock value');
    const afterCost = cost((currentValue - amount) / before.totalQuantity);
    await q(
      `UPDATE inventory.stock_balances SET avg_cost_thb = $3, updated_at = now()
        WHERE product_id = $1 AND warehouse_id = $2`,
      [args.productId, args.warehouseId, afterCost],
    );
    const movementId = Number(inserted.row.id);
    const movementLineId = await addMovementLine(q, {
      movementId,
      lineNo: 1,
      line: { productId: args.productId, quantity: before.totalQuantity, unitCode: p.base_unit, unitCostThb: before.avgCost, fromWarehouseId: args.warehouseId, lotId: args.lotId, metadata: { valueOnly: true, reason: args.reason } },
      unitCost: before.avgCost,
      value: amount,
    });
    await recordCost(q, { movementLineId, productId: args.productId, warehouseId: args.warehouseId, eventDate: args.movementDate, quantityDelta: 0, beforeCost: before.avgCost, afterCost, valueDelta: -amount });
    const journal = await postJournalInTransaction(q, {
      postingDate: args.movementDate,
      description: `Inventory NRV write-down: ${args.reason}`,
      sourceType: 'inventory_movement',
      sourceId: String(movementId),
      sourceEventKey: `inventory-journal:${args.sourceEventKey}`,
      branchId: args.branchId,
      metadata: { movementId, writeDownAmountThb: amount },
      lines: [
        { accountCode: '520200', description: 'Inventory NRV impairment', debitThb: amount, branchId: args.branchId, productId: args.productId, warehouseId: args.warehouseId },
        { accountCode: p.inventory_account_code, description: 'Inventory NRV write-down', creditThb: amount, branchId: args.branchId, productId: args.productId, warehouseId: args.warehouseId },
      ],
    }, actor);
    const movementNo = await finish(q, movementId, actor.id, journal.id);
    await queueBackdatedRecost(q, { ...move, lines: [{ productId: args.productId, quantity: before.totalQuantity, unitCode: p.base_unit, fromWarehouseId: args.warehouseId, lotId: args.lotId }] }, actor.id);
    return { movementId, movementNo, journalId: journal.id, status: 'posted' as const };
  });
}

export async function reverseWriteDown(args: { movementId: number; movementDate: string; sourceEventKey: string }, actor: PostingActor) {
  requirePerm(actor, 'inventory:stock:adjust::allow');
  requirePerm(actor, 'finance:journal:prepare::allow');
  requirePerm(actor, 'finance:journal:approve::allow');
  if (!/^\d{4}-\d{2}-\d{2}$/.test(args.movementDate)) throw new PostingError('Movement date must be YYYY-MM-DD');
  return withTransaction(async (q) => {
    const prior = await q<MoveRow>(`SELECT id::text, movement_no, journal_id::text, status FROM inventory.stock_movements WHERE source_event_key = $1 FOR UPDATE`, [args.sourceEventKey]);
    if (prior.rows[0]) return result(prior.rows[0]);
    const original = await q<{
      movement_id: string;
      branch_id: string;
      product_id: string;
      warehouse_id: string;
      lot_id: string | null;
      quantity: string;
      unit_code: string;
      amount: string;
      unit_cost_before: string;
    }>(
      `SELECT m.id::text AS movement_id, m.branch_id::text, l.product_id::text,
              l.from_warehouse_id::text AS warehouse_id, l.lot_id::text,
              l.quantity::text, l.unit_code, l.extended_cost_thb::text AS amount,
              c.unit_cost_before::text
         FROM inventory.stock_movements m
         JOIN inventory.stock_movement_lines l ON l.movement_id = m.id
         JOIN inventory.costing_events c ON c.movement_line_id = l.id
        WHERE m.id = $1 AND m.kind = 'write_down' AND m.status = 'posted'
        FOR UPDATE OF m`,
      [args.movementId],
    );
    const row = original.rows[0];
    if (!row) throw new PostingError('Posted write-down not found');
    const already = await q<{ present: boolean }>(`SELECT EXISTS (SELECT 1 FROM inventory.stock_movements WHERE reversal_of_id = $1 AND status = 'posted') AS present`, [args.movementId]);
    if (already.rows[0]?.present) throw new PostingError('Write-down was already reversed');
    const productId = Number(row.product_id);
    const warehouseId = Number(row.warehouse_id);
    const lotId = row.lot_id === null ? null : Number(row.lot_id);
    const p = await product(q, productId);
    const before = await lockBalance(q, { productId, warehouseId, lotId });
    if (before.totalQuantity <= 0) throw new PostingError('No remaining stock is available for reversal');
    const originalAmount = money(Number(row.amount));
    const maximum = money(before.totalQuantity * Math.max(0, Number(row.unit_cost_before) - before.avgCost));
    const amount = Math.min(originalAmount, maximum);
    if (amount <= 0) throw new PostingError('No reversible write-down remains at the current cost');
    const afterCost = cost(before.avgCost + amount / before.totalQuantity);
    const inserted = await insertMovement(q, {
      kind: 'write_down_reversal',
      movementDate: args.movementDate,
      sourceType: 'inventory_nrv_reversal',
      sourceId: String(args.movementId),
      sourceEventKey: args.sourceEventKey,
      branchId: Number(row.branch_id),
      metadata: { reversalOfMovementId: args.movementId, amountThb: amount },
      lines: [],
    }, actor.id);
    const movementId = Number(inserted.row.id);
    await q(`UPDATE inventory.stock_movements SET reversal_of_id = $2 WHERE id = $1`, [movementId, args.movementId]);
    await q(
      `UPDATE inventory.stock_balances SET avg_cost_thb = $3, updated_at = now()
        WHERE product_id = $1 AND warehouse_id = $2`,
      [productId, warehouseId, afterCost],
    );
    const movementLineId = await addMovementLine(q, {
      movementId,
      lineNo: 1,
      line: { productId, quantity: before.totalQuantity, unitCode: row.unit_code, unitCostThb: before.avgCost, toWarehouseId: warehouseId, lotId, metadata: { valueOnly: true, reversalOfMovementId: args.movementId } },
      unitCost: before.avgCost,
      value: amount,
    });
    await recordCost(q, { movementLineId, productId, warehouseId, eventDate: args.movementDate, quantityDelta: 0, beforeCost: before.avgCost, afterCost, valueDelta: amount });
    const journal = await postJournalInTransaction(q, {
      postingDate: args.movementDate,
      description: `Reverse inventory NRV write-down ${args.movementId}`,
      sourceType: 'inventory_movement',
      sourceId: String(movementId),
      sourceEventKey: `inventory-journal:${args.sourceEventKey}`,
      branchId: Number(row.branch_id),
      metadata: { movementId, reversalOfMovementId: args.movementId, writeDownReversalThb: amount },
      lines: [
        { accountCode: p.inventory_account_code, description: 'Inventory NRV reversal', debitThb: amount, branchId: Number(row.branch_id), productId, warehouseId },
        { accountCode: '520200', description: 'Inventory impairment reversal', creditThb: amount, branchId: Number(row.branch_id), productId, warehouseId },
      ],
    }, actor);
    const movementNo = await finish(q, movementId, actor.id, journal.id);
    return { movementId, movementNo, journalId: journal.id, status: 'posted' as const };
  });
}

export async function processRecostJob(jobId: number, postingDate: string, actor: PostingActor) {
  requirePerm(actor, 'inventory:stock:adjust::allow');
  requirePerm(actor, 'finance:journal:prepare::allow');
  requirePerm(actor, 'finance:journal:approve::allow');
  if (!/^\d{4}-\d{2}-\d{2}$/.test(postingDate)) throw new PostingError('Posting date must be YYYY-MM-DD');
  try {
    return await withTransaction(async (q) => {
      const job = await q<{ id: string; product_id: string; warehouse_id: string; status: string; branch_id: string }>(
        `SELECT j.id::text, j.product_id::text, j.warehouse_id::text, j.status, w.branch_id::text
           FROM inventory.recost_jobs j JOIN inventory.warehouses w ON w.id = j.warehouse_id
          WHERE j.id = $1 FOR UPDATE OF j`,
        [jobId],
      );
      const row = job.rows[0];
      if (!row) throw new PostingError('Recost job not found');
      if (row.status === 'completed') {
        const completed = await q<{ id: string; movement_no: string; journal_id: string | null; status: string }>(`SELECT id::text, movement_no, journal_id::text, status FROM inventory.stock_movements WHERE source_event_key = $1`, [`recost-job:${jobId}`]);
        return completed.rows[0] ? result(completed.rows[0]) : null;
      }
      await q(`UPDATE inventory.recost_jobs SET status = 'running', error = NULL WHERE id = $1`, [jobId]);
      const productId = Number(row.product_id);
      const warehouseId = Number(row.warehouse_id);
      const branchId = Number(row.branch_id);
      const p = await product(q, productId);
      const balanceRows = await q<{ quantity: string; avg_cost_thb: string }>(`SELECT quantity::text, avg_cost_thb::text FROM inventory.stock_balances WHERE product_id = $1 AND warehouse_id = $2 FOR UPDATE`, [productId, warehouseId]);
      const events = await q<{ quantity_delta: string; value_delta_thb: string; unit_cost_thb: string }>(
        `SELECT c.quantity_delta::text, c.value_delta_thb::text, l.unit_cost_thb::text
           FROM inventory.costing_events c
           JOIN inventory.stock_movement_lines l ON l.id = c.movement_line_id
           JOIN inventory.stock_movements m ON m.id = l.movement_id
          WHERE c.product_id = $1 AND c.warehouse_id = $2 AND m.status = 'posted'
          ORDER BY c.event_date, m.id, c.id`,
        [productId, warehouseId],
      );
      let quantity = 0;
      let value = 0;
      for (const event of events.rows) {
        const delta = Number(event.quantity_delta);
        if (delta > 0) value += delta * Number(event.unit_cost_thb);
        else if (delta < 0) value += delta * (quantity > 0 ? value / quantity : 0);
        else value += Number(event.value_delta_thb);
        quantity += delta;
        if (quantity < -0.000001) throw new PostingError('Recost sequence would create negative stock');
        if (Math.abs(quantity) < 0.000001) {
          quantity = 0;
          value = 0;
        }
      }
      const currentQuantity = balanceRows.rows.reduce((sum, balance) => sum + Number(balance.quantity), 0);
      if (Math.abs(currentQuantity - quantity) > 0.000001) throw new PostingError('Stock balance does not tie to costing-event quantity');
      const currentValue = balanceRows.rows.reduce((sum, balance) => sum + Number(balance.quantity) * Number(balance.avg_cost_thb), 0);
      const expectedCost = quantity > 0 ? cost(Math.max(0, value / quantity)) : 0;
      const expectedValue = money(quantity * expectedCost);
      const difference = money(expectedValue - currentValue);
      await q(`UPDATE inventory.stock_balances SET avg_cost_thb = $3, updated_at = now() WHERE product_id = $1 AND warehouse_id = $2`, [productId, warehouseId, expectedCost]);
      if (Math.abs(difference) < 0.01) {
        await q(`UPDATE inventory.recost_jobs SET status = 'completed', completed_at = now(), error = NULL WHERE id = $1`, [jobId]);
        return null;
      }
      const inserted = await insertMovement(q, { kind: 'recost', movementDate: postingDate, sourceType: 'recost_job', sourceId: String(jobId), sourceEventKey: `recost-job:${jobId}`, branchId, metadata: { jobId, differenceThb: difference }, lines: [] }, actor.id);
      if (inserted.existing) return result(inserted.row);
      const movementId = Number(inserted.row.id);
      const movementLineId = await addMovementLine(q, {
        movementId,
        lineNo: 1,
        line: { productId, quantity: Math.max(quantity, 0.000001), unitCode: p.base_unit, ...(difference > 0 ? { toWarehouseId: warehouseId } : { fromWarehouseId: warehouseId }), metadata: { valueOnly: true, recostJobId: jobId } },
        unitCost: expectedCost,
        value: Math.abs(difference),
      });
      await recordCost(q, { movementLineId, productId, warehouseId, eventDate: postingDate, quantityDelta: 0, beforeCost: currentQuantity > 0 ? currentValue / currentQuantity : 0, afterCost: expectedCost, valueDelta: difference });
      const amount = Math.abs(difference);
      const journal = await postJournalInTransaction(q, {
        postingDate,
        description: `Inventory recost adjustment for job ${jobId}`,
        sourceType: 'inventory_recost',
        sourceId: String(jobId),
        sourceEventKey: `inventory-journal:recost-job:${jobId}`,
        branchId,
        metadata: { movementId, jobId, differenceThb: difference },
        lines: difference > 0
          ? [
              { accountCode: p.inventory_account_code, description: 'Inventory recost increase', debitThb: amount, branchId, productId, warehouseId },
              { accountCode: p.cogs_account_code, description: 'COGS recost adjustment', creditThb: amount, branchId, productId, warehouseId },
            ]
          : [
              { accountCode: p.cogs_account_code, description: 'COGS recost adjustment', debitThb: amount, branchId, productId, warehouseId },
              { accountCode: p.inventory_account_code, description: 'Inventory recost decrease', creditThb: amount, branchId, productId, warehouseId },
            ],
      }, actor);
      const movementNo = await finish(q, movementId, actor.id, journal.id);
      await q(`UPDATE inventory.recost_jobs SET status = 'completed', adjustment_journal_id = $2, completed_at = now(), error = NULL WHERE id = $1`, [jobId, journal.id]);
      return { movementId, movementNo, journalId: journal.id, status: 'posted' as const };
    });
  } catch (error) {
    await withTransaction(async (q) => {
      await q(`UPDATE inventory.recost_jobs SET status = 'failed', error = $2 WHERE id = $1 AND status <> 'completed'`, [jobId, error instanceof Error ? error.message : 'Recost failed']);
    }).catch(() => {});
    throw error;
  }
}
