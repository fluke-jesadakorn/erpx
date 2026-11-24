import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { executiveExternalTokensSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const tokenColumns = "id,name,user_id,holding_id,company_id,assignment_id,assignment_version,role,assignment_scope_kind,assignment_scope_node_id,branch_id,department_id,scope_label,created_at,expires_at,revoked_at,last_used_at";

// Pre-dedupe literals copied from HEAD, used only as byte-identity oracles.
const pre = [
  `SELECT ${tokenColumns} FROM executive_external_tokens WHERE secret_hash=?`,
  `SELECT ${tokenColumns} FROM executive_external_tokens WHERE id=? AND revoked_at IS NULL AND expires_at>?`,
];

const callers = [
  ['executive/connections.ts', 2],
] as const;

describe('Runtime read-tail r8 executive_external_tokens parameterized family', () => {
  test('canonical helper renders byte-identical SQL for every family member and stays read-only', () => {
    expect(executiveExternalTokensSql(tokenColumns, ' WHERE secret_hash=?')).toBe(pre[0]);
    expect(executiveExternalTokensSql(tokenColumns, ' WHERE id=? AND revoked_at IS NULL AND expires_at>?')).toBe(pre[1]);
    for (const sql of pre) readOnly(sql);
  });

  test('the family is one owner literal, composed by every named adapter, never inlined', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM executive_external_tokens${suffix}`')).toHaveLength(2);
    const family = shared.slice(shared.indexOf('export const executiveExternalTokensSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    let uses = 0;
    for (const [name, count] of callers) {
      const text = src(name);
      for (const literal of pre) expect(text, `${name} :: ${literal}`).not.toContain(literal);
      expect(text.match(/executiveExternalTokensSql\(/g) ?? []).toHaveLength(count);
      uses += count;
    }
    expect(uses).toBe(2);
  });

  test('control-plane lock and migration SQL is untouched', () => {
    const migrations = src('runtime/migrations.ts');
    const postgres = src('runtime/postgres.ts');
    expect(migrations).toContain("'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("'SELECT pg_advisory_unlock(714928001)'");
    expect(migrations).not.toContain('executiveExternalTokensSql');
    expect(postgres).not.toContain('executiveExternalTokensSql');
  });

  test('r5 holding_bindings, r6 executive_generation_jobs and r7 runtime_health families are not re-deduped', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM holding_bindings${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM runtime_health${suffix}`')).toHaveLength(2);
  });

  test('retirement:ts-statements stays strictly below the r8 BEFORE=48 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(48);
  });
});
