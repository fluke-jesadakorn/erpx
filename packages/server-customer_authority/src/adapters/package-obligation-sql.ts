import 'server-only';
/** Parameterized read-only SQL families shared by the Customer Authority
 *  package-obligation facts reads (G4 read-tail r22). Each builder owns one SQL
 *  literal and renders every original subselect byte-identically; consumers keep
 *  their own exports, prepare/bind their own statements and retain every portal
 *  consumer. Arguments are closed aliases/predicates chosen in this package,
 *  never request input. The r21 `customer-authority-read-sql` helpers, the
 *  source locks and the control-plane statements stay untouched. Every rendered
 *  fragment is a pure read: no writes, no row or table locks, no advisory locks. */

/** Latest Support response version for one request. */
export const latestSupportVersion = (alias: string, request: string) =>
  `(SELECT max(${alias}.response_version) FROM customer_portal_support_responses ${alias} WHERE ${alias}.request_id=${request})`;

/** Absence of a closed Support fact for one request. */
export const supportAbsent = (table: string, alias: string, predicate: string) =>
  `NOT EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${predicate})`;

/** Command-receipt/audit witness shared by the binding, invitation and Support
 *  response fact subselects of the portal obligation projection. */
export const commandAuditProbe = (negated: boolean, join: string, where: string) =>
  `${negated ? 'NOT ' : ''}EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND ${join} WHERE ${where})`;
