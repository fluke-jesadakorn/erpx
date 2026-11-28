import "server-only";
import { legalItemKeyHeadSql } from './legal-read-tail-r54-sources';

/** Byte-identical latest-approved-cycle read shared by Legal owner adapters (round 22).
 * R60: the base head delegates to the r54 item-key renderer; each call site still appends
 * the same fixed literal tail, prepares and executes its own projection with unchanged
 * binds. No SQL text is inlined here. */
export const legalApprovedCycleSql = (columns: string) =>
  legalItemKeyHeadSql(columns, 'legal_approvals') + " AND decision='approved' ORDER BY cycle DESC LIMIT 1";
