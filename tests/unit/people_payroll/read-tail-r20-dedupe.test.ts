import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  PP_RD20_ROSTER_ASSIGN_BY_ID_SQL,
  PP_RD20_ROSTER_ASSIGN_BY_SHIFT_SQL,
  PP_RD20_PAYROLL_RUNS_SQL,
  PP_RD20_PAYROLL_POSTINGS_SQL,
  PP_RD20_STATUTORY_REMITTANCES_SQL,
  PP_RD20_PAYROLL_ITEMS_SQL,
  PP_RD20_PAYMENT_BATCHES_SQL,
  PP_RD20_PAYMENT_BATCH_ITEMS_SQL,
  PP_RD20_PAYROLL_PAYMENT_SETTLEMENTS_SQL,
  PP_RD20_PAYROLL_RECOVERIES_SQL,
  PP_RD20_PAYROLL_ADVANCES_SQL,
  PP_RD20_PAYROLL_ADVANCE_REPAYMENTS_SQL,
  PP_RD20_HR_ENGAGEMENTS_FROM_SQL,
  PP_RD20_LIFECYCLE_CASES_C_FROM_SQL,
  PP_RD20_LIFECYCLE_CASES_C_JOIN_SQL,
  PP_RD20_APPLIED_PACKAGES_AP_FROM_SQL,
  PP_RD20_HR_PEOPLE_H_JOIN_SQL,
  PP_RD20_HR_PEOPLE_P_COMPACT_JOIN_SQL,
  PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r20-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const quoted = (literal: string) => JSON.stringify(literal);

const literals = [
  PP_RD20_ROSTER_ASSIGN_BY_ID_SQL,
  PP_RD20_ROSTER_ASSIGN_BY_SHIFT_SQL,
  PP_RD20_PAYROLL_RUNS_SQL,
  PP_RD20_PAYROLL_POSTINGS_SQL,
  PP_RD20_STATUTORY_REMITTANCES_SQL,
  PP_RD20_PAYROLL_ITEMS_SQL,
  PP_RD20_PAYMENT_BATCHES_SQL,
  PP_RD20_PAYMENT_BATCH_ITEMS_SQL,
  PP_RD20_PAYROLL_PAYMENT_SETTLEMENTS_SQL,
  PP_RD20_PAYROLL_RECOVERIES_SQL,
  PP_RD20_PAYROLL_ADVANCES_SQL,
  PP_RD20_PAYROLL_ADVANCE_REPAYMENTS_SQL,
  PP_RD20_HR_ENGAGEMENTS_FROM_SQL,
  PP_RD20_LIFECYCLE_CASES_C_FROM_SQL,
  PP_RD20_LIFECYCLE_CASES_C_JOIN_SQL,
  PP_RD20_APPLIED_PACKAGES_AP_FROM_SQL,
  PP_RD20_HR_PEOPLE_H_JOIN_SQL,
  PP_RD20_HR_PEOPLE_P_COMPACT_JOIN_SQL,
  PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL,
];

