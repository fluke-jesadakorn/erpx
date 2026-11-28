import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  childJsonAggSql,
  childJoinJsonAggSql,
} from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const adapterDir = resolve(__dirname, '../../../modules/inventory/server/src/adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

const owners = [
  'count-attachment-purchases.ts',
  'landed-cost-purchases.ts',
  'manufacturing-initial-physical-stock-count-source-sql.ts',
  'manufacturing-package-obligations.ts',
  'pos-initial-physical-stock-count-source-sql.ts',
  'sales-package-obligations.ts',
  'service-initial-physical-stock-count-source-sql.ts',
  'supply-package-obligations.ts',
] as const;

describe('Inventory read-tail r15 parameterized-family dedupe', () => {
  test('canonical helpers own the exact child-document SQL and stay read-only', () => {
    expect(childJsonAggSql('inventory_count_lines', 'line', 'line.count_id=c.id'))
      .toBe('(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_count_lines line WHERE line.count_id=c.id)');
    expect(childJoinJsonAggSql('inventory_count_lot_lines', 'lot', 'inventory_count_lines line ON line.id=lot.count_line_id', 'line.count_id=c.id'))
      .toBe('(SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_count_lot_lines lot JOIN inventory_count_lines line ON line.id=lot.count_line_id WHERE line.count_id=c.id)');
    for (const sql of [childJsonAggSql('t', 'a', 'a.id=b.id'), childJoinJsonAggSql('t', 'a', 'u b ON b.id=a.u_id', 'b.id=c.id')]) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  test('each helper literal is defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} WHERE ${where})')).toHaveLength(2);
    expect(shared.split('(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} JOIN ${join} WHERE ${where})')).toHaveLength(2);
  });

  test('every child selector is composed, never inlined, in the named adapters', () => {
    const inline = /\(SELECT jsonb_agg\(to_jsonb\(\w+\) ORDER BY \w+\.id\) FROM \w+ \w+/;
    let calls = 0;
    for (const name of owners) {
      const text = src(name);
      expect(text, name).not.toMatch(inline);
      calls += (text.match(/\$\{child(?:Join)?JsonAggSql\(/g) ?? []).length;
    }
    expect(calls).toBe(37);
  });

  test('retirement:ts-statements stays strictly below the r15 BEFORE=463 baseline', () => {
    let total = 0;
    for (const file of readdirSync(adapterDir).filter((f) => f.endsWith('.ts'))) {
      total += selectCount(readFileSync(resolve(adapterDir, file), 'utf8'));
    }
    expect(total).toBeLessThan(463);
  });
});
