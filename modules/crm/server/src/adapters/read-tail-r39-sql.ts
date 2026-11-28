/** G4 / retire.crm.read-tail-r39 byte-identical clocked read head.
 *
 * The clocked CTE prefix is the owner select-body plan applied to the caller's
 * body, so this module no longer carries a read token. Callers still pass their
 * own clock CTE and full body: rendered SQL, multiline whitespace and bind order
 * stay byte-identical to the retired inline literals. The revenue read keeps its
 * inline `WITH ${clock},source_journals AS(` head because it adds a second CTE.
 * Locks, command-receipt reads, native command kinds and the r8-r38 families are
 * untouched.
 */
import { crmSelectBodySql } from './read-tail-r46-sql';

export const crmClockedReadSql = (clock: string, body: string) =>
  `WITH ${clock} ${crmSelectBodySql(body)}`;
