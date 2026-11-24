import 'server-only';
/** Canonical parameterized read-only owner read builder shared across People Payroll
 * adapters (G4 read-tail r47). Each builder owns one read-only literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. Every
 * family here is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE
 * or advisory lock) and is not covered by the r1-r46 canonical modules. Lock-bearing
 * reads, native kinds, cross-owner projections and r1-r46 modules stay untouched. */

/** Named organization graph snapshot projection. The alias and table are fixed owner
 * fragments; the fragment carries no SQL binds. */
export const ppRd47GraphSnapshotProjection = (alias: string, table: string) =>
  `SELECT ${alias}.id,${alias}.company_id,to_jsonb(${alias}) snapshot FROM ${table} ${alias}`;
