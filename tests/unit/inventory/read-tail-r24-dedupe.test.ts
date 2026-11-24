import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { branchScopeTupleSql, nativeRecordExistsSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};
const noWrite = (sql: string) => {
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const nativeCases: Array<[string, string, string]> = [
  ['inventory_counts', 'native_count', 'records.id'],
  ['inventory_landed_costs', 'actual_landed_cost', 'records.id'],
  ['inventory_transfers', 'native_transfer', 'records.id'],
  ['inventory_transit', 'native_transit', 'records.id'],
];
const branchAliases = ['count_source', 'source', 'landed_source', 'transfer_source'];

describe('Inventory read-tail r24 native-record EXISTS / branch-scope tuple dedupe', () => {
  test('canonical helpers render byte-identical read-only SQL for every consumer', () => {
    for (const [table, alias, id] of nativeCases) {
      expect(nativeRecordExistsSql(table, alias, id)).toBe(
        `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=${id})`);
      readOnly(nativeRecordExistsSql(table, alias, id));
    }
    for (const alias of branchAliases) {
      expect(branchScopeTupleSql(alias)).toBe(`(?::text IS NULL OR ${alias}.branch_id=?)`);
      noWrite(branchScopeTupleSql(alias));
    }
  });

  test('r24 helper literals are defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('nativeRecordExistsSql =')).toHaveLength(2);
    expect(shared.split('branchScopeTupleSql =')).toHaveLength(2);
  });

  test('native-record EXISTS guards are composed, never inlined, in the three record-purchase adapters', () => {
    const count = src('count-record-purchases.ts');
    const landed = src('landed-cost-record-purchases.ts');
    const transfer = src('transfer-record-purchases.ts');
    expect(count).not.toContain('EXISTS(SELECT 1 FROM inventory_counts native_count WHERE');
    expect(landed).not.toContain('EXISTS(SELECT 1 FROM inventory_landed_costs actual_landed_cost WHERE');
    expect(transfer).not.toContain('EXISTS(SELECT 1 FROM inventory_transfers native_transfer WHERE');
    expect(transfer).not.toContain('EXISTS(SELECT 1 FROM inventory_transit native_transit WHERE');
    expect(count.match(/nativeRecordExistsSql\(/g) ?? []).toHaveLength(1);
    expect(landed.match(/nativeRecordExistsSql\(/g) ?? []).toHaveLength(1);
    expect(transfer.match(/nativeRecordExistsSql\(/g) ?? []).toHaveLength(2);
  });

  test('branch-scope tuples are composed, never inlined, in the three record-purchase adapters', () => {
    const count = src('count-record-purchases.ts');
    const landed = src('landed-cost-record-purchases.ts');
    const transfer = src('transfer-record-purchases.ts');
    expect(count).not.toContain('(?::text IS NULL OR count_source.branch_id=?)');
    expect(count).not.toContain('(?::text IS NULL OR source.branch_id=?)');
    expect(landed).not.toContain('(?::text IS NULL OR landed_source.branch_id=?)');
    expect(transfer).not.toContain('(?::text IS NULL OR transfer_source.branch_id=?)');
    expect(count.match(/branchScopeTupleSql\(/g) ?? []).toHaveLength(2);
    expect(landed.match(/branchScopeTupleSql\(/g) ?? []).toHaveLength(1);
    expect(transfer.match(/branchScopeTupleSql\(/g) ?? []).toHaveLength(1);
  });

  test('retirement:ts-statements stays strictly below the r24 BEFORE=376 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(376);
  });
});
