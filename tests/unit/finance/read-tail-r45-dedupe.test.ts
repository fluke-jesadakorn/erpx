import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  ACTIVE_THB_CHART_GUARD_SQL,
  EARLIER_PERIODS_GUARD_SQL,
  NO_APPROVED_PROFILE_GUARD_SQL,
  PERIOD_OPEN_END_DATE_GUARD_SQL,
  versionStatusGuard,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r45-sql';
import { OPEN_PERIOD_IDS_SQL } from '../../../packages/server-finance/src/adapters/finance-read-tail-r16-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const helper = read('finance-read-tail-r45-sql.ts');
const count = (src: string, needle: string) => src.split(needle).length - 1;

const PARTY = "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND version=? AND status='active')";
const ACCOUNT = "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND version=? AND status='active')";
const OPEN_END = "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND end_date=?)";
const EARLIER = "NOT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND id<>? AND start_date<=? AND end_date>=? AND state<>'closed')";
const THB = "EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency='THB')";
const APPROVED = "NOT EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved')";

describe('Finance read-tail r45 Rank1 version/status guards', () => {
  test('renders version-before-status text byte-identically for both tables', () => {
    expect(versionStatusGuard('finance_parties')).toBe(PARTY);
    expect(versionStatusGuard('finance_accounts')).toBe(ACCOUNT);
    expect(versionStatusGuard('finance_parties')).not.toContain("status='active' AND version=?");
  });

  test('replaces all four inline guards without changing binds or the r20 order', () => {
    const lines = read('finance-invoice-lines.ts');
    const sales = read('sales.ts');
    expect(count(lines, 'versionStatusGuard("finance_parties")')).toBe(2);
    expect(count(lines, 'versionStatusGuard("finance_accounts")')).toBe(1);
    expect(count(sales, 'versionStatusGuard("finance_accounts")')).toBe(1);
    for (const src of [lines, sales]) {
      expect(src).not.toContain(PARTY);
      expect(src).not.toContain(ACCOUNT);
    }
    // The reversed r20 builder stays on its own site; no new reversed use.
    expect(count(lines, 'idCompanyStatusVersionGuard("finance_accounts", "active")')).toBe(1);
    expect(read('finance-read-tail-r20-sql.ts')).toContain("status='${status}' AND version=?");
  });
});

describe('Finance read-tail r45 Rank2 period guards', () => {
  test('renders byte-identically and replaces every inline site', () => {
    expect(PERIOD_OPEN_END_DATE_GUARD_SQL).toBe(OPEN_END);
    expect(EARLIER_PERIODS_GUARD_SQL).toBe(EARLIER);
    const reporting = read('finance-reporting.ts');
    const finance = read('finance.ts');
    expect(count(reporting, 'PERIOD_OPEN_END_DATE_GUARD_SQL')).toBe(2); // import + site
    expect(count(reporting, 'EARLIER_PERIODS_GUARD_SQL')).toBe(3); // import + two sites
    expect(count(finance, 'PERIOD_OPEN_END_DATE_GUARD_SQL')).toBe(2); // import + site
    expect(reporting).not.toContain(OPEN_END);
    expect(reporting).not.toContain(EARLIER);
    expect(finance).not.toContain(OPEN_END);
  });

  test('reuses OPEN_PERIOD_IDS_SQL at the payroll remittance site', () => {
    const payroll = read('payroll-remittance-sources.ts');
    expect(OPEN_PERIOD_IDS_SQL).toBe("SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?");
    expect(payroll).toContain("import { OPEN_PERIOD_IDS_SQL } from './finance-read-tail-r16-sql';");
    expect(count(payroll, 'database.prepare(OPEN_PERIOD_IDS_SQL)')).toBe(1);
    expect(payroll).not.toContain("SELECT id FROM finance_periods WHERE company_id=? AND state='open'");
  });
});

describe('Finance read-tail r45 Rank3 accounting-profile pairs', () => {
  test('keeps the THB literal inside shared text and replaces both pairs', () => {
    expect(ACTIVE_THB_CHART_GUARD_SQL).toBe(THB);
    expect(NO_APPROVED_PROFILE_GUARD_SQL).toBe(APPROVED);
    expect(ACTIVE_THB_CHART_GUARD_SQL).toContain("base_currency='THB'");
    const profiles = read('accounting-profiles.ts');
    expect(count(profiles, 'ACTIVE_THB_CHART_GUARD_SQL')).toBe(3); // import + two sites
    expect(count(profiles, 'NO_APPROVED_PROFILE_GUARD_SQL')).toBe(3); // import + two sites
    expect(profiles).not.toContain(THB);
    expect(profiles).not.toContain(APPROVED);
  });
});

describe('Finance read-tail r45 protected families and retirement', () => {
  test('leaves lock, advisory, native and tax r24 guards alone', () => {
    const sales = read('sales.ts');
    expect(sales).toContain('FOR SHARE OF a,c');
    expect(sales).toContain('FOR SHARE');
    expect(read('finance-withholding-remittance.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('finance-reporting.ts')).toContain('FOR UPDATE');
    expect(read('finance-read-tail-r24-sql.ts')).toContain('SELECT');
    expect(read('finance-tax-register.ts')).toContain('finance_current_tax_entries');
  });

  test('each r45 family is defined exactly once', () => {
    for (const name of [
      'versionStatusGuard',
      'PERIOD_OPEN_END_DATE_GUARD_SQL',
      'EARLIER_PERIODS_GUARD_SQL',
      'ACTIVE_THB_CHART_GUARD_SQL',
      'NO_APPROVED_PROFILE_GUARD_SQL',
    ]) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
    expect(read('finance-read-tail-r44-sql.ts')).toContain('AI_USAGE_MONTH_SQL');
  });

  test('retirement:ts-statements stays at or below the r45 target of 1860', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 13 inline SELECTs collapse into five r45 helpers: -8 (1868 -> 1860); r46 removes nine more (-> 1851); r48 removes seven more (-> 1844); r49 removes 32 more (-> 1812); r50 removes seven more (-> 1805).
    expect(total).toBe(1693);
    expect(total).toBeLessThanOrEqual(1860);
    expect(total).toBeLessThan(1868);
  });
});
