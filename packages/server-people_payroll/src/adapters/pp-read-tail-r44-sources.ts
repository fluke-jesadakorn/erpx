import 'server-only';
/** Canonical parameterized SELECT-only owner read builders shared across People Payroll
 * adapters (G4 read-tail r44). Each builder owns one read-only literal pattern copied
 * verbatim from its former inline copies; consumers import it and keep no local copy.
 * Every family here is a pure read (no INSERT/UPDATE/DELETE, LOCK TABLE, FOR UPDATE/SHARE
 * or advisory lock) and is not covered by the r1-r43 canonical modules. Lock-bearing
 * reads, native kinds, cross-owner projections and r1-r43 modules stay untouched. */

/** Closed named column projection head; bind order has no SQL binds (all parts are
 * fixed owner source-clause fragments). Call sites pass fixed columns/table/alias. */
export const ppRd44NamedSourceProjection = (cols: string, table: string, alias: string) =>
  `FROM (SELECT ${cols} FROM ${table}) ${alias}`;

/** Native authority probe head; the query predicate and result alias are fixed
 * owner fragments, and the caller bind array is unchanged. */
export const ppRd44AuthorityProbe = (query: string, alias: string) =>
  `SELECT ${query} ${alias}`;

/** Purchased-capability clock probe head; predicate/clock/alias are fixed owner
 * fragments. The builder emits both heads byte-identically to the inline copy. */
export const ppRd44PurchaseClockProbe = (predicate: string, clock: string, alias: string) =>
  `(SELECT ${predicate} FROM (SELECT ${clock} now_ms) ${alias})`;
