import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  PP_RD23_HR_PEOPLE_APPROVER_LEGAL_NAME_SQL,
  PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD,
  PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL,
  PP_RD23_HR_COMP_REVISIONS_SQL,
  PP_RD23_HR_TIME_ENTRIES_OVERTIME_SQL,
  PP_RD23_OVERTIME_PAYROLL_HANDOFFS_SQL,
  PP_RD23_HR_PAYROLL_COST_ASSIGNMENTS_SQL,
  PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD,
  PP_RD23_HR_ENGAGEMENTS_ACTIVE_FOR_PERSON_NOT_EXISTS,
  PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD,
  PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS,
  PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS,
  PP_RD23_HR_PAY_GROUPS_IMAGE_SQL,
  PP_RD23_RECOVERY_FIRST_PERIOD_GUARD,
  PP_RD23_POSTED_PAYROLL_RUN_GUARD,
  PP_RD23_IAM_REVIEWS_BY_ID_SQL,
  PP_RD23_PROJECT_WORKER_ENGAGEMENT_SQL,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r23-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const quoted = (literal: string) => JSON.stringify(literal);

const literals = [
  PP_RD23_HR_PEOPLE_APPROVER_LEGAL_NAME_SQL,
  PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD,
  PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL,
  PP_RD23_HR_COMP_REVISIONS_SQL,
  PP_RD23_HR_TIME_ENTRIES_OVERTIME_SQL,
  PP_RD23_OVERTIME_PAYROLL_HANDOFFS_SQL,
  PP_RD23_HR_PAYROLL_COST_ASSIGNMENTS_SQL,
  PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD,
  PP_RD23_HR_ENGAGEMENTS_ACTIVE_FOR_PERSON_NOT_EXISTS,
  PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD,
  PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS,
  PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS,
  PP_RD23_HR_PAY_GROUPS_IMAGE_SQL,
  PP_RD23_RECOVERY_FIRST_PERIOD_GUARD,
  PP_RD23_POSTED_PAYROLL_RUN_GUARD,
  PP_RD23_IAM_REVIEWS_BY_ID_SQL,
  PP_RD23_PROJECT_WORKER_ENGAGEMENT_SQL,
];

