import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { crmCommerceOrderSources } from '@erpx/server-commerce/public/crm-purchase-sources';
import { crmFinanceCustomerSources } from '@erpx/server-finance/public/crm-purchase-sources';
import { followupMetricPrefix } from '../../../packages/server-sales/src/adapters/followup-source-sql';
import {
  SALES_RD_FOLLOWUP_SOURCE_FOLLOWUP,
  SALES_RD_FOLLOWUP_SOURCE_ORDER,
} from '../../../packages/server-sales/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const purchases = read('followup-purchases.ts');
const transport = read('native-sales-read.ts');
const obligations = read('package-obligations.ts');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

/** Pre-cut rendered source reads, recomputed from owner-fixed CRM projections. */
const orders = `SELECT actual.*,jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency) witness FROM (${crmCommerceOrderSources}) actual JOIN (${crmFinanceCustomerSources}) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' WHERE actual.kind IN ('quote','sales')`;
const rows = `${followupMetricPrefix},jsonb_build_object('followup',jsonb_build_array(followup.id,followup.company_id,followup.order_id,followup.created_by),'order',actual.witness) witness FROM customer_followups followup JOIN (${orders}) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id`;
const tail = ' followup_source WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)';
const followup = `SELECT id,company_id,branch_id,department_id,order_kind,witness::text witness,owner_id,due_date,status,version FROM (${rows})${tail}`;
const order = `SELECT id,company_id,branch_id,department_id,order_kind,witness::text witness,NULL::text owner_id,NULL::text due_date,NULL::text status,NULL::bigint version FROM (SELECT actual.*,actual.kind order_kind FROM (${orders}) actual)${tail}`;

const families = [
  ['followup', SALES_RD_FOLLOWUP_SOURCE_FOLLOWUP, followup, '2d0c5dc88dae443e46e4b69f93030841ca94280049b76d57c81cd641f20b3b2f'],
  ['order', SALES_RD_FOLLOWUP_SOURCE_ORDER, order, 'afaf9a042df4f14a3e32856ac47f269bc9f76a99a8398d417ca9e34f6a1a305f'],
] as const;

describe('native Sales followup purchase source reads', () => {
  test.each(families)('%s source is byte-identical in the Rust registry', (_name, kind, sql, hash) => {
    expect((sql.match(/\?/g) ?? []).length).toBe(6);
    expect(sha(sql)).toBe(hash);
    const escaped = sql.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
    expect(registry).toContain(`"${kind}"`);
    expect(registry).toContain(`"${escaped}"`);
    expect(registry).toContain('bind_count: 6,');
    for (const word of ['INSERT', 'UPDATE ', 'DELETE', 'LOCK TABLE']) expect(sql.toUpperCase()).not.toContain(word);
  });

  test('TS adapter sends only bounded identities with no SQL mirror', () => {
    expect(transport).toContain(`export const SALES_RD_FOLLOWUP_SOURCE_FOLLOWUP = '${SALES_RD_FOLLOWUP_SOURCE_FOLLOWUP}';`);
    expect(transport).toContain(`export const SALES_RD_FOLLOWUP_SOURCE_ORDER = '${SALES_RD_FOLLOWUP_SOURCE_ORDER}';`);
    expect(purchases).toContain('const values=[target.id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];');
    expect(purchases).toContain('nativeSalesReadFirst<Source>(SALES_RD_FOLLOWUP_SOURCE_FOLLOWUP,values,database)');
    expect(purchases).toContain('nativeSalesReadFirst<Source>(SALES_RD_FOLLOWUP_SOURCE_ORDER,values,database)');
    expect(purchases).not.toContain('witness::text witness,${target.kind');
    expect(purchases).toContain('const followupExists=');
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
    expect(purchases).toContain('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases.match(/LOCK TABLE/g) ?? []).toHaveLength(1);
    expect(obligations).toContain('LOCK TABLE sales_price_lists,sales_price_list_items,customer_followups,customer_followup_events,customer_contact_notes,customer_contact_note_history,commerce_customer_notifications,commerce_customer_notification_settings,sales_notification_execution_authorizations,sales_notification_execution_attempts,sales_notification_reconciliations,command_receipts,audit_events IN SHARE MODE');
  });
});
