import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { executiveGenerationJobsSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe literals copied from HEAD at BEFORE=41, used only as byte-identity
// oracles. All five compose the existing executiveGenerationJobsSql builder.
const pre = [
  'SELECT * FROM executive_generation_jobs WHERE report_id=? AND user_id=? AND assignment_id=? AND scope_key=? ORDER BY created_at DESC LIMIT 30',
  'SELECT * FROM executive_generation_jobs WHERE id=?',
  '(SELECT count(*) FROM executive_generation_jobs WHERE company_id=? AND created_at>?)<10',
  "(SELECT count(*) FROM executive_generation_jobs WHERE company_id=? AND state IN ('queued','running'))<3",
  "EXISTS(SELECT 1 FROM executive_generation_jobs WHERE id=? AND state='running' AND lease_token=? AND lease_until>?)",
];
const columns = ['*', '*', 'count(*)', 'count(*)', '1'];
const suffixes = [
  ' WHERE report_id=? AND user_id=? AND assignment_id=? AND scope_key=? ORDER BY created_at DESC LIMIT 30',
  ' WHERE id=?',
  ' WHERE company_id=? AND created_at>?',
  " WHERE company_id=? AND state IN ('queued','running')",
  " WHERE id=? AND state='running' AND lease_token=? AND lease_until>?",
];
const wrap = [
  (s: string) => s,
  (s: string) => s,
  (s: string) => `(${s})<10`,
  (s: string) => `(${s})<3`,
  (s: string) => `EXISTS(${s})`,
];
const rendered = pre.map((_, i) => wrap[i](executiveGenerationJobsSql(columns[i], suffixes[i])));

const callers = [
  ['executive/generation.ts', 5],
] as const;

describe('Runtime read-tail r14 executive_generation_jobs generation.ts five reads', () => {
  test('canonical helper renders byte-identical SQL for every family member and stays read-only', () => {
    for (let i = 0; i < pre.length; i++) expect(rendered[i]).toBe(pre[i]);
    for (const sql of rendered) readOnly(sql);
  });

  test('the family is composed by the named adapter, never inlined', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
    const family = shared.slice(shared.indexOf('export const executiveGenerationJobsSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    let uses = 0;
    for (const [name, count] of callers) {
      const text = src(name);
      for (const literal of pre) expect(text, `${name} :: ${literal}`).not.toContain(literal);
      expect(text.match(/executiveGenerationJobsSql\(/g) ?? []).toHaveLength(count);
      uses += count;
    }
    expect(uses).toBe(5);
  });

  test('control-plane lock and migration SQL is untouched', () => {
    const migrations = src('runtime/migrations.ts');
    const postgres = src('runtime/postgres.ts');
    expect(migrations).toContain('planRuntimeControlPlane(RD_MIGRATION_LOCK)');
    expect(migrations).toContain('planRuntimeControlPlane(RD_MIGRATION_UNLOCK)');
    expect(migrations).not.toContain('executiveGenerationJobsSql');
    expect(postgres).not.toContain('executiveGenerationJobsSql');
  });

  test('r1-r13 helpers are not re-deduped', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM holding_bindings${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM runtime_health${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_external_tokens${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_report_shares${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_report_revisions WHERE report_id=?${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_reports${suffix}`')).toHaveLength(2);
  });

  test('retirement:ts-statements stays strictly below the r14 AFTER=36 pin', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBe(13);
    expect(total).toBeLessThan(36);
  });
});
