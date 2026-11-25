import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { assignmentReadSql } from '@erpx/server-identity_access/public/record-source-authority';
import { purchasedCapabilityPredicate } from '@erpx/server-commercial/public/purchased-capability-sql';
import { crmCommerceOrderSources } from '@erpx/server-commerce/public/crm-purchase-sources';
import { crmFinanceCustomerSources } from '@erpx/server-finance/public/crm-purchase-sources';
import { clockNowMs, followupMetricPrefix } from '../../../packages/server-sales/src/adapters/followup-source-sql';
import {
  SALES_RD_FOLLOWUP_METRIC_PROJECTION,
  SALES_RD_FOLLOWUP_METRIC_SOURCE,
} from '../../../packages/server-sales/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../packages/server-sales/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const metric = read('metric-purchases.ts');
const transport = read('native-sales-read.ts');
const obligations = read('package-obligations.ts');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

const crmAssignmentExists = (alias = '') => {
  const p = alias ? `${alias}.` : '';
  return `EXISTS(SELECT 1 FROM assignments${alias ? ` ${alias}` : ''} WHERE ${p}id=? AND (jsonb_array_length(${p}modules_json::jsonb)=0 OR ${p}modules_json::jsonb @> jsonb_build_array('crm'::text)))`;
};
/** Pre-cut renders, recomputed from the deterministic owner SQL ingredients. */
const followupSourceSelect = `${followupMetricPrefix} FROM customer_followups followup JOIN (${crmCommerceOrderSources}) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id`;
const rows = `${followupSourceSelect} JOIN (${crmFinanceCustomerSources}) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id WHERE actual.kind IN ('sales','quote')`;
const enforced = `WITH sales_metric_clock AS MATERIALIZED (${clockNowMs}) SELECT sales_metric_owned.* FROM (${rows}) sales_metric_owned CROSS JOIN sales_metric_clock WHERE ${assignmentReadSql('sales_metric_owned', 'commerce.read')} AND ((sales_metric_owned.order_kind='quote' AND ${purchasedCapabilityPredicate('sales_metric_owned.company_id', 'sales.quotes', 'history', 'sales_metric_clock.now_ms')}) OR (sales_metric_owned.order_kind='sales' AND ${purchasedCapabilityPredicate('sales_metric_owned.company_id', 'sales.orders', 'history', 'sales_metric_clock.now_ms')})) AND ${crmAssignmentExists('selected')}`;

const families = [
  ['projection', SALES_RD_FOLLOWUP_METRIC_PROJECTION, followupSourceSelect, 0, 'fd7bd6767880d22747f199c82464234a100f0d520a9ddfcaea35aa1b7389953b'],
  ['enforced', SALES_RD_FOLLOWUP_METRIC_SOURCE, enforced, 4, 'af6611326cd0f6fccc522166762fc4cb012fdf6e1820495bdfd84174029165e0'],
] as const;

describe('native Sales followup metric reads', () => {
  test.each(families)('%s plan is byte-identical in the Rust registry', (_name, kind, sql, binds, hash) => {
    expect((sql.match(/\?/g) ?? []).length).toBe(binds);
    expect(sha(sql)).toBe(hash);
    const escaped = sql.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n');
    expect(registry).toContain(`"${kind}"`);
    expect(registry).toContain(`"${escaped}"`);
    expect(registry).toContain(`bind_count: ${binds},`);
    expect(sql).toContain('customer_followups followup');
  });

  test('TS adapter plans in Rust and keeps no SQL mirror', () => {
    expect(transport).toContain('export async function nativeSalesPlan(');
    expect(transport).toContain(`export const SALES_RD_FOLLOWUP_METRIC_PROJECTION = '${SALES_RD_FOLLOWUP_METRIC_PROJECTION}';`);
    expect(transport).toContain(`export const SALES_RD_FOLLOWUP_METRIC_SOURCE = '${SALES_RD_FOLLOWUP_METRIC_SOURCE}';`);
    expect(metric).toContain('const kind=enforced?SALES_RD_FOLLOWUP_METRIC_SOURCE:SALES_RD_FOLLOWUP_METRIC_PROJECTION;');
    expect(metric).toContain('const values=enforced?[actor.assignmentId,actor.userId,actor.role,actor.assignmentId]:[];');
    expect(metric).toContain('const planned=await nativeSalesPlan(kind,values);');
    expect(metric).toContain('return {sql:planned.sql,args:planned.values};');
    expect(metric).not.toContain('WITH sales_metric_clock');
    expect(metric).not.toContain('purchasedCapabilityPredicate');
    expect(metric).not.toContain('followupMetricPrefix');
    expect(metric).toContain('const crmAssignmentExists=');
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
