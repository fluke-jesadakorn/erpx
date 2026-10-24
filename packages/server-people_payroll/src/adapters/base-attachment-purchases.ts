import {performanceSourceReadLocks} from '@erpx/server-people_payroll/adapters/performance-purchases';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {AttachmentOperation,AttachmentPolicy} from "@erpx/server-files/public/attachments/types";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {clearancePermissions,type ClearanceKind} from "@erpx/feature-people_payroll/lib/retirement";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {checkPurchasedCapability,peoplePackageEnforcementMode,purchaseCommandBoundary,observePurchaseResults} from "@erpx/server-commercial/public/purchase-command-boundary";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import {requirePeopleCurrentAuthority} from "@erpx/server-people_payroll/adapters/people-purchases";
import {ApiError} from "@erpx/server-runtime/public/http";
const capabilities={hr_person:"people.directory",people_lifecycle_case:"people.lifecycle",people_clearance_task:"people.lifecycle",hr_evaluation_assessment:"people.performance"} as const;
type Target=keyof typeof capabilities;
export type PeopleAttachmentPurchase={companyId:string;capability:string;purpose:"settle-existing";actor:WorkContext;targetType:Target;targetId:string;sourceImage:string;permission?:string};
function target(value:string):value is Target{return Object.hasOwn(capabilities,value);}
function source(type:Target){
 if(type==="hr_evaluation_assessment")return `SELECT d.company_id,e.branch_id,e.department_id,jsonb_build_object('assessment',to_jsonb(d)-'version'-'updated_at','engagement',to_jsonb(e),'cycle',to_jsonb(cycle))::text image FROM hr_evaluation_assessments d JOIN hr_engagements e ON e.id=d.engagement_id AND e.company_id=d.company_id JOIN hr_evaluation_definitions cycle ON cycle.id=d.cycle_id AND cycle.company_id=d.company_id WHERE d.id=? AND d.company_id=?`;
 if(type==="hr_person")return `SELECT p.company_id,e.branch_id,e.department_id,jsonb_build_object('person',to_jsonb(p)-'version'-'updated_at','engagement',jsonb_build_object('id',e.id,'personId',e.person_id,'companyId',e.company_id,'branchId',e.branch_id,'departmentId',e.department_id,'status',e.status))::text image FROM hr_people p LEFT JOIN LATERAL(SELECT x.* FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1)e ON TRUE WHERE p.id=? AND p.company_id=?`;
 const task=type==="people_clearance_task",alias=task?"t":"c";
 return `SELECT c.company_id,e.branch_id,e.department_id,jsonb_build_object('case',to_jsonb(c)${task?"":"-'version'-'updated_at'"},'engagement',jsonb_build_object('id',e.id,'personId',e.person_id,'companyId',e.company_id,'branchId',e.branch_id,'departmentId',e.department_id,'status',e.status)${task?",'task',to_jsonb(t)-'version'-'updated_at'":""})::text image FROM ${task?"people_preparation_tasks t JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id":"people_lifecycle_cases c"} JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id WHERE ${alias}.id=? AND ${alias}.company_id=?`;
}
export async function peopleAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,targetId:string):Promise<PeopleAttachmentPurchase|null>{
 if(!target(policy.targetType)||await peoplePackageEnforcementMode()==="disabled")return null;
 const row=await database.prepare(source(policy.targetType)).bind(targetId,actor.companyId).first<{image:string}>();if(!row)throw new ApiError(404,"PEOPLE_ATTACHMENT_SOURCE_CHANGED","The actual People file source is no longer available.");
 let permission:string|undefined=policy.targetType==="hr_person"&&!hasPermission(actor,"people.documents.manage")?undefined:"people.documents.manage";
 if(policy.targetType==="people_clearance_task"){const task=await database.prepare("SELECT clearance_type FROM people_preparation_tasks WHERE id=? AND company_id=?").bind(targetId,actor.companyId).first<{clearance_type:ClearanceKind}>();permission=task?clearancePermissions[task.clearance_type]??undefined:undefined;}
 return {companyId:actor.companyId,capability:capabilities[policy.targetType],purpose:"settle-existing",actor,targetType:policy.targetType,targetId,sourceImage:row.image,permission};
}
export async function authorizePeopleAttachmentPurchase(database:SqlDatabase,actor:WorkContext,type:string,operation:AttachmentOperation){
 if(!target(type))return;const mode=await peoplePackageEnforcementMode();await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:capabilities[type],purpose:operation==="read"?"history":"settle-existing"});if(mode!=="disabled")await requirePeopleCurrentAuthority(database,actor,`people-file-read:${crypto.randomUUID()}`);
}
/** The original sensitive policy is unchanged. Its actual owner row and
 * selected permission are fenced again after all Files/deferred effects. */
export async function peopleAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly PeopleAttachmentPurchase[]){
 const mode=await peoplePackageEnforcementMode();if(!scopes.length||mode==="disabled")return;
 const boundaries=[...new Map(scopes.map(s=>[`${s.companyId}:${s.capability}`,s])).values()].map(s=>{const key=`people-file-purchase:${crypto.randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,s)!};});
 const guards=(phase:string)=>scopes.flatMap(scope=>{const key=`people-file-${phase}:${crypto.randomUUID()}`,actor=scope.actor,permission=scope.permission?assignmentReadSql("people_file_scope",scope.permission):`EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=people_file_scope.company_id AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=people_file_scope.branch_id OR g.scope_kind='department' AND g.scope_node_id=people_file_scope.department_id))`;
 return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"people-file-source",`EXISTS(SELECT 1 FROM (${source(scope.targetType)}) people_file_scope WHERE image=? AND ${permission} AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))`,[scope.targetId,scope.companyId,scope.sourceImage,actor.assignmentId,actor.userId,actor.role,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];});
 return {before:[...boundaries.flatMap(s=>s.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(s=>s.boundary.after),...boundaries.map(s=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(s.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}

/** Exact existing base/Performance File scopes enter before the Files atomic
 * unit's first data snapshot. No authority derives from a caller label. */
export async function peopleAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly PeopleAttachmentPurchase[]){if(!scopes.length||await peoplePackageEnforcementMode()==='disabled')return [];const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(company=>companyPurchaseReadLocks(database,company));return [...locks.map(lock=>lock[0]),...(scopes.some(scope=>scope.targetType==='hr_evaluation_assessment')?performanceSourceReadLocks(database):[]),database.prepare('LOCK TABLE hr_people,hr_engagements,people_lifecycle_cases,people_preparation_tasks IN SHARE ROW EXCLUSIVE MODE')];}
