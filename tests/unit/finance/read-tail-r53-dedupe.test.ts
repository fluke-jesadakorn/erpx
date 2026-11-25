import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { columnByIdSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r53-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const call = (c: Case) => `columnByIdSql('${c.col}', '${c.table}')`;

type Case = { file: string; col: string; table: string; quoted: string; bind: string };
const CASES: Case[] = [
  { file: 'assets-financial-unit.ts', col: 'asset_id', table: 'finance_asset_disposals', quoted: "'SELECT asset_id FROM finance_asset_disposals WHERE id=? AND company_id=?'", bind: '.bind(command.disposalId,actor.companyId)' },
  { file: 'commerce-entrypoint-facts.ts', col: 'kind', table: 'finance_parties', quoted: "'SELECT kind FROM finance_parties WHERE id=? AND company_id=?'", bind: '.bind(id,companyId)' },
  { file: 'document-purchases.ts', col: 'source_type', table: 'finance_invoices', quoted: '"SELECT source_type FROM finance_invoices WHERE id=? AND company_id=?"', bind: '.bind(id,actor.companyId)' },
  { file: 'finance-bank-automation.ts', col: 'columns_json', table: 'finance_bank_csv_mappings', quoted: '"SELECT columns_json FROM finance_bank_csv_mappings WHERE id=? AND company_id=?"', bind: '.bind(mappingId, actor.companyId)' },
  { file: 'finance-bank-automation.ts', col: 'currency', table: 'finance_accounts', quoted: '"SELECT currency FROM finance_accounts WHERE id=? AND company_id=?"', bind: '.bind(accountId, actor.companyId)' },
  { file: 'finance-business-contracts.ts', col: 'contract_id', table: 'finance_business_billing_runs', quoted: '"SELECT contract_id FROM finance_business_billing_runs WHERE id=? AND company_id=?"', bind: '.bind(parentId, companyId)' },
  { file: 'finance-due-drafts.ts', col: 'kind', table: 'finance_due_drafts', quoted: '"SELECT kind FROM finance_due_drafts WHERE id=? AND company_id=?"', bind: '.bind(id,actor.companyId)' },
  { file: 'finance.ts', col: 'id', table: 'finance_revenue_contracts', quoted: '"SELECT id FROM finance_revenue_contracts WHERE id=? AND company_id=?"', bind: '.bind(input.contractId, actor.companyId)' },
  { file: 'supplier-payment-purchases.ts', col: 'id', table: 'finance_supplier_batches', quoted: '"SELECT id FROM finance_supplier_batches WHERE id=? AND company_id=?"', bind: '.bind(target.id,actor.companyId)' },
  { file: 'tax-signature-attachment-read.ts', col: 'version', table: 'finance_tax_identities', quoted: "'SELECT version FROM finance_tax_identities WHERE id=? AND company_id=?'", bind: '.bind(before.targetId,actor.companyId)' },
  { file: 'workflow-flow-routing-facts.ts', col: 'posted_by', table: 'finance_journals', quoted: '"SELECT posted_by FROM finance_journals WHERE id=? AND company_id=?"', bind: '.bind(journal_id, company_id)' },
];

describe('Finance read-tail r53 closed column-by-id reads', () => {
  test('all 11 sites render byte-identically with id before company_id', () => {
    expect(CASES).toHaveLength(11);
    for (const c of CASES) {
      const rendered = columnByIdSql(c.col, c.table);
      expect(rendered).toBe(c.quoted.slice(1, -1));
      expect(rendered).toBe(`SELECT ${c.col} FROM ${c.table} WHERE id=? AND company_id=?`);
      expect(rendered.indexOf('id=?')).toBeLessThan(rendered.indexOf('company_id=?'));
      expect(count(rendered, 'SELECT ')).toBe(1);
    }
    expect(columnByIdSql('kind', 'finance_parties')).not.toBe(columnByIdSql('kind', 'finance_due_drafts'));
  });

  test('callers delegate with the original id-first binds', () => {
    for (const c of CASES) {
      const src = read(c.file);
      expect(norm(src)).toContain(norm(call(c)));
      expect(norm(src)).toContain(norm(c.bind));
      expect(src).toContain("import { columnByIdSql } from './finance-read-tail-r53-sql';");
      expect(src).not.toContain(c.quoted);
    }
  });

  test('retired column-by-id literals are gone and call counts match the 11 anchors', () => {
    const expected: Record<string, number> = {
      'assets-financial-unit.ts': 1, 'commerce-entrypoint-facts.ts': 1, 'document-purchases.ts': 1,
      'finance-bank-automation.ts': 2, 'finance-business-contracts.ts': 1, 'finance-due-drafts.ts': 1,
      'finance.ts': 1, 'supplier-payment-purchases.ts': 1, 'tax-signature-attachment-read.ts': 1,
      'workflow-flow-routing-facts.ts': 1,
    };
    let total = 0;
    for (const [file, n] of Object.entries(expected)) {
      expect(count(read(file), 'columnByIdSql(')).toBe(n);
      total += n;
    }
    expect(total).toBe(11);
  });
});

describe('Finance read-tail r53 protected families and retirement', () => {
  test('tailed, company-first and different-key reads stay inline', () => {
    expect(read('commerce-entrypoint-facts.ts')).toContain("'SELECT kind FROM finance_parties WHERE id=? AND company_id=? AND person_id IS NULL'");
    expect(read('supplier-payment-purchases.ts')).toContain('"SELECT b.id FROM finance_supplier_batches b JOIN finance_supplier_instructions i ON i.batch_id=b.id WHERE i.id=? AND b.company_id=?"');
    expect(read('commerce-entrypoint-facts.ts')).toContain('WHERE a.company_id=? AND a.currency=c.base_currency');
  });

  test('row locks, advisory, native writes and prior helpers stay untouched', () => {
    expect(read('document-purchases.ts')).toContain('FOR SHARE');
    expect(read('finance-bank-automation.ts')).toContain('FOR UPDATE');
    expect(read('finance-due-drafts.ts')).toContain('nativeDue(tx,true)');
    expect(read('finance-withholding-remittance.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('finance-read-tail-r49-sql.ts')).toContain('export const tableByIdSql =');
    expect(read('finance-read-tail-r52-sql.ts')).toContain('export const versionStatusGuardSql =');
  });

  test('the r53 helper is defined exactly once', () => {
    const helper = read('finance-read-tail-r53-sql.ts');
    expect(count(helper, 'export const columnByIdSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
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
    // 11 column-by-id literals collapse into one helper (-11 literals, +1 declaration: 1764 -> 1754);
    // r54 routes 14 point reads into two helpers (-14 literals, +2 declarations: 1754 -> 1742).
    // r55 folds 18 company count literals into two helpers (-18 literals, +2 declarations: 1742 -> 1726).
    // r56 folds 11 head literals into three helpers (-11 literals, +3 declarations: 1726 -> 1718).
    expect(CASES).toHaveLength(11);
    expect(total).toBe(1702);
    expect(total).toBeLessThan(1764);
  });
});
