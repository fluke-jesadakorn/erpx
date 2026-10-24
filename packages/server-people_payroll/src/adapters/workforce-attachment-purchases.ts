import "server-only";
import {randomUUID} from "node:crypto";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {AttachmentOperation,AttachmentPolicy} from "@erpx/server-files/public/attachments/types";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {companyPurchaseReadLocks,peoplePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults} from "@erpx/server-commercial/public/purchase-command-boundary";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {requirePeopleCurrentAuthority} from "@erpx/server-people_payroll/adapters/people-purchases";
import {workforceSourceReadLocks} from "@erpx/server-people_payroll/adapters/workforce-purchases";
const tables={leave_request:"leave_requests",overtime_request:"overtime_requests",time_entry:"hr_time_entries"} as const;
type Target=keyof typeof tables;
export type WorkforceAttachmentPurchase={companyId:string;capability:"people.workforce";purpose:CapabilityPurpose;actor:WorkContext;targetType:Target;targetId:string;sourceImage:string};
const target=(type:string):type is Target=>Object.hasOwn(tables,type);
/** Actual existing registry targets only. The original Files Leave policy
 * authorizes requester/current reviewer/recorded decision maker; this witness
 * adds no role, module or document permission to employee self-service. */
function source(type:Target){
 const leave=type==='leave_request',state=type==='time_entry'?'r.status':'r.state',placement=type==='time_entry'?'e':'r';
 return `SELECT r.company_id,${placement}.branch_id,${placement}.department_id,${state} state,jsonb_build_object('source',to_jsonb(r)-'version'-'updated_at'${leave?"-'evidence_version'":""},'engagement',to_jsonb(e),'person',jsonb_build_object('id',p.id,'companyId',p.company_id,'userId',p.user_id,'status',p.status,'mergedIntoId',p.merged_into_id)${leave?",'legacyEmployee',to_jsonb(legacy)":""})::text image
 FROM ${tables[type]} r ${leave?'LEFT JOIN employees legacy ON legacy.id=r.employee_id AND legacy.company_id=r.company_id':''}
 LEFT JOIN hr_engagements e ON e.id=r.engagement_id AND e.company_id=r.company_id
 LEFT JOIN hr_people p ON p.id=${leave?'COALESCE(r.person_id,e.person_id,legacy.person_id)':'e.person_id'} AND p.company_id=r.company_id
 WHERE r.id=? AND r.company_id=? AND (r.engagement_id IS NULL OR e.id IS NOT NULL) AND ${leave?'(p.id IS NOT NULL OR legacy.id IS NOT NULL)':'e.id IS NOT NULL AND p.id IS NOT NULL'}`;
}
function purposeFor(state:string):CapabilityPurpose{return ['draft','rejected'].includes(state)?'new-work':'settle-existing';}
export async function workforceAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,'targetType'|'purposeKey'>,targetId:string):Promise<WorkforceAttachmentPurchase|null>{
 if(!target(policy.targetType)||await peoplePackageEnforcementMode()==='disabled')return null;
 const row=await database.prepare(source(policy.targetType)).bind(targetId,actor.companyId).first<{image:string;state:string}>();
 if(!row)throw new ApiError(404,'WORKFORCE_ATTACHMENT_SOURCE_CHANGED','The original Workforce evidence source is unavailable in this Company.');
 return {companyId:actor.companyId,capability:'people.workforce',purpose:purposeFor(row.state),actor,targetType:policy.targetType,targetId,sourceImage:row.image};
}
export async function authorizeWorkforceAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,targetId:string,operation:AttachmentOperation){
 const scope=await workforceAttachmentPurchaseScope(database,actor,policy,targetId);if(!scope)return;
 await requirePeopleCurrentAuthority(database,actor,`workforce-file-read:${randomUUID()}`);
 await checkPurchasedCapability(database,await peoplePackageEnforcementMode(),{...scope,purpose:operation==='read'?'history':scope.purpose});
}
/** Called only at the named Files atomic entry. Writer leases precede the
 * first advisory SELECT; two Companies never upgrade competing SHARE leases. */
export async function workforceAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly WorkforceAttachmentPurchase[]){
 if(!scopes.length||await peoplePackageEnforcementMode()==='disabled')return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(companyId=>companyPurchaseReadLocks(database,companyId));
 return [...locks.map(items=>items[0]),...workforceSourceReadLocks(database),database.prepare('LOCK TABLE employees IN SHARE MODE'),...locks.flatMap(items=>items.slice(1))];
}
export async function workforceAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly WorkforceAttachmentPurchase[]){
 const mode=await peoplePackageEnforcementMode();if(!scopes.length||mode==='disabled')return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetType}:${scope.targetId}:${scope.purpose}`,scope])).values()],boundaries=unique.map(scope=>{const key=`workforce-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`workforce-file-${phase}:${randomUUID()}`;return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,'workforce-file-authority',`EXISTS(SELECT 1 FROM assignments a WHERE a.id=? AND a.user_id=? AND a.role=? AND a.company_id=? AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=a.role AND role.company_id IN ('*',a.company_id) AND role.status='active'))`,[actor.assignmentId,actor.userId,actor.role,actor.companyId]),assertion(database,key,'workforce-file-source',`EXISTS(SELECT 1 FROM (${source(scope.targetType)}) workforce_file WHERE image=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))`,[scope.targetId,scope.companyId,scope.sourceImage,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards('before')],after:[...financeDeferredEffectStatements(database),...guards('after'),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key))],observe:mode==='shadow'?observePurchaseResults:undefined};
}
export function workforceAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==='object';depth++){const value=cause as {code?:string;detail?:string;cause?:unknown};if(value.code==='23514'&&value.detail?.includes('workforce-file-')){const failure=new ApiError(409,'WORKFORCE_ATTACHMENT_CHANGED','The original Workforce evidence source or selected role changed. Refresh before retrying.');failure.cause=cause;return failure;}cause=value.cause;}return null;}
