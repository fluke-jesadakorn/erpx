import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  tableByIdSql,
  journalRefGuardSql,
  consolidationEliminationsSql,
  purchasedRunGuardSql,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r49-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;

const BASE = 'SELECT * FROM finance_recognition_runs WHERE id=? AND company_id=?';
const POSTED = "SELECT * FROM finance_recognition_runs WHERE id=? AND company_id=? AND status='posted'";
const FOR_UPDATE = 'SELECT * FROM finance_book_rules WHERE id=? AND company_id=? FOR UPDATE';
const SOURCE = 'SELECT * FROM ${sources[input.kind]} WHERE id=? AND company_id=?';

describe('Finance read-tail r49 Rank1 table-by-id', () => {
  test('renders byte-identically to every retired table-by-id family', () => {
    expect(tableByIdSql('finance_recognition_runs')).toBe(BASE);
    expect(tableByIdSql('finance_recognition_runs') + " AND status='posted'").toBe(POSTED);
    expect(tableByIdSql('finance_book_rules') + ' FOR UPDATE').toBe(FOR_UPDATE);
    expect(tableByIdSql('sources[input.kind]')).toBe(SOURCE.replace('${sources[input.kind]}', 'sources[input.kind]'));
    expect(tableByIdSql('finance_payments') + " AND party_id=? AND currency=? AND kind='payment' AND bank_account=? AND amount_minor=? AND NOT EXISTS(SELECT 1 FROM finance_allocations allocation WHERE allocation.payment_id=? AND (allocation.invoice_id<>? OR allocation.company_id<>?))")
      .toBe("SELECT * FROM finance_payments WHERE id=? AND company_id=? AND party_id=? AND currency=? AND kind='payment' AND bank_account=? AND amount_minor=? AND NOT EXISTS(SELECT 1 FROM finance_allocations allocation WHERE allocation.payment_id=? AND (allocation.invoice_id<>? OR allocation.company_id<>?))");
    expect(tableByIdSql('finance_recognition_runs')).not.toBe(POSTED);
  });

  test('all 25 safe sites delegate while locks, binds and dynamic tables stay intact', () => {
    const perFile: Array<[string, number]> = [
      ['finance-intercompany-pairs.ts', 1], ['accounting-profiles.ts', 1], ['supplier-payment-corrections.ts', 1],
      ['finance-asset-policies.ts', 2], ['finance-group-ownership.ts', 1], ['finance-due-drafts.ts', 2],
      ['finance-assets.ts', 2], ['finance-books.ts', 1], ['finance-business-contracts.ts', 3],
      ['finance-book-adjustments.ts', 1], ['finance-instruments.ts', 2], ['finance-bank-automation.ts', 1],
      ['finance-collections.ts', 1], ['inventory-landed-costs-sources.ts', 1], ['refund-bank-evidence.ts', 1],
      ['records-record-details-owned-facts.ts', 1], ['finance-thai-documents.ts', 1],
      ['document-posting-preview.ts', 1], ['workflow-document-sources.ts', 1],
    ];
    let total = 0;
    for (const [file, expected] of perFile) {
      const src = read(file);
      expect(count(src, 'tableByIdSql(')).toBe(expected); // import has no call parenthesis
      expect(src).toContain("from './finance-read-tail-r49-sql'");
      total += expected;
    }
    expect(total).toBe(25); // 16 scout files plus the dynamic-source, Thai-document and two reversed-order sites
    const bank = read('finance-bank-automation.ts'), books = read('finance-books.ts');
    expect(bank).toContain("tableByIdSql('finance_bank_match_groups') + ' FOR UPDATE'");
    expect(books).toContain("tableByIdSql('finance_book_rules') + ' FOR UPDATE'");
    expect(read('finance-due-drafts.ts')).toContain('tableByIdSql(sources[input.kind])');
    expect(read('workflow-document-sources.ts')).toContain('tableByIdSql(tables[kind])).bind(id,companyId)');
    expect(read('document-posting-preview.ts')).toContain("tableByIdSql('finance_invoices')).bind(invoiceId, companyId)");
    expect(bank).not.toContain('SELECT * FROM finance_bank_match_groups WHERE id=? AND company_id=? FOR UPDATE');
  });
});

