/** G4 / retire.finance.read-tail-r59 closed asset read-tail renderer.
 *
 * This export renders byte-identical text to the literal it replaces: the
 * column list, JOIN, table aliases, `a.id=?` bind and raw tail are preserved.
 * Callers keep their tail text, binds and `first<Row>()` result parsing;
 * nothing here is cached or executed. Row locks, advisory/native/tax reads,
 * r38 constants and every r15-r58 helper stay inline and untouched.
 */

/** Asset joined to its account by id. `tail` is raw SQL after `a.id=?`. */
export const assetAccountJoinByIdSql = (tail: string) =>
  `SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=?${tail}`;
