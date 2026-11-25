/** G4 / retire.finance.read-tail-r62 closed group report edition read-tail renderer.
 *
 * This export renders byte-identical text to the literal it replaces: the
 * projection, table, `WHERE` head and raw tail are preserved. Callers keep
 * their tail text, group-source predicates, binds and result parsing; nothing
 * here is cached or executed. Row locks, advisory/native/tax reads, r38
 * constants and every r15-r61 helper stay inline and untouched.
 */

/** Group report editions of a holding. `tail` is raw SQL after `WHERE`. */
export const groupReportEditionSql = (tail: string) =>
  `SELECT * FROM finance_group_report_editions r WHERE ${tail}`;
