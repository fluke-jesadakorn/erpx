import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {peoplePackageEnforcementMode,readPurchasedAccess} from '../../commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '../../commercial/public/purchased-capability-sql';
import {requireExpensePurchasedHistory,expenseCurrentAuthorityImage} from './expense-purchases';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {companyPurchaseReadLocks,checkPurchasedCapability} from '../../commercial/public/purchase-command-boundary';
import {isTransactionalSqlDatabase} from '../../runtime/public/runtime/types';
import {holdingResources} from '../../runtime/public/runtime';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {ApiError} from '../../runtime/public/http';
import {expenseAuthoritySnapshot} from './expense-routing';
import {hospitalityOwnerProjectionSources as workflowSources} from '../../workflow/public/hospitality-owner-projection-sources';
/** Closed source IDs resolve only actual claims or their own payment drafts.
 * No physical HR payment table or generic People label supplies this source. */
function actual(id:string,company:string){return `SELECT claim.* FROM finance_expenses claim WHERE claim.company_id=${company} AND (claim.id=${id} OR EXISTS(SELECT 1 FROM expense_payment_drafts payment WHERE payment.id=${id} AND payment.expense_id=claim.id AND payment.company_id=claim.company_id))`;}
export async function expenseSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:'records'|'staff'|'inbox'){
 const mode=await peoplePackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'people.expenses'});return {where:'TRUE',args:[] as unknown[]};}
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id';
 const unit=surface==='records'?recordReadUnits.get(database):undefined,recordScope=surface==='records'?{where:unit?.actor===recordActor(actor)&&unit.companyId===actor.companyId?'workspace_records.id IN(SELECT value FROM erpx_json_each(?))':'FALSE',args:unit?.actor===recordActor(actor)&&unit.companyId===actor.companyId?[JSON.stringify(unit.ids)]:[]}: {where:'TRUE',args:[]};
 const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),purchase=purchasedCapabilityPredicate('expense_source.company_id','people.expenses','history','expense_source_clock.now_ms');
 return {where:`(NOT EXISTS(${actual(id,company)}) OR EXISTS(WITH expense_source_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM (${actual(id,company)}) expense_source CROSS JOIN expense_source_clock CROSS JOIN LATERAL(SELECT expense_source.company_id,expense_source.branch_id,expense_source.department_id) workforce_actor_scope WHERE ${purchase} AND ${current.where} AND ${recordScope.where}))`,args:[...current.args,...recordScope.args]};
}
export async function requireExpenseWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,_moduleId:string,id:string){
 if(await peoplePackageEnforcementMode()==='disabled')return;
 // Keep the input IDs bound, not interpolated into the source proof.
 const row=await database.prepare(`SELECT claim.id FROM (SELECT ?::text id,?::text company_id) expense_input CROSS JOIN LATERAL (${actual('expense_input.id','expense_input.company_id')}) claim`).bind(id,actor.companyId).first<string>('id');
 if(!row)return;
 await requireExpensePurchasedHistory(database,actor);
 const {expenseFromDatabase}=await import('./expenses');await expenseFromDatabase(database,actor,row);
}

/** Code-owned Company input, clock alias and capability; no SQL identifiers from callers. */
export async function expenseCompanyHistoryScope(database:SqlDatabase,companyId:string,alias:"finance_expenses"|"e"="finance_expenses"){
 const mode=await peoplePackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId,capability:"people.expenses"});return "TRUE";}
 return `EXISTS(WITH expense_company_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM expense_company_clock WHERE ${purchasedCapabilityPredicate(`${alias}.company_id`,"people.expenses","history","expense_company_clock.now_ms")})`;
}

/** The native current-reviewer policy is asynchronous. Its permitted IDs are
 * usable ONLY while this same owner unit still holds all native source leases;
 * the callback executes the before-limit SQL before those leases are released. */
type RecordReadUnit={actor:string;companyId:string;ids:readonly string[]};
const recordReadUnits=new WeakMap<SqlDatabase,RecordReadUnit>();
const recordActor=(actor:WorkContext)=>JSON.stringify(actor);
const recordCandidatesSql=`SELECT workspace_source.id record_id,claim.id expense_id,jsonb_build_object(
 'record',to_jsonb(workspace_source),'claim',to_jsonb(claim),
 'payment',(SELECT to_jsonb(payment) FROM expense_payment_drafts payment WHERE payment.id=workspace_source.id AND payment.company_id=claim.company_id AND payment.expense_id=claim.id),
 'approvals',(SELECT COALESCE(jsonb_agg(to_jsonb(instance) ORDER BY instance.id),'[]') FROM (${workflowSources.approval_instances}) instance WHERE instance.record_type='expense' AND instance.record_id=claim.id AND instance.company_id=claim.company_id),
 'steps',(SELECT COALESCE(jsonb_agg(to_jsonb(step) ORDER BY step.id),'[]') FROM (${workflowSources.approval_steps}) step JOIN (${workflowSources.approval_instances}) instance ON instance.id=step.instance_id WHERE instance.record_type='expense' AND instance.record_id=claim.id AND instance.company_id=claim.company_id),
 'decisions',(SELECT COALESCE(jsonb_agg(to_jsonb(decision) ORDER BY decision.id),'[]') FROM (${workflowSources.approval_decisions}) decision JOIN (${workflowSources.approval_instances}) instance ON instance.id=decision.instance_id WHERE instance.record_type='expense' AND instance.record_id=claim.id AND instance.company_id=claim.company_id)
 )::text image FROM workspace_records workspace_source CROSS JOIN LATERAL(${actual('workspace_source.id','workspace_source.company_id')}) claim WHERE workspace_source.company_id=? AND workspace_source.archived_at IS NULL ORDER BY workspace_source.module_id,workspace_source.id,claim.id`;
