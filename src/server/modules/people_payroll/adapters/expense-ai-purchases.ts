import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import 'server-only';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import type {StaffActor} from '../../operations/public/staff-policy';
import {peoplePackageEnforcementMode,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import {ApiError} from '../../runtime/public/http';
import {requireJobAuthority} from '../../identity_access/public/iam-jobs';
import {withExpensePurchasedCommand,expenseFinalRead} from './expense-purchases';
const source=`SELECT jsonb_build_object('id',id,'companyId',company_id,'expenseId',expense_id,'paymentId',payment_draft_id,'actor',actor_json,'evidence',evidence_digest,'provider',provider_json,'financePolicy',finance_policy_version,'stage',stage)::text image,actor_json,company_id FROM expense_ai_proposals WHERE id=?`;
/** The actual queued author and immutable native job source choose this unit;
 * no caller Company label or generated system actor can acquire it. Provider
 * I/O remains outside these short claim/renew/completion transactions. */
export async function withExpenseAiPurchasedMutation<Result>(owner:SqlDatabase,id:string,run:(database:SqlDatabase)=>Promise<Result>){
 if(await peoplePackageEnforcementMode()==='disabled')return run(owner);
 const selected=await owner.prepare(source).bind(id).first<{image:string;actor_json:string|null;company_id:string}>();
 if(!selected?.actor_json)throw new ApiError(403,'EXPENSE_ACCESS_CHANGED','The original Expense job has no current queued author.');
 const actor:StaffActor=JSON.parse(selected.actor_json);requireJobAuthority(actor);if(actor.companyId!==selected.company_id)throw new ApiError(403,"EXPENSE_ACCESS_CHANGED","The queued author belongs to a different Company.");
 return withExpensePurchasedCommand(owner,actor,{key:`expense-ai-job:${id}`,replay:false,purpose:'settle-existing'},async database=>{
  if((await database.prepare(source).bind(id).first<string>('image'))!==selected.image)throw new ApiError(409,'AI_STALE','The original Expense job source changed.');
  const result=await run(database);
  expenseFinalRead(database,actor,async()=>{if(await database.prepare(source).bind(id).first<string>('image')!==selected.image)throw new ApiError(409,'AI_STALE','The original Expense job source changed before commit.');});
  return result;
 });
}
export function expenseJobUnavailable(error:unknown){return error instanceof ApiError&&error.status===403&&['DEPENDENCY_REQUIRED','PACKAGE_REQUIRED','PACKAGE_INACTIVE','PACKAGE_EXPIRED','PACKAGE_DRAINING','EXPENSE_ACCESS_CHANGED','QUEUED_AUTHORITY_CHANGED'].includes(error.code);}
/** Claiming is original durable lease bookkeeping, not a provider/business
 * authorization. A revoked queued author must still reach the original worker
 * refusal and stale/failure cleanup instead of stranding reserved usage. */
export async function withExpenseAiPurchasedClaim<Result>(owner:SqlDatabase,id:string,run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','Expense AI claims require their original Holding transaction.');
 try{return await owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  const before=await database.prepare(source).bind(id).first<{image:string;actor_json:string|null;company_id:string}>();
  if(!before)return run(database);
  await database.batch(companyPurchaseReadLocks(database,before.company_id).slice(1));
  const scope={companyId:before.company_id,capability:'people.expenses',purpose:'settle-existing' as const};await checkPurchasedCapability(database,mode,scope);
  const result=await run(database);await flushFinanceDeferredEffects(database);
  if(await database.prepare(source).bind(id).first<string>('image')!==before.image)throw new ApiError(409,'AI_STALE','The original Expense job binding changed while claiming.');
  await checkPurchasedCapability(database,mode,scope);return result;
 });}catch(error){throw purchaseCommandFailure(error)??error;}
}
