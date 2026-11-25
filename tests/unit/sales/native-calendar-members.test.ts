import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  SALES_RD_CALENDAR_JSONB_MEMBER,
  SALES_RD_CALENDAR_OWNER_MEMBER,
} from '../../../packages/server-sales/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const facts = read('calendar-customer-followups-facts.ts');
const transport = read('native-sales-read.ts');
const obligations = read('package-obligations.ts');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

function rustSql(constant: string): string {
  const marker = `const ${constant}: &str = "`;
  const at = registry.indexOf(marker);
  expect(at).toBeGreaterThanOrEqual(0);
  const end = registry.indexOf('";', at + marker.length);
  expect(end).toBeGreaterThan(at);
  return registry.slice(at + marker.length, end).replace(/\\"/g, '"');
}

const families = [
  ['JSONB', 'SQL_CALENDAR_JSONB_MEMBER', SALES_RD_CALENDAR_JSONB_MEMBER, 47, 'ddb288d52bf8acb52a47bf2eea49610d30b148853bd7fe66f8f3f0891f30a2de'],
  ['OWNER', 'SQL_CALENDAR_OWNER_MEMBER', SALES_RD_CALENDAR_OWNER_MEMBER, 286, '1b151d5c3b91f97a545426aad55b63b5ff385d010e05867d9c02cab041233751'],
] as const;

describe('native Sales calendar membership fragments', () => {
  test.each(families)('%s fragment is byte-identical and allow-listed', (_name, constant, kind, length, hash) => {
    const sql = rustSql(constant);
    expect(sql).toHaveLength(length);
    expect(sha(sql)).toBe(hash);
    expect((sql.match(/\?/g) ?? []).length).toBe(1);
    expect(sql.startsWith('IN')).toBe(true);
    expect(registry).toContain(`"${kind}"`);
    expect(registry).toContain('bind_count: 0,');
    expect(sql.toUpperCase()).not.toMatch(/INSERT|UPDATE |DELETE|LOCK TABLE/);
  });

  test('TS facts compose the planned fragments with the fixed expression', () => {
    expect(transport).toContain(`export const SALES_RD_CALENDAR_JSONB_MEMBER = '${SALES_RD_CALENDAR_JSONB_MEMBER}';`);
    expect(transport).toContain(`export const SALES_RD_CALENDAR_OWNER_MEMBER = '${SALES_RD_CALENDAR_OWNER_MEMBER}';`);
    expect(facts).toContain('nativeSalesPlan(SALES_RD_CALENDAR_JSONB_MEMBER,[])');
    expect(facts).toContain('nativeSalesPlan(SALES_RD_CALENDAR_OWNER_MEMBER,[])');
    expect(facts).toContain('coalesce(o.department_id,o.branch_id,o.company_id) ${jsonbMember}');
    expect(facts).toContain('c.owner_id ${ownerMember}');
    expect(facts).not.toContain('jsonb_array_elements_text');
    expect(facts).not.toContain('people_payrollProjectionSources');
    expect(facts).not.toContain('const inList');
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
