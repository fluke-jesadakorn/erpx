import "server-only";
import { legalItemKeyHeadSql } from './legal-read-tail-r54-sources';
/** Byte-identical Legal item-scoped ordered-list SQL shared by owner adapters (round 35).
 * R68: the item-key point head delegates to the verified r54 renderer with the fixed
 * `item_id` key; only the trusted ORDER BY/LIMIT tail stays local. Every call site still
 * prepares, binds and executes its own read with unchanged binds and byte-identical SQL.
 * Callers pass trusted constant columns/table/order/limit; no request text is interpolated. */
export const legalItemOrderedSql = (
  columns: string,
  table: string,
  order: string,
  limit?: number,
) => legalItemKeyHeadSql(columns, table) + ` ORDER BY ${order}${limit === undefined ? '' : ` LIMIT ${limit}`}`;
