/** Byte-identical SQL heads for the Projects SELECT-only read tail (r34).
 * Family A covers point reads on staff_tasks keyed by task id and Company;
 * Family B covers by-reference reads on staff_projects whose id/company_id
 * come from another relation's columns. Only closed column lists, aliases,
 * reference expressions and whitespace gaps vary. Every caller keeps its own
 * trailing predicates, binds, result types and lock clauses; no SQL comes
 * from an external caller. */

/** staff_tasks point head. `fromAlias` adds the optional FROM alias and
 * `keyPrefix` adds its matching qualifier on the two key columns. The lock
 * suffix stays at each caller. */
export const staffTaskPointHead = (
  columns: string,
  fromAlias = '',
  keyPrefix = '',
) => `SELECT ${columns} FROM staff_tasks${fromAlias ? ` ${fromAlias}` : ''} WHERE ${keyPrefix}id=? AND ${keyPrefix}company_id=?`;

/** staff_projects by-reference head. `refId` and `refCompany` are closed
 * column references; `whereGap`/`andGap` reproduce the original whitespace
 * byte for byte. */
export const staffProjectByRefHead = (
  columns: string,
  alias: string,
  refId: string,
  refCompany: string,
  whereGap = ' ',
  andGap = ' ',
) => `SELECT ${columns} FROM staff_projects ${alias}${whereGap}WHERE ${alias}.id=${refId}${andGap}AND ${alias}.company_id=${refCompany}`;
