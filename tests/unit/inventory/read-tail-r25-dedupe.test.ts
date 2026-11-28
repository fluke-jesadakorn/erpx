import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { journalAccountLineExistsSql, scopeExistsSql } from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

type ScopeCase = [string, string, string, string, string, string, string];
const scopeCases: ScopeCase[] = [
  ['footprint', 'source', 'warehouse_id', 's.warehouse_id', 'product_id', 's.product_id',
    'EXISTS(SELECT 1 FROM footprint source WHERE source.warehouse_id=s.warehouse_id AND source.product_id=s.product_id)'],
  ['inventory_quality_holds', 'hold', 'id', 'origin.hold_id', 'company_id', 'origin.company_id',
    'EXISTS(SELECT 1 FROM inventory_quality_holds hold WHERE hold.id=origin.hold_id AND hold.company_id=origin.company_id)'],
  ['inventory_transit', 'transit', 'transfer_id', 't.id', 'company_id', 't.company_id',
    'EXISTS(SELECT 1 FROM inventory_transit transit WHERE transit.transfer_id=t.id AND transit.company_id=t.company_id)'],
  ['inventory_transit_events', 'event', 'transit_id', 't.id', 'kind', "'cancel'",
    "EXISTS(SELECT 1 FROM inventory_transit_events event WHERE event.transit_id=t.id AND event.kind='cancel')"],
];
type JournalCase = [string, string, string, string];
const journalCases: JournalCase[] = [
  ['inventoryAccountId', 'debit_minor', 'credit_minor',
    "EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'inventoryAccountId' AND line.debit_minor=c.net_value_minor AND line.credit_minor=0)"],
  ['varianceAccountId', 'credit_minor', 'debit_minor',
    "EXISTS(SELECT 1 FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=source.source_proof->>'varianceAccountId' AND line.credit_minor=c.net_value_minor AND line.debit_minor=0)"],
];

describe('Inventory read-tail r25 two-predicate EXISTS / journal account-line dedupe', () => {
  test('canonical helpers render byte-identical read-only SQL for every consumer', () => {
    for (const [table, alias, c1, v1, c2, v2, expected] of scopeCases) {
      expect(scopeExistsSql(table, alias, c1, v1, c2, v2)).toBe(expected);
      readOnly(scopeExistsSql(table, alias, c1, v1, c2, v2));
    }
    for (const [key, amount, zero, expected] of journalCases) {
      expect(journalAccountLineExistsSql(key, amount, zero)).toBe(expected);
      readOnly(journalAccountLineExistsSql(key, amount, zero));
    }
  });

  test('r25 helper literals are defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('scopeExistsSql =')).toHaveLength(2);
    expect(shared.split('journalAccountLineExistsSql =')).toHaveLength(2);
  });

  test('two-predicate EXISTS guards are composed, never inlined, in the package-obligation adapters', () => {
    const mfg = src('manufacturing-package-obligations.ts');
    const sales = src('sales-package-obligations.ts');
    const supply = src('supply-package-obligations.ts');
    for (const [, , , , , , expected] of scopeCases) {
      expect(mfg).not.toContain(expected);
      expect(sales).not.toContain(expected);
      expect(supply).not.toContain(expected);
    }
    expect(mfg.match(/scopeExistsSql\(/g) ?? []).toHaveLength(1);
    expect(sales.match(/scopeExistsSql\(/g) ?? []).toHaveLength(1);
    expect(supply.match(/scopeExistsSql\(/g) ?? []).toHaveLength(2);
  });

  test('journal account-line EXISTS guards are composed, never inlined, in finance-record-sources', () => {
    const finance = src('finance-record-sources.ts');
    for (const [, , , expected] of journalCases) expect(finance).not.toContain(expected);
    expect(finance.match(/journalAccountLineExistsSql\(/g) ?? []).toHaveLength(2);
  });

  test('retirement:ts-statements stays strictly below the r25 BEFORE=373 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(373);
  });
});
