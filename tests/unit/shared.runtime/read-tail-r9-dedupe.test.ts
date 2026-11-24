import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { executiveReportSharesSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

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
  'SELECT * FROM executive_report_shares WHERE report_id=? AND company_id=? AND recipient_id=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?)',
  'SELECT * FROM executive_report_shares WHERE report_id=? AND revoked_at IS NULL ORDER BY created_at',
  'SELECT id,version FROM executive_report_shares WHERE report_id=? AND revoked_at IS NULL ORDER BY id',
  'SELECT 1 FROM executive_report_shares WHERE company_id=? AND recipient_id=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?) LIMIT 1',
];

const callers = [
  ['executive/reports.ts', 3],
  ['executive/access.ts', 1],
] as const;

describe('Runtime read-tail r9 executive_report_shares parameterized family', () => {
  test('canonical helper renders byte-identical SQL for every family member and stays read-only', () => {
    expect(executiveReportSharesSql('*', ' WHERE report_id=? AND company_id=? AND recipient_id=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?)')).toBe(pre[0]);
    expect(executiveReportSharesSql('*', ' WHERE report_id=? AND revoked_at IS NULL ORDER BY created_at')).toBe(pre[1]);
    expect(executiveReportSharesSql('id,version', ' WHERE report_id=? AND revoked_at IS NULL ORDER BY id')).toBe(pre[2]);
    expect(executiveReportSharesSql('1', ' WHERE company_id=? AND recipient_id=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?) LIMIT 1')).toBe(pre[3]);
    for (const sql of pre) readOnly(sql);
  });

  test('the family is one owner literal, composed by every named adapter, never inlined', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM executive_report_shares${suffix}`')).toHaveLength(2);
    const family = shared.slice(shared.indexOf('export const executiveReportSharesSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    let uses = 0;
    for (const [name, count] of callers) {
      const text = src(name);
      for (const literal of pre) expect(text, `${name} :: ${literal}`).not.toContain(literal);
      expect(text.match(/executiveReportSharesSql\(/g) ?? []).toHaveLength(count);
      uses += count;
    }
    expect(uses).toBe(4);
  });

  test('control-plane lock and migration SQL is untouched', () => {
    const migrations = src('runtime/migrations.ts');
    const postgres = src('runtime/postgres.ts');
    expect(migrations).toContain("'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("'SELECT pg_advisory_unlock(714928001)'");
    expect(migrations).not.toContain('executiveReportSharesSql');
    expect(postgres).not.toContain('executiveReportSharesSql');
  });

  test('r5-r8 families are not re-deduped', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM holding_bindings${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM runtime_health${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_external_tokens${suffix}`')).toHaveLength(2);
  });

  test('retirement:ts-statements stays strictly below the r9 BEFORE=47 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(47);
  });
});
