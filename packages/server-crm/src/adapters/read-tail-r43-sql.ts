/** G4 / retire.crm.read-tail-r43 byte-identical scalar-subquery head.
 *
 * The scalar-subquery scaffold is the owner select-body plan over the closed
 * `${projection} FROM ${body}` body, wrapped in parentheses; the module no
 * longer carries a second read token. Callers still pass their own projection
 * and body (including cross-owner source interpolations and caller aliases), so
 * rendered SQL, whitespace and bind order stay byte-identical. Locks, command
 * receipts, native command/read kinds and the r8-r42 families are untouched.
 */
import { crmSelectBodySql } from './read-tail-r46-sql';

export const crmScalarSubquerySql = (projection: string, body: string) =>
  `(${crmSelectBodySql(`${projection} FROM ${body}`)})`;
