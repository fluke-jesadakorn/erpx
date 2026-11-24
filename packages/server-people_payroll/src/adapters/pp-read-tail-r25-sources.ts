import 'server-only';
/** Canonical parameterized SELECT-only owner read fragment shared across People Payroll
 * adapters (G4 read-tail r25). The builder owns one literal copied verbatim from its
 * former inline copies; consumers import it and keep no local copy. It is a pure read:
 * no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. This family
 * is not covered by the r19-r24 canonical modules; prior constants untouched. */
export const ppRd25ByIdCompany = (table: string) => `SELECT * FROM ${table} WHERE id=? AND company_id=?`;
