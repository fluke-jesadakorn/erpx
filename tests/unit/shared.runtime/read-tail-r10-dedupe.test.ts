import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { executiveReportRevisionsSql } from '../../../platform/runtime/server/src/adapters/read-tail-sql';

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
  'SELECT version,created_at FROM executive_report_revisions WHERE report_id=? ORDER BY version DESC LIMIT 30',
  'SELECT definition_json,filters_json FROM executive_report_revisions WHERE report_id=? AND version=?',
];

const callers = [
  ['executive/reports.ts', 2],
] as const;

describe('Runtime read-tail r10 executive_report_revisions parameterized family', () => {
  test('canonical helper renders byte-identical SQL for every family member and stays read-only', () => {
    expect(executiveReportRevisionsSql('version,created_at', ' ORDER BY version DESC LIMIT 30')).toBe(pre[0]);
    expect(executiveReportRevisionsSql('definition_json,filters_json', ' AND version=?')).toBe(pre[1]);
    for (const sql of pre) readOnly(sql);
  });

  test('the family is one owner literal, composed by every named adapter, never inlined', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM executive_report_revisions WHERE report_id=?${suffix}`')).toHaveLength(2);
    const family = shared.slice(shared.indexOf('export const executiveReportRevisionsSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    let uses = 0;
    for (const [name, count] of callers) {
      const text = src(name);
      for (const literal of pre) expect(text, `${name} :: ${literal}`).not.toContain(literal);
      expect(text.match(/executiveReportRevisionsSql\(/g) ?? []).toHaveLength(count);
      uses += count;
    }
    expect(uses).toBe(2);
  });

  test('control-plane lock and migration SQL is untouched', () => {
    const migrations = src('runtime/migrations.ts');
    const postgres = src('runtime/postgres.ts');
    expect(migrations).toContain("'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("'SELECT pg_advisory_unlock(714928001)'");
    expect(migrations).not.toContain('executiveReportRevisionsSql');
    expect(postgres).not.toContain('executiveReportRevisionsSql');
  });

  test('r5-r9 families are not re-deduped', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM holding_bindings${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM runtime_health${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_external_tokens${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_report_shares${suffix}`')).toHaveLength(2);
  });

  test('retirement:ts-statements stays strictly below the r10 BEFORE=44 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(44);
  });
});
