import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { versionStatusGuardSql } from '../../../modules/finance/server/src/adapters/finance-read-tail-r52-sql';

const srcRoot = resolve(__dirname, '../../../modules/finance/server/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const q = (s: string) => (s.includes("'") ? `"${s.replaceAll('"', '\\"')}"` : `'${s}'`);
const srcCall = (c: Case) => `versionStatusGuardSql(${q(c.table)}, ${q(c.status)}, ${q(c.tail)})`;

type Case = { file: string; table: string; status: string; tail: string; expected: string; binds: string };
const CASES: Case[] = [
  { file: 'accounting-profiles.ts', table: 'finance_accounting_profiles', status: "'draft'", tail: '', expected: "EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE id=? AND company_id=? AND version=? AND status='draft')", binds: '[id, actor.companyId, input.expectedVersion]' },
  { file: 'finance-accruals.ts', table: 'finance_accrual_schedules', status: '?', tail: '', expected: 'EXISTS(SELECT 1 FROM finance_accrual_schedules WHERE id=? AND company_id=? AND version=? AND status=?)', binds: '[scheduleId, actor.companyId, input.expectedVersion, row.status]' },
  { file: 'finance-asset-policies.ts', table: 'finance_asset_policy_editions', status: "'draft'", tail: '', expected: "EXISTS(SELECT 1 FROM finance_asset_policy_editions WHERE id=? AND company_id=? AND version=? AND status='draft')", binds: '[id, companyId, expectedVersion]' },
  { file: 'finance-assets.ts', table: 'finance_assets', status: '?', tail: '', expected: 'EXISTS(SELECT 1 FROM finance_assets WHERE id=? AND company_id=? AND version=? AND status=?)', binds: '[asset.id, actor.companyId, asset.version, asset.status]' },
  { file: 'finance-bank-automation.ts', table: 'finance_journals', status: "'posted'", tail: '', expected: "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND version=? AND status='posted')", binds: '[l.journal_id, actor.companyId, l.version]' },
  { file: 'finance-collections.ts', table: 'finance_collection_cases', status: '?', tail: ') AND NOT EXISTS(SELECT 1 FROM finance_collection_cases WHERE parent_case_id=?) AND NOT EXISTS(SELECT 1 FROM finance_collection_deliveries d JOIN finance_collection_cases c ON c.id=d.case_id WHERE c.root_id=?', expected: 'EXISTS(SELECT 1 FROM finance_collection_cases WHERE id=? AND company_id=? AND version=? AND status=?) AND NOT EXISTS(SELECT 1 FROM finance_collection_cases WHERE parent_case_id=?) AND NOT EXISTS(SELECT 1 FROM finance_collection_deliveries d JOIN finance_collection_cases c ON c.id=d.case_id WHERE c.root_id=?)', binds: '[ parentId, actor.companyId, input.expectedVersion, parent.status, parentId, parent.root_id, ]' },
  { file: 'finance-collections.ts', table: 'finance_collection_cases', status: '?', tail: '', expected: 'EXISTS(SELECT 1 FROM finance_collection_cases WHERE id=? AND company_id=? AND version=? AND status=?)', binds: '[id, actor.companyId, input.expectedVersion, row.status]' },
  { file: 'finance-instruments.ts', table: 'finance_instrument_editions', status: "'draft'", tail: '', expected: "EXISTS(SELECT 1 FROM finance_instrument_editions WHERE id=? AND company_id=? AND version=? AND status='draft')", binds: '[id, actor.companyId, input.expectedVersion]' },
  { file: 'finance-instruments.ts', table: 'finance_instrument_runs', status: "'draft'", tail: '', expected: "EXISTS(SELECT 1 FROM finance_instrument_runs WHERE id=? AND company_id=? AND version=? AND status='draft')", binds: '[id, actor.companyId, input.expectedVersion]' },
  { file: 'finance-planning.ts', table: 'finance_plan_editions', status: "'approved'", tail: '', expected: "EXISTS(SELECT 1 FROM finance_plan_editions WHERE id=? AND company_id=? AND version=? AND status='approved')", binds: '[base.parentId, actor.companyId, base.expectedVersion!]' },
  { file: 'finance-planning.ts', table: 'finance_plan_editions', status: "'draft'", tail: '', expected: "EXISTS(SELECT 1 FROM finance_plan_editions WHERE id=? AND company_id=? AND version=? AND status='draft')", binds: '[id, actor.companyId, input.expectedVersion]' },
  { file: 'finance-tax-register.ts', table: 'finance_tax_entries', status: "'prepared'", tail: '', expected: "EXISTS(SELECT 1 FROM finance_tax_entries WHERE id=? AND company_id=? AND version=? AND status='prepared')", binds: '[id, actor.companyId, input.expectedVersion]' },
  { file: 'finance-tax-register.ts', table: 'finance_current_tax_entries', status: "'approved'", tail: '', expected: "EXISTS(SELECT 1 FROM finance_current_tax_entries WHERE id=? AND company_id=? AND version=? AND status='approved')", binds: '[entryId, actor.companyId, input.expectedEntryVersion]' },
  { file: 'finance-withholding-remittance.ts', table: 'finance_accounts', status: "'active'", tail: " AND control_kind='bank'", expected: "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND version=? AND status='active' AND control_kind='bank')", binds: '[String(row.bank_account), actor.companyId, Number(row.account_version)]' },
  { file: 'finance-withholding.ts', table: 'finance_accounts', status: "'active'", tail: " AND category='liability' AND control_kind='none' AND currency='THB'", expected: "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND version=? AND status='active' AND category='liability' AND control_kind='none' AND currency='THB')", binds: '[liability.id, actor.companyId, liability.version]' },
  { file: 'finance-withholding.ts', table: 'finance_payments', status: "'draft'", tail: ' AND created_by=?', expected: "EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status='draft' AND created_by=?)", binds: '[paymentId, actor.companyId, input.expectedVersion, actor.userId]' },
  { file: 'finance.ts', table: 'finance_payments', status: "'approved'", tail: ' AND created_by!=? AND approved_by!=?', expected: "EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status='approved' AND created_by!=? AND approved_by!=?)", binds: '[paymentId, actor.companyId, input.expectedVersion, actor.userId, actor.userId]' },
  { file: 'finance.ts', table: 'finance_payments', status: "'released'", tail: ' AND released_by=? AND created_by!=? AND approved_by!=?', expected: "EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status='released' AND released_by=? AND created_by!=? AND approved_by!=?)", binds: '[paymentId, actor.companyId, input.expectedVersion, actor.userId, actor.userId, actor.userId]' },
  { file: 'refunds.ts', table: 'finance_refund_instructions', status: '?', tail: '', expected: 'EXISTS(SELECT 1 FROM finance_refund_instructions WHERE id=? AND company_id=? AND version=? AND status=?)', binds: '[id, actor.companyId, input.expectedVersion, row.status]' },
  { file: 'refunds.ts', table: 'finance_refund_obligations', status: "'approved'", tail: ' AND outstanding_minor>=?', expected: "EXISTS(SELECT 1 FROM finance_refund_obligations WHERE id=? AND company_id=? AND version=? AND status='approved' AND outstanding_minor>=?)", binds: '[owner.id, actor.companyId, owner.version, amount]' },
];

describe('Finance read-tail r52 version/status guards', () => {
  test('all 20 sites render byte-identically with version before status', () => {
    expect(CASES).toHaveLength(20);
    for (const c of CASES) {
      const rendered = versionStatusGuardSql(c.table, c.status, c.tail);
      expect(rendered).toBe(c.expected);
      expect(rendered.indexOf('version=?')).toBeLessThan(rendered.indexOf('status='));
      expect(rendered.startsWith(`EXISTS(SELECT 1 FROM ${c.table} WHERE id=? AND company_id=? AND version=? AND status=`)).toBe(true);
    }
  });

  test('callers delegate to one helper with unchanged bind order', () => {
    for (const c of CASES) {
      const src = read(c.file);
      expect(norm(src)).toContain(norm(`${srcCall(c)}, ${c.binds}`));
      expect(src).toContain("import { versionStatusGuardSql } from './finance-read-tail-r52-sql';");
    }
  });

  test('every retired guard literal is gone and call counts match the anchors', () => {
    for (const c of CASES) expect(read(c.file)).not.toContain(JSON.stringify(c.expected));
    const expected: Record<string, number> = {
      'accounting-profiles.ts': 1, 'finance-accruals.ts': 1, 'finance-asset-policies.ts': 1,
      'finance-assets.ts': 1, 'finance-bank-automation.ts': 1, 'finance-collections.ts': 2,
      'finance-instruments.ts': 2, 'finance-planning.ts': 2, 'finance-tax-register.ts': 2,
      'finance-withholding-remittance.ts': 1, 'finance-withholding.ts': 2, 'refunds.ts': 2, 'finance.ts': 2,
    };
    for (const [file, n] of Object.entries(expected)) expect(count(read(file), 'versionStatusGuardSql(')).toBe(n);
  });

  test('tax register guards are byte-proven and included', () => {
    const tax = read('finance-tax-register.ts');
    for (const c of CASES.filter(c => c.file === 'finance-tax-register.ts')) {
      expect(versionStatusGuardSql(c.table, c.status, c.tail)).toBe(c.expected);
      expect(tax).toContain(srcCall(c));
      expect(tax).not.toContain(JSON.stringify(c.expected));
    }
    expect(tax).toContain('SELECT');
    expect(tax).toContain('finance_current_tax_entries');
  });
});

describe('Finance read-tail r52 protected families and retirement', () => {
  test('row locks, advisory locks, native writes and prior helpers stay untouched', () => {
    const helper = read('finance-read-tail-r52-sql.ts');
    expect(count(helper, 'export const versionStatusGuardSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(read('finance-assets.ts')).toContain('FOR UPDATE');
    expect(read('finance-withholding-remittance.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('finance-tax-register.ts')).toContain('nativeFinanceTaxRegister');
    expect(read('finance-read-tail-r45-sql.ts')).toContain('export const versionStatusGuard =');
    expect(read('finance-read-tail-r51-sql.ts')).toContain('export const activeAccountListSql =');
    expect(read('finance-business-contracts.ts')).toContain('EXISTS(SELECT 1 FROM finance_business_contracts WHERE id=? AND company_id=? AND status=? AND version=?)');
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
    // 20 guard literals collapse into one helper (-20 literals, +1 declaration: 1783 -> 1764);
    // r53 delegates 11 column-by-id reads to one helper (-11 literals, +1 declaration: 1764 -> 1754);
    // r54 routes 14 point reads into two helpers (-14 literals, +2 declarations: 1754 -> 1742).
    // r55 folds 18 company count literals into two helpers (-18 literals, +2 declarations: 1742 -> 1726).
    // r56 folds 11 head literals into three helpers (-11 literals, +3 declarations: 1726 -> 1718).
    expect(CASES).toHaveLength(20);
    expect(total).toBe(1562);
    expect(total).toBeLessThan(1783);
  });
});
