import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  SALES_RD_PURCHASE_PROJECTION_C_READ,
  SALES_RD_PURCHASE_PROJECTION_C_DENY,
  SALES_RD_PURCHASE_PROJECTION_FP_READ,
  SALES_RD_PURCHASE_PROJECTION_FP_DENY,
} from '../../../modules/sales/server/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../modules/sales/server/src');
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
  return registry.slice(at + marker.length, end).replace(/\\n/g, '\n').replace(/\\"/g, '"');
}

const families = [
  ['C_READ', 'SQL_PURCHASE_PROJECTION_C_READ', SALES_RD_PURCHASE_PROJECTION_C_READ, 2920, 'ee05d2e644d2e48a8f3bb15a1e79d49862da63c0493af1ecbc1ede09ed25280b'],
  ['C_DENY', 'SQL_PURCHASE_PROJECTION_C_DENY', SALES_RD_PURCHASE_PROJECTION_C_DENY, 2921, 'a6040668166b999b2507e071e6b6c47defd2a565de5a553f073212432f820dcb'],
  ['FP_READ', 'SQL_PURCHASE_PROJECTION_FP_READ', SALES_RD_PURCHASE_PROJECTION_FP_READ, 2956, 'b8322970d8d850085116ba9e937902e4000c884dce0bdf95a80cf53cf2bcc77e'],
  ['FP_DENY', 'SQL_PURCHASE_PROJECTION_FP_DENY', SALES_RD_PURCHASE_PROJECTION_FP_DENY, 2957, '135ea23eb2f899381ac5333ec3c52a7817ed6e8217acef4f11eb90e43dcd0de3'],
] as const;

describe('native Sales followup purchase projection', () => {
  test.each(families)('%s statement is byte-identical and allow-listed', (_name, constant, kind, length, hash) => {
    const sql = rustSql(constant);
    expect(sql).toHaveLength(length);
    expect(createHash('sha256').update(sql).digest('hex')).toBe(hash);
    expect((sql.match(/\?/g) ?? []).length).toBe(7);
    expect(sql).toContain('CROSS JOIN followup_clock WHERE followup_native.id=');
    expect(registry).toContain(`"${kind}"`);
    expect(registry).toContain('bind_count: 0,');
    for (const word of ['INSERT', 'UPDATE ', 'DELETE', 'LOCK TABLE']) expect(sql.toUpperCase()).not.toContain(word);
  });

  test('TS selects the owner kind by alias and permission with unchanged binds', () => {
    for (const kind of families) expect(transport).toContain(`export const ${kind[1].replace('SQL_', 'SALES_RD_')} = '${kind[2]}';`);
    expect(purchases).toContain("const read=hasPermission(actor,'commerce.read');");
    expect(purchases).toContain("alias==='c'\n  ? (read?SALES_RD_PURCHASE_PROJECTION_C_READ:SALES_RD_PURCHASE_PROJECTION_C_DENY)\n  : (read?SALES_RD_PURCHASE_PROJECTION_FP_READ:SALES_RD_PURCHASE_PROJECTION_FP_DENY);");
    expect(purchases).toContain('const where=(await nativeSalesPlan(kind,[])).sql;');
    expect(purchases).toContain('args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role]');
    expect(purchases).not.toContain('WITH followup_clock AS MATERIALIZED');
    expect(purchases).not.toContain('purchasedCapabilityPredicate');
    expect(purchases).toContain("assignmentReadSql('native','commerce.sales')");
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
