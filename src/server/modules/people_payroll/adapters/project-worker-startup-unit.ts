import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {requireCompanyAuthority} from '../../operations/public/staff-policy';
import {holdingResources} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {lockPackageSettlementSources,packageWriterFence} from '../../commercial/public/package-locks';
import {companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '../../commercial/public/rules/packages';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled} from '../../pms/public/hospitality-rollout';
import {projectWorkerOrganizationSourceLocks} from '../../organization/public/project-worker-startup-sources';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {financialConstraintError} from '../../workflow/public/commands';
import {registerProjectWorkerUnit,projectWorkerOwnedSelfDirectory,projectWorkerSelfDirectoryImage} from './project-worker-startup-binding';
import {workerStartupSourcePolicy,type WorkerStartupConsumer} from './project-worker-startup-source-sql';
export const projectWorkerSourceChanged=(consumer:WorkerStartupConsumer='projects')=>new ApiError(409,workerStartupSourcePolicy(consumer).errorPrefix+'SOURCE_CHANGED','The selected Company account, worker or reviewed placement changed. Reload this worker preparation.');
export const projectWorkerContextChanged=(consumer:WorkerStartupConsumer='projects')=>new ApiError(409,workerStartupSourcePolicy(consumer).errorPrefix+'CONTEXT_CHANGED','The selected Company account or worker preparation changed. Keep the draft and review it again.');
export function projectWorkerAuthorityImage(actor:WorkContext){return JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);}
async function authorityRetainsOwnedDirectoryEffect(database:SqlDatabase,before:WorkContext,after:WorkContext,consumer:WorkerStartupConsumer){
 const policy=workerStartupSourcePolicy(consumer),projectWorkerSourceRows=policy.sourceRows;
 const effect=projectWorkerOwnedSelfDirectory(database,before);if(!effect||before.iam?.personId!==null||after.iam?.personId!==effect.proof.personId||!effect.image)return false;
 // Compare every authority field unchanged except this proven own-result
 // Person projection. No modified actor is passed to an authorization call.
 const expected=JSON.stringify([before.holdingId,before.companyId,before.branchId,before.departmentId,before.userId,before.assignmentId,before.role,before.authority??null,{...before.iam,personId:effect.proof.personId}]);
 if(expected!==projectWorkerAuthorityImage(after)||effect.image!==await projectWorkerSelfDirectoryImage(database,before,effect.proof))return false;
 return !!await database.prepare(`SELECT 1 FROM (${projectWorkerSourceRows}) source JOIN command_receipts receipt ON receipt.command_id=source.command_id AND receipt.operation=? AND receipt.actor_id=? AND receipt.record_id=source.person_id JOIN ${policy.starterTable} native_source ON native_source.command_id=source.command_id AND native_source.person_id=source.person_id AND native_source.company_id=source.company_id AND native_source.account_id=source.account_id AND native_source.engagement_id=source.engagement_id AND native_source.request_hash=source.request_hash AND native_source.proof_json=source.proof WHERE source.command_id=? AND source.company_id=? AND source.account_id=? AND source.person_id=? AND source.engagement_id=? AND source.proof=?::jsonb`).bind(effect.operation,before.userId,effect.key,before.companyId,before.userId,effect.proof.personId,effect.proof.engagementId,JSON.stringify(effect.proof)).first();
}
/** Query-free, package first and receipt before any original receipt-first
 * disabled Directory/lifecycle writer. No source table is upgraded after a read. */
