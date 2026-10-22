import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {companyPurchaseReadLocks,checkPurchasedCapability,readPurchasedAccesses,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {hireHandoffPayGroupSourceLocks,hireHandoffPayGroupSourceImage} from '@erpx/server-talent/public/purchases';
import {peopleHireHandoffFacts} from '@erpx/server-talent/public/people-hire-handoff-facts';
import type {Source} from '@erpx/server-people_payroll/domain/people-handoffs';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled} from '@erpx/server-pms/public/hospitality-rollout';
import {financialConstraintError,replayCommand} from '@erpx/server-workflow/public/commands';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import type {HireHandoffPayGroupInput} from '@erpx/server-people_payroll/domain/hire-handoff-pay-group';
const capability='recruit.offers',operation='people.pay-group.create';
const authorityImage=(actor:WorkContext)=>JSON.stringify([actor.iam?.queuedAssignmentVersion,actor.iam?.identityVersion,actor.iam?.roleRevision,actor.iam?.hrAppointmentVersion]);
export const hireHandoffPayGroupEnvelope=(handoffId:string,input:HireHandoffPayGroupInput)=>({hireHandoffId:handoffId,...input});
function originalCompanyAuthority(actor:WorkContext){requirePermission(actor,'people.manage');requirePermission(actor,'payroll.prepare');if(actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_HR_SCOPE_REQUIRED','Use the original Company HR and payroll preparation assignment.');}
async function authority(database:SqlDatabase,actor:WorkContext){originalCompanyAuthority(actor);await requireQueuedCommandAuthority(database,actor,'people.manage');const current=await requireQueuedCommandAuthority(database,actor,'payroll.prepare');await assertProductionModuleEnabled(database,actor.companyId,'talent');await assertProductionModuleEnabled(database,actor.companyId,'people_payroll');return current;}
async function owner(actor:WorkContext){const database=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(database))throw new ApiError(503,'TRANSACTION_REQUIRED','Accepted hire preparation requires its owned Holding transaction.');return database;}
async function leases(database:SqlDatabase,companyId:string){const purchase=companyPurchaseReadLocks(database,companyId);await lockPackageSettlementSources(database);await database.batch([database.prepare('LOCK TABLE command_receipts,hr_command_receipts IN SHARE ROW EXCLUSIVE MODE'),...hireHandoffPayGroupSourceLocks(database)]);await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));}
function eligible(source:Source){return source.offer_status==='accepted'&&source.offer_version===source.source_offer_version&&source.candidate_status==='active'&&source.retention_until>=new Date().toISOString().slice(0,10)&&!!source.consent_evidence.trim()&&!!source.accepted_evidence?.trim();}
async function source(database:SqlDatabase,actor:WorkContext,id:string){const row=await peopleHireHandoffFacts(database).readHireSourceRow(id,actor.companyId),image=await hireHandoffPayGroupSourceImage(database,actor,id);if(!row||!image||row.offer_status!=='accepted')throw new ApiError(404,'HIRE_HANDOFF_NOT_FOUND','The original accepted hire handoff is unavailable.');return {row,image};}
const consumed=(database:SqlDatabase,actor:WorkContext,id:string)=>database.prepare('SELECT id FROM talent_hire_consumption_receipts WHERE handoff_id=? AND company_id=?').bind(id,actor.companyId).first();
const groups=(database:SqlDatabase,actor:WorkContext,currency:string)=>database.prepare("SELECT id FROM hr_pay_groups WHERE company_id=? AND currency=? AND status='active' ORDER BY id").bind(actor.companyId,currency).all<{id:string}>();
/** Minimal display decision, never a Payroll settings or directory grant. */
export async function canCreateHireHandoffPayGroup(actor:WorkContext,handoffId:string){
 if(actor.branchId||actor.departmentId||!hasPermission(actor,'people.manage')||!hasPermission(actor,'payroll.prepare'))return false;
 try{const database=await owner(actor);return await database.transaction(async tx=>{await leases(tx,actor.companyId);await authority(tx,actor);const access=(await readPurchasedAccesses(tx,'enforced',actor.companyId,[capability]))[capability];if(!access.settleExisting)return false;const before=await source(tx,actor,handoffId),allowed=eligible(before.row)&&!await consumed(tx,actor,handoffId)&&!(await groups(tx,actor,before.row.currency)).results.length;await authority(tx,actor);await checkPurchasedCapability(tx,'enforced',{companyId:actor.companyId,capability,purpose:'history'});if((await source(tx,actor,handoffId)).image!==before.image)throw new ApiError(409,'HIRE_PAY_GROUP_SOURCE_CHANGED','The accepted hire changed while preparing its pay group.');return allowed;});}catch(error){if(error instanceof ApiError&&[401,403,404].includes(error.status))return false;throw error;}
}
/** Same-owner composition only; callers cannot supply a database or authority facts. */
export async function withHireHandoffPayGroupCommand<Result extends {id:string}>(actor:WorkContext,handoffId:string,input:HireHandoffPayGroupInput,key:string,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 originalCompanyAuthority(actor);const database=await owner(actor);
 try{return await database.transaction(async tx=>{
  await leases(tx,actor.companyId);const authorized=await authority(tx,actor),before=await source(tx,actor,handoffId),receipt=await tx.prepare('SELECT record_id FROM command_receipts WHERE command_id=?').bind(key).first<{record_id:string}>(),purchase={companyId:actor.companyId,capability,purpose:receipt?'history' as const:'settle-existing' as const},fence=`hire-pay-group:${randomUUID()}`,boundary=purchaseCommandBoundary(tx,'enforced',fence,purchase)!;
  await checkPurchasedCapability(tx,'enforced',purchase);await tx.batch(boundary.before);let result:Result;
  if(receipt){const replay=await replayCommand<Result>(tx,actor,key,operation,receipt.record_id,hireHandoffPayGroupEnvelope(handoffId,input));if(!replay)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','Review the original pay-group request.');result=replay;}
  else{
   if(!eligible(before.row)||before.row.offer_version!==input.expectedOfferVersion||before.row.candidate_version!==input.expectedCandidateVersion)throw new ApiError(409,'HIRE_HANDOFF_SOURCE_CHANGED','Review the current accepted offer and candidate before pay-group preparation.');
   if(await consumed(tx,actor,handoffId))throw new ApiError(409,'HIRE_HANDOFF_ALREADY_CONSUMED','This accepted hire is already consumed.');
   if(input.input.currency!==before.row.currency)throw new ApiError(422,'HIRE_PAY_GROUP_INVALID','The pay group must use the actual accepted offer currency.');
   if((await groups(tx,actor,before.row.currency)).results.length)throw new ApiError(409,'HIRE_PAY_GROUP_ALREADY_AVAILABLE','Choose the existing active same-currency Company pay group.');
   result=await run(tx);
  }
  const observed=await tx.batch([...boundary.after,tx.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(fence)]);boundary.observe?.(observed);
  await flushFinanceDeferredEffects(tx);const final=await authority(tx,actor);if(authorityImage(authorized)!==authorityImage(final))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original HR preparation authority changed.');if((await source(tx,actor,handoffId)).image!==before.image)throw new ApiError(409,'HIRE_PAY_GROUP_SOURCE_CHANGED','The accepted hire changed before pay-group creation committed.');
  const actual=await tx.prepare('SELECT to_jsonb(p)::text image FROM hr_pay_groups p WHERE id=? AND company_id=?').bind(result.id,actor.companyId).first<string>('image');if(!actual)throw new ApiError(409,'HIRE_PAY_GROUP_SOURCE_CHANGED','The original Company pay group is unavailable.');
  if(!receipt){const row=JSON.parse(actual) as Record<string,unknown>,expected=input.input;if(row.name!==expected.name||row.country_code!==expected.countryCode||row.currency!==expected.currency||row.frequency!==expected.frequency||row.timezone!==expected.timezone||Number(row.overtime_multiplier_bps)!==expected.overtimeMultiplierBps||row.proration_method!==expected.prorationMethod||row.status!=='active'||row.created_by!==actor.userId||(await groups(tx,actor,before.row.currency)).results.some(group=>group.id!==result.id))throw new ApiError(409,'HIRE_PAY_GROUP_SOURCE_CHANGED','The actual created Company pay group changed before commit.');}
  await checkPurchasedCapability(tx,'enforced',purchase);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
