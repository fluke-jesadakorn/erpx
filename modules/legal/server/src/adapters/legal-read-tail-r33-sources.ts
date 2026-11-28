import "server-only";
import { legalPointByIdSql } from './legal-read-tail-r32-sources';
/** State-constrained point reads shared by Legal owner adapters (round 33).
 * R59: the byte-identical r32 point-read head now delegates to that single
 * renderer; only the trusted key/state tail is local.
 * Every call site still prepares, binds and executes its own read with unchanged
 * binds; no request text is interpolated and rendered SQL stays byte-identical. */
export const legalStatePointSql = (
  columns: string,
  table: string,
  key2: 'company_id' | 'document_id' | 'rule_id',
  state: 'active' | 'approved',
) => legalPointByIdSql(columns, table) + ` AND ${key2}=? AND state='${state}'`;
