import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import type {ExpensePurchaseControls} from '@erpx/feature-people_payroll/lib/expense-purchases';
export type {ExpensePurchaseControls} from '@erpx/feature-people_payroll/lib/expense-purchases';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {captureJobAuthority} from '@erpx/server-identity_access/public/iam-jobs';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {financialConstraintError} from '@erpx/server-workflow/public/commands';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {peoplePackageEnforcementMode,companyPurchaseReadLocks,checkPurchasedCapability,readPurchasedAccess,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';

type Actor=StaffActor & Partial<Pick<WorkContext,'holdingId'>>;
type Command={key:string;replay?:boolean;requireUnit?:true;purpose:CapabilityPurpose|((database:SqlDatabase)=>Promise<CapabilityPurpose>)};
type Unit={actor:string;companyId:string;key:string;read:boolean;replay:boolean;purposes:Set<CapabilityPurpose>;capturedActors:Set<string>;finalGuards:(()=>Promise<void>)[]};
const units=new WeakMap<SqlDatabase,Unit>();
const identity=(actor:StaffActor)=>JSON.stringify(actor);
/** Current native assignment, principal, selected session and IAM are checked
 * without inventing a role or requiring full People. The image is private. */
async function authority(database:SqlDatabase,actor:StaffActor){
 const scope=currentAssignmentReadScope(actor,'workforce_actor_scope');
 const row=await database.prepare(`SELECT jsonb_build_object('assignment',to_jsonb(a),'role',to_jsonb(r),'member',to_jsonb(m),'context',to_jsonb(c))::text image FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope JOIN assignments a ON a.id=? AND a.company_id=workforce_actor_scope.company_id JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) LEFT JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id LEFT JOIN active_work_contexts c ON c.company_id=a.company_id AND c.user_id=a.user_id AND c.session_id=? WHERE ${scope.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,actor.assignmentId,actor.authority?.sessionId??null,...scope.args).first<string>('image');
 if(!row)throw new ApiError(403,'EXPENSE_ACCESS_CHANGED','The original Expense assignment or session is no longer current.');
 return row;
}
/** Only an actual owner-created unit can compose same-database native ports.
 * This is not an authorization shortcut; original owner policies still run. */
export function expenseOwnedUnit(database:SqlDatabase,actor:StaffActor){const unit=units.get(database);return !!unit&&unit.companyId===actor.companyId&&(unit.actor===identity(actor)||unit.capturedActors.has(identity(actor)));}
const newOperations=new Set(['expense.create','expense.update','expense.submit','expense.ai.prepare']);
const settlementOperations=new Set(['expense.approve','expense.reject','expense.cancel','expense.accounting.save','expense.post','expense.reverse','expense.payment.prepare','expense.payment.save','expense.payment.confirm','expense.payment.cancel','expense.receipt.remove','expense.payment.slip.remove']);
export async function requireExpensePortUnit(database:SqlDatabase,actor:StaffActor,key:string,operation:string){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return;
 const unit=units.get(database);
 if(!unit||!expenseOwnedUnit(database,actor)||unit.read||unit.key!==key)throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','Expense preparation requires its owned Holding command transaction.');
 if(!newOperations.has(operation)&&!settlementOperations.has(operation))throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','This operation is not a declared Expense command.');
 const purpose=unit.replay?'history':newOperations.has(operation)?'new-work':'settle-existing';
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'people.expenses',purpose});unit.purposes.add(purpose);
}

async function owned<Result>(database:SqlDatabase,actor:Actor,command:Command,run:(transaction:SqlDatabase)=>Promise<Result>,read=false,suppliedOwner?:SqlDatabase){
 const mode=await peoplePackageEnforcementMode();
 if(mode==='disabled'&&!command.requireUnit)return run(database);
 const nested=units.get(database);
 if(nested){if(!expenseOwnedUnit(database,actor))throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','The Expense unit belongs to a different current actor.');if(!read&&(nested.read||nested.key!==command.key))throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','A new Expense command cannot borrow a read or different command unit.');if(!read){const purpose=nested.replay?'history':typeof command.purpose==='function'?await command.purpose(database):command.purpose;await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'people.expenses',purpose});nested.purposes.add(purpose);}return run(database);}
 const owner=suppliedOwner??(actor.holdingId?(await holdingResources(actor.holdingId)).write:database);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','Expense changes require their owned Holding PostgreSQL transaction.');
 const execute=async(transaction:SqlDatabase,parentRead=false)=>{
  const purchaseLocks=companyPurchaseReadLocks(transaction,actor.companyId);
  // The complete short unit uses the existing settlement fence BEFORE IAM/Org
  // leases, excluding enabled advisory-first writers before either can hold
  // reciprocal source locks. No provider/File byte I/O occurs in this unit.
  // No source SHARE lease is upgraded to an Expense or money writer.
  if(!parentRead){await lockPackageSettlementSources(transaction);await lockQueuedCommandAuthority(transaction);await transaction.batch(purchaseLocks.slice(1));}
  // An original source-leased parent (private Files/Legal) supplies its active
  // transaction for a lookup. Do not acquire a late package/table upgrade or
  // flush an unfinished parent. Its complete source/byte finality remains in
  // that original parent; this lookup still checks current Expense history.
  // The original replay function still verifies full request/actor hash. A
  // colliding key cannot authorize preparation; it fails before owner effects.
  const prior=read||command.replay===false?null:await transaction.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,actor.userId,actor.companyId).first();
  const purchase={companyId:actor.companyId,capability:'people.expenses',purpose:prior?'history' as const:typeof command.purpose==='function'?await command.purpose(transaction):command.purpose};
  await checkPurchasedCapability(transaction,mode,purchase);
  const before=await authority(transaction,actor),actorImage=identity(actor),key=`expense-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(transaction,mode,key,purchase)!;
  if(!read)await transaction.batch(boundary?.before??[]);const unit:Unit={actor:actorImage,companyId:actor.companyId,key:command.key,read,replay:!!prior,purposes:new Set([purchase.purpose]),capturedActors:new Set(),finalGuards:[]};units.set(transaction,unit);
  try{
   const result=await run(transaction);
   // Run assertion/observation cleanup BEFORE flushing. Deferred owner work
   // cannot become an unverified baseline after the last read-only fence.
   if(!read){const observed=await transaction.batch([...(boundary?.after??[]),transaction.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);boundary?.observe?.(observed);}
   if(!parentRead)await flushFinanceDeferredEffects(transaction);
   if(identity(actor)!==actorImage||await authority(transaction,actor)!==before)throw new ApiError(403,'EXPENSE_ACCESS_CHANGED','The original Expense authority changed before commit.');
   for(const finalGuard of unit.finalGuards)await finalGuard();
   for(const purpose of unit.purposes)await checkPurchasedCapability(transaction,mode,{...purchase,purpose});
   return result;
  }finally{units.delete(transaction);}
 };
 try{
  if(database!==owner){const owned=owner as SqlDatabase & {assertActiveTransaction?:(database:SqlDatabase)=>void};if(!owned.assertActiveTransaction)throw new ApiError(503,'TRANSACTION_OWNERSHIP_REQUIRED','Expense requires its actual Holding owner.');owned.assertActiveTransaction(database);return await execute(database,read);}
  return await owner.transaction(execute);
 }catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export function withExpensePurchasedCommand<Result>(database:SqlDatabase,actor:Actor,command:Command,run:(transaction:SqlDatabase)=>Promise<Result>){return owned(database,actor,command,run);}
export function withExpensePurchasedRead<Result>(database:SqlDatabase,actor:Actor,run:(transaction:SqlDatabase)=>Promise<Result>){return owned(database,actor,{key:`expense-read:${randomUUID()}`,purpose:'history'},run,true);}
export async function expensePurchaseControls(actor:WorkContext,provided?:SqlDatabase):Promise<ExpensePurchaseControls|undefined>{
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return undefined;
 const database=provided??(await holdingResources(actor.holdingId)).write;
 return withExpensePurchasedRead(database,actor,async transaction=>{const {newWork,settleExisting,history,revision}=await readPurchasedAccess(transaction,mode,{companyId:actor.companyId,capability:'people.expenses'});return {newWork,settleExisting,history,revision,contextKey:createHash("sha256").update(identity(actor)).digest("hex")};});
}
/** Shared owners retain their existing role/visibility policy and only ask
 * this gate about the actual Company's Expense capability. */
export async function requireExpensePurchasedHistory(database:SqlDatabase,actor:StaffActor){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return;
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'people.expenses',purpose:'history'});await authority(database,actor);
}
export async function expenseDocumentFlow(database:SqlDatabase,actor:StaffActor,view:import("@erpx/feature-shared/lib/flow").FlowView){
 await requireExpensePurchasedHistory(database,actor);const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return view;
 const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'people.expenses'});const purpose=view.state==='draft'||view.state==='rejected'?'newWork':'settleExisting';return access[purpose]?view:{...view,canAct:false,canManage:false};
}
export async function withExpenseFlowAssignment<Result>(database:SqlDatabase,actor:Actor,id:string,command:{key:string;stepKey:string;saveDefault:boolean},run:(transaction:SqlDatabase)=>Promise<Result>){
 return withExpensePurchasedCommand(database,actor,{key:command.key,purpose:async transaction=>{
  const row=await transaction.prepare('SELECT status,workflow_stage FROM finance_expenses WHERE company_id=? AND id=?').bind(actor.companyId,id).first<{status:string;workflow_stage:string}>();
  return command.saveDefault||command.stepKey==='draft'||row?.status==='draft'||row?.status==='rejected'?'new-work':'settle-existing';
 }},run);
}

