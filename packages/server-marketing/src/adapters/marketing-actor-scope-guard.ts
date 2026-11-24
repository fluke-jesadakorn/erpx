import 'server-only';
import {marketingActorScopeSql} from './marketing-actor-scope-sql';

/** Owner-local fixed actor-scope assignment guard head. Every admitted caller
 * passes only fragments built from owner builders (assignmentReadSql,
 * marketingModuleMemberSql) plus a fixed alias/suffix; one builder emits the
 * byte-identical head statement so no duplicate literal survives in the owner.
 * Callers never supply raw query text. */
export const actorScopeAssignmentSql = (
  predicate: string,
  alias: string,
  suffix = '',
): string => `SELECT ${predicate}${suffix} FROM ${marketingActorScopeSql} ${alias}`;
