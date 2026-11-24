import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { salesPackageSettlementSources } from '../../../packages/server-sales/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const src = readFileSync(resolve(srcRoot, 'adapters/package-obligations.ts'), 'utf8');

// Pre-dedupe literals, copied from HEAD, used only as the byte-identity oracle.
const pre = {
  followups: 'SELECT * FROM customer_followups',
  followupEvents: 'SELECT * FROM customer_followup_events',
  notices: 'SELECT * FROM commerce_customer_notifications',
  noticeAuthorities: 'SELECT * FROM sales_notification_execution_authorizations',
  noticeAttempts: 'SELECT * FROM sales_notification_execution_attempts',
} as const;

describe('Sales read-tail r13 allRows generator family', () => {
  test.each(Object.entries(pre) as [keyof typeof pre, string][])(
    '%s stays byte-identical to the pre-dedupe literal',
    (key, literal) => {
      expect(salesPackageSettlementSources[key]).toBe(literal);
    },
  );

  test('the family is one generator literal with five callers and no inlining', () => {
    expect(src.match(/const allRows=/g) ?? []).toHaveLength(1);
    expect(src.match(/allRows\('/g) ?? []).toHaveLength(5);
    expect(src).not.toMatch(/'SELECT \* FROM (customer_followups|commerce_customer_notifications|sales_notification_execution_authorizations)/);
  });

  test('source locks stay independent and untouched', () => {
    expect(src).toContain("LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE");
    expect(src).toContain('salesCommerceSettlementSourceReadLocks(database)');
    expect(src).toContain('crmFinanceSourceReadLocks(database)');
    const family = src.slice(src.indexOf('const allRows='), src.indexOf('type Input='));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Sales read-tail r13 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r13 BEFORE=97 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(97);
  });
});
