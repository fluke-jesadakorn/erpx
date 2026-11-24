import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd48FinanceExpenseVersionGuard,
  ppRd48IdCompanyVersionPairGuard,
  ppRd48ExpenseClaimInvoice,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r48-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

const invoiceProjection = 'SELECT "id","company_id","party_id","kind","invoice_number","invoice_date","due_date","currency","subtotal_minor","tax_minor","total_minor","outstanding_minor","tax_code","debit_account_id","credit_account_id","source_type","source_id","status","journal_id","version","created_by","approved_by","posted_at","created_at","updated_at","tax_account_id","fx_rate_micros","base_total_minor","base_outstanding_minor","line_mode","line_revision" FROM finance_invoices';
const invoiceSql = `SELECT * FROM (${invoiceProjection}) finance_invoices WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=?`;

const render = {
  ppRd48FinanceExpenseVersionGuard: (a: string[]) => ppRd48FinanceExpenseVersionGuard(a[0]),
  ppRd48IdCompanyVersionPairGuard: (a: string[]) => ppRd48IdCompanyVersionPairGuard(a[0], a[1], a[2]),
  ppRd48ExpenseClaimInvoice: (_a: string[]) => ppRd48ExpenseClaimInvoice(),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; args: string[]; literal: string; call: string };

// Each literal is copied verbatim from the pre-r48 inline call site. Byte-identity
// oracles only; they are never live query sources. No bind was introduced or moved.
const oracles: Row[] = [
  {
    file: 'expense-workflow.ts',
    sym: 'ppRd48FinanceExpenseVersionGuard',
    args: ["workflow_stage='accounts_review' AND cancelled_at IS NULL"],
    literal: "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND workflow_stage='accounts_review' AND cancelled_at IS NULL)",
    call: `ppRd48FinanceExpenseVersionGuard("workflow_stage='accounts_review' AND cancelled_at IS NULL")`,
  },
  {
    file: 'expense-workflow.ts',
    sym: 'ppRd48FinanceExpenseVersionGuard',
    args: ["status='approved' AND workflow_stage='gl_posting' AND journal_id IS NULL"],
    literal: "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND status='approved' AND workflow_stage='gl_posting' AND journal_id IS NULL)",
    call: `ppRd48FinanceExpenseVersionGuard("status='approved' AND workflow_stage='gl_posting' AND journal_id IS NULL")`,
  },
  {
    file: 'expense-workflow.ts',
    sym: 'ppRd48FinanceExpenseVersionGuard',
    args: ["status='in_review' AND workflow_stage='accounts_approval' AND cancelled_at IS NULL"],
    literal: "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND status='in_review' AND workflow_stage='accounts_approval' AND cancelled_at IS NULL)",
    call: `ppRd48FinanceExpenseVersionGuard("status='in_review' AND workflow_stage='accounts_approval' AND cancelled_at IS NULL")`,
  },
  {
    file: 'expense-workflow.ts',
    sym: 'ppRd48FinanceExpenseVersionGuard',
    args: ["status='in_review' AND cancelled_at IS NULL"],
    literal: "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND status='in_review' AND cancelled_at IS NULL)",
    call: `ppRd48FinanceExpenseVersionGuard("status='in_review' AND cancelled_at IS NULL")`,
  },
  {
    file: 'expense-payments.ts',
    sym: 'ppRd48IdCompanyVersionPairGuard',
    args: ['finance_expenses', "status='posted'", "workflow_stage='payment'"],
    literal: "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND status='posted' AND workflow_stage='payment')",
    call: `ppRd48IdCompanyVersionPairGuard('finance_expenses', "status='posted'", "workflow_stage='payment'")`,
  },
  {
    file: 'payroll-statutory.ts',
    sym: 'ppRd48IdCompanyVersionPairGuard',
    args: ['hr_engagements', "currency='THB'", "worker_type='employee'"],
    literal: "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND currency='THB' AND worker_type='employee')",
    call: `ppRd48IdCompanyVersionPairGuard('hr_engagements', "currency='THB'", "worker_type='employee'")`,
  },
  {
    file: 'expense-payments.ts',
    sym: 'ppRd48ExpenseClaimInvoice',
    args: [],
    literal: invoiceSql,
    call: 'ppRd48ExpenseClaimInvoice()',
  },
  {
    file: 'expenses.ts',
    sym: 'ppRd48ExpenseClaimInvoice',
    args: [],
    literal: invoiceSql,
    call: 'ppRd48ExpenseClaimInvoice()',
  },
];

