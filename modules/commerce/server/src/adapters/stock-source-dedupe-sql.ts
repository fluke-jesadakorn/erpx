import { commercialOwnerProjectionSources } from "@erpx/inventory-server/public/commercial-owner-projection-sources";

/** Byte-identical Commerce→Inventory stock projection read text shared by the
 * read-tail call sites. Text only: each call site still prepares and executes
 * its own statement and no query result is cached here. */
export const COMMERCE_STOCK_BY_WAREHOUSE_PRODUCT_SQL = `SELECT * FROM (${commercialOwnerProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=?`;
export const COMMERCE_STOCK_ABSENT_GUARD_SQL = `NOT EXISTS(SELECT 1 FROM (${commercialOwnerProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=?)`;
export const COMMERCE_STOCK_VERSION_GUARD_SQL = `EXISTS(SELECT 1 FROM (${commercialOwnerProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? AND version=?)`;
export const COMMERCE_STOCK_SNAPSHOT_GUARD_SQL = `EXISTS(SELECT 1 FROM (${commercialOwnerProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? AND version=? AND quantity_milli=? AND value_minor=?)`;
export const COMMERCE_STOCK_PRODUCT_FOR_UPDATE_SQL = `SELECT product_id FROM (${commercialOwnerProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`;
/** Byte-identical Commerce→Inventory stock row-lock text shared by the read-tail
 * reserve/issue call sites. Text only: each call site still prepares and executes
 * its own FOR UPDATE statement in its supplied transaction; no result cached. */
export const COMMERCE_STOCK_WAREHOUSE_FOR_UPDATE_SQL = `SELECT warehouse_id FROM (${commercialOwnerProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE`;
