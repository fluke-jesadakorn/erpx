/** G4 / retire.crm.read-tail-r46 byte-identical projection/FROM head.
 *
 * The projection/FROM scaffold is the bare select-body renderer applied to
 * the body `${projection} FROM ${from}`; keeping a second read literal in this
 * module would duplicate the owner's canonical plan head. `crmProjectionFromSql`
 * therefore folds into `crmSelectBodySql`, preserving projection, FROM text,
 * multiline whitespace and bind order byte-for-byte for every caller. The r42
 * existence renderer, the r44 UNION ALL renderer, locks, command-receipt reads
 * and cross-owner exports are untouched.
 */

/** Bare projection/body head owned by the r40 scoped-list, distinct-owner and
 *  lead-row-source families. Callers pass their own modifier (DISTINCT / lead.*)
 *  and ORDER tail inside `body`, so rendered SQL stays byte-identical to the
 *  retired inline literals. */
export const crmSelectBodySql = (body: string) => `SELECT ${body}`;

/** Projection/FROM callers pass both parts explicitly; the rendered text is
 *  the owner select-body plan over the combined body. */
export const crmProjectionFromSql = (projection: string, from: string) =>
  crmSelectBodySql(`${projection} FROM ${from}`);
