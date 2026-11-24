import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { runtimeHealthSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

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
  "SELECT heartbeat_at,details_json FROM runtime_health WHERE service='jobs'",
  "SELECT heartbeat_at FROM runtime_health WHERE service='jobs'",
  'SELECT "service","heartbeat_at","details_json" FROM runtime_health',
];

const callers = [
  ['app-native-sources.ts', 2],
  ['platform-owner-projection-sources.ts', 1],
] as const;

describe('Runtime read-tail r7 runtime_health parameterized family', () => {
  test('canonical helper renders byte-identical SQL for every family member and stays read-only', () => {
    expect(runtimeHealthSql('heartbeat_at,details_json', " WHERE service='jobs'")).toBe(pre[0]);
    expect(runtimeHealthSql('heartbeat_at', " WHERE service='jobs'")).toBe(pre[1]);
    expect(runtimeHealthSql('"service","heartbeat_at","details_json"')).toBe(pre[2]);
    for (const sql of pre) readOnly(sql);
  });

  test('the family is one owner literal, composed by every named adapter, never inlined', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM runtime_health${suffix}`')).toHaveLength(2);
    const family = shared.slice(shared.indexOf('export const runtimeHealthSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    let uses = 0;
    for (const [name, count] of callers) {
      const text = src(name);
      for (const literal of pre) expect(text, `${name} :: ${literal}`).not.toContain(literal);
      expect(text.match(/runtimeHealthSql\(/g) ?? []).toHaveLength(count);
      uses += count;
    }
    expect(uses).toBe(3);
  });

  test('control-plane lock and migration SQL is untouched', () => {
    const migrations = src('runtime/migrations.ts');
    const postgres = src('runtime/postgres.ts');
    expect(migrations).toContain("'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("'SELECT pg_advisory_unlock(714928001)'");
    expect(migrations).not.toContain('runtimeHealthSql');
    expect(postgres).not.toContain('runtimeHealthSql');
  });

  test('r5 holding_bindings and r6 executive_generation_jobs families are not re-deduped', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM holding_bindings${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
  });

  test('retirement:ts-statements stays strictly below the r7 BEFORE=50 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(50);
  });
});
