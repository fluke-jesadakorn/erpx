import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {StaffActor} from '../domain/staff-policy';
import type {RoutinePurchases,RoutinePurchase} from '../domain/routine-purchase-controls';
import {ApiError} from '../../runtime/public/http';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {workPackageEnforcementMode,projectsPackageEnforcementMode,readPurchasedAccesses} from '../../commercial/public/purchase-command-boundary';
/** Minimal request-local controls. Private queued actors and native source
 * witnesses never cross this presentation boundary. */
export async function workRoutinePresentation(database:SqlDatabase,actor:StaffActor):Promise<RoutinePurchases|undefined>{
 const mode=await workPackageEnforcementMode();if(mode==='disabled')return;
 const scope=currentAssignmentReadScope(actor,'workforce_actor_scope');if(!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${scope.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...scope.args).first())throw new ApiError(403,'WORK_ROUTINE_AUTHORITY_CHANGED','Choose the original current routine account and Company role.');
 const projectMode=await projectsPackageEnforcementMode(),work=await readPurchasedAccesses(database,mode,actor.companyId,['work.actions','work.advanced-routines']),projects=await readPurchasedAccesses(database,projectMode,actor.companyId,['projects.wbs','projects.tasks']),combine=(values:typeof work):RoutinePurchase=>({newWork:Object.values(values).every(value=>value.newWork),settleExisting:Object.values(values).every(value=>value.settleExisting),history:Object.values(values).every(value=>value.history),revision:Object.values(values).map(value=>value.revision).join(':')});
 return {work:combine(work),projects:combine(projects),context:createHash('sha256').update(JSON.stringify([actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam??null])).digest('hex')};
}
