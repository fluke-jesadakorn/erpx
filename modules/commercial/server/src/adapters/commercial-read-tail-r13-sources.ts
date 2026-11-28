import "server-only";
import { scopedReadHead } from "./commercial-read-tail-r14-sources";
/** Commercial read-tail r13 month-scoped read family. The single scoped head
 * owns the SQL literal and renders each original read byte-identically: the two
 * usage-snapshot header reads (r6) and the billing-usage-meter source read in
 * the usage projection share the same month-scoped shape. The newline column
 * gap is preserved by the head's `sep`. Every caller keeps its own bind/prepare
 * and its exports. Pure read-only: no writes, no row locks and no advisory lock. */
export function monthScopedRead(columns: string, table: string) {
  return scopedReadHead(columns, table, 'month', '\n      ');
}
