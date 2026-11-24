import "server-only";
/** Commercial read-tail r13 parameterized month-scoped read family.
 * The builder owns one SQL literal and renders each original read
 * byte-identically: the two usage-snapshot header reads (r6 family) and the
 * billing-usage-meter source read in the usage projection share the same
 * month-scoped `SELECT`-by-month shape. Every caller keeps its
 * own bind/prepare and its exports. Both arguments are code-owned column and
 * table fragments only, never request input. The r1-r12 helpers, advisory/row
 * locks and native owner statement kinds stay untouched. Pure read-only: no
 * writes, no row locks and no advisory lock. */
export function monthScopedRead(columns: string, table: string) {
  return `SELECT ${columns}
      FROM ${table} WHERE month=?`;
}
