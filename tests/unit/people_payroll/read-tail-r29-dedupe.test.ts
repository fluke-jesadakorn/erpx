import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd29OpenFinancePeriodSql,
  PP_RD29_FINAL_PAY_TERMINAL_SQL,
  PP_RD29_IAM_ROLE_ACTIVE_SQL,
  ppRd29PlacementTerminalSql,
  PP_RD29_LIFECYCLE_CASE_FOR_ENGAGEMENT_SQL,
  ppRd29FinanceExpenseTreasurySql,
  ppRd29FinancePartySupplierSql,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r29-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

describe('People Payroll read-tail r29 parameterized family dedupe', () => {
  test('canonical builders own the exact read-only fragments', () => {
    const period = ppRd29OpenFinancePeriodSql('PERIODS');
    expect(period).toBe("EXISTS(SELECT 1 FROM (PERIODS) finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)");

    const terminal = ppRd29PlacementTerminalSql('ROWS');
    expect(terminal).toBe('NOT EXISTS(SELECT 1 FROM (ROWS) terminal WHERE terminal.id=c.id AND terminal.company_id=c.company_id)');

    const treasury = ppRd29FinanceExpenseTreasurySql("TREASURY('e')");
    expect(treasury).toBe("EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=? AND e.company_id=? AND (TREASURY('e')))");

    const party = ppRd29FinancePartySupplierSql('PARTIES');
    expect(party).toBe("EXISTS(SELECT 1 FROM (PARTIES) finance_parties WHERE id=? AND company_id=? AND kind='supplier' AND status='active' AND currency=?)");

    expect(PP_RD29_FINAL_PAY_TERMINAL_SQL).toBe('NOT EXISTS(SELECT 1 FROM final_pay_terminals fp WHERE fp.preparation_id=p.id AND fp.company_id=p.company_id AND fp.version=p.version)');
    expect(PP_RD29_IAM_ROLE_ACTIVE_SQL).toBe("EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=a.role AND role.company_id IN ('*',a.company_id) AND role.status='active')");
    expect(PP_RD29_LIFECYCLE_CASE_FOR_ENGAGEMENT_SQL).toBe('EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id)');

    for (const literal of [period, terminal, treasury, party, PP_RD29_FINAL_PAY_TERMINAL_SQL, PP_RD29_IAM_ROLE_ACTIVE_SQL, PP_RD29_LIFECYCLE_CASE_FOR_ENGAGEMENT_SQL]) readOnly(literal);
  });

  test('each caller keeps no local copy and uses the shared symbol', () => {
    for (const name of ['expense-workflow.ts', 'expenses.ts']) {
      expect(src(name), name).not.toContain("finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?");
      expect(uses(name, 'ppRd29OpenFinancePeriodSql\\('), name).toBe(1);
    }
    for (const name of ['addon-package-obligations.ts', 'package-obligations.ts']) {
      expect(src(name), name).not.toContain('NOT EXISTS(SELECT 1 FROM final_pay_terminals fp WHERE fp.preparation_id=p.id');
      expect(uses(name, 'PP_RD29_FINAL_PAY_TERMINAL_SQL'), name).toBe(2); // import + interpolation
    }
    for (const name of ['people-purchases.ts', 'workforce-attachment-purchases.ts']) {
      expect(src(name), name).not.toContain("EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=a.role AND role.company_id IN ('*',a.company_id)");
      expect(uses(name, 'PP_RD29_IAM_ROLE_ACTIVE_SQL'), name).toBe(2); // import + interpolation
    }
    for (const name of ['project-package-obligations.ts', 'learning-package-obligations.ts']) {
      expect(src(name), name).not.toContain('NOT EXISTS(SELECT 1 FROM (${projectWorkerPlacementTerminalRows}) terminal WHERE terminal.id=c.id');
      expect(uses(name, 'ppRd29PlacementTerminalSql\\('), name).toBe(1);
    }
    for (const name of ['people-lifecycle.ts', 'calendar-access-facts.ts']) {
      expect(src(name), name).not.toContain('EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id)');
      expect(uses(name, 'PP_RD29_LIFECYCLE_CASE_FOR_ENGAGEMENT_SQL'), name).toBe(2); // import + interpolation
    }
    for (const name of ['expense-payment-cancellation.ts', 'expenses.ts']) {
      expect(src(name), name).not.toContain('EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=? AND e.company_id=? AND (${expensePaymentTreasurySql');
      expect(uses(name, 'ppRd29FinanceExpenseTreasurySql\\('), name).toBe(1);
    }
    for (const name of ['expense-drafts.ts', 'expenses.ts']) {
      expect(src(name), name).not.toContain("finance_parties WHERE id=? AND company_id=? AND kind='supplier' AND status='active' AND currency=?");
      expect(uses(name, 'ppRd29FinancePartySupplierSql\\('), name).toBe(1);
    }
  });

  test('the shared module declares each fragment exactly once', () => {
    const s = src('pp-read-tail-r29-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(7);
    for (const fragment of [
      'EXISTS(SELECT 1 FROM (${periods}) finance_periods WHERE id=? AND company_id=? AND state=\'open\' AND start_date<=? AND end_date>=?)',
      'NOT EXISTS(SELECT 1 FROM final_pay_terminals fp WHERE fp.preparation_id=p.id AND fp.company_id=p.company_id AND fp.version=p.version)',
      "EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=a.role AND role.company_id IN ('*',a.company_id) AND role.status='active')",
      'NOT EXISTS(SELECT 1 FROM (${rows}) terminal WHERE terminal.id=c.id AND terminal.company_id=c.company_id)',
      'EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id)',
      'EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=? AND e.company_id=? AND (${treasury}))',
      "EXISTS(SELECT 1 FROM (${parties}) finance_parties WHERE id=? AND company_id=? AND kind='supplier' AND status='active' AND currency=?)",
    ]) {
      expect(s.includes(fragment), fragment).toBe(true);
    }
  });

  test('lock-bearing reads in touched owners remain untouched', () => {
    expect(src('project-package-obligations.ts')).toContain('LOCK TABLE hr_people,hr_engagements,people_lifecycle_cases');
    expect(src('payroll-readiness.ts')).toContain('FOR UPDATE OF r SKIP LOCKED');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
  });

  test('worktree SELECT count is strictly lower than the recorded 1423 r28 baseline', () => {
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
    expect(total).toBeLessThan(1423);
    expect(total).toBe(1356);
  });
});
