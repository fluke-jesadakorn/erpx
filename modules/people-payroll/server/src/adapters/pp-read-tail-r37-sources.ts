import 'server-only';
/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r37). Each builder owns one read-only literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. Every family here
 * is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock)
 * and is not covered by the r1-r36 canonical modules. Lock-bearing reads, cross-owner
 * predicates and r1-r36 modules stay untouched. */

/** Family 1: five-predicate status-guard EXISTS. Bind order: (id, c2, c4, c5). */
export const ppRd37StatusGuard5 = (table: string, c2: string, status: string, c4: string, c5: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND ${c2}=? AND status='${status}' AND ${c4}=? AND ${c5}=?)`;

/** Family 2: COALESCE legal-name subquery with unlinked-account fallback. */
export const ppRd37CoalesceLegalName = (col: string, alias: string) =>
  `COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=j.${col} AND p.company_id=j.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked account') ${alias}`;

/** Family 3: id/version plus state/status IN guard. Bind order: (id, version, ...states). */
export const ppRd37IdVersionInGuard = (table: string, col: string, placeholders: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND ${col} IN (${placeholders}))`;

/** Family 4: recovery-status EXISTS guard. Bind order: (id, company_id, version, c5). */
export const ppRd37RecoveryGuard = (status: string, c5: string) =>
  `EXISTS(SELECT 1 FROM hr_payroll_recoveries WHERE id=? AND company_id=? AND version=? AND status='${status}' AND ${c5}=?)`;
