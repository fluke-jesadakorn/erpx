import 'server-only';
/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r46). Each builder owns one read-only literal pattern copied
 * verbatim from its former inline copies; consumers import it and keep no local copy.
 * Every family here is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE
 * or advisory lock) and is not covered by the r1-r45 canonical modules. Lock-bearing
 * reads, native kinds, cross-owner projections and r1-r45 modules stay untouched. */

/** Owner file existence probe. The owner table and alias are fixed owner fragments;
 * each caller keeps its single `?` bind unchanged. */
export const ppRd46OwnerFileProbe = (table: string, alias: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=a.owner_id AND ${alias}.company_id=?)`;

/** By-id/by-company status IN point read. The column list, table and status list
 * are fixed owner fragments; bind order stays (id, company_id). */
export const ppRd46ByIdCompanyStatusIn = (cols: string, table: string, statuses: string) =>
  `SELECT ${cols} FROM ${table} WHERE id=? AND company_id=? AND status IN (${statuses})`;

/** Named JOIN source fragment. The columns, table and alias are fixed owner source
 * fragments; the fragment carries no SQL binds. */
export const ppRd46NamedJoinSource = (cols: string, table: string, alias: string) =>
  `JOIN (SELECT ${cols} FROM ${table}) ${alias}`;
