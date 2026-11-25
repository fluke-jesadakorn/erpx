import "server-only";
import {legalItemOrderedSql} from './legal-read-tail-r35-sources';
/** Byte-identical Legal recent-item list SQL shared by owner adapters (round 31).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds. */
/** Fixed five-column recent-item list; the caller supplies the projection, fact table and row limit. */
export const legalRecentItemListSql = (columns: string, table: string, limit: number) =>
  legalItemOrderedSql(columns, table, 'created_at DESC', limit);
