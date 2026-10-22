import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {lockQueuedCommandAuthority} from "@erpx/server-identity_access/public/queued-command-authority";
import {learningWorkerReferenceSourceReadLocks,learningWorkerReferenceWitness} from "@erpx/server-people_payroll/public/learning-worker-references";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {isTransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {workContextGuards,financialConstraintError} from "@erpx/server-workflow/public/commands";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {learningPackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";

import type {LearningCapability,LearningPurchaseControls} from "@erpx/server-learning/domain/purchase-controls";
export type {LearningCapability,LearningPurchaseControls} from "@erpx/server-learning/domain/purchase-controls";
type Target={kind:"course"|"assignment"|"certificate";id:string};
const commands=new WeakMap<SqlDatabase,{actorId:string;assignmentId:string;target:Target;operation:string}>();
/** Private owner marker, never a public transaction registration port. */
export function learningOwnsAttachmentAction(database:SqlDatabase,actor:WorkContext,id:string){const command=commands.get(database);return command?.actorId===actor.userId&&command.assignmentId===actor.assignmentId&&command.target.kind==="assignment"&&command.target.id===id&&command.operation==="learning.assignment.complete";}
const capabilities:LearningCapability[]=["learn.courses","learn.assignments","learn.results","learn.certificates","people.learner-reference"];
async function current(database:SqlDatabase,actor:WorkContext,key:string,permission:string){
 const row=await database.prepare(`SELECT ${assignmentReadSql("learning_scope",permission)}
 AND EXISTS(SELECT 1 FROM assignments learning_module WHERE learning_module.id=? AND learning_module.company_id=learning_scope.company_id
 AND learning_module.user_id=? AND learning_module.role=? AND EXISTS(SELECT 1 FROM erpx_json_each(learning_module.modules_json) module WHERE module.value='learning')) allowed FROM
 (SELECT ?::text company_id,?::text branch_id,?::text department_id) learning_scope`)
  .bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<{allowed:boolean}>();
 if(!row?.allowed)throw new ApiError(403,"LEARNING_ACCESS_CHANGED","Choose a current Learning assignment.");
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
export async function learningPurchaseControls(database:SqlDatabase,actor:WorkContext):Promise<LearningPurchaseControls|undefined>{
 const mode=await learningPackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,`learning-read:${randomUUID()}`,"records.read");
 const access=await readPurchasedAccesses(database,mode,actor.companyId,capabilities);
 return Object.fromEntries(capabilities.map(code=>[code,{newWork:access[code].newWork,settleExisting:access[code].settleExisting,history:access[code].history,revision:access[code].revision}]));
}
export async function requireLearningPurchasedRead(database:SqlDatabase,actor:WorkContext,capability:LearningCapability="learn.assignments"){
 const access=await learningPurchaseControls(database,actor);
 if(access&&!access[capability]?.history)throw new ApiError(403,"PACKAGE_REQUIRED","This Learning work requires its purchased capability or retained history.");
 return access;
}
async function image(database:SqlDatabase,actor:WorkContext,target:Target){
 const table=target.kind==="course"?"learning_courses":target.kind==="assignment"?"learning_assignments":"learning_certificates";
 const row=await database.prepare(`SELECT to_jsonb(source)-'updated_at' image FROM ${table} source WHERE id=? AND company_id=?`)
  .bind(target.id,actor.companyId).first<{image:Record<string,unknown>}>();
 return row?.image??null;
}
function identity(row:Record<string,unknown>|null,kind:Target["kind"]){
 if(!row)return null;
 const keys=kind==="course"?["id","company_id","created_by","title","description","provider","duration_minutes","required_for"]:
 kind==="assignment"?["id","company_id","branch_id","department_id","course_id","course_version","learner_id","created_by"]:
 ["id","company_id","handoff_id","person_id","assignment_id","source_assignment_version","eligibility_code","issued_by"];
 return JSON.stringify(keys.map(key=>[key,row[key]]));
}
async function sourceLeases(database:SqlDatabase,actor:WorkContext){
 const purchase=companyPurchaseReadLocks(database,actor.companyId);
 await database.batch([purchase[0],database.prepare('LOCK TABLE learning_courses,learning_assignments,learning_eligibility_handoffs,learning_certificates,learning_certificate_revocations IN SHARE ROW EXCLUSIVE MODE'),...learningWorkerReferenceSourceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await database.batch(purchase.slice(1));
}
/** The actual workspace owns its read snapshot. Reference choices and original
 * Learning rows remain stable through final effects; no private witness is
 * returned in the workspace or browser props. */
export async function withLearningPurchasedRead<Result>(owner:SqlDatabase,actor:WorkContext,run:(database:SqlDatabase)=>Promise<Result>){
 if(await learningPackageEnforcementMode()==='disabled')return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'LEARNING_READ_TRANSACTION_REQUIRED','Learning references require their complete owned read transaction.');
 const context=JSON.stringify(actor);
 return owner.transaction(async database=>{
  await sourceLeases(database,actor);const key=`learning-reference-read:${randomUUID()}`;
  await current(database,actor,key,'records.read');
  const witness=async()=>JSON.stringify({people:await learningWorkerReferenceWitness(database,actor.companyId),learning:await database.prepare(`SELECT jsonb_build_object(
  'courses',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM learning_courses c WHERE c.company_id=?),
  'assignments',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM learning_assignments a WHERE a.company_id=?))::text image`).bind(actor.companyId,actor.companyId).first<string>('image')});
  const before=await witness(),result=await run(database);await flushFinanceDeferredEffects(database);
  if(JSON.stringify(actor)!==context||await witness()!==before)throw new ApiError(409,'LEARNING_SOURCE_CHANGED','The original learner references changed during the read.');
  await current(database,actor,key,'records.read');return result;
 });
}
/** Owns the complete Learning unit. A supplied/foreign transaction cannot be
 * attached after taking its first snapshot or flushing only an inner receipt. */
export async function withLearningPurchasedCommand<Result>(owner:SqlDatabase,actor:WorkContext,command:{key:string;operation:string;target:Target;purpose:CapabilityPurpose;capabilities:readonly LearningCapability[];additionalCapabilities?:(database:SqlDatabase)=>Promise<LearningCapability[]>;sourceWitness?:(database:SqlDatabase)=>Promise<string>;readOnlySourceWitness?:(database:SqlDatabase)=>Promise<string>;permission?:string;existing?:boolean},run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await learningPackageEnforcementMode();if(mode==="disabled")return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"LEARNING_TRANSACTION_REQUIRED","Learning requires its complete owned Holding transaction.");
 try{return await owner.transaction(async database=>{
  await sourceLeases(database,actor);
  const prior=await database.prepare("SELECT operation FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
   .bind(command.key,actor.userId,actor.companyId).first<{operation:string}>();
  const replay=prior?.operation===command.operation,purpose=replay?"history" as const:command.purpose;
  const before=await image(database,actor,command.target);
  if((command.existing||replay)&&!before)throw new ApiError(404,"LEARNING_SOURCE_NOT_FOUND","Learning source not found in this Company.");
  const key=`learning-purchase:${randomUUID()}`,permission=command.permission??"records.write";
  const scopes=[...new Set([...command.capabilities,...(await command.additionalCapabilities?.(database)??[])])].map(capability=>({companyId:actor.companyId,capability,purpose}));
  const boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
  for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);
  await current(database,actor,key,"records.write");if(permission!=="records.write")await current(database,actor,key,permission);
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const context=JSON.stringify(actor),readOnlyWitness=await command.readOnlySourceWitness?.(database);
  commands.set(database,{actorId:actor.userId,assignmentId:actor.assignmentId,target:command.target,operation:command.operation});
  try{
  const result=await run(database),expected=await image(database,actor,command.target),witness=await command.sourceWitness?.(database);
  if(!expected||before&&identity(before,command.target.kind)!==identity(expected,command.target.kind))throw new ApiError(409,"LEARNING_SOURCE_CHANGED","The Learning source changed during the command.");
  await flushFinanceDeferredEffects(database);
  const final=await image(database,actor,command.target);
  if(JSON.stringify(actor)!==context||JSON.stringify(expected)!==JSON.stringify(final)||command.sourceWitness&&witness!==await command.sourceWitness(database)||command.readOnlySourceWitness&&readOnlyWitness!==await command.readOnlySourceWitness(database))throw new ApiError(409,"LEARNING_SOURCE_CHANGED","The Learning source changed after its owner effects.");
  await current(database,actor,key,"records.write");if(permission!=="records.write")await current(database,actor,key,permission);
  const observations=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));
  boundaries[0]?.boundary.observe?.(observations);return result;
  }finally{commands.delete(database);}
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
