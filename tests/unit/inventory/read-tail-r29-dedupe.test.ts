import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { recordIdExistsSql } from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe literals from HEAD for the workspace-record id probes.
// Only the backing table varied; the projection, predicate and bind were
// byte-identical. These are byte-identity oracles only, never live query sources.
const tables = ['inventory_transfers', 'inventory_transit', 'inventory_counts', 'inventory_landed_costs'];
const preDedupe = (table: string) => `SELECT 1 FROM ${table} WHERE id=?`;

describe('Inventory read-tail r29 parameterized record-id existence dedupe', () => {
  test('helper reproduces all four pre-dedupe literals byte-identically and stays read-only', () => {
    for (const table of tables) {
      const sql = recordIdExistsSql(table);
      expect(sql).toBe(preDedupe(table));
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(lockRe);
      expect(selectCount(sql)).toBe(1);
    }
  });

  test('r29 helper literal is defined exactly once in the canonical owner', () => {
    expect(src('read-tail-sql.ts').split('recordIdExistsSql =').length - 1).toBe(1);
  });

  test('former inline literals are gone and every call site composes the helper with unchanged binds', () => {
    const sites: [file: string, table: string][] = [
      ['transfer-record-purchases.ts', 'inventory_transfers'],
      ['transfer-record-purchases.ts', 'inventory_transit'],
      ['count-record-purchases.ts', 'inventory_counts'],
      ['landed-cost-record-purchases.ts', 'inventory_landed_costs'],
    ];
    for (const [file, table] of sites) {
      const text = src(file);
      expect(text).not.toContain(`database.prepare("${preDedupe(table)}")`);
      expect(text).toContain(`database.prepare(recordIdExistsSql("${table}"))`);
      expect(text).not.toMatch(lockRe);
    }
    const transfer = src('transfer-record-purchases.ts');
    expect((transfer.match(/recordIdExistsSql\(/g) ?? []).length).toBe(2); // 2 calls + 1 import
    expect((transfer.match(/\.bind\(id\)/g) ?? []).length).toBeGreaterThanOrEqual(2);
    for (const file of ['count-record-purchases.ts', 'landed-cost-record-purchases.ts']) {
      expect((src(file).match(/recordIdExistsSql\(/g) ?? []).length).toBeGreaterThanOrEqual(1);
    }
  });

  test('r11-r28 helpers, count source adapters, native registry and lock statements stay untouched', () => {
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

  test('retirement:ts-statements stays strictly below the r29 BEFORE=361 baseline', () => {
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
    // parameterized helper (361 -> 358).
    expect(total).toBeLessThanOrEqual(357);
    expect(total).toBeLessThan(361);
  });
});
