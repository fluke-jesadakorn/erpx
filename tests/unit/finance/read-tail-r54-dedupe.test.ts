import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { imageByTailSql, sourceByTailSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r54-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const q = (s: string) => (s.includes("'") ? `"${s.replaceAll('"', '\\"')}"` : `'${s}'`);
const call = (c: Case) => `${c.helper}(${q(c.alias)},${q(c.table)},${q(c.tail)})`;
const label = (c: Case) => (c.helper === 'imageByTailSql' ? 'image' : 'source');

type Case = { file: string; helper: 'imageByTailSql' | 'sourceByTailSql'; alias: string; table: string; tail: string; expected: string; binds: string };
const CASES: Case[] = [
  { file: 'field-service-references.ts', helper: 'imageByTailSql', alias: 'source', table: 'finance_parties', tail: 'source.id=? AND source.company_id=?', expected: 'SELECT to_jsonb(source)::text image FROM finance_parties source WHERE source.id=? AND source.company_id=?', binds: '.bind(id,companyId)' },
  { file: 'payroll-other-remittance-facts.ts', helper: 'imageByTailSql', alias: 'c', table: 'finance_charts', tail: 'c.id=? AND c.company_id=?', expected: 'SELECT to_jsonb(c)::text image FROM finance_charts c WHERE c.id=? AND c.company_id=?', binds: '[String(header.chart_id),companyId]' },
  { file: 'payroll-other-remittance-facts.ts', helper: 'imageByTailSql', alias: 'a', table: 'finance_accounts', tail: 'a.chart_id=? AND a.company_id=? ORDER BY a.id', expected: 'SELECT to_jsonb(a)::text image FROM finance_accounts a WHERE a.chart_id=? AND a.company_id=? ORDER BY a.id', binds: '[String(header.chart_id),companyId]' },
  { file: 'payroll-other-remittance-facts.ts', helper: 'imageByTailSql', alias: 'p', table: 'finance_periods', tail: 'p.company_id=? ORDER BY p.id', expected: 'SELECT to_jsonb(p)::text image FROM finance_periods p WHERE p.company_id=? ORDER BY p.id', binds: '[companyId]' },
  { file: 'payroll-other-remittance-facts.ts', helper: 'imageByTailSql', alias: 'm', table: 'company_money_settings', tail: 'm.company_id=?', expected: 'SELECT to_jsonb(m)::text image FROM company_money_settings m WHERE m.company_id=?', binds: '[companyId]' },
  { file: 'payroll-other-remittance-facts.ts', helper: 'imageByTailSql', alias: 'm', table: 'company_currency_scales', tail: 'm.company_id=? ORDER BY m.currency', expected: 'SELECT to_jsonb(m)::text image FROM company_currency_scales m WHERE m.company_id=? ORDER BY m.currency', binds: '[companyId]' },
  { file: 'payroll-other-remittance-facts.ts', helper: 'imageByTailSql', alias: 'r', table: 'company_fx_rates', tail: 'r.company_id=? ORDER BY r.id', expected: 'SELECT to_jsonb(r)::text image FROM company_fx_rates r WHERE r.company_id=? ORDER BY r.id', binds: '[companyId]' },
  { file: 'payroll-other-remittance-facts.ts', helper: 'imageByTailSql', alias: 'r', table: 'finance_book_rules', tail: 'r.company_id=? ORDER BY r.id', expected: 'SELECT to_jsonb(r)::text image FROM finance_book_rules r WHERE r.company_id=? ORDER BY r.id', binds: '[companyId]' },
  { file: 'payroll-other-remittance-facts.ts', helper: 'imageByTailSql', alias: 'j', table: 'finance_journals', tail: 'j.id=? AND j.company_id=?', expected: 'SELECT to_jsonb(j)::text image FROM finance_journals j WHERE j.id=? AND j.company_id=?', binds: '.bind(id,companyId)' },
  { file: 'supplier-payment-money.ts', helper: 'sourceByTailSql', alias: 'b', table: 'finance_supplier_batches', tail: 'b.id=? AND b.company_id=?', expected: 'SELECT to_jsonb(b)::text source FROM finance_supplier_batches b WHERE b.id=? AND b.company_id=?', binds: '[batchId,actor.companyId]' },
  { file: 'supplier-payment-money.ts', helper: 'sourceByTailSql', alias: 'i', table: 'finance_invoices', tail: 'i.id=? AND i.company_id=?', expected: 'SELECT to_jsonb(i)::text source FROM finance_invoices i WHERE i.id=? AND i.company_id=?', binds: '[invoiceId,actor.companyId]' },
  { file: 'supplier-payment-money.ts', helper: 'sourceByTailSql', alias: 'p', table: 'finance_payments', tail: 'p.id=? AND p.company_id=?', expected: 'SELECT to_jsonb(p)::text source FROM finance_payments p WHERE p.id=? AND p.company_id=?', binds: '[paymentId,actor.companyId]' },
  { file: 'supplier-payment-money.ts', helper: 'sourceByTailSql', alias: 'b', table: 'finance_bank_transactions', tail: 'b.id=? AND b.company_id=?', expected: 'SELECT to_jsonb(b)::text source FROM finance_bank_transactions b WHERE b.id=? AND b.company_id=?', binds: '[bankId,actor.companyId]' },
  { file: 'supplier-payment-money.ts', helper: 'sourceByTailSql', alias: 's', table: 'finance_bank_statements', tail: 's.id=? AND s.company_id=?', expected: 'SELECT to_jsonb(s)::text source FROM finance_bank_statements s WHERE s.id=? AND s.company_id=?', binds: "[typeof bank?.statement_id==='string'?bank.statement_id:'',actor.companyId]" },
];

describe('Finance read-tail r54 closed image/source read-tail helpers', () => {
  test('all 14 sites render byte-identically to the retired literals', () => {
    expect(CASES).toHaveLength(14);
    for (const c of CASES) {
      const rendered = (c.helper === 'imageByTailSql' ? imageByTailSql : sourceByTailSql)(c.alias, c.table, c.tail);
      expect(rendered).toBe(c.expected);
      expect(rendered).toBe(`SELECT to_jsonb(${c.alias})::text ${label(c)} FROM ${c.table} ${c.alias} WHERE ${c.tail}`);
      expect(count(rendered, 'SELECT ')).toBe(1);
      expect(rendered.indexOf(`::text ${label(c)} FROM ${c.table} ${c.alias} WHERE `)).toBeGreaterThan(0);
      const placeholders = count(c.tail, '?');
      expect(placeholders).toBeGreaterThanOrEqual(1);
      expect(placeholders).toBeLessThanOrEqual(4);
    }
    expect(imageByTailSql('a', 't1', 'a.id=?')).not.toBe(sourceByTailSql('a', 't1', 'a.id=?'));
    expect(imageByTailSql('a', 't1', 'a.id=?')).not.toBe(imageByTailSql('a', 't2', 'a.id=?'));
  });

  test('callers delegate with unchanged tail, binds and raw-text parsing', () => {
    for (const c of CASES) {
      const src = read(c.file);
      expect(norm(src)).toContain(norm(call(c)));
      expect(norm(src)).toContain(norm(c.binds));
      const imports = norm(src).replace(/\{\s+/g, '{').replace(/\s+\}/g, '}');
      expect(imports.includes(`import {${c.helper}} from './finance-read-tail-r54-sql';`) || imports.includes(`import {${c.helper}} from "./finance-read-tail-r54-sql";`)).toBe(true);
      expect(src).not.toContain(`'${c.expected}'`);
      expect(src).not.toContain(`"${c.expected}"`);
    }
    const payroll = read('payroll-other-remittance-facts.ts');
    expect(payroll).toContain('JSON.parse(row.image) as NativeRow');
    expect(payroll).toContain(".first<string>('image')");
    const supplier = read('supplier-payment-money.ts');
    expect(supplier).toContain('const parse=(row:{source:string})=>JSON.parse(row.source) as Row;');
    expect(read('field-service-references.ts')).toContain(".first<string>('image')");
  });

  test('delegation counts match the 1 + 8 + 5 anchors', () => {
    expect(count(read('field-service-references.ts'), 'imageByTailSql(')).toBe(1);
    expect(count(read('payroll-other-remittance-facts.ts'), 'imageByTailSql(')).toBe(8);
    expect(count(read('supplier-payment-money.ts'), 'sourceByTailSql(')).toBe(5);
  });
});

describe('Finance read-tail r54 protected families and retirement', () => {
  test('multi-row set reads, prior helpers, row locks and fences stay inline', () => {
    const supplier = read('supplier-payment-money.ts');
    expect(supplier).toContain("'SELECT to_jsonb(i)::text source FROM finance_supplier_instructions i WHERE i.batch_id=? ORDER BY i.id'");
    expect(supplier).toContain("'SELECT to_jsonb(r)::text source FROM finance_payment_reversals r WHERE r.payment_id=? ORDER BY r.id'");
    expect(supplier).toContain("import { bookAccountMappingImageSql } from './finance-read-tail-r40-sql';");
    expect(supplier).toContain("import { SUPPLIER_RECEIPT_SOURCE_SQL } from './finance-read-tail-r38-sql';");
    expect(supplier).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(supplier).toContain('LOCK TABLE active_work_contexts IN EXCLUSIVE MODE');
    const payroll = read('payroll-other-remittance-facts.ts');
    expect(payroll).toContain("import {PAYROLL_LINE_IMAGE_SQL} from './finance-read-tail-r36-sql';");
    expect(payroll).toContain('LOCK TABLE finance_charts IN SHARE MODE');
    expect(read('field-service-references.ts')).toContain('LOCK TABLE finance_parties IN SHARE ROW EXCLUSIVE MODE');
  });

  test('each r54 helper is defined exactly once', () => {
    const helper = read('finance-read-tail-r54-sql.ts');
    expect(count(helper, 'export const imageByTailSql =')).toBe(1);
    expect(count(helper, 'export const sourceByTailSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(2);
  });

  test('retirement:ts-statements reaches the r56 target of 1718', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 14 point-read literals collapse into two helpers (-14 literals, +2 declarations: 1754 -> 1742).
    // r55 folds 18 company count literals into two helpers (-18 literals, +2 declarations: 1742 -> 1726).
    // r56 folds 11 head literals into three helpers (-11 literals, +3 declarations: 1726 -> 1718).
    expect(CASES).toHaveLength(14);
    expect(total).toBe(1562);
    expect(total).toBeLessThan(1754);
  });
});
