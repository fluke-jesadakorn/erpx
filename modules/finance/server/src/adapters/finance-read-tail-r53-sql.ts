/** G4 / retire.finance.read-tail-r53 closed column-by-id read helper.
 *
 * Renders byte-identical text to the literal it replaces. Callers keep their
 * own binds; nothing here is cached or executed. Company-first, tailed and
 * different-key reads are excluded.
 */

/** One owned column by id and company. No tail: callers with extra predicates
 * or a different key are not eligible and keep their own literals. */
export const columnByIdSql = (column: string, table: string) => `SELECT ${column} FROM ${table} WHERE id=? AND company_id=?`;