describe('People Payroll read-tail r20 byte-identical dedupe', () => {
  test('canonical constants own exact read-only point reads, payroll selects and projection tails', () => {
    expect(PP_RD20_ROSTER_ASSIGN_BY_ID_SQL).toBe('SELECT * FROM people_roster_assignments WHERE id=?');
    expect(PP_RD20_ROSTER_ASSIGN_BY_SHIFT_SQL).toBe("SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned'");
    expect(PP_RD20_PAYROLL_RUNS_SQL).toBe('SELECT * FROM hr_payroll_runs');
    expect(PP_RD20_PAYROLL_POSTINGS_SQL).toBe('SELECT * FROM hr_payroll_postings');
    expect(PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL).toContain('FROM hr_engagements');
    expect(PP_RD20_HR_PEOPLE_H_JOIN_SQL).toContain('FROM hr_people) h');
    for (const sql of literals) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('each duplicate family has one shared literal and composed callers keep no local copy', () => {
    const families = [
      { constant: 'PP_RD20_ROSTER_ASSIGN_BY_ID_SQL', literal: PP_RD20_ROSTER_ASSIGN_BY_ID_SQL, callers: [['people-roster.ts', 4]] as [string, number][] },
      { constant: 'PP_RD20_ROSTER_ASSIGN_BY_SHIFT_SQL', literal: PP_RD20_ROSTER_ASSIGN_BY_SHIFT_SQL, callers: [['people-roster.ts', 2]] as [string, number][] },
      { constant: 'PP_RD20_PAYROLL_RUNS_SQL', literal: PP_RD20_PAYROLL_RUNS_SQL, callers: [['package-obligations.ts', 1], ['addon-package-obligations.ts', 1], ['benefits-package-obligations.ts', 1], ['payroll-other-remittance-sources.ts', 1]] as [string, number][] },
      { constant: 'PP_RD20_PAYROLL_POSTINGS_SQL', literal: PP_RD20_PAYROLL_POSTINGS_SQL, callers: [['package-obligations.ts', 1], ['addon-package-obligations.ts', 1], ['benefits-package-obligations.ts', 1], ['payroll-other-remittance-sources.ts', 1]] as [string, number][] },
      { constant: 'PP_RD20_STATUTORY_REMITTANCES_SQL', literal: PP_RD20_STATUTORY_REMITTANCES_SQL, callers: [['package-obligations.ts', 1], ['addon-package-obligations.ts', 1], ['payroll-other-remittance-sources.ts', 1]] as [string, number][] },
      { constant: 'PP_RD20_HR_ENGAGEMENTS_FROM_SQL', literal: PP_RD20_HR_ENGAGEMENTS_FROM_SQL, callers: [['identity-snapshot-sources.ts', 1], ['collaboration-notification-snapshot-sources.ts', 1]] as [string, number][] },
      { constant: 'PP_RD20_LIFECYCLE_CASES_C_FROM_SQL', literal: PP_RD20_LIFECYCLE_CASES_C_FROM_SQL, callers: [['identity-snapshot-sources.ts', 2]] as [string, number][] },
      { constant: 'PP_RD20_LIFECYCLE_CASES_C_JOIN_SQL', literal: PP_RD20_LIFECYCLE_CASES_C_JOIN_SQL, callers: [['identity-snapshot-sources.ts', 2]] as [string, number][] },
      { constant: 'PP_RD20_APPLIED_PACKAGES_AP_FROM_SQL', literal: PP_RD20_APPLIED_PACKAGES_AP_FROM_SQL, callers: [['identity-snapshot-sources.ts', 2]] as [string, number][] },
      { constant: 'PP_RD20_HR_PEOPLE_H_JOIN_SQL', literal: PP_RD20_HR_PEOPLE_H_JOIN_SQL, callers: [['identity-snapshot-sources.ts', 3]] as [string, number][] },
      { constant: 'PP_RD20_HR_PEOPLE_P_COMPACT_JOIN_SQL', literal: PP_RD20_HR_PEOPLE_P_COMPACT_JOIN_SQL, callers: [['finance-owned-snapshot-sources.ts', 1], ['workflow-final-owner-projections.ts', 2]] as [string, number][] },
      { constant: 'PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL', literal: PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL, callers: [['final-root-owner-projection-sources.ts', 1], ['platform-owner-projection-sources.ts', 1], ['commercial-owner-projection-sources.ts', 1]] as [string, number][] },
    ];
    for (const f of families) {
      for (const [name, count] of f.callers) {
        const s = src(name);
        expect(s, name).not.toContain(quoted(f.literal));
        expect(s.match(new RegExp(f.constant, 'g')) ?? [], name).toHaveLength(count + 1);
      }
    }
    expect(src('people-roster.ts')).toContain('database.prepare(PP_RD20_ROSTER_ASSIGN_BY_ID_SQL).bind(fact0)');
    expect(src('payroll-other-remittance-sources.ts')).toContain("runs:PP_RD20_PAYROLL_RUNS_SQL,postings:PP_RD20_PAYROLL_POSTINGS_SQL");
    expect(src('identity-snapshot-sources.ts')).toContain('"payroll_owner_facts_hr_engagements_from": PP_RD20_HR_ENGAGEMENTS_FROM_SQL');
  });

  test('the shared module declares each literal exactly once', () => {
    const s = src('pp-read-tail-r20-sources.ts');
    for (const literal of literals) expect(s.split(quoted(literal))).toHaveLength(2);
  });

  test('worktree read-only SELECT count is strictly lower than the recorded 1552 r19 baseline', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(1552);
    expect(total).toBe(1393);
  });
});
