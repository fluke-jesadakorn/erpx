/** r28 family: the active stock-products master selector head shared by the
 * Inventory initial-count source and the count-source `activeProducts` reader.
 * The head through `WHERE ` is byte-identical; only the predicate tail differs.
 * A read-only renderer: no bind hoisting, caching, lock or native change. */
export const commerceProductsSelectSql = (tail: string) => `SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE ${tail}`;
