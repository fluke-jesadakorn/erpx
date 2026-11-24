import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { childLineOwnerExistsSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const expected = (table: string, alias: string, childFk: string, parentFk: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=child.${childFk} AND ${alias}.${parentFk}=d.id)`;

const cases: Array<[string, string, string, string]> = [
  ['inventory_count_lines', 'l', 'count_line_id', 'count_id'],
  ['inventory_landed_cost_lines', 'l', 'landed_cost_line_id', 'landed_cost_id'],
  ['inventory_transit_lines', 'l', 'transit_line_id', 'transit_id'],
  ['inventory_transit_events', 'e', 'event_id', 'transit_id'],
  ['inventory_transfer_lines', 'l', 'transfer_line_id', 'transfer_id'],
];

describe('Inventory read-tail r23 parameterized child-line EXISTS-family dedupe', () => {
  test('canonical helper renders byte-identical read-only SQL for every consumer', () => {
    for (const [table, alias, childFk, parentFk] of cases) {
      const sql = childLineOwnerExistsSql(table, alias, childFk, parentFk);
      expect(sql).toBe(expected(table, alias, childFk, parentFk));
      readOnly(sql);
    }
  });

  test('r23 helper literal is defined exactly once in the canonical owner', () => {
    expect(src('read-tail-sql.ts').split('childLineOwnerExistsSql =')).toHaveLength(2);
  });

  test('child-line ownership guards are composed, never inlined, in the attachment read source', () => {
    const text = src('attachment-read-sources.ts');
    expect(text).not.toContain("EXISTS(SELECT 1 FROM inventory_count_lines l WHERE l.id=child.count_line_id");
    expect(text).not.toContain("EXISTS(SELECT 1 FROM inventory_landed_cost_lines l WHERE l.id=child.landed_cost_line_id");
    expect(text).not.toContain("EXISTS(SELECT 1 FROM inventory_transit_lines l WHERE l.id=child.transit_line_id");
    expect(text).not.toContain("EXISTS(SELECT 1 FROM inventory_transit_events e WHERE e.id=child.event_id");
    expect(text).not.toContain("EXISTS(SELECT 1 FROM inventory_transfer_lines l WHERE l.id=child.transfer_line_id");
    expect(text.match(/childLineOwnerExistsSql\(/g) ?? []).toHaveLength(5);
  });

  test('retirement:ts-statements stays strictly below the r23 BEFORE=380 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(380);
  });
});
