import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  SALES_RD_CRM_ASSIGNMENT_PLAIN,
  SALES_RD_CRM_ASSIGNMENT_ALIASED,
} from '../../../modules/sales/server/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../modules/sales/server/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const metric = read('metric-purchases.ts');
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

const families = [
  ['PLAIN', 'SQL_CRM_ASSIGNMENT_PLAIN', SALES_RD_CRM_ASSIGNMENT_PLAIN, '5bc9629ac3f8ebadd9419e0170858bafe2041a1c5fda556ee1aae7ce20941288'],
  ['ALIASED', 'SQL_CRM_ASSIGNMENT_ALIASED', SALES_RD_CRM_ASSIGNMENT_ALIASED, '5aeb0f3f81af8be5e4ab51fbff65336915defd1278688c67dc4c586ff039cb10'],
] as const;

describe('native Sales CRM assignment predicates', () => {
  test.each(families)('%s predicate is byte-identical and allow-listed', (_name, constant, kind, hash) => {
    const sql = rustSql(constant);
    expect(createHash('sha256').update(sql).digest('hex')).toBe(hash);
    expect((sql.match(/\?/g) ?? []).length).toBe(1);
    expect(registry).toContain(`"${kind}"`);
    expect(registry).toContain('bind_count: 0,');
    expect(sql.toUpperCase()).not.toMatch(/INSERT|UPDATE |DELETE|LOCK TABLE/);
  });

  test('TS metric adapter plans both predicates in Rust', () => {
    expect(transport).toContain(`export const SALES_RD_CRM_ASSIGNMENT_PLAIN = '${SALES_RD_CRM_ASSIGNMENT_PLAIN}';`);
    expect(transport).toContain(`export const SALES_RD_CRM_ASSIGNMENT_ALIASED = '${SALES_RD_CRM_ASSIGNMENT_ALIASED}';`);
    expect(metric).toContain("const crmAssignmentExists=async(alias='')=>(await nativeSalesPlan(alias?SALES_RD_CRM_ASSIGNMENT_ALIASED:SALES_RD_CRM_ASSIGNMENT_PLAIN,[])).sql;");
    expect(metric).toContain(' AND ${await crmAssignmentExists()}');
    expect(metric).not.toContain('jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array');
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
