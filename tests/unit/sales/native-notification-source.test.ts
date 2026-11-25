import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  SALES_RD_NOTIFICATION_SOURCE_DEV,
  SALES_RD_NOTIFICATION_SOURCE_PROD,
} from '../../../packages/server-sales/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const purchases = read('notification-purchases.ts');
const transport = read('native-sales-read.ts');
const obligations = read('package-obligations.ts');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const unescape = (value: string) =>
  value.replace(/\\n/g, '\n').replace(/\\"/g, '"').replace(/\\\\/g, '\\');
function rustSql(constant: string): string {
  const marker = `const ${constant}: &str = "`;
  const at = registry.indexOf(marker);
  expect(at).toBeGreaterThanOrEqual(0);
  const end = registry.indexOf('";', at + marker.length);
  expect(end).toBeGreaterThan(at);
  return unescape(registry.slice(at + marker.length, end));
}

const families = [
  ['DEV', 'SQL_NOTIFICATION_SOURCE_DEV', SALES_RD_NOTIFICATION_SOURCE_DEV, 1438, '6e7549ae15e02405837a5c54303e1586b55d8bdb56fae07350577f4172af1d50'],
  ['PROD', 'SQL_NOTIFICATION_SOURCE_PROD', SALES_RD_NOTIFICATION_SOURCE_PROD, 1439, 'c1c3cacfdd40fb256879ac629c66a776dad7791086467ed0f84b774c77491d68'],
] as const;

describe('native Sales notification source reads', () => {
  test.each(families)('%s statement is byte-identical and allow-listed', (_name, constant, kind, length, hash) => {
    const sql = rustSql(constant);
    expect(sql).toHaveLength(length);
    expect(sha(sql)).toBe(hash);
    expect((sql.match(/\?/g) ?? []).length).toBe(1);
    expect(sql).toContain('FROM commerce_customer_notifications n');
    expect(registry).toContain(`"${kind}"`);
    expect(registry).toContain('bind_count: 1,');
    for (const word of ['INSERT', 'UPDATE ', 'DELETE', 'LOCK TABLE']) expect(sql.toUpperCase()).not.toContain(word);
  });

  test('the two environment variants differ only in the feature predicate', () => {
    const dev = rustSql('SQL_NOTIFICATION_SOURCE_DEV');
    const prod = rustSql('SQL_NOTIFICATION_SOURCE_PROD');
    expect(dev.length + 1).toBe(prod.length);
    expect(dev.replace('TRUE release_ready', 'FALSE release_ready')).toBe(prod);
    expect(prod).toContain('release_ready');
  });

  test('TS adapter selects the owner kind by environment with no SQL mirror', () => {
    expect(transport).toContain(`export const SALES_RD_NOTIFICATION_SOURCE_DEV = '${SALES_RD_NOTIFICATION_SOURCE_DEV}';`);
    expect(transport).toContain(`export const SALES_RD_NOTIFICATION_SOURCE_PROD = '${SALES_RD_NOTIFICATION_SOURCE_PROD}';`);
    expect(purchases).toContain("const kind=process.env.NODE_ENV==='production'?SALES_RD_NOTIFICATION_SOURCE_PROD:SALES_RD_NOTIFICATION_SOURCE_DEV;");
    expect(purchases).toContain('nativeSalesReadFirst<SalesNotificationSource>(kind,[id],database)');
    expect(purchases).not.toContain('jsonb_build_array(n.id');
    expect(purchases).not.toContain('scheduledFeaturePredicate');
    expect(purchases).not.toContain('salesNotificationDeliverySources');
    expect(purchases).not.toContain('salesNotificationCustomerSources');
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
    expect(read('followup-purchases.ts')).toContain('const followupExists=');
    expect(read('followup-purchases.ts').match(/LOCK TABLE/g) ?? []).toHaveLength(1);
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
  });
});
