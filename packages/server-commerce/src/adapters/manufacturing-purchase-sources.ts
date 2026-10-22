import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Only existing owner product/warehouse identity; shared Stock commands do not
 * acquire a Manufacturing purchase requirement. */
export const manufacturingWarehouseSources=`SELECT id,company_id,branch_id,status FROM commerce_warehouses`;
export const manufacturingProductSources=`SELECT id,company_id,status,item_type FROM commerce_products`;
export function manufacturingCommerceSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE commerce_warehouses,commerce_products IN SHARE MODE")];}
