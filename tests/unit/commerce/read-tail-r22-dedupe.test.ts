import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { COMMERCE_OWNER_WRITE_NOOP_SQL } from '../../../packages/server-commerce/src/adapters/native-owner-write';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const adapters = resolve(root, 'packages/server-commerce/src/adapters');
const src = resolve(root, 'packages/server-commerce/src');
const commerce = () => readFileSync(resolve(adapters, 'commerce.ts'), 'utf8');
const native = () => readFileSync(resolve(adapters, 'native-owner-write.ts'), 'utf8');
const norm = (s: string) => s.replace(/\s+/g, ' ');

/** Byte-for-byte snapshots of the 17 retired call-site lines at the parent SHA. */
const HEAD_LINES: string[] = [
  "}) => { await nativeOrder('order.version-bump', [facts.now, facts.id], facts.id, `commerce:owner-write:order.version-bump:${facts.id}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('return.create', [facts.id, facts.companyId, facts.branch_id, facts.order_id, facts.fulfillmentId, facts.invoice_id, facts.kind, facts.number, facts.date, facts.reason, facts.subtotal, facts.tax, facts.total, facts.userId, facts.now, facts.now2], facts.id, `commerce:owner-write:return.create:${facts.id}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('return.line', [facts.returnLineId, facts.id, facts.id2, facts.product_id, facts.quantity, facts.returnValue, facts.returnSubtotal, facts.returnTax, facts.index, facts.condition, facts.fact10, facts.fact11, facts.expectedSubtotal], facts.returnLineId, `commerce:owner-write:return.line:${facts.returnLineId}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('return.lot-allocation', [facts.id, facts.returnLineId, facts.fulfillmentAllocationId, facts.lotId, facts.quantity, facts.value, facts.snapshotQuantity, facts.snapshotValue, facts.snapshotVersion], facts.id, `commerce:owner-write:return.lot-allocation:${facts.id}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('fulfillment.version-guarded', [facts.fulfillmentId, facts.expectedVersion], facts.fulfillmentId, `commerce:owner-write:fulfillment.version-guarded:${facts.fulfillmentId}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('return.reject', [facts.userId, facts.reason, facts.fact2, facts.returnId], facts.returnId, `commerce:owner-write:return.reject:${facts.returnId}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('return.stock-move', [facts.fact0, facts.returnId, facts.warehouse_id, facts.product_id, facts.fact4, facts.fact5, facts.return_date, facts.now], facts.fact0, `commerce:owner-write:return.stock-move:${facts.fact0}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('return.approve', [facts.userId, facts.stockJournal, facts.creditJournal, facts.baseCreditMinor, facts.fact4, facts.now, facts.returnId], facts.returnId, `commerce:owner-write:return.approve:${facts.returnId}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('return.post', [facts.credited, facts.refund, facts.carried, facts.baseCreditedMinor, facts.baseRefundMinor, facts.fact5, facts.now, facts.returnId], facts.returnId, `commerce:owner-write:return.post:${facts.returnId}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('order.update-parties', [facts.partyId, facts.warehouseId, facts.branch_id, facts.orderDate, facts.dueDate, facts.note, facts.subtotal, facts.tax, facts.fact8, facts.fact9, facts.fact10, facts.fact11, facts.now, facts.record], facts.record, `commerce:owner-write:order.update-parties:${facts.record}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('order.lines-delete', [facts.record], facts.record, `commerce:owner-write:order.lines-delete:${facts.record}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('order.create', [facts.record, facts.companyId, facts.branch_id, facts.departmentId, facts.kind, facts.kind, facts.companyId, facts.orderDate, facts.fact5, facts.partyId, facts.warehouseId, facts.currency, facts.orderDate, facts.dueDate, facts.note, facts.subtotal, facts.tax, facts.fact14, facts.fact15, facts.fact16, facts.fact17, facts.userId, facts.now, facts.now2], facts.record, `commerce:owner-write:order.create:${facts.record}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('order.lines-create', [facts.id, facts.record, facts.productId, facts.name, facts.qty, facts.price, facts.taxBps, facts.subtotal, facts.tax, facts.index, facts.listPrice, facts.discountBps, facts.itemType], facts.id, `commerce:owner-write:order.lines-create:${facts.id}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('order.transition', [facts.state, facts.fact1, facts.fact2, facts.userId, facts.fact4, facts.fact5, facts.fact6, facts.fact7, facts.fact8, facts.id], facts.id, `commerce:owner-write:order.transition:${facts.id}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('order.convert', [facts.salesId, facts.fact1, facts.userId, facts.fact3, facts.fact4, facts.id], facts.salesId, `commerce:owner-write:order.convert:${facts.salesId}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('order.lines-convert', [facts.salesId, facts.salesId2, facts.id], facts.salesId, `commerce:owner-write:order.lines-convert:${facts.salesId}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
  "}) => { await nativeOrder('fulfillment.invoice-link', [facts.invoice, facts.fulfillmentId], facts.fulfillmentId, `commerce:owner-write:fulfillment.invoice-link:${facts.fulfillmentId}:${ownerWriteNonce(facts)}`, (ownerWriteActor(facts)), (ownerWriteCompany(facts, actor))); return db.prepare('SELECT 1'); },",
];

