import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { lotBalanceGuardSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe inline guard literals from HEAD. The three lot-balance
// existence guards shared one byte-identical prefix and differed only in a
// trailing predicate. These are byte-identity oracles only, never live sources.
const prefix = 'EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?';
const sites: [predicate: string, before: string][] = [
  [' AND version=? AND quantity_milli=? AND value_minor=?',
    `${prefix} AND version=? AND quantity_milli=? AND value_minor=?)`],
  [' AND quantity_milli>=?',
    `${prefix} AND quantity_milli>=?)`],
  [' AND version=?',
    `${prefix} AND version=?)`],
];

describe('Inventory read-tail r35 parameterized lot-balance guard dedupe', () => {
  test('helper reproduces all three pre-dedupe literals byte-identically and stays read-only', () => {
    for (const [predicate, before] of sites) {
      const sql = lotBalanceGuardSql(predicate);
      expect(sql).toBe(before);
      expect(selectCount(sql)).toBe(selectCount(before));
      expect(sql).not.toMatch(lockRe);
    }
  });

  test('the three pre-dedupe guards share one skeleton after predicate masking', () => {
    const rendered = sites.map(([predicate, before]) => before.replace(predicate, '<PRED>'));
    expect(rendered[0]).toBe(rendered[1]);
    expect(rendered[1]).toBe(rendered[2]);
    expect(rendered[0]).toBe(`${prefix}<PRED>)`);
  });

  test('helper template carries exactly one SELECT token and is defined once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    const helperLine = shared.split('\n').find(l => l.startsWith('export const lotBalanceGuardSql'))!;
    expect(selectCount(helperLine)).toBe(1);
    expect(shared.split('export const lotBalanceGuardSql').length - 1).toBe(1);
  });

  test('former inline literals are gone and both call sites compose the helper with unchanged binds', () => {
    const cf = src('commerce-facts.ts');
    expect(cf).not.toContain(`${prefix} AND version=? AND quantity_milli=? AND value_minor=?)`);
    expect(cf).toContain(`lotBalanceGuardSql(' AND version=? AND quantity_milli=? AND value_minor=?')`);
    expect(cf).toContain('guardRule12:');
    expect(cf).toContain('facts.values');

    const it = src('inventory-transit.ts');
    expect(it).not.toContain(`${prefix} AND quantity_milli>=?)`);
    expect(it).not.toContain(`${prefix} AND version=?)`);
    expect(it).toContain(`lotBalanceGuardSql(' AND quantity_milli>=?')`);
    expect(it).toContain(`lotBalanceGuardSql(' AND version=?')`);
    expect(it).toContain('[lot_id,source_warehouse_id,quantity_milli]');
    expect(it).toContain('[lot_id,target_warehouse_id,version]');
  });

  test('r11-r34 helpers, locks, count sources, count adapters and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'initialPhysicalCountJournalSql',
      'childJsonAggSql', 'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'reservedStockAggSql',
      'activeQualityHoldExistsSql', 'stockMoveSumSql', 'sourceScopeRowsSql', 'scopeExistsSql',
      'packageObligationDocumentRowsSql', 'evidencePositionRowsSql', 'recordIdExistsSql',
      'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql',
      'clockScopedPurchaseExistsSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    expect(src('count-record-purchases.ts')).toContain("clockCteSql('count_source_clock')");
    expect(src('inventory-read-projections.ts')).not.toContain('SELECT status FROM inventory_counts');
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r34 BEFORE=349 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r34 ended at 349. r35 folds the 3x lot-balance existence guard into one
    // helper (349 -> 347): three inline SELECT tokens removed, one kept.
    expect(total).toBeLessThanOrEqual(347);
    expect(total).toBeLessThan(349);
  });
});
