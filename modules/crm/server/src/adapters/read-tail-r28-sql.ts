import { crmProjectionFromSql } from './read-tail-r46-sql';

/** G4 / retire.crm.read-tail-r28 byte-identical read families.
 *
 * The renderers below reproduce the pre-dedupe inline SQL exactly; each call
 * site keeps its own projection, predicates and binds verbatim. Only the shared
 * read scaffold moves here. Locks, command-receipt reads, native command kinds
 * and the r8-r27 families are untouched.
 */

/** Direct case-event read. Callers pass their own projection, predicates and
 * order tail; rendered text stays byte-identical to the retired inline SELECT. */
export const crmCaseEventReadSql = (
  projection: string,
  predicates: string,
  tail = '',
) => crmProjectionFromSql(projection, `crm_case_events WHERE ${predicates}${tail}`);

/** Direct opportunity source probe. Callers pass the projection and predicate;
 * rendered text stays byte-identical to the retired inline SELECT. */
export const crmOpportunityProbeSql = (
  projection: string,
  predicate: string,
) => crmProjectionFromSql(projection, `crm_opportunities WHERE ${predicate}`);
