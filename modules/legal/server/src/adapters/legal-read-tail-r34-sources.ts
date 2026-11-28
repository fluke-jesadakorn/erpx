import "server-only";
import { legalPointByIdSql } from './legal-read-tail-r32-sources';
/** Byte-identical Legal id+key point-read SQL shared by owner adapters (round 34).
 * R59: the byte-identical r32 point-read head now delegates to that single
 * renderer; only the trusted key tail is local.
 * Every call site still prepares, binds and executes its own read with unchanged
 * binds; no request text is interpolated and rendered SQL stays byte-identical. */
export const legalPointByIdAndKeySql = (
  columns: string,
  table: string,
  key2: 'user_id' | 'company_id' | 'item_id' | 'document_id',
) => legalPointByIdSql(columns, table) + ` AND ${key2}=?`;