type RecordCandidate={record_id:string;expense_id:string;image:string};
export async function withExpenseWorkspaceRecordRead<Result>(database:SqlDatabase,actor:WorkContext|undefined,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 const mode=await peoplePackageEnforcementMode();if(!actor||mode!=='enforced')return run(database);
 const inherited=recordReadUnits.get(database);if(inherited){if(inherited.actor!==recordActor(actor))throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','The record read belongs to its original Expense actor.');return run(database);}
 // This is query-shape preflight, never authority. If there is no current
 // Expense record the ordinary path stays unchanged; the SQL scope still
 // denies a newly appearing actual Expense source without an owned witness.
 if(!await database.prepare(`SELECT 1 FROM workspace_records workspace_source WHERE workspace_source.company_id=? AND workspace_source.archived_at IS NULL AND EXISTS(${actual('workspace_source.id','workspace_source.company_id')}) LIMIT 1`).bind(actor.companyId).first())return run(database);
 const owner=(await holdingResources(actor.holdingId)).write;
 if(!isTransactionalSqlDatabase(owner)||database!==owner)throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','Expense record visibility requires its complete Holding read transaction.');
 return owner.transaction(async transaction=>{
  await lockPackageSettlementSources(transaction);
  // Original disabled writers are receipt-first. Take their receipt fence
  // before native records/approval sources, with no SHARE-to-writer upgrade.
  await transaction.prepare('LOCK TABLE command_receipts,workspace_records,finance_expenses,expense_payment_drafts,approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE').run();
  await lockQueuedCommandAuthority(transaction);
  await transaction.batch(companyPurchaseReadLocks(transaction,actor.companyId).slice(1));
  const actorImage=recordActor(actor),readCandidates=()=>transaction.prepare(recordCandidatesSql).bind(actor.companyId).all<RecordCandidate>();
  const access=await readPurchasedAccess(transaction,mode,{companyId:actor.companyId,capability:'people.expenses'});
  const authorityImage=access.history?await expenseCurrentAuthorityImage(transaction,actor):null,routingAuthority=access.history?await expenseAuthoritySnapshot(transaction,actor.companyId):null;
  const {expenseVisibleRecordIds}=await import('./expenses'),initial=(await readCandidates()).results,expenseIds=[...new Set(initial.map(row=>row.expense_id))];
  const visible=access.history?await expenseVisibleRecordIds(transaction,actor,expenseIds):new Set<string>();
  const permitted=initial.filter(row=>visible.has(row.expense_id)).map(row=>row.record_id),sourceImage=JSON.stringify((await readCandidates()).results);
  recordReadUnits.set(transaction,{actor:actorImage,companyId:actor.companyId,ids:permitted});
  try{
   const result=await run(transaction);
   await flushFinanceDeferredEffects(transaction);
   if(recordActor(actor)!==actorImage||authorityImage!==null&&await expenseCurrentAuthorityImage(transaction,actor)!==authorityImage)throw new ApiError(403,'EXPENSE_ACCESS_CHANGED','The original Expense record reader changed.');
   if(routingAuthority&&!await transaction.prepare(`SELECT ${routingAuthority.query} unchanged`).bind(...routingAuthority.args).first<boolean>('unchanged'))throw new ApiError(403,'EXPENSE_ACCESS_CHANGED','The native Expense routing authority changed.');
   if(JSON.stringify((await readCandidates()).results)!==sourceImage)throw new ApiError(409,'EXPENSE_RECORD_SOURCE_CHANGED','The native Expense record source changed before the read completed.');
   const currentVisible=access.history?await expenseVisibleRecordIds(transaction,actor,expenseIds,true):new Set<string>();
   if(JSON.stringify([...currentVisible].sort())!==JSON.stringify([...visible].sort()))throw new ApiError(409,'EXPENSE_RECORD_SOURCE_CHANGED','The original Expense visibility changed before the read completed.');
   if(access.history)await checkPurchasedCapability(transaction,mode,{companyId:actor.companyId,capability:'people.expenses',purpose:'history'});
   return result;
  }finally{recordReadUnits.delete(transaction);}
 });
}
