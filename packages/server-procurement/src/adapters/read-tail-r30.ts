/** Procurement read-tail r30 shared fragments. Text only: each builder renders the
 * exact original statement so binds, ordering, locks and writes are unchanged.
 * r17-r29 helpers, FOR UPDATE/FOR SHARE and command_receipts stay untouched. */
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {workforceActorScopeColumns} from './read-tail-sql';
/** Current assignment-scope probe shared by the purchase and supplier-bill
 * boundaries. The alias is the only degree of freedom. */
export const assignmentScopeProbe = (alias: string, permission: string) =>
  `SELECT ${assignmentReadSql(alias,permission)} allowed FROM ${workforceActorScopeColumns} ${alias}`;
/** Version/status scoped existence guard shared by the RFQ and supplier-bill
 * command fences. The table, alias and scope fragment render verbatim. */
export const versionStatusScopeGuard = (table: string, alias: string, scope: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=? AND ${alias}.version=? AND ${alias}.status=? AND ${scope})`;
/** Active master-data lookup used by the RFQ workspace. Before/after fragments
 * carry each master's intentional extra predicates verbatim. */
export const activeMasterLookup = (
  columns: string,
  source: string,
  alias: string,
  order: string,
  before = "",
  after = "",
) => `SELECT ${columns} FROM (${source}) ${alias} WHERE company_id=?${before} AND status='active'${after} ORDER BY ${order}`;
/** Order-scoped aggregate used by request progress. The rendered request-order
 * link guard, FROM fragment, before predicate and trailing ordering each render
 * verbatim; the r19 link helper stays at every call site. */
export const orderScopedAggregate = (
  columns: string,
  from: string,
  link: string,
  orderAlias: string,
  before = "",
  tail = "",
) => `SELECT ${columns} FROM ${from} WHERE ${orderAlias}.company_id=?${before} AND (${orderAlias}.id=? OR ${link})${tail}`;
