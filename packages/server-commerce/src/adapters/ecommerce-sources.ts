import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import { COMMERCE_ACTIVE_WAREHOUSES_SQL, COMMERCE_WAREHOUSE_ACTIVE_SQL } from './guarded-source-dedupe-sql';
import { nativeCommerceReadAll, RD_PRODUCT_ACTIVE_OPTIONS } from './native-commerce-read';
/** Owner reads use the caller's connection, including checkout transactions.
 * The functional-currency configuration read is owned by the native Commerce
 * currency port (`native-ecommerce-sources.ts`); no TS SQL mirror is retained.
 * r19: the active-options read delegates to the mounted native Commerce
 * command-plan port; the tabbed literal below is the normalized authority text. */
const COMMERCE_PRODUCT_ACTIVE_OPTIONS_SQL = "SELECT	id,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name";
export async function ecommerceProductCandidates(db: SqlDatabase, companyId: string) {
    return nativeCommerceReadAll<Row>(RD_PRODUCT_ACTIVE_OPTIONS, [companyId], db);
}
export async function ecommerceWarehouseCandidates(db: SqlDatabase, actor: WorkContext) {
    return (await db.prepare(COMMERCE_ACTIVE_WAREHOUSES_SQL).bind(actor.companyId, actor.branchId, actor.branchId).all<Row>()).results;
}
export function ecommerceWarehouse(db: SqlDatabase, actor: WorkContext, id: string) {
    return db.prepare(COMMERCE_WAREHOUSE_ACTIVE_SQL).bind(id, actor.companyId, actor.branchId, actor.branchId).first<Row>();
}
export async function ecommerceFulfillmentCurrent(db: SqlDatabase, id: string, orderId: unknown, version: number) {
    return !!await db.prepare('SELECT 1 FROM commerce_fulfillments WHERE id=? AND order_id=? AND version=?').bind(id, orderId, version).first();
}
