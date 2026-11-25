import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  accountChartGuardSql,
  partyKindGuardSql,
  FX_RUN_POSTED_GUARD_SQL,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r46-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;

const ACCOUNT = 'EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=?';
const PARTY = 'EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=?';
const FX = "NOT EXISTS(SELECT 1 FROM finance_fx_revaluation_runs WHERE period_id=? AND chart_id=? AND status='posted')";

describe('Finance read-tail r46 Rank1 account/chart guards', () => {
  test('renders each tail byte-identically to the literal it replaces', () => {
    expect(accountChartGuardSql(" AND status='active'")).toBe(`${ACCOUNT} AND status='active')`);
    expect(accountChartGuardSql(" AND currency=? AND category='asset' AND control_kind='bank' AND status='active'"))
      .toBe(`${ACCOUNT} AND currency=? AND category='asset' AND control_kind='bank' AND status='active')`);
    expect(accountChartGuardSql(" AND currency=? AND category=? AND status='active'"))
      .toBe(`${ACCOUNT} AND currency=? AND category=? AND status='active')`);
    expect(accountChartGuardSql(" AND category='expense' AND status='active'"))
      .toBe(`${ACCOUNT} AND category='expense' AND status='active')`);
    // Manufacturing keeps its caller interpolation; all three branches render unchanged.
    expect(accountChartGuardSql(" AND status='active' AND control_kind='none' AND category='asset'"))
      .toBe(`${ACCOUNT} AND status='active' AND control_kind='none' AND category='asset')`);
    expect(accountChartGuardSql(" AND status='active' AND control_kind='none' AND category IN ('expense','other_expense')"))
      .toBe(`${ACCOUNT} AND status='active' AND control_kind='none' AND category IN ('expense','other_expense'))`);
    expect(accountChartGuardSql(" AND status='active' AND control_kind='none' AND category IN ('liability','expense')"))
      .toBe(`${ACCOUNT} AND status='active' AND control_kind='none' AND category IN ('liability','expense'))`);
  });

  test('replaces all six inline account guards with unchanged binds', () => {
    const commerce = read('commerce-facts.ts');
    const planning = read('journal-planning.ts');
    const goods = read('commerce-goods-accounting-facts.ts');
    const transit = read('inventory-transit-sources.ts');
    const manufacturing = read('manufacturing-accounting-sources.ts');

    expect(count(commerce, 'accountChartGuardSql(')).toBe(2);
    expect(count(planning, 'accountChartGuardSql(')).toBe(1);
    expect(count(goods, 'accountChartGuardSql(')).toBe(1);
    expect(count(transit, 'accountChartGuardSql(')).toBe(1);
    expect(count(manufacturing, 'accountChartGuardSql(')).toBe(1);
    for (const src of [commerce, planning, goods, transit, manufacturing]) {
      expect(src).not.toContain(`${ACCOUNT} AND `);
    }
    // Binds stay 3 (status), 4 (bank), 3 (posting), 5 (goods), 3 (loss), 3 (manufacturing).
    expect(commerce).toContain('accountChartGuardSql(" AND status=\'active\'"),facts.values)');
    expect(commerce).toContain('accountChartGuardSql(" AND currency=? AND category=\'asset\' AND control_kind=\'bank\' AND status=\'active\'"),facts.values)');
    expect(planning).toContain('[accountId, actor.companyId, input.chartId]');
    expect(goods).toContain('accountChartGuardSql(" AND currency=? AND category=? AND status=\'active\'"),facts.values)');
    expect(transit).toContain('[expenseAccountId,companyId,chart_id]');
    expect(manufacturing).toContain("control_kind='none' ${accountKind");
    expect(manufacturing).toContain('[account, companyId, chartId]');
  });
});

