import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { reservedAvailabilitySumSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';
import { productionReadProjection } from '../../../packages/server-inventory/src/adapters/production-read-projections';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe literals from HEAD, used strictly as byte-identity oracles
// (never as live query sources). Only the reservation alias and WHERE tail vary;
// every consumer keeps its own bind shape and independent statement.
const headSum = (tail: string) =>
  `COALESCE((SELECT sum(quantity_milli) FROM inventory_active_reservations ${tail}),0)`;

const sites: [key: 'reservedStock' | 'reservedAvailability', tail: string][] = [
  ['reservedStock', 'r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id'],
  ['reservedAvailability', 'WHERE warehouse_id=? AND product_id=?'],
];

describe('Inventory read-tail r39 dedupe', () => {
  test('f1: reservedAvailabilitySumSql reproduces both pre-dedupe literals byte-identically', () => {
    for (const [key, tail] of sites) {
      const sql = reservedAvailabilitySumSql(tail);
      expect(sql).toBe(headSum(tail));
      expect(productionReadProjection[key]).toBe(headSum(tail));
      expect(selectCount(sql)).toBe(1);
      expect(sql).not.toMatch(lockRe);
    }
  });

  test('the two pre-dedupe literals share one skeleton after parameter masking', () => {
    const masked = sites.map(([, tail]) => reservedAvailabilitySumSql(tail).split(tail).join('<TAIL>'));
    expect(masked[0]).toBe(masked[1]);
  });

  test('helper template carries exactly one SELECT token and is defined once', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('export const reservedAvailabilitySumSql').length - 1).toBe(1);
    const from = shared.indexOf('export const reservedAvailabilitySumSql');
    const rest = shared.slice(from);
    const end = rest.search(/\nexport const |\n\/\*\*/);
    const block = end === -1 ? rest : rest.slice(0, end);
    expect(selectCount(block)).toBe(1);
    expect(block).not.toMatch(lockRe);
  });

  test('former inline literals are gone and call sites compose the helper with unchanged arguments', () => {
    const text = src('production-read-projections.ts');
    for (const [key, tail] of sites) {
      expect(text).not.toContain(headSum(tail));
      expect(text).toContain(`reservedAvailabilitySumSql('${tail}')`);
    }
    // The two unrelated projection facts stay untouched.
    expect(text).toContain('"quarantinedStock": "inventory_quarantined_quantity(s.warehouse_id,s.product_id)"');
    expect(text).toContain('"genealogyInputLot": "JOIN inventory_lots il ON il.id=i.lot_id"');
  });

  test('r11-r38 helpers, locks, count sources/adapters, clock CTE and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'initialPhysicalCountJournalSql',
      'childJsonAggSql', 'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'reservedStockAggSql',
      'activeQualityHoldExistsSql', 'stockMoveSumSql', 'sourceScopeRowsSql', 'scopeExistsSql',
      'packageObligationDocumentRowsSql', 'evidencePositionRowsSql', 'recordIdExistsSql',
      'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql', 'scopedFieldSql',
      'clockScopedPurchaseExistsSql', 'lotBalanceGuardSql', 'stockPositionSourcesSql',
      'inventoryMovementEvidenceRowsSql', 'executivePositionPointSql', 'stockSourceColumnsSql',
      'attachmentOwnerVersionGuardSql', 'attachmentLotIdsSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    expect(shared).toContain('WITH ${alias}_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms)');
    expect(src('count-record-purchases.ts')).toContain('WITH count_source_clock AS MATERIALIZED');
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    expect(src('inventory-read-projections.ts')).toContain(
      'export const inventoryCountStatusSql = "SELECT status FROM inventory_counts WHERE id=? AND company_id=?"',
    );
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    expect(src('transfer-attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('count-attachment-purchases.ts')).toContain('FOR SHARE');
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r38 BEFORE=340 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r38 ended at 340. r39 folds two byte-identical reserved-stock SUM heads
    // into one helper (340 -> 339): two inline SELECT tokens removed, one kept
    // in the helper. Locks, count sources/adapters, clock CTE and native
    // transport are not touched.
    expect(total).toBeLessThanOrEqual(339);
    expect(total).toBeLessThan(340);
  });
});
