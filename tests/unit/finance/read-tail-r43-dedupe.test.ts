import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  journalLineCountSql,
  journalLineNetSql,
  BOOKS_AGGREGATE_IMAGE_SQL,
  childBillingRunExistsSql,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r43-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const helper = read('finance-read-tail-r43-sql.ts');

const assets = read('assets-package-obligations.ts');
const benefits = read('benefits-package-obligations.ts');
const crm = read('crm-analytics-sources.ts');
const manufacturing = read('manufacturing-package-obligations.ts');
const supplier = read('supplier-payment-correction-sources.ts');
const booksAssets = read('assets-financial-sources.ts');
const booksExpense = read('expense-posting-setup-sources.ts');
const booksManufacturing = read('manufacturing-existing-posting-setup.ts');
const attachments = read('attachments-finance-facts.ts');
const contracts = read('finance-business-contracts.ts');
const expansion = read('finance-expansion-work.ts');
const obligations = read('package-obligations.ts');

const COUNT_HEAD = 'SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=';
const NET_HEAD = 'SELECT sum(line.debit_minor-line.credit_minor) FROM finance_journal_lines line WHERE line.journal_id=';
const BOOKS_LITERAL = 'SELECT jsonb_agg(to_jsonb(b) ORDER BY b.primary_journal_id,b.secondary_journal_id) FROM books b';
const CHILD_HEAD = 'SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=';

