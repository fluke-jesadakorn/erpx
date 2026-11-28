/** G4 / retire.crm.read-tail-r50 byte-identical parenthesized-select head.
 *
 * The parenthesized expression is the owner select-body plan applied to the
 * body `(${body}`, so this module no longer carries a second read token. The
 * attachment target witness and the marketing-assignment guard keep their
 * expression, wrapper and predicate tail verbatim. Locks, no-space probes,
 * command-receipt reads, native command/read kinds, cross-owner exports and
 * the r8-r49 families are untouched.
 */
import { crmSelectBodySql } from './read-tail-r46-sql';

export const crmSelectExprSql = (body: string) => crmSelectBodySql(`(${body}`);
