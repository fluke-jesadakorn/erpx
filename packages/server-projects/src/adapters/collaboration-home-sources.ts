import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {projectsPackageEnforcementMode,readPurchasedAccess,checkPurchasedCapability} from '@erpx/server-commercial/public/purchase-command-boundary';
import {purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {ApiError} from '@erpx/server-runtime/public/http';
import {projectMemberScopeGuard,projectMemberScopeSql} from '@erpx/server-projects/adapters/project-read-projections';
import {projectOwnerOrMemberColumnSql} from '@erpx/server-projects/adapters/project-read-projections';
import {projectAssignmentScopeSql} from '@erpx/server-projects/adapters/package-obligations';
import {membersJsonSql} from '@erpx/server-projects/adapters/social-purchase-sources';
import {staffProjectByRefHead} from './read-tail-r34';
import {clockNowMsHead} from './read-tail-r41';

/** Owner-local fixed SQL for the Projects collaboration-home read family. The
 * selected-audience and home reads reuse the owner's shared membership,
 * actor-scope, members and id/title options fragments; the clock CTE keeps its
 * own MATERIALIZED frame while sharing the closed clock head. Every source
 * relation and bind order is unchanged. */
const workforceActorScope = `CROSS JOIN ${projectAssignmentScopeSql('workforce_actor_scope')}`;
const projectAudienceClock = `WITH project_audience_clock AS MATERIALIZED(${clockNowMsHead()})`;
const projectsIdTitleOptions = 'SELECT p.id,p.title FROM staff_projects p';

function audienceScope(actor:StaffActor,enforced:boolean){
    const current=currentAssignmentReadScope(actor,'workforce_actor_scope');
    return {where:`p.company_id=workforce_actor_scope.company_id AND ${current.where}
      AND ${projectMemberScopeSql}
      AND ${enforced?`${hasPermission(actor,'records.read')?'TRUE':'FALSE'} AND ${purchasedCapabilityPredicate('p.company_id','projects.wbs','history','project_audience_clock.now_ms')}
      AND ${scheduledFeaturePredicate('p.company_id','projects')} AND iam_role_allows(?::text,p.company_id,'records.read')`:'TRUE'}`,
      args:[actor.companyId,actor.branchId,actor.departmentId,...current.args,actor.userId,actor.userId,...(enforced?[actor.role]:[])]};
}

/** A Social audience is a member reference, not Project management. Keep the
 * original owner/member scope, with current selected authority and purchased
 * history checked in the source statement before its limit. */
export async function socialProjectAudienceOptions(db:SqlDatabase,actor:StaffActor){
    const mode=await projectsPackageEnforcementMode();
    if(mode==='shadow')await readPurchasedAccess(db,mode,{companyId:actor.companyId,capability:'projects.wbs'});
    const scope=audienceScope(actor,mode==='enforced');
    return (await db.prepare(`${projectAudienceClock}
      ${projectsIdTitleOptions} CROSS JOIN project_audience_clock
      ${workforceActorScope}
      WHERE ${scope.where} ORDER BY p.title,p.id LIMIT 100`)
      .bind(...scope.args)
      .all<{id:string;title:string}>()).results;
}

/** Called inside the existing source-first Social owner unit. That unit already
 * leases native Projects/members, purchase facts and IAM through final effects. */
export async function socialProjectAudienceSource(db:SqlDatabase,actor:StaffActor,id:string){
    const mode=await projectsPackageEnforcementMode();
    if(mode!=='enforced'){
      if(mode==='shadow')await readPurchasedAccess(db,mode,{companyId:actor.companyId,capability:'projects.wbs'});
      return null;
    }
    await checkPurchasedCapability(db,mode,{companyId:actor.companyId,capability:'projects.wbs',purpose:'history'});
    const scope=audienceScope(actor,true);
    const row=await db.prepare(`${projectAudienceClock}
      SELECT jsonb_build_object('project',to_jsonb(p),${membersJsonSql})::text image
      FROM staff_projects p CROSS JOIN project_audience_clock
      ${workforceActorScope}
      WHERE ${scope.where} AND p.id=?`).bind(...scope.args,id).first<{image:string}>();
    if(!row)throw new ApiError(403,'PROJECT_AUDIENCE_FORBIDDEN','The original Project audience is unavailable in the current assignment.');
    return row.image;
}

/** Fixed native Social aliases; no caller-provided source label or authority. */
export function socialProjectAudienceReadPredicate(){
    return `(p.audience_kind IS DISTINCT FROM 'project' OR EXISTS(${staffProjectByRefHead('1','audience_project','p.audience_id','p.company_id','\n      ')}
      AND ${projectOwnerOrMemberColumnSql('audience_project','social_purchase_actor.user_id','m')}
      AND iam_role_allows(social_purchase_actor.role,p.company_id,'records.read')
      AND ${purchasedCapabilityPredicate('p.company_id','projects.wbs','history','social_purchase_actor.now_ms')}
      AND ${scheduledFeaturePredicate('p.company_id','projects')}))`;
}

export function collaborationHomeSources(db: SqlDatabase) {
    return { async projects(companyId: string, userId: string, userId2: string) {
            return db.prepare(`${projectsIdTitleOptions} WHERE p.company_id=?
      AND ${projectMemberScopeSql}
      ORDER BY p.title,p.id LIMIT 100`)
                .bind(companyId, userId, userId2).all<{
                id: string;
                title: string;
            }>();
        }, guardProjectAudience(key: string, value: "audience-project", id: string, companyId: string, userId: string, userId2: string) {
            return assertion(db, key, value, projectMemberScopeGuard(projectMemberScopeSql), [id, companyId, userId, userId2]);
        } };
}
