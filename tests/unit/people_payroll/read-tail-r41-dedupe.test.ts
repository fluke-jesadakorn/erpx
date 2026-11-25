import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd32TwoColProjection } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r32-sources';
import { ppRd36TwoPredTail } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r36-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

type Row = [string, 'ppRd32TwoColProjection' | 'ppRd36TwoPredTail', string, string, boolean];
const f: Row[] = [
  ["payroll-purchases.ts", "ppRd32TwoColProjection", "\"source_type\",\"hr_payment_batches\",\"id\",\"company_id\"", "SELECT source_type FROM hr_payment_batches WHERE id=? AND company_id=?", false],
  ["base-attachment-purchases.ts", "ppRd32TwoColProjection", "\"clearance_type\",\"people_preparation_tasks\",\"id\",\"company_id\"", "SELECT clearance_type FROM people_preparation_tasks WHERE id=? AND company_id=?", false],
  ["people-purchases.ts", "ppRd32TwoColProjection", "\"case_id,clearance_type,privileged\",\"people_preparation_tasks\",\"id\",\"company_id\"", "SELECT case_id,clearance_type,privileged FROM people_preparation_tasks WHERE id=? AND company_id=?", false],
  ["hire-handoff-pay-group.ts", "ppRd32TwoColProjection", "\"id\",\"talent_hire_consumption_receipts\",\"handoff_id\",\"company_id\"", "SELECT id FROM talent_hire_consumption_receipts WHERE handoff_id=? AND company_id=?", false],
  ["agent-ai-usage-projection.ts", "ppRd32TwoColProjection", "\"count(*)\",\"expense_ai_usage\",\"company_id\",\"month\"", "SELECT count(*) FROM expense_ai_usage WHERE company_id=? AND month=?", false],
  ["operations-leave-self-service-facts.ts", "ppRd32TwoColProjection", "\"id\",\"employees\",\"person_id\",\"company_id\"", "SELECT id FROM employees WHERE person_id=? AND company_id=?", false],
  ["department-override-facts.ts", "ppRd32TwoColProjection", "\"state\",\"overtime_requests\",\"id\",\"company_id\"", "SELECT state FROM overtime_requests WHERE id=? AND company_id=?", false],
  ["department-override-facts.ts", "ppRd32TwoColProjection", "\"version\",\"T\",\"id\",\"company_id\"", "SELECT version FROM T WHERE id=? AND company_id=?", true],
  ["people-payroll.ts", "ppRd32TwoColProjection", "\"1\",\"hr_evaluation_compensation_drafts\",\"compensation_id\",\"company_id\"", "SELECT 1 FROM hr_evaluation_compensation_drafts WHERE compensation_id=? AND company_id=?", false],
  ["people-payroll.ts", "ppRd32TwoColProjection", "\"id,pay_group_id,created_by,status,version\",\"hr_payroll_account_mappings\",\"id\",\"company_id\"", "SELECT id,pay_group_id,created_by,status,version FROM hr_payroll_account_mappings WHERE id=? AND company_id=?", false],
  ["people-payroll.ts", "ppRd32TwoColProjection", "\"submitted_by\",\"hr_payroll_runs\",\"id\",\"company_id\"", "SELECT submitted_by FROM hr_payroll_runs WHERE id=? AND company_id=?", false],
  ["people-payroll.ts", "ppRd32TwoColProjection", "\"id,status,version,revision,input_digest,totals_json\",\"hr_payroll_runs\",\"id\",\"company_id\"", "SELECT id,status,version,revision,input_digest,totals_json FROM hr_payroll_runs WHERE id=? AND company_id=?", false],
  ["people-payroll.ts", "ppRd32TwoColProjection", "\"id,status,revision,input_digest,currency,period_start,period_end\",\"hr_payroll_runs\",\"id\",\"company_id\"", "SELECT id,status,revision,input_digest,currency,period_start,period_end FROM hr_payroll_runs WHERE id=? AND company_id=?", false],
  ["people-payroll.ts", "ppRd32TwoColProjection", "\"checksum_sha256\",\"hr_payroll_review_exports\",\"run_id\",\"run_revision\"", "SELECT checksum_sha256 FROM hr_payroll_review_exports WHERE run_id=? AND run_revision=?", false],
  ["people-payroll.ts", "ppRd32TwoColProjection", "\"status,version,source_type,created_by,retry_requested_by\",\"hr_payment_batches\",\"id\",\"company_id\"", "SELECT status,version,source_type,created_by,retry_requested_by FROM hr_payment_batches WHERE id=? AND company_id=?", false],
  ["people-payroll.ts", "ppRd32TwoColProjection", "\"id,status,version,source_type,source_id,format_code,retry_requested_by\",\"hr_payment_batches\",\"id\",\"company_id\"", "SELECT id,status,version,source_type,source_id,format_code,retry_requested_by FROM hr_payment_batches WHERE id=? AND company_id=?", false],
  ["people-payroll.ts", "ppRd32TwoColProjection", "\"id,status,version,source_type,source_id,currency,bank_account_id\",\"hr_payment_batches\",\"id\",\"company_id\"", "SELECT id,status,version,source_type,source_id,currency,bank_account_id FROM hr_payment_batches WHERE id=? AND company_id=?", false],
  ["payroll-readiness.ts", "ppRd32TwoColProjection", "\"id,frequency,timezone,currency,schedule_enabled,pay_delay_days,schedule_anchor_date,schedule_version\",\"hr_pay_groups\",\"id\",\"company_id\"", "SELECT id,frequency,timezone,currency,schedule_enabled,pay_delay_days,schedule_anchor_date,schedule_version FROM hr_pay_groups WHERE id=? AND company_id=?", false],
  ["overtime.ts", "ppRd32TwoColProjection", "\"eligible\",\"overtime_eligibility\",\"company_id\",\"engagement_id\"", "SELECT eligible FROM overtime_eligibility WHERE company_id=? AND engagement_id=?", false],
  ["overtime.ts", "ppRd32TwoColProjection", "\"legal_name\",\"hr_people\",\"company_id\",\"user_id\"", "SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=?", false],
  ["overtime.ts", "ppRd32TwoColProjection", "\"version\",\"overtime_eligibility\",\"company_id\",\"engagement_id\"", "SELECT version FROM overtime_eligibility WHERE company_id=? AND engagement_id=?", false],
  ["hire-handoff-workspace.ts", "ppRd32TwoColProjection", "\"id,person_id,engagement_id,compensation_id,source_offer_version,source_candidate_version,created_at\",\"talent_hire_consumption_receipts\",\"handoff_id\",\"company_id\"", "SELECT id,person_id,engagement_id,compensation_id,source_offer_version,source_candidate_version,created_at FROM talent_hire_consumption_receipts WHERE handoff_id=? AND company_id=?", false],
  ["hire-handoff-workspace.ts", "ppRd32TwoColProjection", "\"application_id\",\"talent_offers\",\"id\",\"company_id\"", "SELECT application_id FROM talent_offers WHERE id=? AND company_id=?", false],
  ["clearance-document-sources.ts", "ppRd32TwoColProjection", "\"S status,L label\",\"T\",\"id\",\"company_id\"", "SELECT S status,L label FROM T WHERE id=? AND company_id=?", true],
  ["expense-ai-source.ts", "ppRd36TwoPredTail", "\"*\",\"finance_expenses\",\"id\",\"company_id\",\" AND workflow_version=2\"", "SELECT * FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=2", false],
  ["people-evaluation.ts", "ppRd36TwoPredTail", "\"id,source_type,source_id,description,observed_on,created_by,created_at\",\"hr_evaluation_evidence\",\"assessment_id\",\"company_id\",\" ORDER BY created_at,id\"", "SELECT id,source_type,source_id,description,observed_on,created_by,created_at FROM hr_evaluation_evidence WHERE assessment_id=? AND company_id=? ORDER BY created_at,id", false],
  ["people-evaluation.ts", "ppRd36TwoPredTail", "\"version,status,content_json,actor_id,reason,created_at,period_start,period_end,role_id,role_version\",\"hr_evaluation_assessment_history\",\"id\",\"company_id\",\" ORDER BY version DESC\"", "SELECT version,status,content_json,actor_id,reason,created_at,period_start,period_end,role_id,role_version FROM hr_evaluation_assessment_history WHERE id=? AND company_id=? ORDER BY version DESC", false],
  ["leave-self-service.ts", "ppRd36TwoPredTail", "\"1\",\"leave_holiday_calendars\",\"id\",\"company_id\",\" AND active=true\"", "SELECT 1 FROM leave_holiday_calendars WHERE id=? AND company_id=? AND active=true", false],
  ["expense-drafts.ts", "ppRd36TwoPredTail", "\"branch_id,department_id\",\"hr_engagements\",\"person_id\",\"company_id\",\" ORDER BY start_date DESC LIMIT 1\"", "SELECT branch_id,department_id FROM hr_engagements WHERE person_id=? AND company_id=? ORDER BY start_date DESC LIMIT 1", false],
  ["people-roster.ts", "ppRd36TwoPredTail", "\"user_id\",\"hr_people\",\"id\",\"company_id\",\" AND user_id IS NOT NULL\"", "SELECT user_id FROM hr_people WHERE id=? AND company_id=? AND user_id IS NOT NULL", false],
  ["people-roster.ts", "ppRd36TwoPredTail", "\"*\",\"people_roster_templates\",\"company_id\",\"department_id\",\" ORDER BY start_time,name\"", "SELECT * FROM people_roster_templates WHERE company_id=? AND department_id=? ORDER BY start_time,name", false],
  ["people-payroll.ts", "ppRd36TwoPredTail", "\"1\",\"staff_organization_history\",\"engagement_id\",\"company_id\",\" LIMIT 1\"", "SELECT 1 FROM staff_organization_history WHERE engagement_id=? AND company_id=? LIMIT 1", false],
  ["expense-routing.ts", "ppRd36TwoPredTail", "\"prepared_by,claimant_user_id,accounting_json\",\"finance_expenses\",\"id\",\"company_id\",\" AND workflow_version=2\"", "SELECT prepared_by,claimant_user_id,accounting_json FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=2", false],
  ["people-directory.ts", "ppRd36TwoPredTail", "\"1\",\"people_access_cutoffs\",\"company_id\",\"person_id\",\" AND cancelled_at IS NULL AND restored_at IS NULL\"", "SELECT 1 FROM people_access_cutoffs WHERE company_id=? AND person_id=? AND cancelled_at IS NULL AND restored_at IS NULL", false],
  ["operations-people-evaluation-facts.ts", "ppRd36TwoPredTail", "\"before_json,after_json,created_at\",\"staff_organization_history\",\"company_id\",\"engagement_id\",\" ORDER BY created_at,id\"", "SELECT before_json,after_json,created_at FROM staff_organization_history WHERE company_id=? AND engagement_id=? ORDER BY created_at,id", false],
];

