import "server-only";
import {randomUUID} from "node:crypto";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {StaffActor,StaffPerson} from "@erpx/server-operations/public/staff-policy";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {isTransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {lockQueuedCommandAuthority} from "@erpx/server-identity_access/public/queued-command-authority";
import {StaffPolicyIdentityAccessFacts} from "@erpx/server-identity_access/public/operations-staff-policy-facts";
import {projectWorkerReferenceSourceReadLocks,projectWorkerReferenceWitness} from "@erpx/server-people_payroll/public/project-worker-references";
import {projectTaskSourceReadLocks} from "@erpx/server-operations/public/project-source-locks";
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {projectsPackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";

export type ProjectCapability="projects.wbs"|"projects.tasks"|"projects.time"|"projects.handover"|"projects.costs";
export type ProjectPurchaseTarget={kind:"milestone"|"work_log"|"allocation"|"handoff";id:string}|{kind:"work_date";date:string}|{kind:"cost_line";id:string};
type Source={id:string;company_id:string;branch_id:string|null;department_id:string|null;owner_user_id:string};
async function targetImage(database:SqlDatabase,actor:StaffActor,projectId:string,target:ProjectPurchaseTarget){
 const table=target.kind==='milestone'?'staff_project_milestones':target.kind==='work_log'||target.kind==='work_date'?'staff_project_work_logs':target.kind==='allocation'||target.kind==='cost_line'?'staff_project_cost_allocations':'staff_project_handoffs';
 const identity=target.kind==='work_log'||target.kind==='work_date'?',user_id':target.kind==='allocation'||target.kind==='cost_line'?',journal_line_id,created_by':target.kind==='handoff'?',source_project_version,recipient_id,prepared_by':'';
 const predicate=target.kind==='work_date'?'project_id=? AND work_date=? AND iam_same_person(user_id,?,company_id)':target.kind==='cost_line'?'project_id=? AND journal_line_id=?':'id=?';
 const args=target.kind==='work_date'?[projectId,target.date,actor.userId]:target.kind==='cost_line'?[projectId,target.id]:[target.id];
 const row=await database.prepare(`SELECT id,company_id,project_id${identity} FROM ${table} WHERE company_id=? AND ${predicate}`).bind(actor.companyId,...args).first<Record<string,string|number|null>>();
 if(row&&row.project_id!==projectId)throw new ApiError(409,'PROJECT_SOURCE_CHANGED','The original Project source changed.');
 return row;
}
async function source(database:SqlDatabase,actor:StaffActor,id:string,member:boolean){
 const row=await database.prepare(`SELECT p.id,p.company_id,p.branch_id,p.department_id,p.owner_user_id FROM staff_projects p
 WHERE p.id=? AND p.company_id=?
 AND (NOT ?::boolean OR p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))`)
  .bind(id,actor.companyId,member,actor.userId,actor.userId).first<Source>();
 if(!row)throw new ApiError(404,"PROJECT_NOT_FOUND","Project not found in the current workspace.");
 return row;
}
async function current(database:SqlDatabase,actor:StaffActor,key:string,permission:string){
 if(!hasPermission(actor,permission))throw new ApiError(403,"PROJECT_ACCESS_CHANGED","Choose a current Project assignment.");
 // Explicit Project membership can cross a staff branch. Preserve that owner
 // policy while validating the actual selected assignment, never another role.
 const scope={company_id:actor.companyId,branch_id:actor.branchId,department_id:actor.departmentId};
 const found=await database.prepare(`SELECT ${assignmentReadSql("project_scope",permission)} allowed FROM
 (SELECT ?::text company_id,?::text branch_id,?::text department_id) project_scope`)
  .bind(actor.assignmentId,actor.userId,actor.role,scope.company_id,scope.branch_id,scope.department_id).first<{allowed:boolean}>();
 if(!found?.allowed)throw new ApiError(403,"PROJECT_ACCESS_CHANGED","Choose a current Project assignment.");
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
export async function projectPurchaseControls(database:SqlDatabase,actor:StaffActor,options?:{strictProduct:true}){
 const mode=options?.strictProduct?'enforced':await projectsPackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,`project-read:${randomUUID()}`,"records.read");
 const capabilities=["projects.wbs","projects.tasks","projects.time","projects.handover","projects.costs"];
 const all=await readPurchasedAccesses(database,mode,actor.companyId,capabilities);
 return Object.fromEntries(capabilities.map(code=>[code,{newWork:all[code].newWork,settleExisting:all[code].settleExisting,history:all[code].history,revision:all[code].revision}]));
}
export async function requireProjectPurchasedRead(database:SqlDatabase,actor:StaffActor,id?:string,capability:ProjectCapability="projects.wbs",member=true){
 const access=await projectPurchaseControls(database,actor);if(!access)return;
 if(!access[capability].history)throw new ApiError(403,"PACKAGE_REQUIRED","This Project work requires its purchased capability or retained history.");
 if(id)await source(database,actor,id,member);
 return access[capability];
}

type OwnerUnit={companyId:string;checks:Array<()=>Promise<void>>};
const units=new WeakMap<SqlDatabase,OwnerUnit>();
/** Private named composition for Projects and its actual Operations task port.
 * It cannot register an arbitrary supplied client, and all final checks follow
 * the complete caller effect set rather than an inner command receipt. */
export async function inProjectOwnerUnit<Result>(owner:SqlDatabase,companyId:string,run:(database:SqlDatabase)=>Promise<Result>,sourceLocks?:(database:SqlDatabase)=>Promise<void>){
 const registered=units.get(owner);if(registered){if(registered.companyId!==companyId||sourceLocks)throw new ApiError(503,"PROJECT_TRANSACTION_REQUIRED","A fresh same-Company owned task transaction is required for source locks.");return run(owner);}
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"PROJECT_TRANSACTION_REQUIRED","Projects requires its complete owned Holding transaction.");
 try{return await owner.transaction(async database=>{
  const unit:OwnerUnit={companyId,checks:[]};units.set(database,unit);
  try{if(sourceLocks)await sourceLocks(database);await database.batch(companyPurchaseReadLocks(database,companyId));const result=await run(database);await flushFinanceDeferredEffects(database);for(const check of unit.checks)await check();return result;}
  finally{units.delete(database);}
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export function deferProjectOwnerCheck(database:SqlDatabase,companyId:string,check:()=>Promise<void>){
 const unit=units.get(database);if(!unit||unit.companyId!==companyId)throw new ApiError(503,'PROJECT_TRANSACTION_REQUIRED','The complete Project transaction is required.');unit.checks.push(check);
}
async function standaloneProjectSourceReadLocks(database:SqlDatabase,companyId:string){
 await database.batch([companyPurchaseReadLocks(database,companyId)[0],projectTaskSourceReadLocks(database),database.prepare('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE'),...projectWorkerReferenceSourceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);
}
async function workerReferences(database:SqlDatabase,companyId:string,users:readonly string[]){
 const staff:Array<{user:string;rows:StaffPerson[]}>=[];for(const user of users){const now=Date.now();staff.push({user,rows:(await StaffPolicyIdentityAccessFacts(database).readReadActiveStaffResults(companyId,now,now,user)).results});}
 return JSON.stringify({people:await projectWorkerReferenceWitness(database,companyId,users),staff});
}
async function memberUsers(database:SqlDatabase,id:string){return (await database.prepare('SELECT user_id FROM staff_project_members WHERE project_id=? ORDER BY user_id').bind(id).all<{user_id:string}>()).results.map(row=>row.user_id);}
export async function withProjectPurchasedCommand<Result>(owner:SqlDatabase,actor:StaffActor,
 command:{key:string;operations:readonly string[];projectId:string;purpose:CapabilityPurpose;capability?:ProjectCapability;permission?:string;existing?:boolean;member?:boolean;target?:ProjectPurchaseTarget;memberUserIds?:readonly string[]},run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await projectsPackageEnforcementMode();if(mode==="disabled")return run(owner);
 const members=command.memberUserIds?[...new Set(command.memberUserIds)].sort():null,workers=members?[...new Set([actor.userId,...members])].sort():null,context=JSON.stringify(actor);
 const execute=async(database:SqlDatabase)=>{
  const prior=await database.prepare(`SELECT operation FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?`)
   .bind(command.key,actor.userId,actor.companyId).first<{operation:string}>();
  const replay=!!prior&&command.operations.includes(prior.operation),purpose=replay?"history" as const:command.purpose;
  const before=command.existing||replay?await source(database,actor,command.projectId,command.member!==false):undefined;
  const childBefore=command.target?await targetImage(database,actor,command.projectId,command.target):null;
  const capability=command.capability??"projects.wbs",key=`project-purchase:${randomUUID()}`,permission=command.permission??"records.write";
  const scopes=[...new Set<ProjectCapability>(["projects.wbs",capability])].map(code=>({companyId:actor.companyId,capability:code,purpose}));
  const boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
  for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);
  await current(database,actor,key,permission);await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const workerImage=workers?await workerReferences(database,actor.companyId,workers):null,
   expectedMembers=members?(replay?await memberUsers(database,command.projectId):members):null;
  deferProjectOwnerCheck(database,actor.companyId,async()=>{
   if(workers&&(JSON.stringify(actor)!==context||workerImage!==await workerReferences(database,actor.companyId,workers)||JSON.stringify(expectedMembers)!==JSON.stringify(await memberUsers(database,command.projectId))))throw new ApiError(409,'PROJECT_WORKER_REFERENCE_CHANGED','The original worker reference or prepared Project membership changed during the command.');
   const after=await source(database,actor,command.projectId,command.member!==false);
   if(before&&Object.keys(before).some(field=>before[field as keyof Source]!==after[field as keyof Source]))throw new ApiError(409,"PROJECT_SOURCE_CHANGED","The original Project owner or workspace changed.");
   if(command.target){const childAfter=await targetImage(database,actor,command.projectId,command.target);if(!childAfter||childBefore&&JSON.stringify(childBefore)!==JSON.stringify(childAfter))throw new ApiError(409,'PROJECT_SOURCE_CHANGED','The original Project source changed.');}
   await current(database,actor,key,permission);
   const observations=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));
   boundaries[0]?.boundary.observe?.(observations);
  });
  return run(database);
 };
 if(units.get(owner)?.companyId===actor.companyId){if(members)throw new ApiError(503,'PROJECT_TRANSACTION_REQUIRED','Project membership requires its complete fresh worker-source unit.');return execute(owner);}
 return inProjectOwnerUnit(owner,actor.companyId,execute,database=>standaloneProjectSourceReadLocks(database,actor.companyId));
}
