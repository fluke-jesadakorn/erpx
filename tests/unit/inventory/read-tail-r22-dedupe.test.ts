import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { journalLineSumSql, sourceExistsSql } from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const expectedSourceExists = (rows: string, alias: string, id: string, company: string) =>
  `EXISTS(SELECT 1 FROM (${rows}) ${alias} WHERE ${alias}.id=${id} AND ${alias}.company_id=${company})`;
const expectedJournalSum = (column: string) =>
  `(SELECT sum(${column}) FROM finance_journal_lines WHERE journal_id=j.id)`;

describe('Inventory read-tail r22 parameterized EXISTS/agg-family dedupe', () => {
  test('canonical helpers render byte-identical read-only SQL', () => {
    expect(sourceExistsSql('<rows>', 'source', 'c.id', 'c.company_id')).toBe(
      expectedSourceExists('<rows>', 'source', 'c.id', 'c.company_id'));
    expect(sourceExistsSql('<rows>', 'actual_cost', 'count.id', 'count.company_id')).toBe(
      expectedSourceExists('<rows>', 'actual_cost', 'count.id', 'count.company_id'));
    expect(journalLineSumSql('debit_minor')).toBe(expectedJournalSum('debit_minor'));
    expect(journalLineSumSql('credit_minor')).toBe(expectedJournalSum('credit_minor'));
    for (const sql of [
      expectedSourceExists('<rows>', 'source', 'c.id', 'c.company_id'),
      expectedJournalSum('debit_minor'),
      expectedJournalSum('credit_minor'),
    ]) readOnly(sql);
  });

  test('r22 helper literals are defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('sourceExistsSql =')).toHaveLength(2);
    expect(shared.split('journalLineSumSql =')).toHaveLength(2);
  });

  test('source-proof EXISTS guards are composed, never inlined, in the five consumers', () => {
    const mfg = src('manufacturing-package-obligations.ts');
    const pos = src('pos-package-obligations.ts');
    const sales = src('sales-package-obligations.ts');
    const landed = src('inventory-landed-costs.ts');
    expect(mfg).not.toContain('FROM (${manufacturingInitialPhysicalStockCountSourceRows}) source WHERE');
    expect(pos).not.toContain('FROM (${posInitialPhysicalStockCountSourceRows}) source WHERE');
    expect(sales).not.toContain('FROM (${qualityDisposalCountRows}) source WHERE');
    expect(sales).not.toContain('FROM (${initialPhysicalStockCountSourceRows}) source WHERE');
    expect(landed).not.toContain('FROM (${landedCostPurchaseSourceRows}) actual_cost WHERE');
    expect(mfg.match(/sourceExistsSql\(/g) ?? []).toHaveLength(1);
    expect(pos.match(/sourceExistsSql\(/g) ?? []).toHaveLength(1);
    expect(sales.match(/sourceExistsSql\(/g) ?? []).toHaveLength(2);
    expect(landed.match(/sourceExistsSql\(/g) ?? []).toHaveLength(0);
    expect(landed).toContain("from './native-lane-17/client'");
  });

  test('journal-line SUM terms are composed, never inlined, in finance-record-sources', () => {
    const text = src('finance-record-sources.ts');
    expect(text).not.toContain('(SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=j.id)');
    expect(text).not.toContain('(SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=j.id)');
    expect(text.match(/journalLineSumSql\(/g) ?? []).toHaveLength(4);
  });

  test('retirement:ts-statements stays strictly below the r22 BEFORE=387 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(387);
  });
});
