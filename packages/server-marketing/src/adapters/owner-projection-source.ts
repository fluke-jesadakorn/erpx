/** Closed owner-local projection-source head (r40). The four Marketing owner
 * projections share this one renderer: a caller picks an owner-owned column
 * list and table, and the helper emits the original quoted projection byte for
 * byte. No caller SQL, predicate or relation name crosses this boundary. The
 * hospitality, last-audit and commercial sources stay byte-identical, while
 * locks, command_receipts settlement and native read ports stay untouched. */
export function marketingProjectionSourceSql(columns: readonly string[], table: string): string {
  return `SELECT ${columns.map((column) => `"${column}"`).join(',')} FROM ${table}`;
}
