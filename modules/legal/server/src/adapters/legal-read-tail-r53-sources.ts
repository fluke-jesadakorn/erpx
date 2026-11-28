import "server-only";
import { legalPointByIdSql } from './legal-read-tail-r32-sources';

/** Byte-identical Legal three-key point-read head shared by owner adapters (round 53).
 * Delegates to the verified r32 head and appends the trusted key and company clauses.
 * The caller supplies only trusted constant projection/table/key; no request text
 * reaches the head, no bind is added and each rendered statement stays byte-identical
 * to the former inline copy. */
export const legalPointByIdKeyAndCompanySql = (
  columns: string,
  table: string,
  key2: 'rule_id' | 'item_id',
) => legalPointByIdSql(columns, table) + ` AND ${key2}=? AND company_id=?`;
