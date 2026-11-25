/** G4 / retire.crm.read-tail-r33 byte-identical clocked-source EXISTS head.
 *
 * The witness inside the clocked scope is the owner select-one plan over the
 * sourced join, so this module no longer carries a second read token. Callers
 * still pass their clock alias/CTE, source projection, row alias, id/company
 * expressions and predicate tail byte-identically. Transaction locks,
 * command-receipt reads, native command kinds and the r8-r32 families are
 * untouched.
 */
import { crmSelectOneSql } from './read-tail-r42-sql';

export const crmClockedSourceScopeSql = (
  clock: string,
  cte: string,
  source: string,
  alias: string,
  id: string,
  company: string,
  tail: string,
) =>
  `EXISTS(WITH ${clock}${cte} ${crmSelectOneSql(`(${source}) ${alias} CROSS JOIN ${clock} WHERE ${alias}.id=${id} AND ${alias}.company_id=${company} AND ${tail}`)}`;
