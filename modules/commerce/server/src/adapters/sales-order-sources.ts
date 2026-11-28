import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/finance-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as salesProjectionSources } from "@erpx/sales-server/public/commercial-owner-projection-sources";
import 'server-only';
import {nativeCommerceReadAll,nativeCommerceReadFirst,RD_CURRENCY_BY_COMPANY,RD_SALES_NOTIFICATION_LINES} from './native-commerce-read';
import type { SqlDatabase, SqlResult } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type {Row} from '@erpx/commerce-server/public/commerce';
import {
  nativeSalesCustomerOpenOrders,
  nativeSalesOrderCounts,
  nativeSalesPricingProduct,
  nativeSalesPricingProducts,
  nativeSalesSupportOrderSource,
} from './native-lane-14/sales-client';
function orderScope(actor: WorkContext, alias = 'o') { return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] }; }
export function salesPricingCurrency(write: SqlDatabase, actor: WorkContext) { return nativeCommerceReadFirst<Row>(RD_CURRENCY_BY_COMPANY, [actor.companyId], write); }
/** Native Sales reads: `write` stays in each signature for call-site
 * compatibility only. No SQL is issued on it for this family. */
export function salesPricingProducts(_write: SqlDatabase, actor: WorkContext): Promise<SqlResult<Row>> {
    return nativeSalesPricingProducts(actor);
}
export function salesPricingProduct(_write: SqlDatabase, actor: WorkContext, id: string): Promise<Row | null> {
    return nativeSalesPricingProduct(actor, id);
}
export async function salesOrderCounts(_write: SqlDatabase, actor: WorkContext, ids: string[]): Promise<{
    party_id: string;
    count: number;
}[]> {
    return (await nativeSalesOrderCounts(actor, ids)) as { party_id: string; count: number }[];
}
export async function salesCustomerOpenOrders(_write: SqlDatabase, actor: WorkContext, id: string): Promise<Row[]> {
    return nativeSalesCustomerOpenOrders(actor, id);
}
export async function salesSupportOrderSource(_write: SqlDatabase, actor: WorkContext, id: string): Promise<{ id: string } | null> {
    return nativeSalesSupportOrderSource(actor, id);
}
export async function salesOrderWorkRows(write: SqlDatabase, actor: WorkContext) { const s = orderScope(actor); return (await write.prepare(`SELECT o.*,p.name customer_name FROM commerce_orders o JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id WHERE ${s.sql} AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') ORDER BY o.due_date,o.id`).bind(...s.args).all<Row>()).results; }
export async function salesDeliveryNotificationFacts(write: SqlDatabase, actor: WorkContext, fulfillmentId: string) { const s = orderScope(actor); return await write.prepare(`SELECT f.id fulfillment_id,f.version fulfillment_version,f.number fulfillment_number,f.fulfillment_date,o.id order_id,o.number order_number,o.branch_id,o.department_id,p.id party_id,p.name party_name,p.version party_version,p.notification_email,p.notification_consent,ns.adapter,ns.version settings_version FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id LEFT JOIN (${salesProjectionSources.commerce_customer_notification_settings}) ns ON ns.company_id=o.company_id WHERE f.id=? AND o.kind='sales' AND ${s.sql}`).bind(fulfillmentId, ...s.args).first<Row>(); }
export async function salesDeliveryNotificationLines(write: SqlDatabase, actor: WorkContext, fulfillmentId: string) { return nativeCommerceReadAll<Row>(RD_SALES_NOTIFICATION_LINES,[fulfillmentId],write); }

/** Source leases for the named Sales price-list owner, before its first snapshot. */
export function salesPricingPurchaseSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_settings,commerce_products IN SHARE MODE')];}
