import "server-only";
import {randomUUID} from "node:crypto";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from "@erpx/server-workflow/public/commands";
import {projectsPackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {AttachmentOperation} from "@erpx/server-files/public/attachments/types";
import {projectTaskRecordPolicy} from "@erpx/server-operations/public/project-task-record-policy";
import {projectPurchaseControls} from "@erpx/server-projects/adapters/purchases";
import {projectTaskOwnsAttachmentAction} from "@erpx/server-projects/adapters/task-purchases";

type Target='project'|'staff_task';
export type ProjectAttachmentPurchase={companyId:string;capability:'projects.wbs'|'projects.tasks';purpose:CapabilityPurpose;actor:WorkContext;targetType:Target;targetId:string;witness:string;ownedTaskAction:boolean;writer:'project'|'manager'|'assignee'|'delegate'};
function source(type:Target,ownedTaskAction=false){
 if(type==='project')return "SELECT p.*,p.id project_id,p.owner_user_id project_owner_id,(to_jsonb(p)-'version'-'updated_at')::text image FROM staff_projects p WHERE p.id=? AND p.company_id=?";
 return `SELECT t.*,p.owner_user_id project_owner_id,jsonb_build_object('task',to_jsonb(t)-'version'-'updated_at'${ownedTaskAction?"-'state'-'completion_note'-'blocked_reason'":''},'project',to_jsonb(p)-'version'-'updated_at','delegations',COALESCE((SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM staff_task_delegations d WHERE d.task_id=t.id),'[]'::jsonb))::text image FROM staff_tasks t JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id WHERE t.id=? AND t.company_id=?`;
}
type Source={image:string;state?:string;creator_user_id?:string;assignee_user_id?:string;project_owner_id:string};
export async function projectAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,targetType:string,targetId:string):Promise<ProjectAttachmentPurchase|null>{
 const mode=await projectsPackageEnforcementMode();if(mode==='disabled'||!['project','staff_task'].includes(targetType))return null;
 const type=targetType as Target,ownedTaskAction=type==='staff_task'&&projectTaskOwnsAttachmentAction(database,actor,targetId),row=await database.prepare(source(type,ownedTaskAction)).bind(targetId,actor.companyId).first<Source>();
 if(!row){if(type==='staff_task')return null;throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original Project file source is unavailable.');}
 const writer=type==='project'?'project':row.state==='draft'?'manager':samePerson(actor,row.assignee_user_id)?'assignee':'delegate';
 return {companyId:actor.companyId,capability:type==='project'?'projects.wbs':'projects.tasks',purpose:type==='staff_task'&&row.state==='draft'?'new-work':'settle-existing',actor,targetType:type,targetId,witness:row.image,ownedTaskAction,writer};
}
export async function authorizeProjectAttachmentPurchase(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation:AttachmentOperation){
 const scope=await projectAttachmentPurchaseScope(database,actor,type,id);if(!scope)return;
 await projectPurchaseControls(database,actor);
 await checkPurchasedCapability(database,await projectsPackageEnforcementMode(),{...scope,purpose:operation==='read'?'history':scope.purpose});
}
/** Utility source locks precede the first business snapshot in an owned Files
 * transaction, including an initially empty delegation/source relation. */
export async function projectAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly ProjectAttachmentPurchase[]){
 if(!scopes.length||await projectsPackageEnforcementMode()==='disabled')return [];
 const companies=[...new Set(scopes.map(scope=>scope.companyId))].sort(),locks=companies.map(id=>companyPurchaseReadLocks(database,id));
 return [...locks.map(row=>row[0]),database.prepare('LOCK TABLE staff_projects,staff_project_members,staff_tasks,staff_task_delegations,social_task_origins,social_posts IN SHARE MODE'),...locks.flatMap(row=>row.slice(1))];
}
export async function projectAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly ProjectAttachmentPurchase[]){
 const mode=await projectsPackageEnforcementMode();if(!scopes.length||mode==='disabled')return;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetType}:${scope.targetId}:${scope.purpose}`,scope])).values()];
 const boundaries=unique.map(scope=>{const key=`project-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{
  const actor=scope.actor,key=`project-file-${phase}:${randomUUID()}`,viewer='project_file_actor',taskPolicy=projectTaskRecordPolicy('project_file_source.id','project_file_source.company_id',viewer);
  const writer=scope.writer==='project'?`iam_same_person(project_file_source.project_owner_id,${viewer}.user_id,project_file_source.company_id) AND project_file_source.lifecycle_status='active'`:scope.writer==='manager'?`(iam_same_person(project_file_source.creator_user_id,${viewer}.user_id,project_file_source.company_id) OR iam_same_person(project_file_source.project_owner_id,${viewer}.user_id,project_file_source.company_id))`:scope.writer==='assignee'?`iam_same_person(project_file_source.assignee_user_id,${viewer}.user_id,project_file_source.company_id)`:`EXISTS(SELECT 1 FROM staff_task_delegations d WHERE d.task_id=project_file_source.id AND d.company_id=project_file_source.company_id AND d.status='active' AND d.starts_at<=${viewer}.now_ms AND d.expires_at>${viewer}.now_ms AND iam_same_person(d.delegate_user_id,${viewer}.user_id,d.company_id))`;
  return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,'project-file-source',`EXISTS(WITH ${viewer} AS MATERIALIZED (SELECT ?::text user_id,?::text assignment_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)
 SELECT 1 FROM (${source(scope.targetType,scope.ownedTaskAction)}) project_file_source CROSS JOIN ${viewer} WHERE image=? AND ${writer} AND ${scope.targetType==='staff_task'?taskPolicy:'TRUE'}
 AND EXISTS(SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) project_file_scope WHERE ${assignmentReadSql('project_file_scope',scope.targetType==='project'?'records.write':'records.read')}))`,[actor.userId,actor.assignmentId,scope.targetId,scope.companyId,scope.witness,actor.companyId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role]),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)];
 });
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards('before')],after:[...financeDeferredEffectStatements(database),...guards('after'),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key))],observe:mode==='shadow'?observePurchaseResults:undefined};
}
export function projectAttachmentFailure(error:unknown){let current=error;for(let i=0;i<5&&current&&typeof current==='object';i++){const value=current as {code?:string;detail?:string;cause?:unknown};if(value.code==='23514'&&value.detail?.includes('project-file-'))return new ApiError(409,'PROJECT_ATTACHMENT_CHANGED','The original Project file source or current authority changed.');current=value.cause;}return null;}
