import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd37StatusGuard5,
  ppRd37CoalesceLegalName,
  ppRd37IdVersionInGuard,
  ppRd37RecoveryGuard,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r37-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(`${symbol}\\(`, 'g')) ?? []).length;

describe('People Payroll read-tail r37 four-family dedupe', () => {
  const f1: Array<[string, string[], string, string]> = [
    ['people-payroll.ts', ['hr_payroll_items', 'run_id', 'posted', 'net_minor', 'input_snapshot_json'], "EXISTS(SELECT 1 FROM hr_payroll_items WHERE id=? AND run_id=? AND status='posted' AND net_minor=? AND input_snapshot_json=?)", 'ppRd37StatusGuard5("hr_payroll_items", "run_id", "posted", "net_minor", "input_snapshot_json")'],
    ['payroll-readiness.ts', ['hr_pay_groups', 'company_id', 'active', 'overtime_multiplier_bps', 'proration_method'], "EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active' AND overtime_multiplier_bps=? AND proration_method=?)", 'ppRd37StatusGuard5("hr_pay_groups", "company_id", "active", "overtime_multiplier_bps", "proration_method")'],
    ['payroll-readiness.ts', ['hr_payroll_runs', 'company_id', 'preview', 'version', 'input_digest'], "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='preview' AND version=? AND input_digest=?)", 'ppRd37StatusGuard5("hr_payroll_runs", "company_id", "preview", "version", "input_digest")'],
  ];
  const f2: Array<[string, string[], string, string]> = [
    ['finance-journal-people-facts.ts', ['created_by', 'created_by_name'], "COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=j.created_by AND p.company_id=j.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked account') created_by_name", 'ppRd37CoalesceLegalName("created_by", "created_by_name")'],
    ['finance-journal-people-facts.ts', ['posted_by', 'posted_by_name'], "COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=j.posted_by AND p.company_id=j.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked account') posted_by_name", 'ppRd37CoalesceLegalName("posted_by", "posted_by_name")'],
  ];
  const f3: Array<[string, string, string, string]> = [
    ['leave.ts', 'leave_requests', 'state', 'ppRd37IdVersionInGuard("leave_requests", "state", states.map(() => "?").join(","))'],
    ['commerce-expense-payment-effects.ts', 'finance_expenses', 'status', 'ppRd37IdVersionInGuard("finance_expenses", "status", states.map(()=>"?").join(","))'],
  ];
  const f4: Array<[string, string[], string, string]> = [
    ['payroll-recoveries.ts', ['consent_pending', 'employee_user_id'], "EXISTS(SELECT 1 FROM hr_payroll_recoveries WHERE id=? AND company_id=? AND version=? AND status='consent_pending' AND employee_user_id=?)", 'ppRd37RecoveryGuard("consent_pending", "employee_user_id")'],
    ['payroll-recoveries.ts', ['approved', 'consent_sha256'], "EXISTS(SELECT 1 FROM hr_payroll_recoveries WHERE id=? AND company_id=? AND version=? AND status='approved' AND consent_sha256=?)", 'ppRd37RecoveryGuard("approved", "consent_sha256")'],
  ];

  test('the four shared builders own one byte-identical read-only literal each', () => {
    for (const [, args, expected] of f1) { expect(ppRd37StatusGuard5(...(args as [string, string, string, string, string]))).toBe(expected); readOnly(expected); }
    for (const [, args, expected] of f2) { expect(ppRd37CoalesceLegalName(...(args as [string, string]))).toBe(expected); readOnly(expected); }
    for (const [, table, col] of f3) { const literal = ppRd37IdVersionInGuard(table, col, '?'); expect(literal).toContain(`SELECT 1 FROM ${table} WHERE id=? AND version=? AND ${col} IN (?)`); readOnly(literal); }
    for (const [, args, expected] of f4) { expect(ppRd37RecoveryGuard(...(args as [string, string]))).toBe(expected); readOnly(expected); }
    const helper = src('pp-read-tail-r37-sources.ts');
    expect((helper.match(/export const /g) ?? []).length).toBe(4);
  });

  test('each caller keeps no local copy and uses the shared builder', () => {
    const all: Array<[string, string]> = [];
    for (const [name, , , call] of f1) all.push([name, call]);
    for (const [name, , , call] of f2) all.push([name, call]);
    for (const [name, , , call] of f3) all.push([name, call]);
    for (const [name, , , call] of f4) all.push([name, call]);
    for (const [name, call] of all) expect(src(name), name).toContain(call);
    const expectedCounts: Record<string, number> = {
      'people-payroll.ts': 1, 'payroll-readiness.ts': 2,
      'finance-journal-people-facts.ts': 2, 'leave.ts': 1,
      'commerce-expense-payment-effects.ts': 1, 'payroll-recoveries.ts': 2,
    };
    for (const [name, expectedCount] of Object.entries(expectedCounts)) {
      const total = ['ppRd37StatusGuard5', 'ppRd37CoalesceLegalName', 'ppRd37IdVersionInGuard', 'ppRd37RecoveryGuard']
        .reduce((n, s) => n + uses(name, s), 0);
      expect(total, name).toBe(expectedCount);
    }
  });

  test('lock-bearing reads and r1-r36 families stay untouched', () => {
    expect(src('pp-read-tail-r36-sources.ts')).toContain('export const ppRd36SinglePred');
    expect(src('pp-read-tail-r35-sources.ts')).toContain('export const ppRd35PayrollItemsByRun');
    expect(src('payroll-recoveries.ts')).toContain("'SELECT id FROM hr_payroll_recoveries WHERE id=? FOR UPDATE'");
    expect(src('payroll-recoveries.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-roster.ts')).toContain('"SELECT id FROM hr_people WHERE id=? AND company_id=? FOR UPDATE"');
  });

  test('worktree SELECT count is strictly lower than the recorded 1356 r36 baseline', () => {
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
    expect(total).toBeLessThan(1356);
    expect(total).toBe(1265);
  });
});
