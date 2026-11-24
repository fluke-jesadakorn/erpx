import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd43ActiveLeavePolicyProjection,
  ppRd43AiReservationProjection,
  ppRd43DraftPaymentProjection,
  ppRd43EffectiveCutoffProjection,
  ppRd43EmployeeEngagementProjection,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r43-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

const render = {
  ppRd43AiReservationProjection: (cols: string) => ppRd43AiReservationProjection(cols),
  ppRd43EmployeeEngagementProjection: (cols: string) => ppRd43EmployeeEngagementProjection(cols),
  ppRd43DraftPaymentProjection: (cols: string) => ppRd43DraftPaymentProjection(cols),
  ppRd43EffectiveCutoffProjection: (cols: string) => ppRd43EffectiveCutoffProjection(cols),
  ppRd43ActiveLeavePolicyProjection: (cols: string) => ppRd43ActiveLeavePolicyProjection(cols),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; cols: string; literal: string; call: string; absent: string; binds: string };

// Each literal is copied verbatim from the pre-r43 inline call site; bind arrays are unchanged.
const oracles: Row[] = [
  {
    file: 'finance-ai-quota-sources.ts',
    sym: 'ppRd43AiReservationProjection',
    cols: "company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')",
    literal: "SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL",
    call: "ppRd43AiReservationProjection(\"company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')\")",
    absent: 'FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL',
    binds: 'financeAiExpenseUsageSql',
  },
  {
    file: 'finance-ai-quota-sources.ts',
    sym: 'ppRd43AiReservationProjection',
    cols: "company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD'),to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')",
    literal: "SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD'),to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL",
    call: "ppRd43AiReservationProjection(`company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD'),to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')`)",
    absent: 'FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL',
    binds: 'financeAiExpenseDatedReservationsSql(timezoneSql:string)',
  },
  {
    file: 'finance-ai-quota-sources.ts',
    sym: 'ppRd43AiReservationProjection',
    cols: "company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD')",
    literal: "SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL",
    call: "ppRd43AiReservationProjection(`company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD')`)",
    absent: 'FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL',
    binds: 'financeAiExpenseDailyReservationsSql(timezoneSql:string)',
  },
  {
    file: 'expense-ai.ts',
    sym: 'ppRd43AiReservationProjection',
    cols: "company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')",
    literal: "SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL",
    call: "ppRd43AiReservationProjection(\"company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')\")",
    absent: 'FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL',
    binds: 'business_date=?)<?',
  },
  {
    file: 'expense-ai.ts',
    sym: 'ppRd43AiReservationProjection',
    cols: "company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')",
    literal: "SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL",
    call: "ppRd43AiReservationProjection(\"company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')\")",
    absent: 'FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL',
    binds: 'month=?)<?',
  },
  {
    file: 'payroll-readiness.ts',
    sym: 'ppRd43EmployeeEngagementProjection',
    cols: 'e.id,e.start_date,e.end_date,p.id person_id,p.legal_name',
    literal: "SELECT e.id,e.start_date,e.end_date,p.id person_id,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id",
    call: "ppRd43EmployeeEngagementProjection('e.id,e.start_date,e.end_date,p.id person_id,p.legal_name')",
    absent: 'e.start_date,e.end_date,p.id person_id,p.legal_name FROM hr_engagements',
    binds: '.bind(company_id, pay_group_id, currency, period_end, period_start, scope_engagement_id, scope_engagement_id2)',
  },
  {
    file: 'payroll-readiness.ts',
    sym: 'ppRd43EmployeeEngagementProjection',
    cols: 'e.id',
    literal: "SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id",
    call: "ppRd43EmployeeEngagementProjection('e.id')",
    absent: 'SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id',
    binds: '.bind(company_id, pay_group_id, currency, period_end, period_start, scope_engagement_id, scope_engagement_id2)',
  },
  {
    file: 'expense-ai-source.ts',
    sym: 'ppRd43DraftPaymentProjection',
    cols: '1',
    literal: "SELECT 1 FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'",
    call: "ppRd43DraftPaymentProjection('1')",
    absent: "FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'",
    binds: '.bind(paymentId, claim.id, actor.companyId)',
  },
  {
    file: 'expense-ai-source.ts',
    sym: 'ppRd43DraftPaymentProjection',
    cols: '*',
    literal: "SELECT * FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'",
    call: "ppRd43DraftPaymentProjection('*')",
    absent: "FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'",
    binds: '.bind(paymentId, id, companyId)',
  },
  {
    file: 'technology-provisioning-facts.ts',
    sym: 'ppRd43EffectiveCutoffProjection',
    cols: '1',
    literal: 'SELECT 1 FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3',
    call: "ppRd43EffectiveCutoffProjection('1')",
    absent: 'FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2',
    binds: '[actor.companyId, e.person_id, observedNow]',
  },
  {
    file: 'technology-provisioning-facts.ts',
    sym: 'ppRd43EffectiveCutoffProjection',
    cols: 'max(effective_at) at',
    literal: 'SELECT max(effective_at) at FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3',
    call: "ppRd43EffectiveCutoffProjection('max(effective_at) at')",
    absent: 'FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2',
    binds: '[op.company_id, plan.personId, observedNow]',
  },
  {
    file: 'people-hr-settings.ts',
    sym: 'ppRd43ActiveLeavePolicyProjection',
    cols: 'id,name,version,effective_from',
    literal: "SELECT id,name,version,effective_from FROM leave_policy_versions WHERE company_id=? AND status='active' ORDER BY name,version DESC",
    call: "ppRd43ActiveLeavePolicyProjection('id,name,version,effective_from')",
    absent: "FROM leave_policy_versions WHERE company_id=? AND status='active'",
    binds: "policies: (companyId: string) => write.prepare",
  },
  {
    file: 'flow-settings-facts.ts',
    sym: 'ppRd43ActiveLeavePolicyProjection',
    cols: 'name,version,status',
    literal: "SELECT name,version,status FROM leave_policy_versions WHERE company_id=? AND status='active' ORDER BY name,version DESC",
    call: "ppRd43ActiveLeavePolicyProjection('name,version,status')",
    absent: "FROM leave_policy_versions WHERE company_id=? AND status='active'",
    binds: '.bind(companyId).all<{ name: string; version: number; status: string }>()',
  },
];