/** Native owner-only composition; final guards are read-only and run after all cleanup/deferred effects. */
export function expenseFinalRead(database:SqlDatabase,actor:StaffActor,guard:()=>Promise<void>){const unit=units.get(database);if(!unit||!expenseOwnedUnit(database,actor))throw new ApiError(503,"EXPENSE_TRANSACTION_REQUIRED","The Expense final read belongs to its original unit.");unit.finalGuards.push(guard);}
/** The shared reroute endpoint accepts an approval-instance ID. Resolve its
 * native owner in an early leased unit; an Expense instance cannot borrow a
 * Task/Leave label, including across the classification/command boundary. */
export async function withExpenseApprovalReroute<Result>(database:SqlDatabase,actor:Actor,id:string,key:string,run:(transaction:SqlDatabase)=>Promise<Result>){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return run(database);
 const owner=actor.holdingId?(await holdingResources(actor.holdingId)).write:database;
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','Approval ownership requires its original Holding transaction.');
 const execute=async(transaction:SqlDatabase)=>{
  await lockPackageSettlementSources(transaction);
  // Legacy reroute commands insert their receipt before approval/claim effects.
  await transaction.batch([transaction.prepare('LOCK TABLE command_receipts,approval_instances IN SHARE ROW EXCLUSIVE MODE')]);
  const source=()=>transaction.prepare("SELECT jsonb_build_object('id',id,'companyId',company_id,'kind',record_type,'recordId',record_id)::text image,record_type FROM approval_instances WHERE id=? AND company_id=?").bind(id,actor.companyId).first<{image:string;record_type:string}>(),before=await source();
  if(before?.record_type==='expense')return owned(transaction,actor,{key,purpose:'settle-existing'},async tx=>{
   const result=await run(tx);expenseFinalRead(tx,actor,async()=>{if((await source())?.image!==before.image)throw new ApiError(409,'EXPENSE_APPROVAL_SOURCE_CHANGED','The original approval owner changed.');});return result;
  },false,owner);
  const result=await run(transaction);await flushFinanceDeferredEffects(transaction);
  if((await source())?.image!==before?.image)throw new ApiError(409,'EXPENSE_APPROVAL_SOURCE_CHANGED','The original approval owner changed.');return result;
 };
 if(database!==owner){const actual=owner as SqlDatabase & {assertActiveTransaction?:(tx:SqlDatabase)=>void};if(!actual.assertActiveTransaction)throw new ApiError(503,'TRANSACTION_OWNERSHIP_REQUIRED','Approval reroute requires its actual Holding owner.');actual.assertActiveTransaction(database);return execute(database);}
 return owner.transaction(execute);
}

