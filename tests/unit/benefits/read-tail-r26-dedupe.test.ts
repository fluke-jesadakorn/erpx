import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { benefitSourceProbe } from '../../../packages/server-benefits/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r26 fold. The inline
// `SELECT 1 FROM (${people.people}) p` existence-probe head in
// attachment-purchases.ts now routes through the owner-local benefitSourceProbe;
// rendered SQL, bind order and source locks must be unchanged.
const preDedupeProbe = (source: string, alias: string) => `SELECT 1 FROM (${source}) ${alias}`;
const renderings = [
  ['people snapshot', 'people_snapshot', 'p',
    'SELECT 1 FROM (people_snapshot) p',
    'fa76d303b47b9df52a95faf34d902d1361cdf34ecac23e593715b9c0518fe937'],
  ['people actor', 'people_snapshot', 'actor',
    'SELECT 1 FROM (people_snapshot) actor',
    'da27293664589b17387de7da3dd7e33de1fb0cd980a9871c3c5221723ae82c72'],
  ['engagement snapshot', 'engagement_snapshot', 'e',
    'SELECT 1 FROM (engagement_snapshot) e',
    'efd88d09a38b6a6fddc1dd9c37e5e8732b3ce725712713ee02448689605b51b4'],
] as const;

const kpi = () => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) {
        total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    }
  };
  walk(srcRoot);
  return total;
};

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('benefits read-tail r26 source-probe head dedupe', () => {
  test.each(renderings)('probe $0 is byte-identical to the pre-dedupe rendering', (_name, source, alias, before, digest) => {
    const sql = benefitSourceProbe(source, alias);
    expect(sql).toBe(before);
    expect(sql).toBe(preDedupeProbe(source, alias));
    expect(sha(sql)).toBe(digest);
    readOnly(sql);
  });

  test('the plan-file people scope keeps its exact pre-dedupe SQL and bind shape', () => {
    const sql = `EXISTS(${benefitSourceProbe('people_snapshot', 'p')} JOIN (engagement_snapshot) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=benefit_file_source.company_id AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`;
    expect(sha(sql)).toBe('eacf13b88c3cf002209000a5bd1fca03cb3a50a5cf3c48651891e473390098ce');
    expect(sql).toBe(
      `EXISTS(${preDedupeProbe('people_snapshot', 'p')} JOIN (engagement_snapshot) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=benefit_file_source.company_id AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`,
    );
    expect(sql.match(/\?/g)).toHaveLength(1);
    readOnly(sql);
  });

  test('the renderer owns the single source-probe SELECT and the caller routes through it', () => {
    const head = src('benefits-read-sql.ts');
    expect(head).toContain('export const benefitSourceProbe = (source: string, alias: string) =>');
    expect(head).toContain('`SELECT 1 FROM (${source}) ${alias}`');
    expect(head.match(/SELECT 1 FROM \(\$\{source\}\) \$\{alias\}/g) ?? []).toHaveLength(1);
    const attach = src('attachment-purchases.ts');
    expect(attach).toContain('benefitSourceProbe(people.people,\'p\')');
    expect(attach).toContain('EXISTS(${benefitSourceProbe(people.people,\'p\')} JOIN (${people.engagements}) e');
    expect(attach).not.toContain('EXISTS(SELECT 1 FROM (${people.people}) p');
    expect(attach.match(/benefitSourceProbe\(/g) ?? []).toHaveLength(2);
  });

  test('the fold adds no write, lock, FOR UPDATE or FOR SHARE statement', () => {
    for (const name of ['benefits-read-sql.ts', 'attachment-purchases.ts']) {
      expect(src(name), name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(src('people-package-obligations.ts')).toContain('LOCK TABLE benefit_plans,benefit_plan_events,benefit_dependents,benefit_dependent_events,benefit_enrollment_windows,benefit_window_events,benefit_elections,benefit_election_events,benefit_payroll_inputs IN SHARE MODE');
    expect(src('purchases.ts')).toContain('LOCK TABLE benefit_plans,benefit_plan_events,benefit_enrollment_windows,benefit_window_events,benefit_dependents,benefit_dependent_events,benefit_elections,benefit_election_events,benefit_payroll_inputs IN SHARE ROW EXCLUSIVE MODE');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE benefit_plans,benefit_plan_events,benefit_enrollment_windows,benefit_window_events,benefit_dependents,benefit_dependent_events,benefit_elections,benefit_election_events,benefit_payroll_inputs,command_receipts IN SHARE MODE');
  });

  test('retirement:ts-statements is 30, strictly below the r26 BEFORE=34 baseline', () => {
    expect(kpi()).toBe(30);
    expect(kpi()).toBeLessThan(34);
  });
});
