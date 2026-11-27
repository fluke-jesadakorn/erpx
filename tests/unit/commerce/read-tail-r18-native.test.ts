import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');
const norm = (value: string) => value.replace(/\s+/g, '');

const plans = read('packages/server-commerce/src/adapters/native_commerce_read_sql.rs');
const authority = read('packages/server-commerce/src/adapters/settings-dedupe-sql.ts');
const client = read('packages/server-commerce/src/adapters/native-commerce-read.ts');
/** Rust string literals escape the double quotes in the posting column aliases. */
const rustSql = plans.replace(/\\"/g, '"');

/** G4 / retire.commerce.native-read-r18: three settings reads move from the
 * registry-only public port to the mounted `/commerce/command-plan` resolver.
 * The TypeScript callers delegate to the same kinds and no executable SELECT
 * literal remains in the owner's TypeScript read tail. */
describe('commerce read-tail r18 native settings kinds', () => {
  const kinds = [
    {
      kind: 'commerce_rd_settings_by_company',
      SQL: 'SELECT * FROM commerce_settings WHERE company_id=?',
      binds: 1,
    },
    {
      kind: 'commerce_rd_currency_by_company',
      SQL: 'SELECT currency FROM commerce_settings WHERE company_id=?',
      binds: 1,
    },
    {
      kind: 'commerce_rd_posting_mapping_state',
      SQL: 'SELECT chart_id "chartId",currency,receivable_account_id "receivableAccountId",payable_account_id "payableAccountId",revenue_account_id "revenueAccountId",inventory_account_id "inventoryAccountId",clearing_account_id "clearingAccountId",cogs_account_id "cogsAccountId",input_tax_account_id "inputTaxAccountId",output_tax_account_id "outputTaxAccountId",cash_account_id "cashAccountId" FROM commerce_settings WHERE company_id=?',
      binds: 1,
    },
  ];

  it('registers the three settings kinds with their exact statements and binds', () => {
    for (const { kind, SQL, binds } of kinds) {
      expect(plans, kind).toContain(`"${kind}"`);
      expect(rustSql, kind).toContain(SQL);
      expect(plans, kind).toContain(`bind_count: ${binds},`);
      expect(SQL.match(/\?/g)?.length).toBe(binds);
      expect(SQL).toMatch(/^SELECT /);
      expect(SQL).not.toMatch(/FOR UPDATE|LOCK |;|INSERT |UPDATE |DELETE /);
    }
    // r18 added these 3 settings kinds; r19 added 6 product kinds and r20
    // added 5 warehouse kinds, so the mounted registry now holds 20.
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 66);');
    expect(plans).toContain('READ_SETTINGS_BY_COMPANY,');
    expect(plans).toContain('READ_CURRENCY_BY_COMPANY,');
    expect(plans).toContain('READ_POSTING_MAPPING_STATE,');
  });

  it('keeps byte parity between the native plans and the retained authority text', () => {
    for (const { SQL } of kinds) {
      expect(norm(rustSql), SQL).toContain(norm(SQL));
      expect(norm(authority), SQL).toContain(norm(SQL));
    }
    // The registry-only plans still pin the same three statements.
    const registry = read('packages/server-commerce/src/public/native_read.rs');
    for (const { SQL } of kinds.slice(0, 2)) expect(norm(registry), SQL).toContain(norm(SQL));
    // The 11-column registry statement is assembled with concat! fragments.
    expect(registry).toContain('SQL_POSTING_MAPPING_STATE');
    expect(registry).toContain('chart_id \\"chartId\\"');
    expect(registry).toContain('cash_account_id \\"cashAccountId\\"');
    expect(registry).toContain('FROM commerce_settings WHERE company_id=?');
  });

  it('exposes the kinds as TypeScript native-client constants and delegates callers to them', () => {
    for (const { kind } of kinds) {
      expect(client).toContain(`'${kind}'`);
    }
    expect(client).toContain('export const RD_SETTINGS_BY_COMPANY');
    expect(client).toContain('export const RD_CURRENCY_BY_COMPANY');
    expect(client).toContain('export const RD_POSTING_MAPPING_STATE');
    for (const file of [
      'posting-setup.ts',
      'sales-goods-posting-setup.ts',
      'manufacturing-posting-setup.ts',
      'pos-posting-setup.ts',
      'pos-posting-setup-sources.ts',
      'manufacturing-posting-setup-sources.ts',
      'pos-facts.ts',
      'pos-core-facts.ts',
      'sales-order-sources.ts',
      'expense-posting-configuration.ts',
    ]) {
      const text = read(`packages/server-commerce/src/adapters/${file}`);
      expect(text, file).toContain('nativeCommerceReadFirst');
      expect(text, file).not.toMatch(
        /prepare\(COMMERCE_(SETTINGS_BY_COMPANY|CURRENCY_BY_COMPANY|POSTING_MAPPING_STATE)_SQL\)/,
      );
    }
  });

  it('leaves the excluded guard and warehouse fragments untouched', () => {
    const guards = read('packages/server-commerce/src/adapters/guarded-source-dedupe-sql.ts');
    const registry = read('packages/server-commerce/src/public/native_read.rs');
    expect(registry).toContain('"settings.posting-absent-guard"');
    expect(registry).toContain('"warehouse.identity"');
    expect(guards).toContain("NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)");
    expect(guards).toContain("'SELECT id,company_id,branch_id,status FROM commerce_warehouses'");
    expect(authority).toContain('LOCK TABLE commerce_settings IN SHARE MODE');
  });

  it('pins the owner TypeScript SELECT KPI at 338 (before 339)', () => {
    const src = resolve(root, 'packages/server-commerce/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(338);
    expect(authority.match(/SELECT /g) ?? []).toHaveLength(0);
  });
});