describe('People Payroll read-tail r43 five-family dedupe', () => {
  test('the five shared builders own byte-identical, read-only literals', () => {
    expect(ppRd43AiReservationProjection('id')).toBe('SELECT id FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL');
    expect(ppRd43EmployeeEngagementProjection('id')).toBe("SELECT id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id");
    expect(ppRd43DraftPaymentProjection('id')).toBe("SELECT id FROM expense_payment_drafts WHERE id=? AND expense_id=? AND company_id=? AND status='draft'");
    expect(ppRd43EffectiveCutoffProjection('id')).toBe('SELECT id FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3');
    expect(ppRd43ActiveLeavePolicyProjection('id')).toBe("SELECT id FROM leave_policy_versions WHERE company_id=? AND status='active' ORDER BY name,version DESC");
    for (const o of oracles) {
      expect(render[o.sym](o.cols), o.file + ' ' + o.sym).toBe(o.literal);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(13);
  });

  test('each caller uses its builder, keeps no inline copy and keeps its binds', () => {
    const shared = src('pp-read-tail-r43-sources.ts');
    expect((shared.match(/export const ppRd43/g) ?? []).length).toBe(5);
    const expectedCounts: Record<string, [Sym, number]> = {
      'finance-ai-quota-sources.ts': ['ppRd43AiReservationProjection', 3],
      'expense-ai.ts': ['ppRd43AiReservationProjection', 2],
      'payroll-readiness.ts': ['ppRd43EmployeeEngagementProjection', 2],
      'expense-ai-source.ts': ['ppRd43DraftPaymentProjection', 2],
      'technology-provisioning-facts.ts': ['ppRd43EffectiveCutoffProjection', 2],
      'people-hr-settings.ts': ['ppRd43ActiveLeavePolicyProjection', 1],
      'flow-settings-facts.ts': ['ppRd43ActiveLeavePolicyProjection', 1],
    };
    for (const o of oracles) {
      const text = src(o.file);
      expect(text, o.file + ' builder call').toContain(o.call);
      expect(text, o.file + ' inline copy removed').not.toContain(o.absent);
      expect(text, o.file + ' binds unchanged').toContain(o.binds);
    }
    for (const [name, [sym, count]] of Object.entries(expectedCounts)) {
      expect(uses(name, sym), name).toBe(count);
    }
  });

  test('locks, native kinds, cross-owner projections and r1-r42 builders stay untouched', () => {
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('payroll-purchases.ts')).toContain('FOR SHARE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('expense-ai.ts')).toContain('financeProjectionSources.finance_ai_usage');
    expect(src('expense-ai.ts')).toContain('financeProjectionSources.finance_ai_policies');
    expect(src('pp-read-tail-r42-sources.ts')).toContain('export const ppRd42FourColOwnerProjection');
    expect(src('pp-read-tail-r42-sources.ts')).toContain('export const ppRd42IdVersionLiteralGuard');
    expect(src('pp-read-tail-r40-sources.ts')).toContain('export const ppRd40UnlinkedUserByCompany');
    expect(src('pp-read-tail-r38-sources.ts')).toContain('export const ppRd38SinglePredTail');
    expect(src('pp-read-tail-r36-sources.ts')).toContain('export const ppRd36TwoPredTail');
    expect(src('pp-read-tail-r32-sources.ts')).toContain('export const ppRd32TwoColProjection');
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1303', () => {
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
    expect(total).toBeLessThan(1303);
    expect(total).toBe(1272);
  });
});
