import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { packageClockSql } from '@erpx/commercial-server/public/purchased-capability-sql';
import {
  PP_RD21_CLOCK_NOW_MS_SQL,
  PP_RD21_SCOPE_COMPANY_SQL,
  ppRd21IdCompanyStatusVersionGuard,
  ppRd21IdVersionStatusGuard,
} from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-r21-sources';

const src = (name: string) => readFileSync(new URL(`../../../modules/people-payroll/server/src/adapters/${name}`, import.meta.url), 'utf8');
const quoted = (literal: string) => JSON.stringify(literal);

const literals = [
  PP_RD21_CLOCK_NOW_MS_SQL,
  PP_RD21_SCOPE_COMPANY_SQL,
  ppRd21IdCompanyStatusVersionGuard('hr_payroll_runs', 'posted'),
  ppRd21IdVersionStatusGuard('expense_payment_drafts', 'draft'),
];

describe('People Payroll read-tail r21 byte-identical dedupe', () => {
  test('canonical constants/builders own the exact read-only fragments', () => {
    expect(PP_RD21_CLOCK_NOW_MS_SQL).toBe(`(SELECT ${packageClockSql} now_ms)`);
    expect(PP_RD21_SCOPE_COMPANY_SQL).toBe('WITH scope AS(SELECT ?::text company_id');
    expect(ppRd21IdCompanyStatusVersionGuard('hr_payroll_runs', 'posted'))
      .toBe("EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)");
    expect(ppRd21IdVersionStatusGuard('expense_payment_drafts', 'draft'))
      .toBe("EXISTS(SELECT 1 FROM expense_payment_drafts WHERE id=? AND version=? AND status='draft')");
    for (const sql of literals) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('each family has one shared symbol and composed callers keep no local copy', () => {
    const families = [
      { name: 'PP_RD21_CLOCK_NOW_MS_SQL', literal: PP_RD21_CLOCK_NOW_MS_SQL, callers: [
        ['calendar-roster-facts.ts', 1], ['expense-company-history-scope.ts', 1], ['expense-record-purchases.ts', 1],
        ['financial-product-entry.ts', 1], ['people-lifecycle-jobs.ts', 1], ['project-worker-startup-entry.ts', 1],
        ['record-purchases.ts', 3], ['roster-notification-purchases.ts', 1], ['workforce-record-purchases.ts', 1],
      ] as [string, number][] },
      { name: 'PP_RD21_SCOPE_COMPANY_SQL', literal: PP_RD21_SCOPE_COMPANY_SQL, callers: [
        ['addon-package-obligations.ts', 2], ['benefits-package-obligations.ts', 1], ['expense-posting-setup-sources.ts', 1],
        ['final-pay-cancellation-terminal-sources.ts', 1], ['learning-package-obligations.ts', 1], ['package-obligations.ts', 1],
        ['performance-package-obligations.ts', 1], ['project-package-obligations.ts', 1], ['talent-package-obligations.ts', 1],
      ] as [string, number][] },
      { name: 'ppRd21IdCompanyStatusVersionGuard', literal: ppRd21IdCompanyStatusVersionGuard('hr_payroll_runs', 'posted'), callers: [
        ['payroll-adjustments.ts', 1], ['payroll-payslips.ts', 1], ['payroll-remittance.ts', 1], ['people-payroll.ts', 3],
      ] as [string, number][] },
      { name: 'ppRd21IdVersionStatusGuard', literal: ppRd21IdVersionStatusGuard('expense_payment_drafts', 'draft'), callers: [
        ['expense-ai-source.ts', 1], ['expense-payments.ts', 2], ['people-evaluation.ts', 6],
      ] as [string, number][] },
    ];
    for (const f of families) {
      for (const [name, count] of f.callers) {
        const s = src(name);
        expect(s, name).not.toContain(quoted(f.literal));
        expect(s.match(new RegExp(f.name, 'g')) ?? [], name).toHaveLength(count + 1);
      }
    }
    expect(src('record-purchases.ts')).toContain('MATERIALIZED ${PP_RD21_CLOCK_NOW_MS_SQL}');
    expect(src('package-obligations.ts')).toContain('${PP_RD21_SCOPE_COMPANY_SQL},?::boolean is_base,?::bigint now_ms');
    expect(src('payroll-adjustments.ts')).toContain("ppRd21IdCompanyStatusVersionGuard('hr_payroll_runs', 'posted')");
    expect(src('people-evaluation.ts')).toContain("ppRd21IdVersionStatusGuard('hr_evaluation_assessments', 'confirmed')");
  });

  test('the shared module declares each fragment exactly once', () => {
    const s = src('pp-read-tail-r21-sources.ts');
    expect(s.split('`(SELECT ${packageClockSql} now_ms)`')).toHaveLength(2);
    expect(s.split('`WITH scope AS(SELECT ?::text company_id`')).toHaveLength(2);
    expect(s.split("`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND status='${status}' AND version=?)`")).toHaveLength(2);
    expect(s.split("`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status='${status}')`")).toHaveLength(2);
  });

  test('worktree read-only SELECT count is strictly lower than the recorded 1523 r20 baseline', () => {
    const root = new URL('../../../modules/people-payroll/server/src/', import.meta.url);
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
    expect(total).toBeLessThan(1523);
    expect(total).toBe(1164);
  });
});
