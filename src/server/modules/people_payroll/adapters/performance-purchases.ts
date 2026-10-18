import 'server-only';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {financialConstraintError} from '../../workflow/public/commands';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';
import {captureJobAuthority} from '../../identity_access/public/iam-jobs';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {holdingResources} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {peoplePackageEnforcementMode,companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure,readPurchasedAccess} from '../../commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '../../commercial/public/rules/packages';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {requirePeopleCurrentAuthority} from './people-purchases';
import {hasPermission} from '@/lib/identity_access/permissions';
import {peopleEvaluationTaskSourceLocks,peopleEvaluationTaskSourceImage} from '../../operations/public/people-evaluation-work-facts';
import {peopleEvaluationProjectSourceLocks,peopleEvaluationProjectSourceImage} from '../../projects/public/people-evaluation-facts';
import {peopleEvaluationLearningSourceLocks,peopleEvaluationLearningSourceImage} from '../../learning/public/people-evaluation-facts';

const tables=['hr_evaluation_definitions','hr_evaluation_definition_history','hr_evaluation_assessments','hr_evaluation_assessment_history','hr_evaluation_evidence','hr_evaluation_pay_basis','hr_evaluation_compensation_drafts','hr_evaluation_ai_jobs','hr_compensation_revisions','hr_people','hr_engagements','hr_pay_groups','hr_payroll_runs','hr_payroll_items'];
/** Evaluation/compensation source writer leases establish no data snapshot. */
export function performanceSourceReadLocks(database:SqlDatabase){return [database.prepare(`LOCK TABLE ${tables.join(',')} IN SHARE ROW EXCLUSIVE MODE`)];}
export function performanceSourceImage(database:SqlDatabase,companyId:string){return database.prepare(`SELECT jsonb_build_object(${tables.map(table=>`'${table}',(SELECT jsonb_agg(to_jsonb(row) ORDER BY to_jsonb(row)::text) FROM ${table} row ${table==='hr_compensation_revisions'?'JOIN hr_engagements placement ON placement.id=row.engagement_id WHERE placement.company_id=?':table==='hr_payroll_items'?'JOIN hr_payroll_runs run ON run.id=row.run_id WHERE run.company_id=?':'WHERE row.company_id=?'})`).join(',')})::text image`).bind(...tables.map(()=>companyId)).first<string>('image');}
async function referenceSourceLeases(database:SqlDatabase){await peopleEvaluationTaskSourceLocks(database);await database.batch([...peopleEvaluationProjectSourceLocks(database),...peopleEvaluationLearningSourceLocks(database)]);}
async function referenceSourceImage(database:SqlDatabase,actor:WorkContext){const tasks=await peopleEvaluationTaskSourceImage(database,actor),projects=await peopleEvaluationProjectSourceImage(database,actor),learning=await peopleEvaluationLearningSourceImage(database,actor);return JSON.stringify({tasks,projects,learning});}
async function completeSourceImage(database:SqlDatabase,actor:WorkContext){return JSON.stringify({native:await performanceSourceImage(database,actor.companyId),references:await referenceSourceImage(database,actor)});}
export async function requirePerformanceHistory(database:SqlDatabase,actor:WorkContext){const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return;await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'people.performance',purpose:'history'});await requirePeopleCurrentAuthority(database,actor,`performance-read:${randomUUID()}`);}
export async function performancePurchaseControls(actor:WorkContext,database?:SqlDatabase){const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return undefined;const access=await readPurchasedAccess(database??(await holdingResources(actor.holdingId)).write,mode,{companyId:actor.companyId,capability:'people.performance'});return {newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision};}
export async function withPerformancePurchasedRead<Result>(actor:WorkContext,run:(database?:SqlDatabase)=>Promise<Result>,provided?:SqlDatabase){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return run(provided);const owner=provided??(await holdingResources(actor.holdingId)).write;
 if(!isTransactionalSqlDatabase(owner)){await requirePerformanceHistory(owner,actor);return run(owner);}
 return owner.transaction(async database=>{await lockPackageSettlementSources(database);await referenceSourceLeases(database);await database.batch(performanceSourceReadLocks(database));await lockQueuedCommandAuthority(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));await requirePerformanceHistory(database,actor);const witness=await completeSourceImage(database,actor),result=await run(database);await flushFinanceDeferredEffects(database);await requirePerformanceHistory(database,actor);if(await completeSourceImage(database,actor)!==witness)throw new ApiError(409,'PERFORMANCE_SOURCE_CHANGED','The Evaluation source changed during its read.');return result;});
}
/** Complete normal Evaluation commands retain their actual input/receipt,
 * original independent review and compensation money effects in this unit. */
