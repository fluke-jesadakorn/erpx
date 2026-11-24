import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { activeAccountListSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r51-sql';
import { financeReferenceListSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r42-sql';
import { versionStatusGuardSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r52-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;

type RefCase = { file: string; table: string; predicate: string; orderBy: string; expected: string };
const RANK1: RefCase[] = [
  { file: 'finance-asset-policies.ts', table: 'finance_asset_policy_editions', predicate: 'company_id=?', orderBy: 'created_at DESC LIMIT 500', expected: 'SELECT * FROM finance_asset_policy_editions WHERE company_id=? ORDER BY created_at DESC LIMIT 500' },
  { file: 'finance-asset-policies.ts', table: 'finance_asset_policy_runs', predicate: 'company_id=?', orderBy: 'created_at DESC LIMIT 500', expected: 'SELECT * FROM finance_asset_policy_runs WHERE company_id=? ORDER BY created_at DESC LIMIT 500' },
  { file: 'finance-book-adjustments.ts', table: 'finance_book_adjustments', predicate: 'company_id=?', orderBy: 'created_at DESC LIMIT 200', expected: 'SELECT * FROM finance_book_adjustments WHERE company_id=? ORDER BY created_at DESC LIMIT 200' },
  { file: 'finance-business-contracts.ts', table: 'finance_business_contracts', predicate: 'company_id=?', orderBy: 'created_at DESC LIMIT 200', expected: 'SELECT * FROM finance_business_contracts WHERE company_id=? ORDER BY created_at DESC LIMIT 200' },
  { file: 'finance-business-contracts.ts', table: 'finance_recognition_runs', predicate: 'company_id=?', orderBy: 'created_at DESC LIMIT 500', expected: 'SELECT * FROM finance_recognition_runs WHERE company_id=? ORDER BY created_at DESC LIMIT 500' },
  { file: 'finance-business-contracts.ts', table: 'finance_billing_corrections', predicate: 'company_id=?', orderBy: 'created_at DESC LIMIT 500', expected: 'SELECT * FROM finance_billing_corrections WHERE company_id=? ORDER BY created_at DESC LIMIT 500' },
  { file: 'finance-engines.ts', table: 'finance_calculation_runs', predicate: 'company_id=?', orderBy: 'created_at DESC LIMIT 50', expected: 'SELECT * FROM finance_calculation_runs WHERE company_id=? ORDER BY created_at DESC LIMIT 50' },
  { file: 'finance-instruments.ts', table: 'finance_instrument_editions', predicate: 'company_id=?', orderBy: 'created_at DESC,id LIMIT 500', expected: 'SELECT * FROM finance_instrument_editions WHERE company_id=? ORDER BY created_at DESC,id LIMIT 500' },
  { file: 'finance-instruments.ts', table: 'finance_instrument_runs', predicate: 'company_id=?', orderBy: 'created_at DESC,id LIMIT 500', expected: 'SELECT * FROM finance_instrument_runs WHERE company_id=? ORDER BY created_at DESC,id LIMIT 500' },
  { file: 'finance-workspace.ts', table: 'finance_charts', predicate: 'company_id=?', orderBy: 'created_at', expected: 'SELECT * FROM finance_charts WHERE company_id=? ORDER BY created_at' },
  { file: 'finance-company-tools.ts', table: 'finance_company_tools', predicate: 'company_id=?', orderBy: 'tool', expected: 'SELECT * FROM finance_company_tools WHERE company_id=? ORDER BY tool' },
  { file: 'finance-ai.ts', table: 'finance_ai_proposals', predicate: 'company_id=?', orderBy: 'updated_at DESC,id LIMIT 30', expected: 'SELECT * FROM finance_ai_proposals WHERE company_id=? ORDER BY updated_at DESC,id LIMIT 30' },
  { file: 'finance-assets.ts', table: 'finance_assets', predicate: 'company_id=?', orderBy: 'asset_number,id', expected: 'SELECT * FROM finance_assets WHERE company_id=? ORDER BY asset_number,id' },
  { file: 'finance-books.ts', table: 'finance_dimension_definitions', predicate: 'company_id=?', orderBy: 'code', expected: 'SELECT * FROM finance_dimension_definitions WHERE company_id=? ORDER BY code' },
  { file: 'finance-books.ts', table: 'finance_dimension_values', predicate: 'company_id=?', orderBy: 'dimension_id,code', expected: 'SELECT * FROM finance_dimension_values WHERE company_id=? ORDER BY dimension_id,code' },
  { file: 'finance-invoice-read.ts', table: 'finance_charts', predicate: 'company_id=?', orderBy: 'name', expected: 'SELECT * FROM finance_charts WHERE company_id=? ORDER BY name' },
  { file: 'finance-bank-automation.ts', table: 'finance_bank_csv_mappings', predicate: 'company_id=?', orderBy: 'name', expected: 'SELECT * FROM finance_bank_csv_mappings WHERE company_id=? ORDER BY name' },
];

type AccountCase = { file: string; projection: string; orderBy: string; expected: string };
const RANK2: AccountCase[] = [
  { file: 'finance-accruals.ts', projection: 'id,code,name,currency', orderBy: 'code', expected: "SELECT id,code,name,currency FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code" },
  { file: 'finance-books.ts', projection: 'id,chart_id,code,name', orderBy: 'chart_id,code', expected: "SELECT id,chart_id,code,name FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY chart_id,code" },
  { file: 'finance-business-contracts.ts', projection: 'id,chart_id,code,name,category,currency,control_kind', orderBy: 'code', expected: "SELECT id,chart_id,code,name,category,currency,control_kind FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code" },
  { file: 'pos-facts.ts', projection: 'id,code,name,chart_id,category,currency,control_kind', orderBy: 'code', expected: "SELECT id,code,name,chart_id,category,currency,control_kind FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code" },
  { file: 'pms-hospitality-setup-facts.ts', projection: 'id,code,name,category,currency,chart_id', orderBy: 'code', expected: "SELECT id,code,name,category,currency,chart_id FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code" },
];

describe('Finance read-tail r51 Rank1 ordered company lists', () => {
  test('all 17 r42 reference-list calls render byte-identically to the retired literals', () => {
    expect(RANK1).toHaveLength(17);
    for (const c of RANK1) {
      expect(financeReferenceListSql(c.table, c.predicate, c.orderBy)).toBe(c.expected);
      expect(read(c.file)).not.toContain(JSON.stringify(c.expected));
    }
  });

  test('delegation counts match the 17 sites with binds and LIMIT text retained', () => {
    const expected: Record<string, number> = {
      'finance-asset-policies.ts': 2,
      'finance-book-adjustments.ts': 1,
      'finance-business-contracts.ts': 3,
      'finance-engines.ts': 1,
      'finance-instruments.ts': 2,
      'finance-workspace.ts': 1,
      'finance-company-tools.ts': 1,
      'finance-ai.ts': 1,
      'finance-assets.ts': 1,
      'finance-books.ts': 2,
      'finance-invoice-read.ts': 2,
      'finance-bank-automation.ts': 1,
    };
    for (const [file, n] of Object.entries(expected)) expect(count(read(file), 'financeReferenceListSql(')).toBe(n);
    expect(read('finance-asset-policies.ts')).toContain("financeReferenceListSql('finance_asset_policy_editions','company_id=?','created_at DESC LIMIT 500')");
    expect(read('finance-instruments.ts')).toContain("financeReferenceListSql('finance_instrument_editions','company_id=?','created_at DESC,id LIMIT 500')");
    expect(read('finance-book-adjustments.ts')).toContain('financeReferenceListSql(\'finance_book_adjustments\',\'company_id=?\',\'created_at DESC LIMIT 200\'),');
  });
});

describe('Finance read-tail r51 Rank2 active account lists', () => {
  test('all five projection/order variants render byte-identically to the retired literals', () => {
    expect(RANK2).toHaveLength(5);
    for (const c of RANK2) {
      expect(activeAccountListSql(c.projection, c.orderBy)).toBe(c.expected);
      const src = read(c.file);
      expect(src).toContain(`activeAccountListSql('${c.projection}','${c.orderBy}')`);
      expect(src).not.toContain(JSON.stringify(c.expected));
    }
  });

  test('the finance-invoice-read workspace reuse keeps the status literal and company bind', () => {
    const expected = "SELECT * FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code";
    expect(financeReferenceListSql('finance_accounts', "company_id=? AND status='active'", 'code')).toBe(expected);
    const src = read('finance-invoice-read.ts');
    expect(src).toContain("financeReferenceListSql('finance_accounts',\"company_id=? AND status='active'\",'code')");
    expect(src).not.toContain(JSON.stringify(expected));
    expect(src).toContain(".bind(actor.companyId).all<FinanceRow>()");
  });
});

describe('Finance read-tail r51 helper definitions and retained guards', () => {
  test('each new family is defined exactly once and r42 is reused unchanged', () => {
    expect(count(read('finance-read-tail-r51-sql.ts'), 'export const activeAccountListSql =')).toBe(1);
    expect(count(read('finance-read-tail-r42-sql.ts'), 'export const financeReferenceListSql =')).toBe(1);
  });

  test('S4 version/status EXISTS guards delegated to r52; locks, tax, advisory and native code stay untouched', () => {
    expect(read('finance-business-contracts.ts')).toContain('EXISTS(SELECT 1 FROM finance_business_contracts WHERE id=? AND company_id=? AND status=? AND version=?)');
    const draft = (table: string) => versionStatusGuardSql(table, "'draft'", '');
    expect(draft('finance_asset_policy_editions')).toBe("EXISTS(SELECT 1 FROM finance_asset_policy_editions WHERE id=? AND company_id=? AND version=? AND status='draft')");
    expect(draft('finance_instrument_editions')).toBe("EXISTS(SELECT 1 FROM finance_instrument_editions WHERE id=? AND company_id=? AND version=? AND status='draft')");
    expect(draft('finance_instrument_runs')).toBe("EXISTS(SELECT 1 FROM finance_instrument_runs WHERE id=? AND company_id=? AND version=? AND status='draft')");
    expect(read('finance-asset-policies.ts')).toContain("versionStatusGuardSql('finance_asset_policy_editions', \"'draft'\", '')");
    expect(read('finance-instruments.ts')).toContain("versionStatusGuardSql('finance_instrument_editions', \"'draft'\", '')");
    expect(read('finance-instruments.ts')).toContain("versionStatusGuardSql('finance_instrument_runs', \"'draft'\", '')");
    expect(read('advisory-lock-dedupe-sql.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('pos-core-facts.ts')).toContain('FOR SHARE');
    expect(read('commerce-facts.ts')).toContain('FOR UPDATE');
    expect(read('finance-tax-register.ts')).toContain('SELECT');
    expect(readRepo('packages/server-collaboration/src/adapters/native-finance-billing-corrections.ts')).toContain('Native-owned finance billing correction statements.');
  });

  test('retirement:ts-statements reaches the r52 target of 1764', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 17 ordered company lists reuse r42, five active-account lists use the new
    // r51 projection helper, and the invoice-read reuse keeps status='active':
    // -23 literals, +1 helper declaration (1805 -> 1783); r52 delegates 20
    // version/status guards to one helper (-20 literals, +1: 1783 -> 1764).
    expect(total).toBe(1764);
    expect(total).toBeLessThan(1805);
  });
});

function readRepo(path: string) {
  return readFileSync(resolve(__dirname, '../../..', path), 'utf8');
}
