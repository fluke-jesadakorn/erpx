/** G4 / retire.crm.read-tail-r30 byte-identical read family.
 *
 * The current-scope authority selector is the owner select-body plan over the
 * caller's assignment/alias/tail, so this module no longer carries a read token.
 * Callers still pass their own assignment expression, scope source, alias and
 * optional predicate tail byte-identically. Locks, command-receipt reads,
 * native command kinds and the r8-r29 families are untouched.
 */
import { crmSelectBodySql } from './read-tail-r46-sql';

/** One parameterized renderer for the current assignment-scope authority read.
 * Only the assignment expression, scope source, alias and predicate tail vary. */
export const crmCurrentScopeAllowedSql = (
  assignment: string,
  scope: string,
  alias: string,
  tail = '',
) => crmSelectBodySql(`${assignment}${tail} allowed FROM ${scope} ${alias}`);
