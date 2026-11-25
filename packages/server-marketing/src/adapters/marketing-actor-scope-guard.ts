import 'server-only';
import {marketingActorScopeSql} from './marketing-actor-scope-sql';
import {marketingSelectHeadSql} from './select-head-sql';

/** Owner-local fixed actor-scope assignment guard head. Every admitted caller
 * passes only fragments built from owner builders (assignmentReadSql,
 * marketingModuleMemberSql) plus a fixed alias/suffix; one builder emits the
 * byte-identical head statement so no duplicate literal survives in the owner.
 * r50 folds the previously inline terminal head into the canonical owner-local
 * renderer. Callers never supply raw query text. */
export const actorScopeAssignmentSql = (
  predicate: string,
  alias: string,
  suffix = '',
): string => marketingSelectHeadSql(`${predicate}${suffix} FROM ${marketingActorScopeSql} ${alias}`);
