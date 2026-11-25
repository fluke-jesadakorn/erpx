/** G4 / retire.finance.read-tail-r54 closed image/source read-tail helpers.
 *
 * Both exports render byte-identical text to the literal they replace: the
 * to_jsonb projection, output label, table/alias and WHERE tail are preserved.
 * Callers keep the original alias, output label, WHERE/ORDER BY tail, binds and
 * raw-text parsing. `tail` is raw SQL text (1-4 `?` binds plus any ORDER BY);
 * nothing here is cached or executed. Row locks, fences and the multi-row set
 * reads stay inline, as do the r36/r38/r40 helpers and every r15-r53 helper.
 */

/** Point row rendered as the `image` output label. */
export const imageByTailSql = (alias: string, table: string, tail: string) =>
  `SELECT to_jsonb(${alias})::text image FROM ${table} ${alias} WHERE ${tail}`;

/** Point row rendered as the `source` output label. */
export const sourceByTailSql = (alias: string, table: string, tail: string) =>
  `SELECT to_jsonb(${alias})::text source FROM ${table} ${alias} WHERE ${tail}`;
