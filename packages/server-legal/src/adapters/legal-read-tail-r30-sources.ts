import "server-only";
import { legalPointByIdSql } from './legal-read-tail-r32-sources';
/** Byte-identical Legal ordered history list SQL shared by owner adapters (round 30).
 * Round 33 continuation: the former inline point-read literal is deleted; the single
 * point-read head is legalPointByIdSql (r32 -> r36) and only the trusted fixed key
 * substitution and ORDER BY tail remain here. Rendered SQL stays byte-identical and
 * every call site still prepares, binds and executes its own read with unchanged binds. */
/** Fixed-column ordered history list; the caller supplies the fixed legal fact table and key column. */
export const legalOrderedHistorySql = (table: string, key: string) =>
  legalPointByIdSql('*', table).replace('WHERE id=?', `WHERE ${key}=?`) + ' ORDER BY created_at,id';
