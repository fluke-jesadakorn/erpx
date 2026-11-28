import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type {Row} from '@erpx/commerce-server/public/commerce';
import { nativeCommerceReadAll, nativeCommerceReadFirst, RD_PRODUCT_ACTIVE_OPTIONS, RD_WAREHOUSE_ACTIVE, RD_WAREHOUSES_ACTIVE } from './native-commerce-read';
import { commerceFulfillmentGuardSql } from './guarded-source-dedupe-sql';
/** Owner reads use the caller's connection, including checkout transactions.
 * The functional-currency configuration read is owned by the native Commerce
 * currency port (`native-ecommerce-sources.ts`); no TS SQL mirror is retained.
 * r19: the active-options read delegates to the mounted native Commerce
 * command-plan port; the tabbed literal below is the normalized authority text.
 * r20: the warehouse candidate/active reads delegate to the same port; their
 * authority text lives in `guarded-source-dedupe-sql.ts`. */
const COMMERCE_PRODUCT_ACTIVE_OPTIONS_SQL = "SELECT	id,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name";
export async function ecommerceProductCandidates(db: SqlDatabase, companyId: string) {
    return nativeCommerceReadAll<Row>(RD_PRODUCT_ACTIVE_OPTIONS, [companyId], db);
}
export async function ecommerceWarehouseCandidates(db: SqlDatabase, actor: WorkContext) {
    return nativeCommerceReadAll<Row>(RD_WAREHOUSES_ACTIVE, [actor.companyId, actor.branchId, actor.branchId], db);
}
export function ecommerceWarehouse(db: SqlDatabase, actor: WorkContext, id: string) {
    return nativeCommerceReadFirst<Row>(RD_WAREHOUSE_ACTIVE, [id, actor.companyId, actor.branchId, actor.branchId], db);
}
export async function ecommerceFulfillmentCurrent(db: SqlDatabase, id: string, orderId: unknown, version: number) {
    return !!await db.prepare(commerceFulfillmentGuardSql('id=? AND order_id=? AND version=?')).bind(id, orderId, version).first();
}
