import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { assignmentReadSql } from '@erpx/server-identity_access/public/record-source-authority';
import { packageClockSql, purchasedCapabilityPredicate } from '@erpx/server-commercial/public/purchased-capability-sql';
import { branchScopeTupleSql, clockScopedPurchaseExistsSql, sourceScopeRowsSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe `visible` literals from HEAD. The transfer and
// landed-cost record-purchase guards shared one byte-identical clock CTE,
// two-key scope relation, branch/assignment authority and capability tail;
// only the source rows, row alias, permission and capability predicate varied.
// These are byte-identity oracles only, never live query sources.
const transferCapability = purchasedCapabilityPredicate('transfer_source.company_id', 'supply.transfers', 'history', 'transfer_source_clock.now_ms');
const landedCapability = ['supply.advanced-inventory', 'supply.receiving', 'supply.stock']
  .map(capability => purchasedCapabilityPredicate('landed_source.company_id', capability, 'history', 'landed_source_clock.now_ms'))
  .join(' AND ');
const sites: [alias: string, rows: string, permission: string, capability: string][] = [
  ['transfer_source', '<TRANSFER_SOURCES>', 'inventory.read', transferCapability],
  ['landed_source', sourceScopeRowsSql('<LANDED_SOURCE_ROWS>', 's'), 'finance.read', landedCapability],
];
const preDedupe = (alias: string, rows: string, permission: string, capability: string) =>
  `EXISTS(WITH ${alias}_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM (${rows}) ${alias} CROSS JOIN ${alias}_clock WHERE ${alias}.id=id AND ${alias}.company_id=company AND ${branchScopeTupleSql(alias)} AND ${assignmentReadSql(alias, permission)} AND ${capability})`;

describe('Inventory read-tail r34 parameterized clock-scoped purchase dedupe', () => {
  test('helper reproduces both pre-dedupe literals byte-identically and stays read-only', () => {
    for (const [alias, rows, permission, capability] of sites) {
      const sql = clockScopedPurchaseExistsSql(alias, rows, 'id', 'company', permission, capability);
      expect(sql).toBe(preDedupe(alias, rows, permission, capability));
      expect(selectCount(sql)).toBe(selectCount(preDedupe(alias, rows, permission, capability)));
      // The helper template itself carries exactly two SELECT tokens; the rest
      // come from the shared clock/assignment SQL constants at both sites.
      expect(selectCount(src('read-tail-sql.ts').split('clockScopedPurchaseExistsSql =')[1].split('\n')[1])).toBe(2);
      expect(sql).not.toMatch(lockRe);
    }
  });

  test('the two pre-dedupe envelopes share one skeleton after parameter masking', () => {
    const mask = (s: string, alias: string, rows: string, permission: string, capability: string) => s
      .split(capability).join('<CAP>')
      .split(rows).join('<ROWS>')
      .split(permission).join('<PERM>')
      .split(alias).join('<ALIAS>');
    const rendered = sites.map(([alias, rows, permission, capability]) =>
      mask(clockScopedPurchaseExistsSql(alias, rows, 'id', 'company', permission, capability), alias, rows, permission, capability));
    expect(rendered[0]).toBe(rendered[1]);
  });

  test('r34 helper literal is defined exactly once in the canonical owner', () => {
    expect(src('read-tail-sql.ts').split('clockScopedPurchaseExistsSql =').length - 1).toBe(1);
  });

  test('former inline literals are gone and both call sites compose the helper with unchanged capability and binds', () => {
    for (const [file, alias, resource] of [
      ['transfer-record-purchases.ts', 'transfer_source', 'sources'],
      ['landed-cost-record-purchases.ts', 'landed_source', "sourceScopeRowsSql(landedCostPurchaseSourceRows,'s')"],
    ] as const) {
      const text = src(file);
      expect(text).not.toContain(`EXISTS(WITH ${alias}_clock AS MATERIALIZED`);
      expect(text).not.toContain('assignmentReadSql');
      expect(text).not.toContain('packageClockSql');
      expect(text).toContain(`clockScopedPurchaseExistsSql('${alias}',${resource},id,company,`);
      expect(text).toContain('_clock.now_ms")');
      expect(text).toContain('args:[actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role]');
    }
  });

  test('r11-r33 helpers, locks, count sources, count adapters and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'initialPhysicalCountJournalSql',
      'childJsonAggSql', 'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'reservedStockAggSql',
      'activeQualityHoldExistsSql', 'stockMoveSumSql', 'sourceScopeRowsSql', 'scopeExistsSql',
      'packageObligationDocumentRowsSql', 'evidencePositionRowsSql', 'recordIdExistsSql',
      'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql', 'scopedFieldSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    // The count-record-purchases count source is deliberately not folded in.
    expect(src('count-record-purchases.ts')).toContain("clockCteSql('count_source_clock')");
    expect(src('inventory-read-projections.ts')).not.toContain('SELECT status FROM inventory_counts');
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r33 BEFORE=351 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r33 folded the 2x single-field scoped row probe into one helper (352 -> 351).
    // r34 folds the 2x clock-scoped purchase existence envelope into one helper
    // (351 -> 349): four inline SELECT tokens removed, two kept in the helper.
    expect(total).toBeLessThanOrEqual(349);
    expect(total).toBeLessThan(351);
  });
});
