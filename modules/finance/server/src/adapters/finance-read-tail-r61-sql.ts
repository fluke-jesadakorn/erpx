/** G4 / retire.finance.read-tail-r61 closed book-rule read-tail renderer.
 *
 * This export renders byte-identical text to the literal it replaces: the
 * `primary_chart_id` projection, table, `WHERE` head and raw tail are preserved.
 * Callers keep their tail text, bind order and result parsing; nothing here is
 * cached or executed. Row locks, advisory/native/tax reads, r38 constants and
 * every r15-r60 helper stay inline and untouched.
 */

/** Primary chart of an approved book rule. `tail` is raw SQL after `WHERE`. */
export const primaryChartIdByRuleSql = (tail: string) =>
  `SELECT primary_chart_id FROM finance_book_rules WHERE ${tail}`;
