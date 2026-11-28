import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { executiveReportsSql } from '../../../platform/runtime/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../platform/runtime/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe literals copied from HEAD, used only as byte-identity oracles.
const pre = [
  'SELECT * FROM executive_reports WHERE id=? AND company_id=?',
  'SELECT r.*,s.definition_json shared_definition FROM executive_reports r LEFT JOIN executive_report_shares s ON s.report_id=r.id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?) WHERE r.company_id=? AND ((r.owner_id=? AND ?=1) OR s.id IS NOT NULL) ORDER BY r.updated_at DESC LIMIT 200',
];

const callers = [
  ['executive/reports.ts', 3],
] as const;

describe('Runtime read-tail r11 executive_reports parameterized family', () => {
  test('canonical helper renders byte-identical SQL for every family member and stays read-only', () => {
    expect(executiveReportsSql('*', ' WHERE id=? AND company_id=?')).toBe(pre[0]);
    expect(executiveReportsSql('r.*,s.definition_json shared_definition', ' r LEFT JOIN executive_report_shares s ON s.report_id=r.id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?) WHERE r.company_id=? AND ((r.owner_id=? AND ?=1) OR s.id IS NOT NULL) ORDER BY r.updated_at DESC LIMIT 200')).toBe(pre[1]);
    for (const sql of pre) readOnly(sql);
  });

  test('the family is one owner literal, composed by every named adapter, never inlined', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM executive_reports${suffix}`')).toHaveLength(2);
    const family = shared.slice(shared.indexOf('export const executiveReportsSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    let uses = 0;
    for (const [name, count] of callers) {
      const text = src(name);
      for (const literal of pre) expect(text, `${name} :: ${literal}`).not.toContain(literal);
      expect(text.match(/executiveReportsSql\(/g) ?? []).toHaveLength(count);
      uses += count;
    }
    expect(uses).toBe(3);
  });

  test('control-plane lock and migration SQL is untouched', () => {
    const reports = src('executive/reports.ts');
    const migrations = src('runtime/migrations.ts');
    const postgres = src('runtime/postgres.ts');
    // Report lock / version guard are control-plane statements, not family members.
    expect(reports).toContain('const REPORT_LOCK_SQL = "SELECT id FROM executive_reports WHERE id=? FOR UPDATE"');
    expect(reports).toContain('const REPORT_VERSION_GUARD_SQL = "EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)"');
    expect(migrations).toContain("'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("'SELECT pg_advisory_unlock(714928001)'");
    expect(migrations).not.toContain('executiveReportsSql');
    expect(postgres).not.toContain('executiveReportsSql');
  });

  test('r5-r10 families are not re-deduped', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM holding_bindings${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM runtime_health${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_external_tokens${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_report_shares${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_report_revisions WHERE report_id=?${suffix}`')).toHaveLength(2);
  });

  test('retirement:ts-statements stays strictly below the r11 BEFORE=43 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(43);
  });
});
