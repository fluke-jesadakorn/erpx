import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { executiveReportSharesSql, executiveReportsSql, holdingBindingsSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe literals copied from HEAD at BEFORE=36, used only as byte-identity
// oracles. All five compose the existing executiveReportsSql /
// executiveReportSharesSql / holdingBindingsSql builders. The jobs literal had a
// newline + indentation before FROM; its oracle is the whitespace-normalized SQL
// the builder renders (the predicate and projection are unchanged).
const pre = [
  'EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND version=?)',
  'EXISTS(SELECT 1 FROM executive_report_shares WHERE id=? AND version=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?))',
  "COALESCE((SELECT jsonb_agg(jsonb_build_object('id',id,'version',version) ORDER BY id) FROM executive_report_shares WHERE report_id=? AND revoked_at IS NULL),'[]'::jsonb)=?::jsonb",
];
const rendered = [
  `EXISTS(${executiveReportsSql('1', ' WHERE id=? AND version=?')})`,
  `EXISTS(${executiveReportSharesSql('1', ' WHERE id=? AND version=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?)')})`,
  `COALESCE((${executiveReportSharesSql("jsonb_agg(jsonb_build_object('id',id,'version',version) ORDER BY id)", ' WHERE report_id=? AND revoked_at IS NULL')}),'[]'::jsonb)=?::jsonb`,
];
const jobsPre = 'SELECT holding_id id,slug,display_name "displayName" FROM holding_bindings WHERE status=\'active\' ORDER BY holding_id';
const jobsRendered = holdingBindingsSql('holding_id id,slug,display_name "displayName"', " WHERE status='active' ORDER BY holding_id");
const billingCols = 'h.holding_id,h.slug,h.display_name,a.version,a.payment_mode';
const billingProjection = 'PROJECTION';
const billingSuffix = ` h LEFT JOIN (${billingProjection}) a ON a.holding_id=h.holding_id WHERE h.holding_id <> 'h003' ORDER BY h.display_name`;
const billingPre = `SELECT ${billingCols} FROM holding_bindings${billingSuffix}`;
const billingRendered = holdingBindingsSql(billingCols, billingSuffix);

const nestedShare = 'EXISTS(SELECT 1 FROM executive_report_shares s WHERE s.report_id=r.id AND s.company_id=r.company_id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?))';
const nestedShareRendered = `EXISTS(${executiveReportSharesSql('1', ' s WHERE s.report_id=r.id AND s.company_id=r.company_id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?)')})`;

describe('Runtime read-tail r15 report guards/share jsonb and holding bindings reads', () => {
  test('canonical helpers render byte-identical SQL for every family member and stay read-only', () => {
    for (let i = 0; i < pre.length; i++) expect(rendered[i]).toBe(pre[i]);
    for (const sql of rendered) readOnly(sql);
    expect(jobsRendered).toBe(jobsPre);
    expect(billingRendered).toBe(billingPre);
    expect(nestedShareRendered).toBe(nestedShare);
    for (const sql of [jobsRendered, billingRendered, nestedShareRendered]) readOnly(sql);
  });

  test('report guards are composed by reports.ts, never inlined', () => {
    const reports = src('executive/reports.ts');
    for (const literal of pre) expect(reports, `${literal}`).not.toContain(literal);
    expect(reports.match(/executiveReportsSql\(/g) ?? []).toHaveLength(3);
    expect(reports.match(/executiveReportSharesSql\(/g) ?? []).toHaveLength(5);
  });

  test('the r12 nested share is composed by mcp.ts, never inlined', () => {
    const mcp = src('executive/mcp.ts');
    expect(mcp).not.toContain(nestedShare);
    expect(mcp.match(/executiveReportSharesSql\(/g) ?? []).toHaveLength(1);
    expect(mcp.match(/executiveReportsSql\(/g) ?? []).toHaveLength(1);
  });

  test('holding bindings reads are composed by the named adapters, never inlined', () => {
    const jobs = src('runtime/jobs.ts');
    expect(jobs).not.toContain('SELECT holding_id id,slug,display_name "displayName"');
    expect(jobs.match(/holdingBindingsSql\(/g) ?? []).toHaveLength(2);
    const billing = src('billing-holding-facts.ts');
    expect(billing).not.toContain('FROM holding_bindings h LEFT JOIN');
    expect(billing.match(/holdingBindingsSql\(/g) ?? []).toHaveLength(3);
  });

  test('control-plane lock / guard and migration SQL is untouched', () => {
    const reports = src('executive/reports.ts');
    expect(reports).toContain('const REPORT_LOCK_SQL = "SELECT id FROM executive_reports WHERE id=? FOR UPDATE"');
    expect(reports).toContain('const REPORT_VERSION_GUARD_SQL = "EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)"');
    const migrations = src('runtime/migrations.ts');
    const postgres = src('runtime/postgres.ts');
    expect(migrations).toContain("'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("'SELECT pg_advisory_unlock(714928001)'");
    expect(migrations).not.toContain('executiveReportsSql');
    expect(migrations).not.toContain('holdingBindingsSql');
    expect(postgres).not.toContain('executiveReportsSql');
    expect(postgres).not.toContain('holdingBindingsSql');
  });

  test('r1-r14 helpers are not re-deduped', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM holding_bindings${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM runtime_health${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_external_tokens${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_report_shares${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_report_revisions WHERE report_id=?${suffix}`')).toHaveLength(2);
    expect(shared.split('`SELECT ${columns} FROM executive_reports${suffix}`')).toHaveLength(2);
  });

  test('retirement:ts-statements records BEFORE=36 strictly lower to 29', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBe(29);
    expect(total).toBeLessThan(36);
  });
});
