import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { SALES_RD_FOLLOWUP_ORDERS } from '../../../packages/server-sales/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const purchases = read('followup-purchases.ts');
const transport = read('native-sales-read.ts');
const obligations = read('package-obligations.ts');

function rustSql(constant: string): string {
  const marker = `const ${constant}: &str = "`;
  const at = registry.indexOf(marker);
  expect(at).toBeGreaterThanOrEqual(0);
  const end = registry.indexOf('";', at + marker.length);
  expect(end).toBeGreaterThan(at);
  return registry.slice(at + marker.length, end).replace(/\\"/g, '"');
}

describe('native Sales followup orders source', () => {
  test('retired orders literal is byte-identical in the Rust registry', () => {
    const sql = rustSql('SQL_FOLLOWUP_ORDERS');
    expect(sql).toHaveLength(476);
    expect(createHash('sha256').update(sql).digest('hex')).toBe(
      '08324423ee9ace5ba060076c8e19d2dc1515d11675e68c33ff2f366ad38ec8b0',
    );
    expect((sql.match(/\?/g) ?? []).length).toBe(0);
    expect(sql).toContain("actual.kind IN ('quote','sales')");
    expect(registry).toContain(`"${SALES_RD_FOLLOWUP_ORDERS}"`);
    expect(registry).toContain('bind_count: 0,');
    expect(sql.toUpperCase()).not.toMatch(/INSERT|UPDATE |DELETE|LOCK TABLE/);
  });

  test('TS composes the owner-planned source with no SQL mirror', () => {
    expect(transport).toContain(`export const SALES_RD_FOLLOWUP_ORDERS = '${SALES_RD_FOLLOWUP_ORDERS}';`);
    expect(purchases).toContain('const followupOrders=async()=>(await nativeSalesPlan(SALES_RD_FOLLOWUP_ORDERS,[])).sql;');
    expect(purchases).toContain('export const followupPurchaseRows=async()=');
    expect((purchases.match(/await followupOrders\(\)/g) ?? []).length).toBe(2);
    expect((purchases.match(/await followupPurchaseRows\(\)/g) ?? []).length).toBe(1);
    expect(purchases).not.toContain('const orders=`SELECT');
    expect(purchases).not.toContain('crmCommerceOrderSources');
    expect(purchases).not.toContain('crmFinanceCustomerSources');
  });

  test('r7-r17 generators and source locks stay untouched', () => {
    for (const literal of [
      'const orderDeliveryExistsSql=',
      'const commandAuditPrefixSql=',
      'const allRows=',
      'const factKind=',
      'const scopedSource=',
      'const ecommerceOrderScope=',
    ]) expect(obligations).toContain(literal);
    expect(purchases).toContain('const followupExists=');
    expect(purchases.match(/LOCK TABLE/g) ?? []).toHaveLength(1);
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
  });
});
