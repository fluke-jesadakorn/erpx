import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {actorScopeCteSql} from './actor-scope-read-sql';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/** Shared byte-identical workforce-actor-scope current read for private attachment/ticket reads. */
export async function workforceActorScopeCurrent(database:SqlDatabase,actor:WorkContext):Promise<boolean>{
 const scope=currentAssignmentReadScope(actor,'workforce_actor_scope');
 return !!(await database.prepare(`SELECT 1 FROM(${actorScopeCteSql(',?::text branch_id,?::text department_id')})workforce_actor_scope WHERE ${scope.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...scope.args).first());
}