describe('People Payroll read-tail r23 byte-identical / parameterized family dedupe', () => {
  test('canonical constants own the exact read-only fragments', () => {
    expect(PP_RD23_HR_PEOPLE_APPROVER_LEGAL_NAME_SQL).toBe('SELECT legal_name FROM hr_people WHERE company_id=? AND iam_same_person(user_id,?,company_id) AND merged_into_id IS NULL LIMIT 1');
    expect(PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD).toBe("EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN ('draft','rejected'))");
    expect(PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL).toBe('SELECT company_id,month FROM expense_ai_usage');
    expect(PP_RD23_HR_COMP_REVISIONS_SQL).toBe("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1");
    expect(PP_RD23_HR_TIME_ENTRIES_OVERTIME_SQL).toBe("SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id");
    expect(PP_RD23_OVERTIME_PAYROLL_HANDOFFS_SQL).toBe('SELECT id,request_id,request_version,minutes,work_date FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ? ORDER BY id');
    expect(PP_RD23_HR_PAYROLL_COST_ASSIGNMENTS_SQL).toBe("SELECT id,effective_from,effective_to,branch_id,department_id,evidence_status,version FROM hr_payroll_cost_assignments WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from,id");
    expect(PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD).toBe("EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND currency=? AND status='active')");
    expect(PP_RD23_HR_ENGAGEMENTS_ACTIVE_FOR_PERSON_NOT_EXISTS).toBe("NOT EXISTS(SELECT 1 FROM hr_engagements WHERE company_id=? AND person_id=? AND status!='ended')");
    expect(PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD).toBe('EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status=?)');
    expect(PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS).toBe('SELECT 1 FROM leave_types WHERE id=? AND company_id=?');
    expect(PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS).toBe('SELECT 1 FROM hr_engagements WHERE id=? AND company_id=?');
    expect(PP_RD23_HR_PAY_GROUPS_IMAGE_SQL).toBe('SELECT to_jsonb(g)::text image FROM hr_pay_groups g WHERE g.id=? AND g.company_id=?');
    expect(PP_RD23_RECOVERY_FIRST_PERIOD_GUARD).toBe("NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND adjustment_of_run_id IS NULL AND status='posted' AND period_start<=? AND period_end>=?)");
    expect(PP_RD23_POSTED_PAYROLL_RUN_GUARD).toBe("EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted')");
    expect(PP_RD23_IAM_REVIEWS_BY_ID_SQL).toBe('SELECT id,kind,payload,created_by,created_person FROM iam_reviews WHERE id=$1 AND company_id=$2');
    expect(PP_RD23_PROJECT_WORKER_ENGAGEMENT_SQL).toBe('SELECT * FROM hr_engagements WHERE id=? AND person_id=? AND company_id=?');
    for (const sql of literals) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('each family has one shared symbol and composed callers keep no local copy', () => {
    const families = [
      { name: 'PP_RD23_HR_PEOPLE_APPROVER_LEGAL_NAME_SQL', literal: PP_RD23_HR_PEOPLE_APPROVER_LEGAL_NAME_SQL, callers: [['people-app-entrypoint-facts.ts', 1], ['department-override-facts.ts', 1]] as [string, number][] },
      { name: 'PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD', literal: PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD, callers: [['expense-workflow.ts', 1], ['expense-drafts.ts', 1]] as [string, number][] },
      { name: 'PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL', literal: PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL, callers: [['collaboration-ai-usage.ts', 1], ['finance-ai-quota-sources.ts', 1]] as [string, number][] },
      { name: 'PP_RD23_HR_COMP_REVISIONS_SQL', literal: PP_RD23_HR_COMP_REVISIONS_SQL, callers: [['people-payroll.ts', 1], ['payroll-readiness.ts', 1]] as [string, number][] },
      { name: 'PP_RD23_HR_TIME_ENTRIES_OVERTIME_SQL', literal: PP_RD23_HR_TIME_ENTRIES_OVERTIME_SQL, callers: [['people-payroll.ts', 1], ['payroll-readiness.ts', 1]] as [string, number][] },
      { name: 'PP_RD23_OVERTIME_PAYROLL_HANDOFFS_SQL', literal: PP_RD23_OVERTIME_PAYROLL_HANDOFFS_SQL, callers: [['people-payroll.ts', 1], ['payroll-readiness.ts', 1]] as [string, number][] },
      { name: 'PP_RD23_HR_PAYROLL_COST_ASSIGNMENTS_SQL', literal: PP_RD23_HR_PAYROLL_COST_ASSIGNMENTS_SQL, callers: [['people-payroll.ts', 1], ['payroll-readiness.ts', 1]] as [string, number][] },
      { name: 'PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD', literal: PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD, callers: [['people-payroll.ts', 1], ['people-handoffs.ts', 1]] as [string, number][] },
      { name: 'PP_RD23_HR_ENGAGEMENTS_ACTIVE_FOR_PERSON_NOT_EXISTS', literal: PP_RD23_HR_ENGAGEMENTS_ACTIVE_FOR_PERSON_NOT_EXISTS, callers: [['people-lifecycle.ts', 1], ['people-directory.ts', 1]] as [string, number][] },
      { name: 'PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD', literal: PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD, callers: [['people-evaluation.ts', 3]] as [string, number][] },
      { name: 'PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS', literal: PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS, callers: [['leave-self-service.ts', 2]] as [string, number][] },
      { name: 'PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS', literal: PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS, callers: [['leave-self-service.ts', 2]] as [string, number][] },
      { name: 'PP_RD23_HR_PAY_GROUPS_IMAGE_SQL', literal: PP_RD23_HR_PAY_GROUPS_IMAGE_SQL, callers: [['payroll-readiness.ts', 2]] as [string, number][] },
      { name: 'PP_RD23_RECOVERY_FIRST_PERIOD_GUARD', literal: PP_RD23_RECOVERY_FIRST_PERIOD_GUARD, callers: [['payroll-recoveries.ts', 2]] as [string, number][] },
      { name: 'PP_RD23_POSTED_PAYROLL_RUN_GUARD', literal: PP_RD23_POSTED_PAYROLL_RUN_GUARD, callers: [['people-payroll.ts', 2]] as [string, number][] },
      { name: 'PP_RD23_IAM_REVIEWS_BY_ID_SQL', literal: PP_RD23_IAM_REVIEWS_BY_ID_SQL, callers: [['people-iam-purchases.ts', 2]] as [string, number][] },
      { name: 'PP_RD23_PROJECT_WORKER_ENGAGEMENT_SQL', literal: PP_RD23_PROJECT_WORKER_ENGAGEMENT_SQL, callers: [['project-worker-startup-jobs.ts', 2]] as [string, number][] },
    ];
    for (const f of families) {
      for (const [name, count] of f.callers) {
        const s = src(name);
        expect(s, name).not.toContain(quoted(f.literal));
        expect(s.match(new RegExp(f.name, 'g')) ?? [], name).toHaveLength(count + 1);
      }
    }
  });

  test('the shared module declares each fragment exactly once', () => {
    const s = src('pp-read-tail-r23-sources.ts');
    for (const literal of literals) {
      expect(s.split(literal)).toHaveLength(2);
    }
  });

  test('worktree read-only SELECT count is strictly lower than the recorded 1485 r22 baseline', () => {
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
    expect(total).toBeLessThan(1485);
    expect(total).toBe(1214);
  });
});
