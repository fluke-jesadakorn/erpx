import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd27CalendarAbsenceSql,
  ppRd27LeaveRequestListSql,
  ppRd27IdCompanyVersionStatusLiteralGuard,
  ppRd27IdByIdCompanySql,
  ppRd27IdCompanyEngagementApprovedVersionGuard,
  ppRd27IdVersionStatusInGuard,
  ppRd27StateVersionScopeByIdSql,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r27-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('People Payroll read-tail r27 parameterized family dedupe', () => {
  test('canonical builders own the exact read-only fragments', () => {
    const calendar = ppRd27CalendarAbsenceSql('coalesce(l.person_id,e.person_id) IN (SELECT jsonb_array_elements_text(?::jsonb))');
    expect(calendar).toContain("SELECT 1 FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id");
    expect(calendar).toContain("AND coalesce(l.person_id,e.person_id) IN (SELECT jsonb_array_elements_text(?::jsonb)) AND EXISTS(SELECT 1 FROM (");
    // five from the canonical fragment plus one from the injected r19 inList expression
    expect(calendar.match(/SELECT /g) ?? []).toHaveLength(6);

    const leaveList = ppRd27LeaveRequestListSql('r.company_id=?');
    expect(leaveList).toBe('SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name\n      FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id\n      WHERE r.company_id=? ORDER BY r.created_at DESC,r.id DESC LIMIT ?');

    const guards: string[] = [
      ppRd27IdCompanyVersionStatusLiteralGuard('expense_payment_drafts', 'draft'),
      ppRd27IdByIdCompanySql('leave_policy_versions'),
      ppRd27IdCompanyEngagementApprovedVersionGuard('hr_time_entries'),
      ppRd27IdVersionStatusInGuard('hr_payroll_advance_repayments', "'open','reserved'"),
      ppRd27StateVersionScopeByIdSql('overtime_requests'),
    ];
    expect(guards[0]).toBe("EXISTS(SELECT 1 FROM expense_payment_drafts WHERE id=? AND company_id=? AND version=? AND status='draft')");
    expect(guards[1]).toBe('SELECT id FROM leave_policy_versions WHERE id=? AND company_id=?');
    expect(guards[2]).toBe("EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND company_id=? AND engagement_id=? AND status='approved' AND version=?)");
    expect(guards[3]).toBe("EXISTS(SELECT 1 FROM hr_payroll_advance_repayments WHERE id=? AND version=? AND status IN ('open','reserved'))");
    expect(guards[4]).toBe('SELECT state,version,company_id,branch_id,department_id FROM overtime_requests WHERE id=?');
    for (const literal of [calendar, leaveList, ...guards]) readOnly(literal);
  });

  test('each caller keeps no local copy and uses the shared symbol', () => {
    const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

    for (const name of ['calendar-appointments-facts.ts', 'calendar-index-facts.ts']) {
      const s = src(name);
      expect(s, name).not.toContain('SELECT 1 FROM leave_requests l LEFT JOIN employees');
      expect(uses(name, 'ppRd27CalendarAbsenceSql'), name).toBe(2);
    }

    expect(src('leave-self-service.ts')).not.toContain('WHERE ${clauses.join(" AND ")} ORDER BY r.created_at');
    expect(uses('leave-self-service.ts', 'ppRd27LeaveRequestListSql')).toBe(3);
    expect(src('leave-self-service.ts')).not.toContain('SELECT id FROM leave_policy_versions');
    expect(uses('leave-self-service.ts', 'ppRd27IdByIdCompanySql')).toBe(2);

    expect(src('expense-payments.ts')).not.toContain("EXISTS(SELECT 1 FROM expense_payment_drafts WHERE id=? AND company_id=? AND version=? AND status='draft')");
    expect(uses('expense-payments.ts', 'ppRd27IdCompanyVersionStatusLiteralGuard')).toBe(2);
    expect(src('payroll-recoveries.ts')).not.toContain("EXISTS(SELECT 1 FROM hr_payroll_account_mappings WHERE id=? AND company_id=? AND version=? AND status='approved')");
    expect(uses('payroll-recoveries.ts', 'ppRd27IdCompanyVersionStatusLiteralGuard')).toBe(2);

    expect(src('payroll-adjustments.ts')).not.toContain("EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND company_id=? AND engagement_id=? AND status='approved' AND version=?)");
    expect(uses('payroll-adjustments.ts', 'ppRd27IdCompanyEngagementApprovedVersionGuard')).toBe(2);
    expect(src('payroll-statutory.ts')).not.toContain("EXISTS(SELECT 1 FROM hr_thai_payroll_profiles WHERE id=? AND company_id=? AND engagement_id=? AND status='approved' AND version=?)");
    expect(uses('payroll-statutory.ts', 'ppRd27IdCompanyEngagementApprovedVersionGuard')).toBe(2);

    expect(src('payroll-readiness.ts')).not.toContain("EXISTS(SELECT 1 FROM hr_payroll_advance_repayments WHERE id=? AND version=? AND status IN ('open','reserved'))");
    expect(uses('payroll-readiness.ts', 'ppRd27IdVersionStatusInGuard')).toBe(2);
    expect(src('people-lifecycle.ts')).not.toContain("EXISTS(SELECT 1 FROM people_lifecycle_cases WHERE id=? AND version=? AND status IN ('draft','waiting_it'))");
    expect(uses('people-lifecycle.ts', 'ppRd27IdVersionStatusInGuard')).toBe(2);

    expect(src('workflow-projection-source.ts')).not.toContain('SELECT state,version,company_id,branch_id,department_id FROM overtime_requests WHERE id=?');
    expect(src('workflow-projection-source.ts')).not.toContain('SELECT state,version,company_id,branch_id,department_id FROM leave_requests WHERE id=?');
    expect(uses('workflow-projection-source.ts', 'ppRd27StateVersionScopeByIdSql')).toBe(3);

    expect(src('people-payroll.ts')).not.toContain('SELECT id FROM hr_engagements WHERE id=? AND company_id=?');
    expect(uses('people-payroll.ts', 'ppRd27IdByIdCompanySql')).toBe(2);
  });

  test('the shared module declares each fragment exactly once', () => {
    const s = src('pp-read-tail-r27-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(7);
    for (const fragment of [
      'SELECT 1 FROM leave_requests l LEFT JOIN employees',
      'SELECT r.id,r.number,r.start_date',
      'EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND status=\'${status}\')',
      'SELECT id FROM ${table} WHERE id=? AND company_id=?',
      'EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND engagement_id=? AND status=\'approved\' AND version=?)',
      'EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status IN (${statuses}))',
      'SELECT state,version,company_id,branch_id,department_id FROM ${table} WHERE id=?',
    ]) {
      expect(s.includes(fragment), fragment).toBe(true);
    }
  });

  test('lock-bearing reads in touched owners remain untouched', () => {
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE');
    expect(src('payroll-recoveries.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(src('leave-self-service.ts')).toContain('FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('FOR UPDATE');
  });

  test('worktree SELECT count is strictly lower than the recorded 1443 r26 baseline', () => {
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
    expect(total).toBeLessThan(1443);
    expect(total).toBe(1200);
  });
});