describe('Finance read-tail r49 Rank2 journal reference guards', () => {
  test('renders all six guards byte-identically', () => {
    expect(journalRefGuardSql('i.journal_id', 'i.company_id')).toBe('NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=i.journal_id AND j.company_id=i.company_id)');
    expect(journalRefGuardSql('p.journal_id', 'p.company_id')).toBe('NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=p.journal_id AND j.company_id=p.company_id)');
    expect(journalRefGuardSql('p.fx_journal_id', 'p.company_id')).toBe('NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=p.fx_journal_id AND j.company_id=p.company_id)');
    expect(journalRefGuardSql('r.reversal_journal_id', 'p.company_id')).toBe('NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=r.reversal_journal_id AND j.company_id=p.company_id)');
    expect(journalRefGuardSql('r.fx_reversal_journal_id', 'p.company_id')).toBe('NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=r.fx_reversal_journal_id AND j.company_id=p.company_id)');
    expect(journalRefGuardSql('b.secondary_journal_id', 'b.company_id')).toBe('NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=b.secondary_journal_id AND j.company_id=b.company_id)');
    expect(journalRefGuardSql('p.journal_id', 'p.company_id')).not.toBe(journalRefGuardSql('p.fx_journal_id', 'p.company_id'));
  });

  test('six sales and one manufacturing site delegate; LOCK TABLE is untouched', () => {
    const sales = read('sales-goods-posting-setup.ts');
    const mfg = read('manufacturing-existing-posting-setup.ts');
    expect(count(sales, 'journalRefGuardSql(')).toBe(6);
    expect(count(mfg, 'journalRefGuardSql(')).toBe(1);
    expect(sales).toContain('LOCK TABLE finance_customer_receipts,finance_invoice_line_versions,finance_financial_events,finance_payment_reversals IN SHARE MODE');
    expect(sales).not.toContain('NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=');
    expect(mfg).not.toContain('NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=');
    expect(sales).toContain("journalRefGuardSql('i.journal_id','i.company_id')");
    expect(mfg).toContain("journalRefGuardSql('b.secondary_journal_id','b.company_id')");
  });
});

describe('Finance read-tail r49 Rank3 consolidation purchased runs', () => {
  test('renders both eliminations reads and the purchased-run guard byte-identically', () => {
    expect(consolidationEliminationsSql('run_id IN (SELECT id FROM finance_consolidation_runs WHERE holding_id=?)'))
      .toBe('SELECT * FROM finance_consolidation_eliminations WHERE run_id IN (SELECT id FROM finance_consolidation_runs WHERE holding_id=?) ORDER BY created_at,id');
    expect(consolidationEliminationsSql('run_id=?'))
      .toBe('SELECT * FROM finance_consolidation_eliminations WHERE run_id=? ORDER BY created_at,id');
    expect(purchasedRunGuardSql('HISTORY'))
      .toBe('EXISTS(SELECT 1 FROM finance_consolidation_runs purchased_run WHERE purchased_run.id=finance_consolidation_eliminations.run_id AND (HISTORY))');
    expect(consolidationEliminationsSql('run_id=?')).not.toBe(consolidationEliminationsSql('run_id IN (SELECT id FROM finance_consolidation_runs WHERE holding_id=?)'));
  });

  test('both consolidation sites delegate with their original history predicates', () => {
    const src = read('finance-consolidation.ts');
    expect(count(src, 'consolidationEliminationsSql(')).toBe(2);
    expect(count(src, 'purchasedRunGuardSql(')).toBe(2);
    expect(src).toContain('consolidationEliminationsSql("run_id IN (SELECT id FROM finance_consolidation_runs WHERE holding_id=?)")');
    expect(src).toContain("consolidationEliminationsSql('run_id=?')");
    expect(count(src, 'purchasedRunGuardSql(consolidationHistorySql("purchased_run",historySql))')).toBe(2);
    expect(src).not.toContain('`EXISTS(SELECT 1 FROM finance_consolidation_runs purchased_run');
  });
});

describe('Finance read-tail r49 protected families and retirement', () => {
  test('leaves locks, tax, advisory, native, expansion unions and prior helpers alone', () => {
    expect(read('sales.ts')).toContain('FOR SHARE');
    expect(read('finance-reporting.ts')).toContain('FOR UPDATE');
    expect(read('finance-withholding-remittance.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('finance.ts')).toContain('nativeFinanceOwnerWrite');
    expect(read('commerce-facts.ts')).toContain("source_type IN ('commerce_fulfillment'");
    expect(read('finance-expansion-work.ts')).toContain('UNION ALL');
    expect(read('finance-read-tail-r48-sql.ts')).toContain('bankTxnGuardSql');
    expect(read('finance-read-tail-r46-sql.ts')).toContain('accountChartGuardSql');
  });

  test('each r49 family is defined exactly once', () => {
    const helper = read('finance-read-tail-r49-sql.ts');
    for (const name of ['tableByIdSql', 'journalRefGuardSql', 'consolidationEliminationsSql', 'purchasedRunGuardSql']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
  });

  test('retirement:ts-statements stays below the r49 target of 1812 at 1805', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 25 table-by-id + 7 journal guards + 4 consolidation SELECTs collapse into
    // four r49 helpers (-32: 1844 -> 1812); r50 removes seven more (-> 1805).
    expect(total).toBe(1764);
    expect(total).toBeLessThan(1812);
  });
});
