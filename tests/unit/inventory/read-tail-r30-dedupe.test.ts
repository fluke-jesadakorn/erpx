import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { childLineRowsSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe literals from HEAD for the child-line row reads. Only the
// table and its line-FK column varied; projection, predicate and bind were
// byte-identical. These are byte-identity oracles only, never live query sources.
const sites: [table: string, fk: string][] = [
  ['inventory_transfer_lot_allocations', 'transfer_line_id'],
  ['inventory_transit_lots', 'transit_line_id'],
];
const preDedupe = (table: string, fk: string) => `SELECT * FROM ${table} WHERE ${fk}=? ORDER BY id`;

describe('Inventory read-tail r30 parameterized child-line dedupe', () => {
  test('helper reproduces both pre-dedupe literals byte-identically and stays read-only', () => {
    for (const [table, fk] of sites) {
      const sql = childLineRowsSql(table, fk);
      expect(sql).toBe(preDedupe(table, fk));
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(lockRe);
      expect(selectCount(sql)).toBe(1);
    }
  });

  test('r30 helper literal is defined exactly once in the canonical owner', () => {
    expect(src('read-tail-sql.ts').split('childLineRowsSql =').length - 1).toBe(1);
  });

  test('former inline literals are gone and every call site composes the helper with unchanged binds', () => {
    const transit = src('inventory-transit.ts');
    for (const [table, fk] of sites) {
      expect(transit).not.toContain(`db.prepare('${preDedupe(table, fk)}')`);
      expect(transit).toContain(`db.prepare(childLineRowsSql('${table}','${fk}'))`);
      expect(transit).not.toMatch(lockRe);
    }
    expect((transit.match(/childLineRowsSql\(/g) ?? []).length).toBe(2); // 2 call sites
    expect(transit).toContain('childLineRowsSql,noLaterLedgerMovementSql'); // 1 import
  });

  test('r11-r29 helpers, count source adapters, native registry and lock statements stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql',
      'packageObligationDocumentRowsSql',
      'sourceScopeRowsSql',
      'scopeExistsSql',
      'journalAccountLineExistsSql',
      'childLineOwnerExistsSql',
      'nativeRecordExistsSql',
      'branchScopeTupleSql',
      'pendingLossSumSql',
      'transitLossDecisionPendingSql',
      'evidencePositionRowsSql',
      'recordIdExistsSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    const nativeRead = readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8');
    expect(nativeRead.length).toBeGreaterThan(0);
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r29 BEFORE=358 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r29 deduped the 4x byte-identical id-existence probe into one shared
    // parameterized helper (361 -> 358). r30 folds the 2x byte-identical
    // child-line row selector into one helper (358 -> 357). r31 lowers it
    // further to 353, so the exact pin is relaxed to the r30 baseline.
    expect(total).toBeLessThanOrEqual(357);
    expect(total).toBeLessThan(358);
  });
});
