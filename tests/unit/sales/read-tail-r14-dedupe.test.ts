import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { readSalesPackageSettlementFacts } from '../../../packages/server-sales/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const src = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');

// SHA-256 of the full statement rendered by HEAD before the r14 dedupe, for
// both selections. Byte identity of the whole query is the oracle: no branch
// predicate, bind order or relation may drift under the generator refactor.
const preRendered = {
  sales: '6e58228f3864bfe8bc0af13a59ec45bb2682081a8c83fef082f610a8a4c9ba5f',
  ecommerce: '3706e17efee2b1ca4f41237de188b66864314297adc4c11a5d7c5ab3e4a29684',
} as const;

async function rendered(selection: 'sales' | 'ecommerce'): Promise<string> {
  const captured: string[] = [];
  const db = {
    prepare(sql: string) {
      captured.push(sql);
      return { bind: () => ({ all: async () => ({ results: [] }) }) };
    },
  };
  await readSalesPackageSettlementFacts(db as never, 'c1', selection);
  if (captured.length !== 1) throw new Error(`expected one prepared statement, got ${captured.length}`);
  return captured[0]!;
}

describe('Sales read-tail r14 factKind generator family', () => {
  test.each(Object.entries(preRendered) as ['sales' | 'ecommerce', string][])(
    '%s statement stays byte-identical to the pre-dedupe render',
    async (selection, sha) => {
      const sql = await rendered(selection);
      expect(createHash('sha256').update(sql).digest('hex')).toBe(sha);
    },
  );

  test('the family is one generator literal with 21 callers and no inlined prefix', () => {
    expect(src.match(/const factKind=/g) ?? []).toHaveLength(1);
    expect(src.match(/\$\{factKind\('/g) ?? []).toHaveLength(21);
    expect(src).not.toMatch(/UNION ALL SELECT '/);
  });

  test('r7-r13 generator/prefix helpers and source locks stay independent', () => {
    expect(src).toContain('const allRows='); // r13
    expect(src).toContain('const orderDeliveryExistsSql='); // r7
    expect(src).toContain('const commandAuditPrefixSql='); // r9
    expect(src).toContain('const ecommerceNoteOrderScope='); // r7
    expect(src).toContain('const ecommerceFollowupOrderScope='); // r7
    expect(src).toContain('const ecommerceNoticeOrderScope='); // r7
    expect(src).toContain('salesCommerceSettlementSourceReadLocks(database)');
    expect(src).toContain('crmFinanceSourceReadLocks(database)');
    expect(src).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
    const family = src.slice(src.indexOf('const factKind='), src.indexOf('type Input='));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Sales read-tail r14 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r14 BEFORE=93 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(93);
  });
});