export async function projectWorkerSourceLocks(database:SqlDatabase,writer=false){
 if(writer)await lockPackageSettlementSources(database);else await database.batch([packageWriterFence(database)]);
 await database.prepare(`LOCK TABLE command_receipts IN ${writer?'SHARE ROW EXCLUSIVE':'SHARE'} MODE`).run();
 await database.prepare(`LOCK TABLE hr_people,hr_engagements,employees,people_lifecycle_cases,people_preparation_tasks,staff_notifications,audit_events,outbox_events,iam_audit IN ${writer?'SHARE ROW EXCLUSIVE':'SHARE'} MODE`).run();
 await database.batch(projectWorkerOrganizationSourceLocks(database,writer));
 await database.prepare('LOCK TABLE people_lifecycle_settings,people_access_packages,people_package_revisions,people_position_packages,people_lifecycle_grants,people_applied_packages,people_access_cutoffs,people_provision_operations,people_provision_grants,hr_compensation_revisions,hr_pay_groups,leave_policy_assignments,leave_policy_versions,leave_policy_rules,leave_types IN SHARE MODE').run();
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);
}
export async function projectWorkerSchemaAvailable(database:SqlDatabase,consumer:WorkerStartupConsumer='projects'){const policy=workerStartupSourcePolicy(consumer);return !!await database.prepare('SELECT to_regclass(?) IS NOT NULL AND to_regclass(?) IS NOT NULL ready').bind(policy.starterTable,policy.placementTable).first<boolean>('ready');}
export async function requireProjectWorkerAuthority(database:SqlDatabase,actor:WorkContext,consumer:WorkerStartupConsumer='projects'){
 requirePermission(actor,'people.manage');requirePermission(actor,'organization.manage');
 if(actor.beta||actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_HR_REQUIRED','Use a current Company HR and Organization assignment.');
 const current=await requireQueuedCommandAuthority(database,actor,'people.manage');requirePermission(current,'organization.manage');
 if(current.companyId!==actor.companyId||current.scope.kind!=='holding'||current.branchId||current.departmentId)throw new ApiError(403,'COMPANY_HR_REQUIRED','Use a current Company HR and Organization assignment.');
 await requireCompanyAuthority(database,current);
 for(const feature of workerStartupSourcePolicy(consumer).releases)await assertProductionModuleEnabled(database,actor.companyId,feature);
 return current;
}
type Unit={database:SqlDatabase;actor:WorkContext;guard:(check:()=>Promise<void>)=>void;schemaAvailable:boolean};
/** Closed commands register the actual original owner snapshots. Cleanup and
 * deferred effects finish before all final read-only source/authority checks. */
export async function withWorkerStartupUnit<Result>(consumer:WorkerStartupConsumer,actor:WorkContext,purpose:CapabilityPurpose|((database:SqlDatabase)=>Promise<CapabilityPurpose>),run:(unit:Unit)=>Promise<Result>,write=false){
 const policy=workerStartupSourcePolicy(consumer),projectWorkerCapabilities=policy.capabilities,owner=(await holdingResources(actor.holdingId)).write;
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Project worker preparation requires its owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  await projectWorkerSourceLocks(database,write);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  const current=await requireProjectWorkerAuthority(database,actor,consumer),schemaAvailable=await projectWorkerSchemaAvailable(database,consumer);
  if(write&&!schemaAvailable)throw new ApiError(503,policy.errorPrefix+'SCHEMA_REQUIRED','Apply the reviewed worker preparation schema before making changes.');
  const actualPurpose=typeof purpose==='function'?await purpose(database):purpose;
  for(const capability of projectWorkerCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:actualPurpose});
  const key=`project-worker:${randomUUID()}`,boundaries=write?projectWorkerCapabilities.map((capability,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${key}:${index}`,{companyId:actor.companyId,capability,purpose:actualPurpose})!})):[];
  if(boundaries.length)await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const release=registerProjectWorkerUnit(database,current,consumer),checks:(()=>Promise<void>)[]=[];
  try{
   const result=await run({database,actor:current,schemaAvailable,guard:check=>checks.push(check)});
   if(boundaries.length)await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));
   if(write)await flushFinanceDeferredEffects(database);
   const final=await requireProjectWorkerAuthority(database,current,consumer);if(projectWorkerAuthorityImage(current)!==projectWorkerAuthorityImage(final)&&!await authorityRetainsOwnedDirectoryEffect(database,current,final,consumer))throw projectWorkerContextChanged(consumer);
   for(const check of checks)await check();
   for(const capability of projectWorkerCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:actualPurpose});
   return result;
  }finally{release();}
 });}catch(error){if(String(error).includes(policy.errorPrefix+'SOURCE_CHANGED'))throw projectWorkerSourceChanged(consumer);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

/** Original Projects adapter contract remains bound to its original source. */
export function withProjectWorkerUnit<Result>(actor:WorkContext,purpose:CapabilityPurpose|((database:SqlDatabase)=>Promise<CapabilityPurpose>),run:(unit:Unit)=>Promise<Result>,write=false){return withWorkerStartupUnit('projects',actor,purpose,run,write);}