describe('Finance read-tail r43 byte-identical NEW families', () => {
  test('journal-line count probes render byte-identically at all twelve sites', () => {
    expect(journalLineCountSql('journal.id')).toBe(`${COUNT_HEAD}journal.id`);
    expect(journalLineCountSql('secondary.id')).toBe(`${COUNT_HEAD}secondary.id`);
    expect(journalLineCountSql('j.id')).toBe(`${COUNT_HEAD}j.id`);
    expect(journalLineCountSql('original.id')).toBe(`${COUNT_HEAD}original.id`);
    expect(journalLineCountSql('correction.id')).toBe(`${COUNT_HEAD}correction.id`);
    expect(journalLineCountSql('original_secondary.id')).toBe(`${COUNT_HEAD}original_secondary.id`);
    expect(journalLineCountSql('correction_secondary.id')).toBe(`${COUNT_HEAD}correction_secondary.id`);
    expect(assets.split('${journalLineCountSql(').length - 1).toBe(2);
    expect(benefits.split('${journalLineCountSql(').length - 1).toBe(1);
    expect(crm.split('${journalLineCountSql(').length - 1).toBe(3);
    expect(manufacturing.split('${journalLineCountSql(').length - 1).toBe(1);
    expect(supplier.split('${journalLineCountSql(').length - 1).toBe(5);
    for (const src of [assets, benefits, crm, manufacturing, supplier]) {
      expect(src).not.toContain(COUNT_HEAD);
    }
    // Non-byte-identical uppercase/alias siblings stay inline (skipped).
    expect(read('finance-read-tail-r23-sql.ts')).toContain('SELECT COUNT(*) FROM finance_journal_lines l WHERE l.journal_id=j.id');
    expect(read('ledger.ts')).toContain('SELECT COUNT(*) FROM finance_journal_lines WHERE journal_id=j.id');
    expect(read('payroll-other-remittance-obligations.ts')).toContain('SELECT COUNT(*) FROM finance_journal_lines line WHERE line.journal_id=j.id');
    expect(read('people-package-obligations.ts')).toContain('SELECT COUNT(*) FROM finance_journal_lines l WHERE l.journal_id=j.id');
  });

  test('journal-line net probes render byte-identically at the four unsuffixed sites', () => {
    expect(journalLineNetSql('journal.id')).toBe(`${NET_HEAD}journal.id`);
    expect(journalLineNetSql('secondary.id')).toBe(`${NET_HEAD}secondary.id`);
    expect(assets.split('${journalLineNetSql(').length - 1).toBe(2);
    expect(crm.split('${journalLineNetSql(').length - 1).toBe(1);
    expect(manufacturing.split('${journalLineNetSql(').length - 1).toBe(1);
    for (const src of [assets, crm, manufacturing]) {
      expect(src).not.toContain(NET_HEAD);
    }
    // Non-byte-identical account-scoped siblings stay inline (skipped).
    expect(read('bank-statement-settlement.ts')).toContain(`${NET_HEAD}journal.id AND line.account_id=r.bank_account`);
  });

  test('books aggregate image renders byte-identically at all three sites', () => {
    expect(BOOKS_AGGREGATE_IMAGE_SQL).toBe(BOOKS_LITERAL);
    expect(booksAssets.split('${BOOKS_AGGREGATE_IMAGE_SQL}').length - 1).toBe(1);
    expect(booksExpense.split('${BOOKS_AGGREGATE_IMAGE_SQL}').length - 1).toBe(1);
    expect(booksManufacturing.split('${BOOKS_AGGREGATE_IMAGE_SQL}').length - 1).toBe(1);
    for (const src of [booksAssets, booksExpense, booksManufacturing]) {
      expect(src).not.toContain(BOOKS_LITERAL);
    }
  });

  test('child billing-run existence guard renders byte-identically at all five sites', () => {
    expect(childBillingRunExistsSql('finance_business_billing_runs.id')).toBe(`${CHILD_HEAD}finance_business_billing_runs.id`);
    expect(childBillingRunExistsSql('b.id')).toBe(`${CHILD_HEAD}b.id`);
    expect(childBillingRunExistsSql('r.id')).toBe(`${CHILD_HEAD}r.id`);
    expect(attachments.split('${childBillingRunExistsSql(').length - 1).toBe(2);
    expect(contracts.split('${childBillingRunExistsSql(').length - 1).toBe(1);
    expect(expansion.split('${childBillingRunExistsSql(').length - 1).toBe(1);
    expect(obligations.split('${childBillingRunExistsSql(').length - 1).toBe(1);
    expect(attachments).toContain("`no_charge_status='draft' AND NOT EXISTS(${childBillingRunExistsSql('finance_business_billing_runs.id')})`");
    expect(obligations).toContain("`r.no_charge_status='draft' AND NOT EXISTS(${childBillingRunExistsSql('r.id')})`");
    for (const src of [attachments, contracts, expansion, obligations]) {
      expect(src).not.toContain(CHILD_HEAD);
    }
    // Non-byte-identical parent-run guard from r38 stays inline (skipped).
    expect(read('finance-read-tail-r38-sql.ts')).toContain('SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?');
    expect(read('finance-billing-corrections.ts')).toContain('BILLING_RUN_CHILD_EXISTS_SQL');
  });

  test('each r43 family is defined exactly once and r15-r42 helpers stay in place', () => {
    for (const name of ['journalLineCountSql', 'journalLineNetSql', 'BOOKS_AGGREGATE_IMAGE_SQL', 'childBillingRunExistsSql']) {
      expect(helper.split(`export const ${name} =`).length - 1).toBe(1);
    }
    expect(read('finance-read-tail-r15-sql.ts')).toContain('OPEN_PERIOD_LIST_SQL');
    expect(read('finance-read-tail-r22-sql.ts')).toContain('journalLineEvidenceSql');
    expect(read('finance-read-tail-r38-sql.ts')).toContain('BILLING_RUN_POINT_SQL');
    expect(read('finance-read-tail-r41-sql.ts')).toContain('APPROVED_SECONDARY_CHART_CORRELATED_GUARD');
    expect(read('finance-read-tail-r42-sql.ts')).toContain('eliminationPendingSql');
    expect(read('advisory-lock-dedupe-sql.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('finance-business-contracts.ts')).toContain('idCompanyStatusVersionGuard');
    expect(read('finance-tax-register.ts')).toContain('SELECT');
    expect(assets).toContain('LOCK TABLE finance_assets');
  });
});

describe('Finance read-tail r43 retirement check', () => {
  test('retirement:ts-statements stays strictly below the re-measured BEFORE=1904 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Twelve count + four net + three books + five child sites collapse into
    // four helpers: 24 inline statements - 4 helper statements = -20 (<= -18).
    expect(total).toBeLessThanOrEqual(1886);
    expect(total).toBeLessThan(1904);
  });
});