describe('People Payroll read-tail r48 three-family dedupe', () => {
  test('the three shared builders own byte-identical, read-only literals', () => {
    expect(ppRd48FinanceExpenseVersionGuard('x')).toBe('EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND x)');
    expect((ppRd48FinanceExpenseVersionGuard('x').match(/\?/g) ?? []).length).toBe(3);
    expect(ppRd48IdCompanyVersionPairGuard('hr_engagements', 'a', 'b')).toBe('EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND a AND b)');
    expect((ppRd48IdCompanyVersionPairGuard('hr_engagements', 'a', 'b').match(/\?/g) ?? []).length).toBe(3);
    expect(ppRd48ExpenseClaimInvoice()).toBe(invoiceSql);
    expect((ppRd48ExpenseClaimInvoice().match(/\?/g) ?? []).length).toBe(3);
    for (const o of oracles) {
      expect(render[o.sym](o.args), o.file + ' ' + o.call).toBe(o.literal);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(8);
  });

  test('each caller uses its builder, keeps no inline head and keeps its binds and labels', () => {
    expect((src('pp-read-tail-r48-sources.ts').match(/export const ppRd48/g) ?? []).length).toBe(3);
    const expectedCounts: Record<string, [Sym, number][]> = {
      'expense-workflow.ts': [['ppRd48FinanceExpenseVersionGuard', 4]],
      'expense-payments.ts': [['ppRd48IdCompanyVersionPairGuard', 1], ['ppRd48ExpenseClaimInvoice', 1]],
      'payroll-statutory.ts': [['ppRd48IdCompanyVersionPairGuard', 1]],
      'expenses.ts': [['ppRd48ExpenseClaimInvoice', 1]],
    };
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.literal);
      expect(src(o.file), o.file + ' imports r48 builders').toContain("from './pp-read-tail-r48-sources'");
    }
    for (const [name, pairs] of Object.entries(expectedCounts)) {
      for (const [sym, count] of pairs) expect(uses(name, sym), name + ' ' + sym).toBe(count);
    }
    const wf = src('expense-workflow.ts');
    expect(wf).toContain(`"accounting-version",\n        ppRd48FinanceExpenseVersionGuard("workflow_stage='accounts_review' AND cancelled_at IS NULL"),\n        [id, actor.companyId, input.expectedVersion],`);
    expect(wf).toContain(`assertion(write,key,"expense-version",ppRd48FinanceExpenseVersionGuard("status='approved' AND workflow_stage='gl_posting' AND journal_id IS NULL"),[id,actor.companyId,input.expectedVersion])`);
    expect(wf).toContain(`assertion(write, key, "expense-version", ppRd48FinanceExpenseVersionGuard("status='in_review' AND workflow_stage='accounts_approval' AND cancelled_at IS NULL"), [id, actor.companyId, input.expectedVersion])`);
    expect(wf).toContain(`"expense-version",\n        ppRd48FinanceExpenseVersionGuard("status='in_review' AND cancelled_at IS NULL"),\n        [id, actor.companyId, input.expectedVersion],`);
    const pay = src('expense-payments.ts');
    expect(pay).toContain(`"payment-claim-version",\n        ppRd48IdCompanyVersionPairGuard('finance_expenses', "status='posted'", "workflow_stage='payment'"),\n        [id, actor.companyId, expectedVersion],`);
    expect(pay).toContain('write.prepare(ppRd48ExpenseClaimInvoice()).bind(expense.invoice_id, actor.companyId, expense.id)');
    expect(src('payroll-statutory.ts')).toContain(`ppRd48IdCompanyVersionPairGuard('hr_engagements', "currency='THB'", "worker_type='employee'"), [engagementId, companyId, expectedEngagementVersion]`);
    expect(src('expenses.ts')).toContain('write.prepare(ppRd48ExpenseClaimInvoice()).bind(claim.invoice_id, actor.companyId, id)');
  });

  test('locks, advisory, native kinds, cross-owner projections and r1-r47 builders stay untouched', () => {
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('SELECT id FROM hr_time_entries WHERE id=? FOR SHARE');
    expect(src('payroll-statutory.ts')).toContain('lockProfile(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")');
    expect(src('payroll-statutory.ts')).toContain('lockYtd(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")');
    expect(src('payroll-statutory.ts')).toContain('guardRetainedYtdItem(key: string, value: string, id: string, run_id: string, run_version: number, input_digest: string, result_json: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id WHERE i.id=? AND i.run_id=? AND r.version=? AND r.status=\'posted\' AND r.input_digest=? AND i.result_json=?)"');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('financeProjectionSources.finance_accounts');
    expect(src('expenses.ts')).toContain("financeProjectionSources.finance_journals}) finance_journals WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=? AND status='posted'");
    expect(src('expenses.ts')).toContain('`EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN (${states.map(() => "?").join(",")}))`');
    expect(src('pp-read-tail-r47-sources.ts')).toContain('export const ppRd47GraphSnapshotProjection');
    expect(src('pp-read-tail-r46-sources.ts')).toContain('export const ppRd46OwnerFileProbe');
    expect(src('pp-read-tail-r48-sources.ts')).toContain('hospitalityOwnerProjectionSources as financeProjectionSources');
    expect(src('pp-read-tail-r48-sources.ts')).toContain('${financeProjectionSources.finance_invoices}');
    for (const name of ['expense-workflow.ts', 'expense-payments.ts', 'payroll-statutory.ts', 'expenses.ts']) {
      expect(src(name), name + ' has no foreign r48 symbol').not.toMatch(/ppRd48(?!FinanceExpenseVersionGuard|IdCompanyVersionPairGuard|ExpenseClaimInvoice)/);
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1245', () => {
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
    expect(total).toBeLessThan(1245);
    expect(total).toBe(1224);
  });
});
