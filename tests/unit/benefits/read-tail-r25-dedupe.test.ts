import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { benefitSourceProbe } from '../../../packages/server-benefits/src/adapters/benefits-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r25 fold. The inline
// `SELECT 1 FROM (${people.engagements}) e` existence-probe head in
// record-purchases.ts now routes through the owner-local benefitSourceProbe;
// rendered SQL and bind order must be unchanged.
const preDedupeProbe = (source: string, alias: string) => `SELECT 1 FROM (${source}) ${alias}`;
const renderings = [
  ['engagement snapshot', 'engagement_snapshot', 'e',
    'SELECT 1 FROM (engagement_snapshot) e',
    'efd88d09a38b6a6fddc1dd9c37e5e8732b3ce725712713ee02448689605b51b4'],
  ['plan snapshot', 'plan_snapshot', 'p',
    'SELECT 1 FROM (plan_snapshot) p',
    '11465c2c9ce1658a84486a45b3df8288bd2f2a47de0ec1c751010c2e6e6d489e'],
  ['election snapshot', 'election_snapshot', 'e',
    'SELECT 1 FROM (election_snapshot) e',
    'cb82ffe37cced2901044ff5a05d3a779dec32bf315e6c3746b3ad3f4a19ef420'],
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

describe('benefits read-tail r25 source-probe head dedupe', () => {
  test.each(renderings)('probe $0 is byte-identical to the pre-dedupe rendering', (_name, source, alias, before, digest) => {
    const sql = benefitSourceProbe(source, alias);
    expect(sql).toBe(before);
    expect(sql).toBe(preDedupeProbe(source, alias));
    expect(sha(sql)).toBe(digest);
    readOnly(sql);
  });

  test('the active-engagement scope keeps its exact pre-dedupe SQL and bind shape', () => {
    const sql = `EXISTS(${benefitSourceProbe('engagement_snapshot', 'e')} WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.status='active')`;
    expect(sha(sql)).toBe('2f6c9e49edef0d0f871de66611bff7d1e0da974e06ac6cf3b568342d6d507bec');
    expect(sql).toBe(
      `EXISTS(${preDedupeProbe('engagement_snapshot', 'e')} WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.status='active')`,
    );
    expect(sql.match(/\?/g)).toBeNull();
    readOnly(sql);
  });

  test('the renderer owns the single source-probe SELECT and the caller routes through it', () => {
    const head = src('benefits-read-sql.ts');
    expect(head).toContain('export const benefitSourceProbe = (source: string, alias: string) =>');
    expect(head).toContain('selectOneFrom(`(${source})`, ` ${alias}`)');
    expect(head.match(/`SELECT 1 FROM \$\{source\}\$\{tail\}`/g) ?? []).toHaveLength(1);
    const record = src('record-purchases.ts');
    expect(record).toContain('benefitSourceProbe(people.engagements,\'e\')');
    expect(record).toContain('EXISTS(${benefitSourceProbe(people.engagements,\'e\')} WHERE e.person_id=p.id');
    expect(record).not.toContain('EXISTS(SELECT 1 FROM (${people.engagements}) e');
    expect(record.match(/benefitSourceProbe\(/g) ?? []).toHaveLength(2);
  });

  test('the fold adds no write, lock, FOR UPDATE or FOR SHARE statement', () => {
    for (const name of ['benefits-read-sql.ts', 'record-purchases.ts']) {
      expect(src(name), name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(src('people-package-obligations.ts')).toContain('LOCK TABLE benefit_plans,benefit_plan_events,benefit_dependents,benefit_dependent_events,benefit_enrollment_windows,benefit_window_events,benefit_elections,benefit_election_events,benefit_payroll_inputs IN SHARE MODE');
    expect(src('purchases.ts')).toContain('LOCK TABLE benefit_plans,benefit_plan_events,benefit_enrollment_windows,benefit_window_events,benefit_dependents,benefit_dependent_events,benefit_elections,benefit_election_events,benefit_payroll_inputs IN SHARE ROW EXCLUSIVE MODE');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE benefit_plans,benefit_plan_events,benefit_enrollment_windows,benefit_window_events,benefit_dependents,benefit_dependent_events,benefit_elections,benefit_election_events,benefit_payroll_inputs,command_receipts IN SHARE MODE');
  });

  test('retirement:ts-statements is 26, strictly below the r25 BEFORE=35 baseline', () => {
    expect(kpi()).toBe(26);
    expect(kpi()).toBeLessThan(35);
  });
});
