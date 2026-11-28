import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { commercialOwnerProjectionSources as commerce } from '@erpx/commerce-server/public/commercial-owner-projection-sources';
import {
  SALES_RD_PRICING_LIST_BY_ID,
  SALES_RD_PRICING_LIST_ACTIVE_BY_ID,
  SALES_RD_PRICING_LIST_ACTIVE,
  SALES_RD_PRICING_ITEMS_BY_LIST,
} from '../../../modules/sales/server/src/adapters/native-sales-read';

const srcRoot = resolve(__dirname, '../../../modules/sales/server/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = read('native_sales_read_sql.rs');
const pricing = read('pricing.ts');
const transport = read('native-sales-read.ts');
const obligations = read('package-obligations.ts');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

/** Pre-cut rendered TypeScript statements, recomputed from the owner-local
 * Commerce settings projection. No column, bind or order may drift. */
const currency = `(SELECT currency FROM (${commerce.commerce_settings}) commerce_settings WHERE company_id=?)`;
const head = (tail: string) => `SELECT * FROM sales_price_lists ${tail}`;
const byId = head('WHERE id=? AND company_id=?');
const byIdActive = `${byId} AND status='active' AND currency=${currency} AND valid_from<=? AND valid_until>=? AND (customer_id IS NULL OR customer_id=?)`;
const active = head(`WHERE company_id=? AND status='active' AND currency=${currency} AND valid_from<=? AND valid_until>=? AND (customer_id=? OR customer_id IS NULL) ORDER BY CASE WHEN customer_id=? THEN 0 ELSE 1 END,valid_from DESC,activated_at DESC LIMIT 1`);
const items = 'SELECT * FROM sales_price_list_items WHERE price_list_id=?';

const families = [
  ['byId', SALES_RD_PRICING_LIST_BY_ID, byId, 2, 59, 'e392053f5374623de9070539bc8d485bd767b6358821040b043b54b1f601d5e5'],
  ['byIdActive', SALES_RD_PRICING_LIST_ACTIVE_BY_ID, byIdActive, 6, 493, '521812b680d2daa33606dc5e046b39e275647262e492e098fac11c21a109f25e'],
  ['active', SALES_RD_PRICING_LIST_ACTIVE, active, 6, 577, '56372974db963b1381b116eb44c6e341906f1fc5fe6eb66df4d43f92ba635efd'],
  ['items', SALES_RD_PRICING_ITEMS_BY_LIST, items, 1, 58, '9b0969157e73761ab0092356994690afa36acf6e787ed9aa000c8217c8ccdf4a'],
] as const;

describe('native Sales pricing resolution reads', () => {
  test.each(families)('%s is byte-identical in the Rust command-plan registry', (_name, kind, sql, binds, length, hash) => {
    expect((sql.match(/\?/g) ?? []).length).toBe(binds);
    expect(sql).toHaveLength(length);
    expect(sha(sql)).toBe(hash);
    const escaped = sql.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
    expect(registry).toContain(`"${kind}"`);
    expect(registry).toContain(`"${escaped}"`);
    expect(registry).toContain(`bind_count: ${binds},`);
    for (const word of ['INSERT', 'UPDATE ', 'DELETE', 'LOCK TABLE']) expect(sql.toUpperCase()).not.toContain(word);
  });

  test('TS transport sends only bounded identities and keeps no SQL head', () => {
    expect(transport).toContain(`export const SALES_RD_PRICING_LIST_BY_ID = '${SALES_RD_PRICING_LIST_BY_ID}';`);
    expect(transport).toContain(`export const SALES_RD_PRICING_LIST_ACTIVE_BY_ID = '${SALES_RD_PRICING_LIST_ACTIVE_BY_ID}';`);
    expect(transport).toContain(`export const SALES_RD_PRICING_LIST_ACTIVE = '${SALES_RD_PRICING_LIST_ACTIVE}';`);
    expect(transport).toContain(`export const SALES_RD_PRICING_ITEMS_BY_LIST = '${SALES_RD_PRICING_ITEMS_BY_LIST}';`);
    expect(pricing).toContain('nativeSalesReadFirst<Row>(SALES_RD_PRICING_LIST_BY_ID,[id,actor.companyId],write)');
    expect(pricing).toContain('nativeSalesReadFirst<Row>(SALES_RD_PRICING_LIST_ACTIVE_BY_ID,[input.priceListId,actor.companyId,actor.companyId,input.orderDate,input.orderDate,input.partyId],db)');
    expect(pricing).toContain('nativeSalesReadFirst<Row>(SALES_RD_PRICING_LIST_ACTIVE,[actor.companyId,actor.companyId,input.orderDate,input.orderDate,input.partyId,input.partyId],db)');
    expect(pricing).toContain('nativeSalesReadAll<Row>(SALES_RD_PRICING_ITEMS_BY_LIST,[id],db)');
    expect(pricing).not.toContain('priceListSelect');
    expect(pricing).not.toContain('activeCurrencySql');
    expect(pricing).not.toContain('byKey(');
    expect(() => read('pricing-source-sql.ts')).toThrow();
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
