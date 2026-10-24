import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {isTransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {peoplePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccess,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {requirePeopleCurrentAuthority} from "@erpx/server-people_payroll/public/people-purchases";
import {activeBenefitPerson,benefitPeopleSourceLocks,benefitPeopleSourceWitness} from "@erpx/server-people_payroll/public/benefits-owner-facts";
import {benefitEligibilitySourceLocks,benefitEligibilitySourceWitness} from "@erpx/server-learning/public/benefit-source-facts";
import type {BenefitRecordKind} from "@erpx/server-benefits/domain/benefits";

export type BenefitPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
type Target={kind:BenefitRecordKind;id:string};
type References={personId?:string;planId?:string;windowId?:string;dependentIds?:readonly string[]};
type Command={key:string;operation:string;target:Target;purpose:CapabilityPurpose;review?:boolean;existing?:boolean;references?:Omit<References,"personId">;employee?:boolean};
const tables={plan:"benefit_plans",window:"benefit_enrollment_windows",dependent:"benefit_dependents",election:"benefit_elections"} as const;
const permission=(review:boolean)=>review?"benefits.review":"records.write";

export const benefitAssignmentModulePredicate="((selected.role LIKE 'custom_%' AND iam_role_allows(selected.role,selected.company_id,'module.benefits')) OR (selected.role NOT LIKE 'custom_%' AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('benefits'::text))))";
async function current(database:SqlDatabase,actor:WorkContext,key:string,write:boolean,review=false){
 if(!hasModuleAccess(actor.modules,"benefits"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Benefits.");
 await requirePeopleCurrentAuthority(database,actor,key,write?"records.write":"records.read");
 if(!await database.prepare(`SELECT 1 FROM assignments selected WHERE selected.id=? AND ${benefitAssignmentModulePredicate}`).bind(actor.assignmentId).first())throw new ApiError(403,"BENEFIT_ACCESS_CHANGED","The current assignment no longer includes Benefits.");
 if(review)await requirePeopleCurrentAuthority(database,actor,key,permission(true));
}
export function benefitSourceReadLocks(database:SqlDatabase){
 return [database.prepare("LOCK TABLE benefit_plans,benefit_plan_events,benefit_enrollment_windows,benefit_window_events,benefit_dependents,benefit_dependent_events,benefit_elections,benefit_election_events,benefit_payroll_inputs IN SHARE ROW EXCLUSIVE MODE"),...benefitPeopleSourceLocks(database),...benefitEligibilitySourceLocks(database)];
}
export async function benefitPurchaseControls(actor:WorkContext,provided?:SqlDatabase):Promise<BenefitPurchaseControls|undefined>{
 const mode=await peoplePackageEnforcementMode();if(mode==="disabled")return undefined;
 const database=provided??(await holdingResources(actor.holdingId)).write;
 await current(database,actor,`benefit-read:${randomUUID()}`,false);
 const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"people.benefits"});
 return {newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision};
}
export async function withBenefitPurchasedRead<Result>(actor:WorkContext,run:(database:SqlDatabase,live:WorkContext,purchase?:BenefitPurchaseControls)=>Promise<Result>,empty?:()=>Result){
 const database=(await holdingResources(actor.holdingId)).write,mode=await peoplePackageEnforcementMode();
 if(mode==="disabled")return run(database,actor);
 if(empty&&(!hasModuleAccess(actor.modules,"benefits")||!hasPermission(actor,"records.read")))return empty();
 const purchase=await benefitPurchaseControls(actor,database);
 if(purchase&&!purchase.history){if(empty)return empty();throw new ApiError(403,"PACKAGE_REQUIRED","This Benefits work requires its purchased capability or retained history.");}
 const live=await loadIamAuthority(database,actor.companyId,actor.userId,actor.role,actor.authority?.sessionId);
 const authorized={...actor,iam:{...live,permissions:live.permissions.filter(code=>hasPermission(actor,code)),grantablePermissions:live.grantablePermissions.filter(code=>hasPermission(actor,code))}};
 return run(database,authorized,purchase);
}
async function row(database:SqlDatabase,actor:WorkContext,target:Target){
 const image=await database.prepare(`SELECT to_jsonb(r)::text image FROM ${tables[target.kind]} r WHERE r.id=? AND r.company_id=?`).bind(target.id,actor.companyId).first<string>("image");
 return image?JSON.parse(image) as Record<string,unknown>:null;
}
function identity(value:Record<string,unknown>){
 // Only the original transition's status/version and review attribution change.
 const mutable=new Set(["status","version","updated_at","submitted_by","acknowledged_by","released_by","reviewed_by","provider_reference"]);
 return JSON.stringify(Object.entries(value).filter(([field])=>!mutable.has(field)).sort(([left],[right])=>left.localeCompare(right)));
}
async function references(database:SqlDatabase,actor:WorkContext,refs:References,electionId?:string){
 const select=async(table:"benefit_plans"|"benefit_enrollment_windows",id?:string)=>id?database.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE id=? AND company_id=?`).bind(id,actor.companyId).first<string>("image"):null;
 return JSON.stringify({
  person:refs.personId?await benefitPeopleSourceWitness(database,actor.companyId,refs.personId):null,
  plan:await select("benefit_plans",refs.planId),window:await select("benefit_enrollment_windows",refs.windowId),
  dependents:refs.dependentIds?.length?(await database.prepare("SELECT to_jsonb(d) image FROM benefit_dependents d WHERE d.company_id=? AND d.id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY d.id").bind(actor.companyId,JSON.stringify(refs.dependentIds)).all()).results:[],
  learning:electionId?await benefitEligibilitySourceWitness(database,actor.companyId,electionId):null,
 });
}
function targetReferences(value:Record<string,unknown>|null):References{
 if(!value)return {};
 return {personId:value.person_id?String(value.person_id):undefined,planId:value.plan_id?String(value.plan_id):undefined,windowId:value.window_id?String(value.window_id):undefined,dependentIds:typeof value.dependent_ids_json==="string"?JSON.parse(value.dependent_ids_json) as string[]:[]};
}
async function delivery(database:SqlDatabase,actor:WorkContext,target:Target){
 if(target.kind!=="election")return null;
 return database.prepare("SELECT COALESCE(jsonb_agg(to_jsonb(h) ORDER BY h.id),'[]'::jsonb)::text image FROM benefit_payroll_inputs h WHERE h.election_id=? AND h.company_id=?").bind(target.id,actor.companyId).first<string>("image");
}
/** Every preparatory source/evidence read and the original command receipt,
 * money handoff, audit and outbox run in this complete Holding transaction. */
export async function withBenefitPurchasedCommand<Result>(actor:WorkContext,command:Command,run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await peoplePackageEnforcementMode(),owner=(await holdingResources(actor.holdingId)).write;
 if(mode==="disabled")return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"BENEFIT_TRANSACTION_REQUIRED","Benefits requires its original complete Holding transaction.");
 try{return await owner.transaction(async database=>{
  const locks=companyPurchaseReadLocks(database,actor.companyId);
  await database.batch([locks[0],...benefitSourceReadLocks(database),...locks.slice(1)]);
  const key=`benefit-purchase:${randomUUID()}`,prior=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first();
  const purchase={companyId:actor.companyId,capability:"people.benefits",purpose:prior?"history" as const:command.purpose};
  await checkPurchasedCapability(database,mode,purchase);await current(database,actor,key,true,command.review);
  const before=await row(database,actor,command.target);
  if((command.existing||prior)&&!before)throw new ApiError(404,"BENEFIT_SOURCE_NOT_FOUND","The original Benefits source was not found in this Company.");
  const person=command.employee?await activeBenefitPerson(database,actor.companyId,actor.userId):null;
  const refs={...targetReferences(before),...command.references,...(person?{personId:person.id}:{})},existingElection=before&&command.target.kind==="election"?command.target.id:undefined;
  const beforeReferences=await references(database,actor,refs,existingElection),boundary=purchaseCommandBoundary(database,mode,key,purchase)!;
  await database.batch(boundary.before);
  const result=await run(database),expected=await row(database,actor,command.target);
  if(!expected||before&&identity(before)!==identity(expected))throw new ApiError(409,"BENEFIT_SOURCE_CHANGED","The original Benefits owner identity changed.");
  if(beforeReferences!==await references(database,actor,refs,existingElection))throw new ApiError(409,"BENEFIT_SOURCE_CHANGED","A Benefits worker, reviewed source or consumed certificate changed.");
  const finalRefs=targetReferences(expected),expectedReferences=await references(database,actor,finalRefs,command.target.kind==="election"?command.target.id:undefined),expectedDelivery=await delivery(database,actor,command.target);
  await flushFinanceDeferredEffects(database);
  if(JSON.stringify(expected)!==JSON.stringify(await row(database,actor,command.target))||expectedReferences!==await references(database,actor,finalRefs,command.target.kind==="election"?command.target.id:undefined)||expectedDelivery!==await delivery(database,actor,command.target)||beforeReferences!==await references(database,actor,refs,existingElection))throw new ApiError(409,"BENEFIT_SOURCE_CHANGED","The actual Benefits source changed after its owner effects.");
  await current(database,actor,key,true,command.review);
  const observations=await database.batch([...boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);boundary.observe?.(observations);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
