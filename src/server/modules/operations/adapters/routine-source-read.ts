import 'server-only';
import type {SqlDatabase,SqlValue} from '../../runtime/public/runtime/types';
import type {StaffActor} from '../domain/staff-policy';
import {permissionForRoles} from '@/lib/identity_access/permissions';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {workPackageEnforcementMode,projectsPackageEnforcementMode,readPurchasedAccesses} from '../../commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '../../commercial/public/purchased-capability-sql';
import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
import {finalRootOwnerProjectionSources as projects} from '../../projects/public/final-root-owner-projection-sources';
/** Fixed native template alias t. Published templates retain the original
 * Company visibility; unpublished templates remain creator/Company-owner
 * only. Running a Project routine separately needs original Project membership. */
export async function routineSourceReadScope(database:SqlDatabase,actor:StaffActor){
 const mode=await workPackageEnforcementMode(),projectMode=await projectsPackageEnforcementMode();if(mode==='shadow')await readPurchasedAccesses(database,mode,actor.companyId,['work.actions','work.advanced-routines']);if(mode!=='enforced'&&projectMode!=='enforced')return {where:'TRUE',args:[] as SqlValue[]};
 const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),permission=permissionForRoles(['holding_owner']),clock='routine_source_clock.now_ms';
 const project=`EXISTS(SELECT 1 FROM (${projects.staff_projects}) routine_project WHERE routine_project.id=t.project_id AND routine_project.company_id=t.company_id${projectMode==='enforced'?` AND iam_role_allows(?,t.company_id,'records.read') AND ${['projects.wbs','projects.tasks'].map(code=>purchasedCapabilityPredicate('t.company_id',code,'history',clock)).join(' AND ')}`:''})`;
 return {where:`EXISTS(WITH routine_source_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM routine_source_clock CROSS JOIN (SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE t.company_id=workforce_actor_scope.company_id AND ${current.where} AND ${scheduledFeaturePredicate('t.company_id','operations')} AND (t.published=1 OR t.creator_user_id=? OR EXISTS(SELECT 1 FROM assignments routine_owner WHERE routine_owner.id=? AND routine_owner.company_id=t.company_id AND routine_owner.scope_kind='holding' AND iam_role_allows(routine_owner.role,routine_owner.company_id,'${permission}'))) AND CASE WHEN t.project_id IS NULL THEN ${mode==='enforced'?['work.actions','work.advanced-routines'].map(code=>purchasedCapabilityPredicate('t.company_id',code,'history',clock)).join(' AND '):'TRUE'} ELSE ${project} END)`,args:[actor.companyId,actor.branchId,actor.departmentId,...current.args,actor.userId,actor.assignmentId,...(projectMode==='enforced'?[actor.role]:[])] as SqlValue[]};
}
