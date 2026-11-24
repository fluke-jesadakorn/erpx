import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { executiveReportsSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe literal copied from HEAD, used only as a byte-identity oracle.
// It is a multiline executive_reports read whose nested share EXISTS predicate
// carries the r9 share-literal shape and alias; the whole read is rebuilt from
// the existing canonical executive_reports builder, so the nested literal is
// preserved verbatim rather than re-deduped into the r9 family.
const pre = [
  'SELECT r.id FROM executive_reports r WHERE r.company_id=? AND r.id>?\n      AND (r.owner_id=? OR EXISTS(SELECT 1 FROM executive_report_shares s WHERE s.report_id=r.id AND s.company_id=r.company_id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?)))\n      ORDER BY r.id LIMIT ?',
];
const suffix = ' r WHERE r.company_id=? AND r.id>?\n      AND (r.owner_id=? OR EXISTS(SELECT 1 FROM executive_report_shares s WHERE s.report_id=r.id AND s.company_id=r.company_id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?)))\n      ORDER BY r.id LIMIT ?';
const nestedShare = 'EXISTS(SELECT 1 FROM executive_report_shares s WHERE s.report_id=r.id AND s.company_id=r.company_id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?))';

const callers = [
  ['executive/mcp.ts', 1],
] as const;

describe('Runtime read-tail r12 executive_reports mcp dashboards read', () => {
  test('canonical helper renders byte-identical SQL and stays read-only', () => {
    expect(executiveReportsSql('r.id', suffix)).toBe(pre[0]);
    for (const sql of pre) readOnly(sql);
    expect(pre[0]).toContain(nestedShare);
  });

  test('the family is one owner literal, composed by every named adapter, never inlined', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM executive_reports${suffix}`')).toHaveLength(2);
    const family = shared.slice(shared.indexOf('export const executiveReportsSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    let uses = 0;
    for (const [name, count] of callers) {
      const text = src(name);
      expect(text, `${name} :: pre literal`).not.toContain(pre[0]);
      expect(text).toContain(nestedShare);
      expect(text.match(/executiveReportsSql\(/g) ?? []).toHaveLength(count);
      uses += count;
    }
    expect(uses).toBe(1);
  });

  test('control-plane lock and migration SQL is untouched', () => {
    const reports = src('executive/reports.ts');
    const migrations = src('runtime/migrations.ts');
    const postgres = src('runtime/postgres.ts');
    expect(reports).toContain('const REPORT_LOCK_SQL = "SELECT id FROM executive_reports WHERE id=? FOR UPDATE"');
    expect(reports).toContain('const REPORT_VERSION_GUARD_SQL = "EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)"');
    expect(migrations).toContain("'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("'SELECT pg_advisory_unlock(714928001)'");
    expect(postgres).not.toContain('executiveReportsSql');
  });

  test('r5-r11 families are not re-deduped', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM holding_bindings${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM runtime_health${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_external_tokens${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_report_shares${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_report_revisions WHERE report_id=?${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_reports${suffix}`')).toHaveLength(2);
  });

  test('retirement:ts-statements stays strictly below the r12 BEFORE=42 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(42);
  });
});
