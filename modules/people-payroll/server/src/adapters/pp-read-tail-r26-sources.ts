import 'server-only';
/** Canonical parameterized SELECT-only owner read fragments shared across People Payroll
 * adapters (G4 read-tail r26). Each builder owns one literal copied verbatim from its
 * former inline copies; consumers import it and keep no local copy. Every family here is
 * a pure read (no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock)
 * and is not covered by the r19-r25 canonical modules. Lock-bearing reads stay untouched. */
/** id/company/version/status bind guard; distinct from r21 (status-literal, status before version) */
export const ppRd26IdCompanyVersionStatusGuard = (table: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND status=?)`;
/** attachment-target scope projection used by finance and sensitive fact readers */
export const ppRd26AttachmentTargetScopeSql = (table: string) =>
  `SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM ${table} WHERE id=? AND company_id=?`;
/** command-receipt prior replay read; distinct from r15 (extra operation/actor/authority predicates) */
export const ppRd26CommandReceiptPriorSql = (table: string) =>
  `SELECT 1 FROM ${table} WHERE command_id=?`;
/** issued-record uniqueness guard keyed by run */
export const ppRd26RunIdGuard = (table: string) =>
  `NOT EXISTS(SELECT 1 FROM ${table} WHERE run_id=?)`;
