import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { holdingBindingsSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe literals copied from HEAD, used only as byte-identity oracles.
const pre = [
  'SELECT holding_id, slug, display_name FROM holding_bindings',
  "SELECT holding_id, slug, display_name FROM holding_bindings WHERE holding_id=? AND status='active'",
  'SELECT "holding_id","slug","display_name","write_binding","read_binding","files_binding","status" FROM holding_bindings',
  'SELECT 1 FROM holding_bindings WHERE holding_id=$1',
  'SELECT slug FROM holding_bindings WHERE holding_id=$1',
  'SELECT slug FROM holding_bindings WHERE holding_id=?',
  'SELECT holding_id,status FROM holding_bindings WHERE slug=?',
  'SELECT 1 FROM holding_bindings WHERE slug=?',
  "SELECT 1 FROM holding_bindings WHERE holding_id=$1 AND holding_id<>'h003'",
  "SELECT slug FROM holding_bindings WHERE holding_id=? AND status='active'",
];

const callers = [
  ['runtime.ts', 1],
  ['runtime/jobs.ts', 1],
  ['platform-owner-projection-sources.ts', 1],
  ['billing-holding-facts.ts', 2],
  ['pos-stripe-binding-facts.ts', 1],
  ['identity-management-binding-facts.ts', 2],
  ['app-native-sources.ts', 1],
  ['executive/connections.ts', 1],
] as const;

describe('Runtime read-tail r5 holding_bindings parameterized family', () => {
  test('canonical helper renders byte-identical SQL for every family member and stays read-only', () => {
    expect(holdingBindingsSql('holding_id, slug, display_name')).toBe(pre[0]);
    expect(holdingBindingsSql('holding_id, slug, display_name', " WHERE holding_id=? AND status='active'")).toBe(pre[1]);
    expect(holdingBindingsSql('"holding_id","slug","display_name","write_binding","read_binding","files_binding","status"')).toBe(pre[2]);
    expect(holdingBindingsSql('1', ' WHERE holding_id=$1')).toBe(pre[3]);
    expect(holdingBindingsSql('slug', ' WHERE holding_id=$1')).toBe(pre[4]);
    expect(holdingBindingsSql('slug', ' WHERE holding_id=?')).toBe(pre[5]);
    expect(holdingBindingsSql('holding_id,status', ' WHERE slug=?')).toBe(pre[6]);
    expect(holdingBindingsSql('1', ' WHERE slug=?')).toBe(pre[7]);
    expect(holdingBindingsSql('1', " WHERE holding_id=$1 AND holding_id<>'h003'")).toBe(pre[8]);
    expect(holdingBindingsSql('slug', " WHERE holding_id=? AND status='active'")).toBe(pre[9]);
    for (const sql of pre) readOnly(sql);
  });

  test('the family is one owner literal, composed by every named adapter, never inlined', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM holding_bindings${suffix}`')).toHaveLength(2);
    const family = shared.slice(shared.indexOf('export const holdingBindingsSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    let uses = 0;
    for (const [name, count] of callers) {
      const text = src(name);
      for (const literal of pre) expect(text, `${name} :: ${literal}`).not.toContain(literal);
      expect(text.match(/holdingBindingsSql\(/g) ?? []).toHaveLength(count);
      uses += count;
    }
    expect(uses).toBe(10);
  });

  test('retirement:ts-statements stays strictly below the r5 BEFORE=62 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(62);
  });
});
