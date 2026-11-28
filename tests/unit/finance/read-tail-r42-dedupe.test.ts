import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  accrualRunRowsSql,
  accrualScheduleRowsSql,
  bankImportMarkerExistsSql,
  billingCorrectionPointSql,
  chartPointSql,
  consolidationGroupAccountsSql,
  eliminationPendingSql,
  financeAiProposalPointSql,
  financeReferenceListSql,
  intercompanyPairPointSql,
  payrollPostPeriodSql,
} from '../../../modules/finance/server/src/adapters/finance-read-tail-r42-sql';

const srcRoot = resolve(__dirname, '../../../modules/finance/server/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const readRepo = (path: string) => readFileSync(resolve(__dirname, '../../..', path), 'utf8');
const helper = read('finance-read-tail-r42-sql.ts');
const count = (src: string, needle: string) => src.split(needle).length - 1;

const AI_POINT = 'SELECT * FROM finance_ai_proposals WHERE id=? AND company_id=?';
const PAYROLL_PERIOD = "SELECT id,start_date FROM finance_periods WHERE company_id=? AND state='open'";
const REF_ACCOUNTS = 'SELECT * FROM finance_accounts WHERE chart_id=? AND company_id=? ORDER BY code';
const REF_PERIODS = 'SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC';
const ACCRUAL_SCHEDULE = 'SELECT s.*,da.code debit_code,da.name debit_name,ca.code credit_code,ca.name credit_name FROM finance_accrual_schedules s JOIN finance_accounts da ON da.id=s.debit_account_id JOIN finance_accounts ca ON ca.id=s.credit_account_id ';
const ACCRUAL_RUN = 'SELECT r.*,s.description FROM finance_accrual_schedule_runs r JOIN finance_accrual_schedules s ON s.id=r.schedule_id ';
const CONSOL_ACCOUNTS = 'SELECT a.id,a.company_id,a.chart_id,a.code,a.name,a.currency,c.base_currency\n';
const ELIM_PENDING = 'SELECT 1 FROM finance_consolidation_eliminations WHERE run_id=? AND ';
const BILLING_POINT = 'SELECT * FROM finance_billing_corrections WHERE id=? AND company_id=?';
const PAIR_POINT = 'SELECT * FROM finance_intercompany_document_pairs WHERE id=? AND holding_id=?';
const CHART_POINT = 'SELECT id,base_currency,status FROM finance_charts WHERE id=? AND company_id=?';
const BANK_MARKER = "SELECT 1 FROM command_receipts WHERE operation='finance.bank.import' AND authority_json::jsonb->>'companyId'=? AND ";

describe('Finance read-tail r42 byte-identical NEW families', () => {
  test('AI proposal point reads render byte-identically at all three sites', () => {
    expect(financeAiProposalPointSql('')).toBe(AI_POINT);
    expect(financeAiProposalPointSql(" AND kind='invoice_review'")).toBe(`${AI_POINT} AND kind='invoice_review'`);
    expect(financeAiProposalPointSql(" AND kind='invoice_review' AND source_id=?")).toBe(`${AI_POINT} AND kind='invoice_review' AND source_id=?`);
    const src = read('finance-ai.ts');
    expect(count(src, 'financeAiProposalPointSql(')).toBe(3);
    expect(src).not.toContain(AI_POINT);
  });

  test('payroll post-period resolution renders byte-identically at all three sites', () => {
    expect(payrollPostPeriodSql(' AND start_date<=? AND end_date>=? ORDER BY start_date')).toBe(`${PAYROLL_PERIOD} AND start_date<=? AND end_date>=? ORDER BY start_date`);
    expect(payrollPostPeriodSql(' AND start_date>? ORDER BY start_date LIMIT 1')).toBe(`${PAYROLL_PERIOD} AND start_date>? ORDER BY start_date LIMIT 1`);
    expect(payrollPostPeriodSql(' AND start_date<=? AND end_date>=?')).toBe(`${PAYROLL_PERIOD} AND start_date<=? AND end_date>=?`);
    const src = read('payroll-owner-facts.ts');
    expect(count(src, 'payrollPostPeriodSql(')).toBe(3);
    expect(src).not.toContain(PAYROLL_PERIOD);
  });

  test('reference-list ORDER BY variants render byte-identically at all four sites', () => {
    expect(financeReferenceListSql('finance_accounts', 'chart_id=? AND company_id=?', 'code,id')).toBe(`${REF_ACCOUNTS},id`);
    expect(financeReferenceListSql('finance_accounts', 'chart_id=? AND company_id=?', 'code')).toBe(REF_ACCOUNTS);
    expect(financeReferenceListSql('finance_periods', 'company_id=?', 'start_date DESC,id')).toBe(`${REF_PERIODS},id`);
    expect(financeReferenceListSql('finance_periods', 'company_id=?', 'start_date DESC')).toBe(REF_PERIODS);
    expect(count(read('record-presentation.ts'), 'financeReferenceListSql(')).toBe(2);
    expect(count(read('ledger.ts'), 'financeReferenceListSql(')).toBe(2);
  });

  test('accrual list and detail render byte-identically across both schedule and run families', () => {
    expect(accrualScheduleRowsSql('WHERE s.company_id=? ORDER BY s.created_at DESC LIMIT 100')).toBe(`${ACCRUAL_SCHEDULE}WHERE s.company_id=? ORDER BY s.created_at DESC LIMIT 100`);
    expect(accrualScheduleRowsSql('WHERE s.id=? AND s.company_id=?')).toBe(`${ACCRUAL_SCHEDULE}WHERE s.id=? AND s.company_id=?`);
    expect(accrualRunRowsSql('WHERE r.company_id=? ORDER BY r.period_start DESC,r.id DESC LIMIT 200')).toBe(`${ACCRUAL_RUN}WHERE r.company_id=? ORDER BY r.period_start DESC,r.id DESC LIMIT 200`);
    expect(accrualRunRowsSql('WHERE r.id=? AND r.company_id=?')).toBe(`${ACCRUAL_RUN}WHERE r.id=? AND r.company_id=?`);
    const src = read('finance-accruals.ts');
    expect(count(src, 'accrualScheduleRowsSql(')).toBe(2);
    expect(count(src, 'accrualRunRowsSql(')).toBe(2);
    expect(src).not.toContain(ACCRUAL_SCHEDULE.trim());
  });

  test('consolidation group-accounts preserve exact indentation at both sites', () => {
    const tail6 = `      FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id\n      JOIN finance_group_entities e ON e.company_id=a.company_id AND e.holding_id=? AND e.status='active'\n      WHERE a.status='active' AND c.status='active' ORDER BY a.company_id,a.code`;
    const tail4 = `    FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id\n    JOIN finance_group_entities e ON e.company_id=a.company_id AND e.holding_id=? AND e.status='active'\n    WHERE a.id IN (?) AND a.status='active' AND c.status='active'`;
    expect(consolidationGroupAccountsSql(tail6)).toBe(`${CONSOL_ACCOUNTS}${tail6}`);
    expect(consolidationGroupAccountsSql(tail4)).toBe(`${CONSOL_ACCOUNTS}${tail4}`);
    expect(count(read('finance-consolidation.ts'), 'consolidationGroupAccountsSql(')).toBe(2);
  });

  test('elimination pending status variants render byte-identically', () => {
    expect(eliminationPendingSql("status='proposed'")).toBe(`${ELIM_PENDING}status='proposed' LIMIT 1`);
    expect(eliminationPendingSql("status<>'posted'")).toBe(`${ELIM_PENDING}status<>'posted' LIMIT 1`);
    expect(count(read('finance-consolidation.ts'), 'eliminationPendingSql(')).toBe(2);
  });

  test('billing-correction, intercompany-pair, chart-point and bank-marker pairs render byte-identically', () => {
    expect(billingCorrectionPointSql('')).toBe(BILLING_POINT);
    expect(billingCorrectionPointSql(" AND status='posted' AND reviewed_by=? AND replacement_bill_id=?")).toBe(`${BILLING_POINT} AND status='posted' AND reviewed_by=? AND replacement_bill_id=?`);
    expect(count(read('finance-billing-corrections.ts'), 'billingCorrectionPointSql(')).toBe(1);
    expect(count(read('finance-business-contracts.ts'), 'billingCorrectionPointSql(')).toBe(1);

    expect(intercompanyPairPointSql(' AND company_id=?')).toBe(`${PAIR_POINT} AND company_id=?`);
    expect(intercompanyPairPointSql(" AND status='confirmed'")).toBe(`${PAIR_POINT} AND status='confirmed'`);
    expect(count(read('finance-intercompany-pairs.ts'), 'intercompanyPairPointSql(')).toBe(1);
    expect(count(read('finance-group-editions.ts'), 'intercompanyPairPointSql(')).toBe(1);

    expect(chartPointSql('')).toBe(CHART_POINT);
    expect(chartPointSql(" AND status='active'")).toBe(`${CHART_POINT} AND status='active'`);
    expect(count(read('finance-business-contracts.ts'), 'chartPointSql(')).toBe(1);
    expect(count(read('finance-instruments.ts'), 'chartPointSql(')).toBe(1);

    expect(bankImportMarkerExistsSql("result_json::jsonb->'marker'->>'instructionId'")).toBe(`${BANK_MARKER}result_json::jsonb->'marker'->>'instructionId'=?`);
    expect(bankImportMarkerExistsSql("result_json::jsonb->>'refundInstructionId'")).toBe(`${BANK_MARKER}result_json::jsonb->>'refundInstructionId'=?`);
    expect(count(read('refund-bank-evidence.ts'), 'bankImportMarkerExistsSql(')).toBe(2);
  });

  test('each r42 family is defined exactly once and locks/tax/assertions/native/r15-r41 stay in place', () => {
    for (const name of [
      'financeAiProposalPointSql', 'payrollPostPeriodSql', 'financeReferenceListSql',
      'accrualScheduleRowsSql', 'accrualRunRowsSql', 'consolidationGroupAccountsSql',
      'eliminationPendingSql', 'billingCorrectionPointSql', 'intercompanyPairPointSql',
      'chartPointSql', 'bankImportMarkerExistsSql',
    ]) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
    expect(read('advisory-lock-dedupe-sql.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('pos-core-facts.ts')).toContain('FOR SHARE');
    expect(read('commerce-facts.ts')).toContain('FOR UPDATE');
    expect(read('finance-tax-register.ts')).toContain('SELECT');
    expect(read('finance.ts')).toContain('assertion(write, id, "credited-payment-source"');
    expect(readRepo('modules/collaboration/server/src/adapters/native-finance-billing-corrections.ts')).toContain('Native-owned finance billing correction statements.');
    expect(read('finance-read-tail-r15-sql.ts')).toContain('CHART_BASE_CURRENCY_BY_ID_SQL');
    expect(read('finance-read-tail-r32-sql.ts')).toContain('ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL');
    expect(read('finance-read-tail-r38-sql.ts')).toContain('ACCRUAL_SCHEDULE_POINT_SQL');
    expect(read('finance-read-tail-r40-sql.ts')).toContain('PAYMENT_FULL_ROW_SQL');
    expect(read('finance-read-tail-r41-sql.ts')).toContain('APPROVED_SECONDARY_CHART_CORRELATED_GUARD');
  });
});

describe('Finance read-tail r42 retirement check', () => {
  test('retirement:ts-statements stays at or below the r42 target of 1904', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThanOrEqual(1904);
    expect(total).toBeLessThan(1919);
  });
});
