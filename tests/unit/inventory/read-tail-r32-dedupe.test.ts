import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { ecommerceQualitySettlementSources } from '@erpx/server-quality/public/ecommerce-package-sources';
import { salesQualitySettlementSources } from '@erpx/server-quality/public/sales-package-sources';
import { disposalCountsSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe quality-disposal count sources from HEAD. Both settled
// reads shared one byte-identical projection/join; only the quality hold source
// rows and the trailing source-type predicate varied. These are byte-identity
// oracles only, never live query sources.
const salesTail = " WHERE hold.source_type='customer_return'";
const preDedupe = (holds: string, tail = '') =>
  `SELECT count.*,hold.source_type quality_source_type,hold.source_id quality_source_id FROM inventory_counts count JOIN (${holds}) hold ON hold.id=count.quality_hold_id${tail}`;

describe('Inventory read-tail r32 parameterized disposal-count dedupe', () => {
  test('helper reproduces both pre-dedupe literals byte-identically and stays read-only', () => {
    for (const [holds, tail] of [
      [salesQualitySettlementSources.holds, salesTail],
      [ecommerceQualitySettlementSources.returnHoldSources, ''],
    ]) {
      const sql = disposalCountsSql(holds, tail);
      expect(sql).toBe(preDedupe(holds, tail));
      expect(sql).toContain('SELECT ');
      expect(selectCount(sql)).toBe(selectCount(preDedupe(holds, tail)));
      expect(sql).not.toMatch(lockRe);
    }
  });

  test('r32 helper literal is defined exactly once in the canonical owner', () => {
    expect(src('read-tail-sql.ts').split('disposalCountsSql =').length - 1).toBe(1);
  });

  test('former inline literals are gone and both call sites compose the helper with unchanged sources', () => {
    const sales = src('sales-package-obligations.ts');
    expect(sales).not.toContain('SELECT count.*,hold.source_type');
    expect(sales).toContain(`disposalCountsSql(baseQuality.holds, "${salesTail}")`);
    expect((sales.match(/disposalCountsSql\(/g) ?? []).length).toBe(1);

    const ecommerce = src('ecommerce-package-obligations.ts');
    expect(ecommerce).not.toContain('SELECT count.*,hold.source_type');
    expect(ecommerce).toContain('disposalCountsSql(ecommerceQualitySettlementSources.returnHoldSources)');
    expect((ecommerce.match(/disposalCountsSql\(/g) ?? []).length).toBe(1);
  });

  test('r11-r31 helpers, locks, count source adapters and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'initialPhysicalCountJournalSql',
      'childJsonAggSql', 'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'reservedStockAggSql',
      'activeQualityHoldExistsSql', 'stockMoveSumSql', 'sourceScopeRowsSql', 'scopeExistsSql',
      'packageObligationDocumentRowsSql', 'evidencePositionRowsSql', 'recordIdExistsSql',
      'childLineRowsSql', 'scopeAllowedSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    // Lock guards remain in their owning gates; the dedupe only touches the source literal.
    expect(src('sales-package-obligations.ts')).toContain('LOCK TABLE inventory_order_reservations');
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r31 BEFORE=353 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r31 folded the 3x parameterized scope probe into one helper (357 -> 353).
    // r32 folds the 2x parameterized quality-disposal count source into one
    // helper (353 -> 352): two inline SELECT tokens removed, one kept in the
    // single canonical helper. r33 lowers it further to 351, so the exact pin
    // is relaxed to the r32 baseline.
    expect(total).toBeLessThanOrEqual(352);
    expect(total).toBeLessThan(353);
  });
});
