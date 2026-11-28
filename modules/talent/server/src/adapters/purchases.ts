import "server-only";
import {talentActorScopeSelect} from './talent-scope-sql';
import {talentExistsSelect} from './talent-read-tail-r36-sources';
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {assignmentReadSql} from "@erpx/identity-access-server/public/record-source-authority";
import {iamMutationGuards} from "@erpx/identity-access-server/public/iam-store";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {workContextGuards,financialConstraintError} from '@erpx/workflow-server/public/command-guards';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {flushFinanceDeferredEffects} from "@erpx/finance-server/public/finance-transactions";
import {nativeTalentPlan,nativeTalentReadFirst} from "./native-talent-read";
import {talentHirePeopleSourceReadLocks} from "@erpx/people-payroll-server/public/hire-purchase-sources";
import {talentPackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/commercial-server/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/commercial-server/public/rules/packages";
export type TalentCapability="recruit.positions"|"recruit.candidates"|"recruit.applications"|"recruit.offers";
export type TalentTarget={kind:"requisition"|"candidate"|"application"|"proposal"|"offer"|"handoff";id:string};
type Command={key:string;operation:string;capability:TalentCapability;purpose:CapabilityPurpose;target:TalentTarget;creating?:boolean;permissions:readonly string[]};
const capabilities:readonly TalentCapability[]=["recruit.positions","recruit.candidates","recruit.applications","recruit.offers"],compositions=new WeakMap<SqlDatabase,{companyId:string;checks:(()=>Promise<void>)[]}>();
// All owner-source images (candidate, requisition, application, proposal,
// offer, handoff) are now planned by the native owner read port.
export function talentOwnerSourceArgs(actor:WorkContext,target:TalentTarget){return [target.id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];}
const candidateImageKind=(file:boolean)=>file?"talent_rd_candidate_source_image_file":"talent_rd_candidate_source_image";
const requisitionImageKind=(file:boolean)=>file?"talent_rd_requisition_source_image_file":"talent_rd_requisition_source_image";
const compositeImageKind=(kind:"application"|"proposal"|"offer"|"handoff",file:boolean)=>file?`talent_rd_${kind}_source_image_file`:`talent_rd_${kind}_source_image`;
const ownerImageKind=(target:TalentTarget,file:boolean)=>
 target.kind==="candidate"?candidateImageKind(file):
 target.kind==="requisition"?requisitionImageKind(file):
 target.kind==="offer"||target.kind==="handoff"?compositeImageKind(target.kind,file):
 target.kind==="application"||target.kind==="proposal"?compositeImageKind(target.kind,file):null;
export async function talentOwnerSourceImage(database:SqlDatabase,actor:WorkContext,target:TalentTarget,file=false){
 const kind=ownerImageKind(target,file);
 if(!kind)throw new ApiError(422,'TALENT_OWNER_IMAGE_KIND_REQUIRED','The owner source image kind is not registered.');
 const values=target.kind==="candidate"?[target.id,actor.companyId]:talentOwnerSourceArgs(actor,target);
 const row=await nativeTalentReadFirst<{image:string}>(kind,values,database);
 return row?.image??null;
}
/** Deferred owner-source witness guard. Every owner-source image family now
 * reuses the exact statement planned by the native owner port; the guard still
 * executes on the caller's unit at commit time. */
export async function talentOwnerSourceWitnessAssertion(database:SqlDatabase,key:string,step:string,actor:WorkContext,target:TalentTarget,witness:string,file=false){
 const kind=ownerImageKind(target,file);
 if(!kind)throw new ApiError(422,'TALENT_OWNER_IMAGE_KIND_REQUIRED','The owner source image kind is not registered.');
 const native=await nativeTalentPlan(kind,target.kind==="candidate"?[target.id,actor.companyId]:talentOwnerSourceArgs(actor,target));
 const args=[...native.values,witness] as (string|number|null)[];
 return assertion(database,key,step,`(${native.sql})=?`,args);
}
function talentOwnerSourceTableLocks(database:SqlDatabase,handoff:boolean){return [database.prepare("LOCK TABLE talent_requisitions,talent_candidates,talent_applications,talent_proposals,talent_offers,talent_hire_handoffs,talent_hire_consumption_receipts IN SHARE ROW EXCLUSIVE MODE"),...(handoff?talentHirePeopleSourceReadLocks(database):[])];}
export function talentOwnerSourceReadLocks(database:SqlDatabase,companyId:string,handoff=false){const locks=companyPurchaseReadLocks(database,companyId);return [locks[0],...talentOwnerSourceTableLocks(database,handoff),...locks.slice(1)];}
/** Closed original accepted-hire source leases, before the composing owner snapshot. */
export function hireHandoffPayGroupSourceLocks(database:SqlDatabase){return talentOwnerSourceTableLocks(database,true);}
export function hireHandoffPayGroupSourceImage(database:SqlDatabase,actor:WorkContext,handoffId:string){return talentOwnerSourceImage(database,actor,{kind:"handoff",id:handoffId});}
export async function requireTalentCurrentAuthority(database:SqlDatabase,actor:WorkContext,permissions:readonly string[],key:string){
 for(const permission of permissions){if(!await database.prepare(talentActorScopeSelect("talent_scope",`${assignmentReadSql("talent_scope",permission)} allowed`)).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed"))throw new ApiError(403,"TALENT_ACCESS_CHANGED","Choose a current authorized Recruitment work role.");}
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),await nativeClearCommandAssertions(database, actor.companyId, key)]);
}
export async function talentPurchaseControls(actor:WorkContext,provided?:SqlDatabase){const mode=await talentPackageEnforcementMode();if(mode==="disabled")return undefined;const database=provided??(await holdingResources(actor.holdingId)).write;await requireTalentCurrentAuthority(database,actor,["records.read","records.review"],`talent-controls:${randomUUID()}`);const purchases=await readPurchasedAccesses(database,mode,actor.companyId,capabilities);return Object.fromEntries(Object.entries(purchases).map(([capability,access])=>[capability,{newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision}]));}
export async function withTalentPurchasedRead<Result>(actor:WorkContext,required:readonly TalentCapability[],run:(database?:SqlDatabase)=>Promise<Result>,provided?:SqlDatabase,permissions:readonly string[]=["records.read","records.review"]){
 const mode=await talentPackageEnforcementMode();if(mode==="disabled")return run(provided);const database=provided??(await holdingResources(actor.holdingId)).write;await requireTalentCurrentAuthority(database,actor,permissions,`talent-read:${randomUUID()}`);for(const capability of required)await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:"history"});return run(database);
}
export async function withTalentPurchasedCommand<Result>(actor:WorkContext,command:Command,run:(database?:SqlDatabase)=>Promise<Result>,provided?:SqlDatabase){
 const mode=await talentPackageEnforcementMode();if(mode==="disabled")return run(provided);const owner=(await holdingResources(actor.holdingId)).write;
 const execute=async(database:SqlDatabase)=>{const composition=compositions.get(database);if(!composition||composition.companyId!==actor.companyId)throw new ApiError(503,"TALENT_COMPOSITION_REQUIRED","Use the original owned Recruitment transaction.");
  const receipt=await database.prepare(talentExistsSelect('command_receipts', "command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")).bind(command.key,command.operation,actor.userId,actor.companyId).first(),purchase={companyId:actor.companyId,capability:command.capability,purpose:receipt?"history" as const:command.purpose},key=`talent-purchase:${randomUUID()}`;
  await requireTalentCurrentAuthority(database,actor,command.permissions,key);if(!command.creating&&!await talentOwnerSourceImage(database,actor,command.target))throw new ApiError(404,command.target.kind==="handoff"?"HIRE_HANDOFF_NOT_FOUND":command.target.kind==="candidate"?"CANDIDATE_NOT_FOUND":"TALENT_RECORD_NOT_FOUND","The actual Recruitment source is unavailable in this workspace.");await checkPurchasedCapability(database,mode,purchase);const boundary=purchaseCommandBoundary(database,mode,key,purchase)!;await database.batch(boundary.before);const result=await run(database),witness=await talentOwnerSourceImage(database,actor,command.target);if(!witness)throw new ApiError(409,"TALENT_SOURCE_CHANGED","The actual Recruitment owner changed.");composition.checks.push(async()=>{await requireTalentCurrentAuthority(database,actor,command.permissions,key);const observed=await database.batch([await talentOwnerSourceWitnessAssertion(database,key,"talent-owner-source",actor,command.target,witness),...boundary.after,await nativeClearCommandAssertions(database, actor.companyId, key)]);boundary.observe?.(observed);});return result;
 };
 try{if(provided)return await execute(provided);if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Recruitment requires its original Holding transaction.");return await owner.transaction(async database=>{await database.batch(talentOwnerSourceReadLocks(database,actor.companyId,command.target.kind==="handoff"));compositions.set(database,{companyId:actor.companyId,checks:[]});try{const result=await execute(database);await flushFinanceDeferredEffects(database);for(const check of compositions.get(database)!.checks)await check();return result;}finally{compositions.delete(database);}});}catch(error){if((error as {code?:string}).code==="23514"&&String((error as {detail?:string}).detail).includes("talent-owner-source")){const failure=new ApiError(409,"TALENT_SOURCE_CHANGED","The Recruitment source changed before commit. Refresh before retrying.");failure.cause=error;throw failure;}throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export function withTalentHirePurchasedCommand<Result>(actor:WorkContext,handoffId:string,key:string,run:(database?:SqlDatabase)=>Promise<Result>){return withTalentPurchasedCommand(actor,{key,operation:"people.hire-handoff.consume",capability:"recruit.offers",purpose:"settle-existing",target:{kind:"handoff",id:handoffId},permissions:["people.manage"]},run);}
export function withTalentPrivacyPurchasedCommand<Result>(actor:WorkContext,candidateId:string,input:{action:"hold"|"release"|"withdraw"|"erase"},key:string,run:(database?:SqlDatabase)=>Promise<Result>){const permission=input.action==="hold"?"legal.hold":input.action==="release"?"legal.hold.release":input.action==="erase"?"legal.destroy":"people.manage";return withTalentPurchasedCommand(actor,{key,operation:`talent.privacy.${input.action}`,capability:"recruit.candidates",purpose:input.action==="withdraw"?"settle-existing":"history",target:{kind:"candidate",id:candidateId},permissions:[permission]},run);}