describe('Finance read-tail r46 Rank2 party kind guards', () => {
  test('renders every status/currency order byte-identically', () => {
    expect(partyKindGuardSql(" AND status='active' AND currency=? AND person_id IS NULL"))
      .toBe(`${PARTY} AND status='active' AND currency=? AND person_id IS NULL)`);
    expect(partyKindGuardSql(" AND currency=? AND status='active' AND person_id IS NULL"))
      .toBe(`${PARTY} AND currency=? AND status='active' AND person_id IS NULL)`);
    expect(partyKindGuardSql(' AND version=? AND person_id IS NULL'))
      .toBe(`${PARTY} AND version=? AND person_id IS NULL)`);
    expect(partyKindGuardSql(" AND status='active' AND currency=? AND person_id IS NULL"))
      .not.toBe(partyKindGuardSql(" AND currency=? AND status='active' AND person_id IS NULL"));
  });

  test('replaces all four inline party guards with four binds each', () => {
    const commerce = read('commerce-facts.ts');
    const finance = read('finance.ts');
    expect(count(commerce, 'partyKindGuardSql(')).toBe(2);
    expect(count(finance, 'partyKindGuardSql(')).toBe(2);
    expect(commerce).not.toContain(`${PARTY} AND `);
    expect(finance).not.toContain(`${PARTY} AND `);
    expect(commerce).toContain('partyKindGuardSql(" AND status=\'active\' AND currency=? AND person_id IS NULL")');
    expect(commerce).toContain('partyKindGuardSql(" AND version=? AND person_id IS NULL")');
    expect(count(finance, 'partyKindGuardSql(" AND currency=? AND status=\'active\' AND person_id IS NULL")')).toBe(2);
    expect(finance).toContain('[input.partyId!, actor.companyId, input.kind === "payable" ? "supplier" : "customer", input.currency]');
    expect(finance).toContain('[input.partyId!, actor.companyId, input.kind === "payment" ? "supplier" : "customer", currency]');
  });
});

describe('Finance read-tail r46 Rank3 FX posted-run guard', () => {
  test('shares one exact guard at both prepare and approve sites', () => {
    expect(FX_RUN_POSTED_GUARD_SQL).toBe(FX);
    const finance = read('finance.ts');
    expect(count(finance, 'FX_RUN_POSTED_GUARD_SQL')).toBe(3); // import + two sites
    expect(finance).not.toContain(FX);
    expect(finance).toContain('"posted-run", FX_RUN_POSTED_GUARD_SQL, [input.periodId, input.chartId]');
    expect(finance).toContain('"posted-run", FX_RUN_POSTED_GUARD_SQL, [run.period_id, run.chart_id]');
  });
});

describe('Finance read-tail r46 protected families and retirement', () => {
  test('leaves prior helpers, locks, tax, advisory and native families alone', () => {
    const prior = read('finance-read-tail-r40-sql.ts');
    const r45 = read('finance-read-tail-r45-sql.ts');
    expect(prior).toContain('ACTIVE_ACCOUNT_CATEGORY_PROBE_SQL');
    expect(prior).toContain('PAYMENT_FULL_ROW_SQL');
    expect(r45).toContain('versionStatusGuard');
    expect(r45).toContain('ACTIVE_THB_CHART_GUARD_SQL');
    expect(read('sales.ts')).toContain('FOR SHARE');
    expect(read('finance-reporting.ts')).toContain('FOR UPDATE');
    expect(read('finance-withholding-remittance.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('finance-tax-register.ts')).toContain('finance_current_tax_entries');
    expect(read('finance.ts')).toContain('nativeFinanceOwnerWrite');
  });

  test('each r46 family is defined exactly once', () => {
    const helper = read('finance-read-tail-r46-sql.ts');
    for (const name of ['accountChartGuardSql', 'partyKindGuardSql', 'FX_RUN_POSTED_GUARD_SQL']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
  });

  test('retirement:ts-statements stays at or below the r46 target of 1851', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Twelve inline SELECTs collapse into three r46 helpers: -9 (1860 -> 1851); r48 removes seven more (-> 1844); r49 removes 32 more (-> 1812); r50 removes seven more (-> 1805).
    expect(total).toBe(1597);
    expect(total).toBeLessThan(1860);
  });
});
