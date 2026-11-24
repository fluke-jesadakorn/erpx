import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
import {
  APPROVED_SECONDARY_CHART_CORRELATED_GUARD,
  withholdingRemittedGuardSql,
  ORIGINAL_PAYMENT_REVERSAL_GUARD,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r41-sql';
import { financePaymentReversalSources } from '../../../packages/server-finance/src/adapters/supplier-payment-correction-sources';

vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const helper = read('finance-read-tail-r41-sql.ts');
const financeAssets = read('finance-assets.ts');
const finance = read('finance.ts');
const financeInstruments = read('finance-instruments.ts');
const financeInvoiceRead = read('finance-invoice-read.ts');
const whRemittance = read('finance-withholding-remittance.ts');
const withholding = read('finance-withholding.ts');
const salesReceipts = read('sales-customer-receipts.ts');
const salesReversal = read('sales-payment-reversal.ts');
const supplierCorrections = read('supplier-payment-corrections.ts');
const businessContracts = read('finance-business-contracts.ts');
const packageObligations = read('package-obligations.ts');
const supplyObligations = read('supply-package-obligations.ts');

const SEC_CHART_LITERAL = "NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved')";
const WITHHOLDING_LITERAL = "SELECT 1 FROM finance_withholding_remittance_items i JOIN finance_withholding_remittances r ON r.id=i.remittance_id WHERE i.assessment_id=";
const PAYMENT_REVERSAL_LITERAL = `AND (original.status='settled' OR EXISTS(SELECT 1 FROM (${financePaymentReversalSources}) reversal WHERE reversal.id=original.id AND reversal.company_id=original.company_id))`;

describe('Finance read-tail r41 byte-identical NEW families', () => {
  test('approved-secondary-chart correlated guard renders byte-identically at all five sites', () => {
    expect(APPROVED_SECONDARY_CHART_CORRELATED_GUARD).toBe(SEC_CHART_LITERAL);
    expect(financeAssets.split('${APPROVED_SECONDARY_CHART_CORRELATED_GUARD}').length - 1).toBe(1);
    expect(finance.split('${APPROVED_SECONDARY_CHART_CORRELATED_GUARD}').length - 1).toBe(2);
    expect(financeInstruments.split('${APPROVED_SECONDARY_CHART_CORRELATED_GUARD}').length - 1).toBe(1);
    expect(financeInvoiceRead.split('${APPROVED_SECONDARY_CHART_CORRELATED_GUARD}').length - 1).toBe(1);
    for (const src of [financeAssets, finance, financeInstruments, financeInvoiceRead]) {
      expect(src).not.toContain(SEC_CHART_LITERAL);
    }
    // Non-byte-identical correlated sibling stays inline (skipped).
    expect(businessContracts).toContain("NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=c.company_id AND r.secondary_chart_id=c.id AND r.status='approved')");
  });

  test('withholding confirmed guard renders byte-identically for both assessment refs', () => {
    expect(withholdingRemittedGuardSql('?')).toBe(`${WITHHOLDING_LITERAL}? AND r.status='confirmed'`);
    expect(withholdingRemittedGuardSql('a.id')).toBe(`${WITHHOLDING_LITERAL}a.id AND r.status='confirmed'`);
    expect(whRemittance.split("withholdingRemittedGuardSql('?')").length - 1).toBe(1);
    expect(whRemittance.split("withholdingRemittedGuardSql('a.id')").length - 1).toBe(1);
    expect(withholding.split("withholdingRemittedGuardSql('?')").length - 1).toBe(2);
    for (const src of [whRemittance, withholding]) {
      expect(src).not.toContain(WITHHOLDING_LITERAL);
    }
    // Non-byte-identical alias/ref sibling stays inline (skipped).
    expect(packageObligations).toContain("JOIN finance_withholding_remittances m ON m.id=i.remittance_id WHERE i.assessment_id=r.id AND m.status='confirmed'");
  });

  test('original payment-reversal existence guard renders byte-identically at four sites', () => {
    expect(ORIGINAL_PAYMENT_REVERSAL_GUARD).toBe(PAYMENT_REVERSAL_LITERAL);
    expect(ORIGINAL_PAYMENT_REVERSAL_GUARD).toContain('AND (original.status=\'settled\' OR EXISTS(SELECT 1 FROM (');
    expect(ORIGINAL_PAYMENT_REVERSAL_GUARD).toContain(') reversal WHERE reversal.id=original.id AND reversal.company_id=original.company_id))');
    expect(salesReceipts.split('${ORIGINAL_PAYMENT_REVERSAL_GUARD}').length - 1).toBe(2);
    expect(salesReversal.split('${ORIGINAL_PAYMENT_REVERSAL_GUARD}').length - 1).toBe(1);
    expect(supplierCorrections.split('${ORIGINAL_PAYMENT_REVERSAL_GUARD}').length - 1).toBe(1);
    for (const src of [salesReceipts, salesReversal, supplierCorrections]) {
      expect(src).not.toContain("AND (original.status='settled' OR EXISTS(SELECT 1 FROM (${financePaymentReversalSources}) reversal WHERE reversal.id=original.id AND reversal.company_id=original.company_id))");
    }
    // Non-byte-identical fifth sibling stays inline (skipped).
    expect(supplyObligations).toContain('EXISTS(SELECT 1 FROM (${financePaymentReversalSources})');
  });

  test('each r41 family is defined exactly once and r15-r40 helpers stay in place', () => {
    for (const name of ['APPROVED_SECONDARY_CHART_CORRELATED_GUARD', 'withholdingRemittedGuardSql', 'ORIGINAL_PAYMENT_REVERSAL_GUARD']) {
      expect(helper.split(`export const ${name} =`).length - 1).toBe(1);
    }
    expect(read('finance-read-tail-r15-sql.ts')).toContain("APPROVED_SECONDARY_CHART_RULE_SQL");
    expect(read('finance-read-tail-r40-sql.ts')).toContain('PAYMENT_FULL_ROW_SQL');
    expect(read('advisory-lock-dedupe-sql.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('finance-tax-register.ts')).toContain('SELECT');
  });
});

describe('Finance read-tail r41 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r41 BEFORE=1929 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThanOrEqual(1919);
    expect(total).toBeLessThan(1929);
  });
});
