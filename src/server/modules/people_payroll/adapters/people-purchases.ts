import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "../../identity_access/public/authorization";
import {hasPermission} from "@/lib/identity_access/permissions";
import {requirePermission} from "../../identity_access/public/iam-permissions";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {iamMutationGuards,loadIamAuthority} from "../../identity_access/public/iam-store";
import {workContextGuards,financialConstraintError} from "../../workflow/public/commands";
import {holdingResources} from "../../runtime/public/runtime";
import {isTransactionalSqlDatabase,type SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import {flushFinanceDeferredEffects} from "../../finance/public/finance-transactions";
import {peoplePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure,companyPurchaseReadLocks,holdingPurchaseReadLocks,readPurchasedAccesses,type PackageEnforcementMode} from "../../commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "../../commercial/public/rules/packages";

type Capability="people.directory"|"people.lifecycle"|"foundation.identity";
type Target={kind:"person"|"engagement"|"compensation"|"lifecycle"|"preparation-plan"|"preparation-complete";id:string};
type Source={id:string;company_id:string;person_id:string|null;engagement_id:string|null};
type Command={key:string;operation:string;capability:Capability;purpose:CapabilityPurpose;permission?:string;target?:Target;resultKind?:Target["kind"]};
const compositions=new WeakMap<SqlDatabase,{companyId?:string;checks:(()=>Promise<void>)[]}>();
async function source(database:SqlDatabase,target:Target):Promise<Source|null>{
 if(target.kind.startsWith("preparation-"))return database.prepare(`SELECT t.id,t.company_id,c.person_id,c.engagement_id,t.case_id,t.position_id,t.kind,t.clearance_type,t.privileged${target.kind==="preparation-complete"?",t.assignee_id":""} FROM people_preparation_tasks t LEFT JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id WHERE t.id=?`).bind(target.id).first<Source>();
 const select=target.kind==="person"?"SELECT id,company_id,id person_id,NULL::text engagement_id FROM hr_people":target.kind==="engagement"?"SELECT id,company_id,person_id,id engagement_id FROM hr_engagements":target.kind==="compensation"?"SELECT c.id,e.company_id,e.person_id,c.engagement_id FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id":"SELECT id,company_id,person_id,engagement_id FROM people_lifecycle_cases";
 return database.prepare(`${select} WHERE ${target.kind==="compensation"?"c.":""}id=?`).bind(target.id).first<Source>();
}
async function current(database:SqlDatabase,actor:WorkContext,key:string,permission?:string){
 const where=permission?assignmentReadSql("people_owner_scope",permission):`EXISTS(SELECT 1 FROM assignments a WHERE a.id=? AND a.user_id=? AND a.role=? AND a.company_id=people_owner_scope.company_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=a.role AND role.company_id IN ('*',a.company_id) AND role.status='active') AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=people_owner_scope.branch_id OR a.scope_kind='department' AND a.scope_node_id=people_owner_scope.department_id))`;
 if(permission)requirePermission(actor,permission);
 const row=await database.prepare(`SELECT ${where} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) people_owner_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<{allowed:boolean}>();
 if(!row?.allowed)throw new ApiError(403,"PEOPLE_ACCESS_CHANGED","Choose a current authorized People work role.");
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
/** Full People readers use this base capability. Typed worker/learner and
 * organization contact readers retain their smaller owner contracts. */
export async function withPeoplePurchasedRead<Result>(actor:WorkContext,capability:Capability,run:(database:SqlDatabase,actor:WorkContext)=>Promise<Result>,permission?:string,provided?:SqlDatabase){
 const mode=await peoplePackageEnforcementMode(),database=provided??(await holdingResources(actor.holdingId)).write;
 if(mode==="disabled")return run(database,actor);
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:"history"});await current(database,actor,`people-read:${randomUUID()}`,permission);
 const live=await loadIamAuthority(database,actor.companyId,actor.userId,actor.role,actor.authority?.sessionId);
 return run(database,{...actor,iam:{...live,permissions:live.permissions.filter(permission=>hasPermission(actor,permission)),grantablePermissions:live.grantablePermissions.filter(permission=>hasPermission(actor,permission))}});
}
/** The original owner still prepares every version/organization/independence,
 * money, receipt/audit/outbox effect on this same Holding transaction. */
export async function withPeoplePurchasedCommand<Result>(actor:WorkContext,command:Command,run:(database:SqlDatabase)=>Promise<Result>,providedOwner?:SqlDatabase){
 const mode=await peoplePackageEnforcementMode();if(mode==="disabled")return run(providedOwner??(await holdingResources(actor.holdingId)).write);
 const owner=(await holdingResources(actor.holdingId)).write;
 const execute=async(database:SqlDatabase)=>{
  const composition=compositions.get(database);if(!composition||composition.companyId&&composition.companyId!==actor.companyId)throw new ApiError(503,"PEOPLE_COMPOSITION_REQUIRED","People requires the complete owned Holding transaction.");
  await database.batch(companyPurchaseReadLocks(database,actor.companyId));
  const replay=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first();
  const scope={companyId:actor.companyId,capability:command.capability,purpose:replay?"history" as const:command.purpose},key=`people-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  const before=command.target?await source(database,command.target):null;
  if(command.target&&(!before||before.company_id!==actor.companyId))throw new ApiError(404,"PEOPLE_SOURCE_NOT_FOUND","The People source was not found in this Company.");
  await checkPurchasedCapability(database,mode,scope);await database.batch(boundary.before);await current(database,actor,key,command.permission);
  const result=await run(database),created=command.resultKind&&result&&typeof result==="object"&&"id" in result?{kind:command.resultKind,id:String(result.id)}:null,target=command.target??created,expected=before??(target?await source(database,target):null),createdSource=created?await source(database,created):null;
  if(target&&(!expected||expected.company_id!==actor.companyId)||created&&(!createdSource||createdSource.company_id!==actor.companyId||before?.person_id&&createdSource.person_id!==before.person_id||before?.engagement_id&&createdSource.engagement_id!==before.engagement_id))throw new ApiError(409,"PEOPLE_SOURCE_CHANGED","The People source or Company changed.");
  const check=async()=>{
   await current(database,actor,key,command.permission);
   if(target){const after=await source(database,target);if(JSON.stringify(expected)!==JSON.stringify(after))throw new ApiError(409,"PEOPLE_SOURCE_CHANGED","The original People owner identity changed.");}
   if(created&&JSON.stringify(createdSource)!==JSON.stringify(await source(database,created)))throw new ApiError(409,"PEOPLE_SOURCE_CHANGED","The created People record changed its original owner.");
   const observed=await database.batch([...boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);boundary.observe?.(observed);
  };
  await check();composition.checks.push(check);return result;
 };
 if(providedOwner&&!isTransactionalSqlDatabase(providedOwner)){
  const ownership=owner as SqlDatabase&{assertActiveTransaction?:(database:SqlDatabase)=>void};if(!ownership.assertActiveTransaction)throw new ApiError(503,"TRANSACTION_OWNERSHIP_REQUIRED","People requires an active owned Holding transaction.");ownership.assertActiveTransaction(providedOwner);return execute(providedOwner);
 }
 return ownedPeopleTransaction(owner,actor,mode,execute);
}
async function ownedPeopleTransaction<Result>(owner:SqlDatabase,actor:WorkContext|undefined,mode:PackageEnforcementMode,run:(database:SqlDatabase)=>Promise<Result>){
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"PEOPLE_TRANSACTION_REQUIRED","People requires its owned Holding transaction.");
 try{return await owner.transaction(async database=>{
  if(mode==="disabled")return run(database);
  const composition={companyId:actor?.companyId,checks:[] as (()=>Promise<void>)[]};compositions.set(database,composition);
  try{await database.batch(actor?companyPurchaseReadLocks(database,actor.companyId):holdingPurchaseReadLocks(database));const result=await run(database);await flushFinanceDeferredEffects(database);for(const check of composition.checks)await check();return result;}finally{compositions.delete(database);}
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
/** Organization's complete draft transaction owns its original atomic group;
 * a nested lifecycle cancellation registers its final checks on that owner. */
export async function peopleOwnerTransaction<Result>(owner:SqlDatabase,actor:WorkContext|undefined,run:(database:SqlDatabase)=>Promise<Result>){return ownedPeopleTransaction(owner,actor,await peoplePackageEnforcementMode(),run);}
/** Minimal foundation facts: no contact details, compensation, lifecycle,
 * leave balances, private documents or payroll snapshots. */
export async function peopleReferenceRows(actor:WorkContext,kind:"worker"|"learner",provided?:SqlDatabase){
 const database=provided??(await holdingResources(actor.holdingId)).write,capability=kind==="worker"?"people.worker-reference":"people.learner-reference",mode=await peoplePackageEnforcementMode();
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:"history"});if(mode!=="disabled")await current(database,actor,`people-reference:${randomUUID()}`,"records.read");
 return (await database.prepare("SELECT p.id,p.legal_name name,e.id engagement_id,e.company_id,e.branch_id,e.department_id,e.job_title FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY p.legal_name,e.id").bind(actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<{id:string;name:string;engagement_id:string;company_id:string;branch_id:string|null;department_id:string|null;job_title:string}>()).results;
}
/** Mixed overview counts must not preload private Payroll without its add-on. */
export async function peopleOverviewPurchases(database:SqlDatabase,actor:WorkContext){return readPurchasedAccesses(database,await peoplePackageEnforcementMode(),actor.companyId,["people.directory","people.lifecycle","people.payroll","people.expenses"]);}

/** Minimal request-local controls only; no grant, authority or private source
 * snapshots are serialized into browser props. Owner permissions still apply. */
export async function peoplePurchaseControls(actor:WorkContext,provided?:SqlDatabase):Promise<import("@/lib/people_payroll/purchase-controls").PeoplePurchaseControls>{
 const mode=await peoplePackageEnforcementMode(),database=provided??(await holdingResources(actor.holdingId)).write;
 if(mode!=="disabled")await current(database,actor,`people-controls:${randomUUID()}`);
 const access=await readPurchasedAccesses(database,mode,actor.companyId,["people.directory","people.lifecycle","foundation.identity"]),minimal=(capability:string)=>{const {newWork,settleExisting,history,revision}=access[capability];return {newWork,settleExisting,history,revision};};
 return {directory:minimal("people.directory"),lifecycle:minimal("people.lifecycle"),identity:minimal("foundation.identity")};
}

export {current as requirePeopleCurrentAuthority};

/** Calendar's original task owner retains its receipt, notifications, complete
 * clearance/evidence effects and deferred constraints in this same unit. */
export async function withPeoplePreparationTransaction<Result>(owner:SqlDatabase,actor:WorkContext,input:{id:string;action:"task.plan"|"task.complete"},key:string,run:(database:SqlDatabase)=>Promise<Result>){
 return peopleOwnerTransaction(owner,actor,async database=>{
  if(await peoplePackageEnforcementMode()==="disabled")return run(database);
  const task=await database.prepare("SELECT case_id,clearance_type,privileged FROM people_preparation_tasks WHERE id=? AND company_id=?").bind(input.id,actor.companyId).first<{case_id:string|null;clearance_type:import("@/lib/people_payroll/retirement").ClearanceKind|null;privileged:boolean}>();
  if(!task)throw new ApiError(404,"PEOPLE_SOURCE_NOT_FOUND","The original preparation task was not found in this Company.");
  const {clearancePermissions}=await import("@/lib/people_payroll/retirement");
  const permission=task.clearance_type?(input.action==="task.plan"&&hasPermission(actor,"people.manage")?"people.manage":clearancePermissions[task.clearance_type]??undefined):task.privileged||input.action==="task.plan"&&!hasPermission(actor,"organization.manage")?"iam.directory.manage":input.action==="task.plan"?"organization.manage":hasPermission(actor,"iam.directory.manage")?"iam.directory.manage":undefined;
  return withPeoplePurchasedCommand(actor,{key,operation:`calendar.${input.action}`,capability:task.case_id?"people.lifecycle":"foundation.identity",purpose:"settle-existing",permission,target:{kind:input.action==="task.plan"?"preparation-plan":"preparation-complete",id:input.id}},run,database);
 });
}
