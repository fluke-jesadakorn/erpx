import { commercialOwnerProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";

/** Byte-identical Commerce→Inventory stock projection read text shared by the
 * read-tail call sites. Text only: each call site still prepares and executes
 * its own statement and no query result is cached here. */
export const COMMERCE_STOCK_BY_WAREHOUSE_PRODUCT_SQL = `SELECT * FROM (${commercialOwnerProjectionSources.commerce_stock}) commerce_stock WHERE warehouse_id=? AND product_id=?`;
