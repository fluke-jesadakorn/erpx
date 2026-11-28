import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import { COMMERCE_WAREHOUSE_IDENTITY_SQL } from "./guarded-source-dedupe-sql";
/** Only existing owner product/warehouse identity; shared Stock commands do not
 * acquire a Manufacturing purchase requirement. */
export const manufacturingWarehouseSources=COMMERCE_WAREHOUSE_IDENTITY_SQL;
export const manufacturingProductSources=`SELECT id,company_id,status,item_type FROM commerce_products`;
export function manufacturingCommerceSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE commerce_warehouses,commerce_products IN SHARE MODE")];}
