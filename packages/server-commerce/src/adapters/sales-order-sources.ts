import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as salesProjectionSources } from "@erpx/server-sales/public/commercial-owner-projection-sources";
import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { Row } from '@erpx/server-commerce/public/commerce';
function orderScope(actor: WorkContext, alias = 'o') { return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] }; }
export function salesPricingCurrency(write: SqlDatabase, actor: WorkContext) { return write.prepare('SELECT currency FROM commerce_settings WHERE company_id=?').bind(actor.companyId).first<Row>(); }
export function salesPricingProducts(write: SqlDatabase, actor: WorkContext) { return write.prepare("SELECT id,sku,name,tax_bps FROM commerce_products WHERE company_id=? AND status='active' ORDER BY sku").bind(actor.companyId).all<Row>(); }
export function salesPricingProduct(write: SqlDatabase, actor: WorkContext, id: string) { return write.prepare("SELECT id,version FROM commerce_products WHERE id=? AND company_id=? AND status='active'").bind(id, actor.companyId).first<Row>(); }
export async function salesOrderCounts(write: SqlDatabase, actor: WorkContext, ids: string[]) {
    const s = orderScope(actor), placeholders = ids.map(() => '?').join(',');
    return (await write.prepare(`SELECT o.party_id,COUNT(*) count FROM commerce_orders o WHERE ${s.sql} AND o.party_id IN (${placeholders}) AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') GROUP BY o.party_id`).bind(...s.args, ...ids).all<{
        party_id: string;
        count: number;
    }>()).results;
}
export async function salesCustomerOpenOrders(write: SqlDatabase, actor: WorkContext, id: string) { const s = orderScope(actor); return (await write.prepare(`SELECT o.id,o.number,o.kind,o.state,o.due_date,o.currency,(o.subtotal_minor+o.tax_minor) total_minor,o.version FROM commerce_orders o WHERE ${s.sql} AND o.party_id=? AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') ORDER BY o.due_date,o.id LIMIT 30`).bind(...s.args, id).all<Row>()).results; }
export async function salesSupportOrderSource(write: SqlDatabase, actor: WorkContext, id: string) {
    const s = orderScope(actor);
    return await write.prepare(`SELECT o.id FROM commerce_orders o WHERE ${s.sql} AND o.party_id=? AND o.kind IN ('quote','sales') ORDER BY o.updated_at DESC LIMIT 1`).bind(...s.args, id).first<{
        id: string;
    }>();
}
export async function salesOrderWorkRows(write: SqlDatabase, actor: WorkContext) { const s = orderScope(actor); return (await write.prepare(`SELECT o.*,p.name customer_name FROM commerce_orders o JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id WHERE ${s.sql} AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') ORDER BY o.due_date,o.id`).bind(...s.args).all<Row>()).results; }
export async function salesDeliveryNotificationFacts(write: SqlDatabase, actor: WorkContext, fulfillmentId: string) { const s = orderScope(actor); return await write.prepare(`SELECT f.id fulfillment_id,f.version fulfillment_version,f.number fulfillment_number,f.fulfillment_date,o.id order_id,o.number order_number,o.branch_id,o.department_id,p.id party_id,p.name party_name,p.version party_version,p.notification_email,p.notification_consent,ns.adapter,ns.version settings_version FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id LEFT JOIN (${salesProjectionSources.commerce_customer_notification_settings}) ns ON ns.company_id=o.company_id WHERE f.id=? AND o.kind='sales' AND ${s.sql}`).bind(fulfillmentId, ...s.args).first<Row>(); }
export async function salesDeliveryNotificationLines(write: SqlDatabase, actor: WorkContext, fulfillmentId: string) { return (await write.prepare('SELECT p.sku,fl.quantity_milli,p.unit FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal').bind(fulfillmentId).all<Row>()).results; }

/** Source leases for the named Sales price-list owner, before its first snapshot. */
export function salesPricingPurchaseSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_settings,commerce_products IN SHARE MODE')];}