export async function withPerformancePurchasedCommand<Result>(actor:WorkContext,command:{key:string;operation:string;purpose:CapabilityPurpose|((database:SqlDatabase)=>Promise<CapabilityPurpose>)},run:(database:SqlDatabase|undefined,current:WorkContext)=>Promise<Result>){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return run(undefined,actor);const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'PERFORMANCE_TRANSACTION_REQUIRED','Evaluation requires its original Holding transaction.');
 try{return await owner.transaction(async database=>{
  const locks=companyPurchaseReadLocks(database,actor.companyId);await lockPackageSettlementSources(database);await referenceSourceLeases(database);await database.batch(performanceSourceReadLocks(database));await lockQueuedCommandAuthority(database);await database.batch(locks.slice(1));
  const current=await captureJobAuthority(database,actor),context=JSON.stringify(current),prior=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first(),purpose=prior?'history' as const:typeof command.purpose==='function'?await command.purpose(database):command.purpose,scope={companyId:actor.companyId,capability:'people.performance',purpose},key=`performance-purchase:${randomUUID()}`;
  await checkPurchasedCapability(database,mode,scope);await requirePeopleCurrentAuthority(database,current,key);const boundary=purchaseCommandBoundary(database,mode,key,scope)!;await database.batch(boundary.before);const reference=await referenceSourceImage(database,current),result=await run(database,current),witness=await performanceSourceImage(database,current.companyId);
  await flushFinanceDeferredEffects(database);if(JSON.stringify(current)!==context)throw new ApiError(409,'PERFORMANCE_SOURCE_CHANGED','The selected Evaluation context changed before completion.');await requirePeopleCurrentAuthority(database,current,key);
  // Retained cycle closure must still describe the same stored frozen cycle
  // after all native effects, including an immediate new unfinished review.
  if(!prior&&purpose==='settle-existing'&&typeof command.purpose==='function'&&await command.purpose(database)!==purpose)throw new ApiError(409,'PERFORMANCE_SOURCE_CHANGED','The retained Evaluation closure changed before completion.');
  if(await performanceSourceImage(database,current.companyId)!==witness||await referenceSourceImage(database,current)!==reference)throw new ApiError(409,'PERFORMANCE_SOURCE_CHANGED','The original Evaluation or compensation source changed before completion.');const observed=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);boundary.observe?.(observed);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

/** Closing or reaffirming a stored unchanged cycle is retained settlement.
 * Unfinished assessments stay blocked, including already-closed legacy cycles;
 * this does not reopen them or waive the original frozen-cycle rule. */
export async function performanceDefinitionPurpose(database:SqlDatabase,actor:WorkContext,input:unknown):Promise<CapabilityPurpose>{
 if(!hasPermission(actor,'organization.manage')||!hasPermission(actor,'people.manage'))return 'new-work';
 if(!input||typeof input!=='object'||!('id'in input)||typeof input.id!=='string'||!('kind'in input)||input.kind!=='cycle'||!('data'in input)||!input.data||typeof input.data!=='object')return 'new-work';
 const data=input.data;if(!('status'in data)||data.status!=='closed')return 'new-work';
 const row=await database.prepare("SELECT payload_json,EXISTS(SELECT 1 FROM hr_evaluation_assessments a WHERE a.cycle_id=cycle.id AND a.company_id=cycle.company_id AND a.status<>'confirmed') unfinished FROM hr_evaluation_definitions cycle WHERE cycle.id=? AND cycle.company_id=? AND cycle.kind='cycle'").bind(input.id,actor.companyId).first<{payload_json:string;unfinished:boolean}>();if(!row)return 'new-work';
 const prior:unknown=JSON.parse(row.payload_json);if(!prior||typeof prior!=='object'||!('status'in prior))return 'new-work';
 const stable=(value:object)=>JSON.stringify(Object.entries(value).filter(([key])=>key!=='status').sort(([a],[b])=>a.localeCompare(b)));
 if(stable(prior)!==stable(data))return 'new-work';
 if(row.unfinished)throw new ApiError(409,'PERFORMANCE_CYCLE_OBLIGATIONS_REMAIN','Finish the original assessment reviews before closing their annual cycle.');
 return 'settle-existing';
}

/** Existing queued work needs settlement authority before provider access and
 * again in its original final native effects; history alone starts no job. */
export async function performanceJobEffects(database:SqlDatabase,actor:WorkContext){const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return [];const scope={companyId:actor.companyId,capability:'people.performance',purpose:'settle-existing' as const};await checkPurchasedCapability(database,mode,scope);await requirePeopleCurrentAuthority(database,actor,`performance-job-current:${randomUUID()}`);const key=`performance-job-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;return [...boundary.before,...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)];}
export async function performanceExecutionSourceImage(database:SqlDatabase,actor:WorkContext){return await peoplePackageEnforcementMode()==='disabled'?undefined:completeSourceImage(database,actor);}

/** No SQL transaction spans the provider call. Its final result owns a new
 * source-first unit and compares the image taken BEFORE the latest AI source
 * was read. Statements are built inside this exact unit, never copied from a
 * pool or accepted from an arbitrary caller transaction. */
export async function withPerformanceAiResult(actor:WorkContext,owner:SqlDatabase,expected:string|null|undefined,run:(database:SqlDatabase)=>Promise<void>,after:(database:SqlDatabase)=>Promise<void>){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled'){await run(owner);return;}
 if(!isTransactionalSqlDatabase(owner)||!expected)throw new ApiError(503,'PERFORMANCE_TRANSACTION_REQUIRED','Evaluation AI completion requires its source Holding transaction.');
 try{await owner.transaction(async database=>{
  await lockPackageSettlementSources(database);await referenceSourceLeases(database);await database.batch(performanceSourceReadLocks(database));await lockQueuedCommandAuthority(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  const context=JSON.stringify(actor),scope={companyId:actor.companyId,capability:'people.performance',purpose:'settle-existing' as const};await checkPurchasedCapability(database,mode,scope);await requirePeopleCurrentAuthority(database,actor,`performance-ai:${randomUUID()}`);
  if(await completeSourceImage(database,actor)!==expected)throw new ApiError(409,'PERFORMANCE_SOURCE_CHANGED','Evaluation evidence changed before the AI result could be recorded.');
  const reference=await referenceSourceImage(database,actor);await run(database);const witness=await performanceSourceImage(database,actor.companyId);await flushFinanceDeferredEffects(database);
  if(JSON.stringify(actor)!==context||await performanceSourceImage(database,actor.companyId)!==witness||await referenceSourceImage(database,actor)!==reference)throw new ApiError(409,'PERFORMANCE_SOURCE_CHANGED','Evaluation or Payroll source changed while completing the AI result.');
  await requirePeopleCurrentAuthority(database,actor,`performance-ai-final:${randomUUID()}`);await checkPurchasedCapability(database,mode,scope);await after(database);
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
