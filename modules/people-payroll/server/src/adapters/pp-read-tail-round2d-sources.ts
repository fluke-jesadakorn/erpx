import 'server-only';
/** Canonical parameterized existence guard shared by the r57 people-id and r64
 * time-entry-id read-tail builders. The former owner-local heads are now rendered
 * from one template; each builder supplies its exact table and passes the caller's
 * predicate tail through unchanged, so rendered SQL stays byte-identical. Pure
 * read only: no writes, row/table locks, advisory statements, native kinds or
 * cross-owner projections. */

export const ppRdRound2dExistsById = (
  table: string,
  tail: string,
) => `EXISTS(SELECT 1 FROM ${table} WHERE id=?${tail})`;
