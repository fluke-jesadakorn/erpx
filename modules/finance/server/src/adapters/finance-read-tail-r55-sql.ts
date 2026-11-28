/** G4 / retire.finance.read-tail-r55 closed company count helpers.
 *
 * Both exports render byte-identical text to the literal they replace: the
 * `count(*)` projection, table, company predicate and raw tail are preserved.
 * Callers keep their output aliases, bind order and raw-text parsing; nothing
 * here is cached or executed. Tailed aggregates, `count(DISTINCT ...)`,
 * aliased/JOIN/derived-table counts and every r15-r54 helper stay inline.
 */

/** Company-first point count. `tail` is raw SQL text after `company_id=?`. */
export const companyCountSql = (table: string, tail: string) =>
  `SELECT count(*) FROM ${table} WHERE company_id=?${tail}`;

/** Correlated count against the outer `p` period row: zero `company_id=?`
 * binds. `tail` is raw SQL text after `<alias>.company_id=p.company_id`. */
export const correlatedCountSql = (alias: string, table: string, tail: string) =>
  `SELECT count(*) FROM ${table} ${alias} WHERE ${alias}.company_id=p.company_id${tail}`;
