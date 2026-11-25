/** Procurement read-tail r34 shared fragments. Text only: each builder renders the
 * exact original statement so binds, ordering, locks and writes are unchanged.
 * r17-r33 helpers, FOR UPDATE/FOR SHARE locks and command_receipts stay untouched. */
import {existsSourceWhereHead} from './read-tail-r53';
import {columnsFromWhereHead} from './read-tail-r54';
/** Calendar scoped list read. The column list, FROM source, branch scope, scope
 * expression, per-caller predicates and ordering render verbatim. The trailing
 * LIMIT bind stays at each call site. The byte-identical columns/FROM/WHERE head
 * now comes from r54. */
export const calendarScopedListRead = (
  columns: string,
  from: string,
  branchScope: string,
  scopeExpr: string,
  predicates: string,
  scopeWhere: string,
  order: string,
) => columnsFromWhereHead(columns, from, `${branchScope} AND ${scopeExpr} AND ${predicates} AND ${scopeWhere} ORDER BY ${order} LIMIT ?`);
/** Approved employee-request EXISTS guard. The source expression (including the
 * optional request-order JOIN), column prefix, core predicates and the two
 * evidence/items witnesses render verbatim; binds stay at each call site. The
 * byte-identical EXISTS/FROM/WHERE head now comes from r53. */
export const approvedRequestExists = (
  sourceExpr: string,
  prefix: string,
  where: string,
  evidence: string,
  items: string,
) => `${existsSourceWhereHead(sourceExpr)}${where} AND ${prefix}evidence_version=${evidence} AND ${prefix}items_json=${items} AND ${prefix}approval_json::jsonb->>'decision'='approved')`;
/** Point guard on a supplier-bill match: `id=?` is shared; the caller's exact
 * extra predicates render verbatim after it. No binds, locks or writes added. */
export const supplierBillPointGuard = (tail: string) =>
  `EXISTS(SELECT 1 FROM procurement_bill_matches WHERE id=?${tail})`;
/** RFQ master read by scope. The WHERE body (id equality or the scope/order tail)
 * renders verbatim. */
export const rfqScopedRead = (where: string) =>
  `SELECT r.* FROM procurement_rfqs r WHERE ${where}`;