describe('People Payroll read-tail r41 two-predicate dedupe', () => {
  test('both shared builders own byte-identical read-only literals', () => {
    expect(ppRd32TwoColProjection('id', 't', 'a', 'b')).toBe('SELECT id FROM t WHERE a=? AND b=?');
    expect(ppRd36TwoPredTail('id', 't', 'a', 'b')).toBe('SELECT id FROM t WHERE a=? AND b=?');
    expect(ppRd36TwoPredTail('id', 't', 'a', 'b', ' ORDER BY id')).toBe('SELECT id FROM t WHERE a=? AND b=? ORDER BY id');
    for (const [, sym, argsJson, expected] of f) {
      const args = JSON.parse('[' + argsJson + ']') as string[];
      const literal = sym === 'ppRd32TwoColProjection'
        ? ppRd32TwoColProjection(...(args as [string, string, string, string]))
        : ppRd36TwoPredTail(...(args as [string, string, string, string, string?]));
      expect(literal, sym + ' ' + argsJson).toBe(expected);
      readOnly(literal);
    }
    expect(f.length).toBe(35);
  });

  test('each caller uses the shared builder and keeps no local copy', () => {
    const expectedCounts: Record<string, number> = {
      "agent-ai-usage-projection.ts": 1,
      "base-attachment-purchases.ts": 1,
      "clearance-document-sources.ts": 1,
      "department-override-facts.ts": 2,
      "expense-ai-source.ts": 1,
      "expense-drafts.ts": 1,
      "expense-routing.ts": 1,
      "hire-handoff-pay-group.ts": 1,
      "hire-handoff-workspace.ts": 2,
      "leave-self-service.ts": 1,
      "operations-leave-self-service-facts.ts": 1,
      "operations-people-evaluation-facts.ts": 1,
      "overtime.ts": 3,
      "payroll-purchases.ts": 1,
      "payroll-readiness.ts": 1,
      "people-directory.ts": 1,
      "people-evaluation.ts": 2,
      "people-payroll.ts": 10,
      "people-purchases.ts": 1,
      "people-roster.ts": 2,
    };
    for (const [name, sym, argsJson, , dyn] of f) {
      if (dyn) continue;
      const call = sym + '(' + argsJson + ')';
      expect(src(name), name + ' call').toContain(call);
    }
  expect(src("department-override-facts.ts")).toContain("ppRd32TwoColProjection(\"version\",tables[kind],\"id\",\"company_id\")");
  expect(src("clearance-document-sources.ts")).toContain("ppRd32TwoColProjection(`${status} status,${label} label`,table,\"id\",\"company_id\")");
    for (const [name, expectedCount] of Object.entries(expectedCounts)) {
      const total = uses(name, 'ppRd32TwoColProjection') + uses(name, 'ppRd36TwoPredTail');
      expect(total, name).toBeGreaterThanOrEqual(expectedCount);
    }
  });

  test('lock-bearing reads, native kinds and r1-r40 builders stay untouched', () => {
    expect(src('pp-read-tail-r40-sources.ts')).toContain('export const ppRd40UnlinkedUserByCompany');
    expect(src('pp-read-tail-r39-sources.ts')).toContain('export const ppRd39NumberedThreePred');
    expect(src('pp-read-tail-r38-sources.ts')).toContain('export const ppRd38SinglePredTail');
    expect(src('pp-read-tail-r36-sources.ts')).toContain('export const ppRd36TwoPredTail');
    expect(src('pp-read-tail-r32-sources.ts')).toContain('export const ppRd32TwoColProjection');
    expect(src('overtime.ts')).toContain('FOR UPDATE');
    expect(src('payroll-purchases.ts')).toContain('FOR SHARE');
    expect(src('people-roster.ts')).toContain('FOR UPDATE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
  });

  test('worktree SELECT count is strictly lower than the recorded 1341 r40 baseline', () => {
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
    expect(total).toBeLessThan(1341);
    expect(total).toBe(1199);
  });
});
