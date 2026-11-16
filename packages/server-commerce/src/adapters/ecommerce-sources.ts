import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
/** Owner reads use the caller's connection, including checkout transactions. */
export function ecommerceCurrencyConfiguration(db: SqlDatabase, companyId: string) {
    return db.prepare(`SELECT s.currency,c.base_currency FROM commerce_settings s JOIN (${financeProjectionSources.finance_charts}) c ON c.id=s.chart_id AND c.company_id=s.company_id WHERE s.company_id=?`).bind(companyId).first<{
        currency: string;
        base_currency: string;
    }>();
}
export async function ecommerceProductCandidates(db: SqlDatabase, companyId: string) {
    return (await db.prepare("SELECT id,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name").bind(companyId).all<Row>()).results;
}
export async function ecommerceWarehouseCandidates(db: SqlDatabase, actor: WorkContext) {
    return (await db.prepare("SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name").bind(actor.companyId, actor.branchId, actor.branchId).all<Row>()).results;
}
export function ecommerceWarehouse(db: SqlDatabase, actor: WorkContext, id: string) {
    return db.prepare("SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)").bind(id, actor.companyId, actor.branchId, actor.branchId).first<Row>();
}
export async function ecommerceFulfillmentCurrent(db: SqlDatabase, id: string, orderId: unknown, version: number) {
    return !!await db.prepare('SELECT 1 FROM commerce_fulfillments WHERE id=? AND order_id=? AND version=?').bind(id, orderId, version).first();
}
