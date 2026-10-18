import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Commerce consumes independently approved pricing; it never edits or
 * purchases a new shared customer/product identity through this port. */
export function commerceOwnerPricingSourceReadLocks(database:SqlDatabase){return [
 database.prepare("LOCK TABLE sales_price_lists,sales_price_list_items IN SHARE MODE")
];}