/** Private witness for the same owner's complete Files metadata unit. */
export function expenseCurrentAuthorityImage(database:SqlDatabase,actor:StaffActor){return authority(database,actor);}

/** Preserve the original queued-author service, inside the same leased unit.
 * Only its actual returned actor is registered; callers cannot nominate a new
 * role, permission, Company or arbitrary same-user identity for the command. */
export async function captureExpenseQueueActor(database:SqlDatabase,actor:StaffActor){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return captureJobAuthority(database,actor);
 const unit=units.get(database);if(!unit||unit.read||!expenseOwnedUnit(database,actor))throw new ApiError(503,'EXPENSE_TRANSACTION_REQUIRED','Queued Expense authority requires its original command unit.');
 const captured=await captureJobAuthority(database,actor),capturedIdentity=identity(captured),current=await authority(database,captured);
 unit.capturedActors.add(capturedIdentity);
 unit.finalGuards.push(async()=>{if(identity(captured)!==capturedIdentity||await authority(database,captured)!==current)throw new ApiError(403,'EXPENSE_ACCESS_CHANGED','The original queued Expense authority changed.');});
 return captured;
}

/** Work retains its original staff/role creation policy; this is only the
 * declared Expense purchase intersection for that existing entry control. */
export async function expenseWorkCreationAvailable(database:SqlDatabase,actor:WorkContext){
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return true;
 return (await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'people.expenses'})).newWork;
}
