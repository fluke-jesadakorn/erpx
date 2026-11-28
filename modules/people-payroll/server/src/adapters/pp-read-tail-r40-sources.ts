import 'server-only';
/** Canonical parameterized SELECT-only owner read builder shared across People Payroll
 * adapters (G4 read-tail r40). The builder owns one read-only literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. It is a pure read
 * (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock) and is not
 * covered by the r1-r39 canonical modules. Lock-bearing reads, cross-owner projections,
 * native kinds and prior builders stay untouched. */

/** Unlinked (not merged) person id/legal-name by company + user, LIMIT 1. Bind order: (company_id, user_id). */
export const ppRd40UnlinkedUserByCompany = (cols: string) =>
  `SELECT ${cols} FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL LIMIT 1`;