const delegate = (line: string) => line.replace("db.prepare('SELECT 1')", 'db.prepare(COMMERCE_OWNER_WRITE_NOOP_SQL)');

describe('Commerce read-tail r22 noop-delegation retirement', () => {
  test('constant byte oracle: noop text is byte-identical to the retired SELECT 1 literal', () => {
    expect(COMMERCE_OWNER_WRITE_NOOP_SQL).toBe('SELECT 1');
    expect(Buffer.from(COMMERCE_OWNER_WRITE_NOOP_SQL)).toEqual(Buffer.from('SELECT 1'));
    expect(createHash('sha256').update(COMMERCE_OWNER_WRITE_NOOP_SQL, 'utf8').digest('hex')).toBe('e004ebd5b5532a4b85984a62f8ad48a81aa3460c1ca07701f386135d72cdecf5');
    expect(native().match(/export const COMMERCE_OWNER_WRITE_NOOP_SQL = "SELECT 1";/g) ?? []).toHaveLength(1);
  });

  test('all 17 sites delegate to the constant; zero residual literals', () => {
    const s = commerce();
    expect(HEAD_LINES).toHaveLength(17);
    expect(s.split('db.prepare(COMMERCE_OWNER_WRITE_NOOP_SQL)').length - 1).toBe(17);
    expect(s).not.toMatch(/db\.prepare\((?:"|')SELECT 1(?:"|')\)/);
    expect(s.split('await nativeOrder(').length - 1).toBe(17);
    expect(norm(s)).toContain(norm('import { COMMERCE_OWNER_WRITE_NOOP_SQL, nativeCommerceOwnerWrite, NativeCommerceOwnerWriteError } from "@erpx/server-commerce/adapters/native-owner-write";'));
  });

  test('native kinds, positional arrays, keys and return contracts are byte-unchanged', () => {
    const s = commerce();
    for (const before of HEAD_LINES) {
      const after = delegate(before);
      expect(before).toContain("db.prepare('SELECT 1')");
      expect(s).toContain(after);
      expect(after).toContain('; return db.prepare(COMMERCE_OWNER_WRITE_NOOP_SQL); },');
      expect(after.match(/nativeOrder\('[^']+', \[[^\]]*\]/)).not.toBeNull();
    }
  });

  test('protected r18-r21 read/warehouse/renderer families stay untouched', () => {
    const s = commerce();
    const counts: Record<string, number> = {
      COMMERCE_STOCK_BY_WAREHOUSE_PRODUCT_SQL: 2,
      COMMERCE_FULFILLMENT_ORDER_ID_SQL: 2,
      COMMERCE_PRODUCT_MASTER_ROWS_SQL: 1,
      COMMERCE_WAREHOUSE_MASTER_ROWS_SQL: 1,
      'EXISTS(SELECT 1': 8,
      nativeOrder: 18,
    };
    for (const [token, n] of Object.entries(counts)) expect(s.split(token).length - 1, token).toBe(n);
  });

  test('pins the owner TypeScript SELECT KPI at 422 (before 438)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(422);
  });
});
