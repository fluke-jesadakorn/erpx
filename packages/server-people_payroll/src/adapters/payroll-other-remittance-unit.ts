import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {payrollOtherFinanceLeases,payrollOtherFinanceState} from '@erpx/server-finance/public/payroll-other-remittance-facts';
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {payrollOtherSource,payrollOtherSourceLeases,payrollOtherRemittanceTables,type PayrollOtherSource} from '@erpx/server-people_payroll/adapters/payroll-other-remittance-sources';
export {payrollOtherSource,payrollOtherRemittanceTables};
type Kind='read'|'remit'|'bank-import'|'bank-ignore';
type Unit={actor:WorkContext;current:WorkContext;runId:string;kind:Kind;source:PayrollOtherSource;guards:(()=>Promise<void>)[]};
const units=new WeakMap<SqlDatabase,Unit>();
const image=(actor:WorkContext)=>JSON.stringify([actor.userId,actor.assignmentId,actor.companyId,actor.role,actor.branchId,actor.departmentId,actor.authority??null,actor.iam]);
export function payrollOtherContext(actor:WorkContext,source:PayrollOtherSource,financeImage:string,remittances:unknown){return createHash('sha256').update(JSON.stringify([image(actor),source.image,financeImage,remittances])).digest('hex');}
export function assertPayrollOtherRemittanceUnit(database:SqlDatabase,actor:WorkContext,runId:string,kind?:Kind){const unit=units.get(database);if(!unit||unit.actor!==actor||unit.runId!==runId||kind&&unit.kind!==kind)throw new ApiError(503,'PAYROLL_TRANSACTION_REQUIRED','Use the closed Payroll liability owner command in its original Holding transaction.');return unit;}
async function authority(database:SqlDatabase,actor:WorkContext,kind:Kind){
 requirePermission(actor,'finance.pay');if(actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_PAYROLL_SCOPE_REQUIRED','Use a Company Treasury assignment for this original Payroll liability.');
 const current=await requireQueuedCommandAuthority(database,actor,'finance.pay');
 if(kind==='bank-import'||kind==='bank-ignore'){requirePermission(actor,'finance.write');requirePermission(current,'finance.write');}
 await assertProductionModuleEnabled(database,actor.companyId,'people_payroll');await assertProductionModuleEnabled(database,actor.companyId,'finance');return current;
}
/** All leases precede the first query/advisory. Writes use the original strong
 * package fence then receipt fence, excluding enabled advisory-first writers.
 * A read takes no Company advisory and performs no writes or lock upgrades. */
async function leases(database:SqlDatabase,companyId:string,write:boolean){
 const purchase=companyPurchaseReadLocks(database,companyId);if(write)await lockPackageSettlementSources(database);else await database.batch([purchase[0]]);
 await database.prepare(`LOCK TABLE command_receipts IN ${write?'SHARE ROW EXCLUSIVE':'SHARE'} MODE`).run();
 await database.batch([...payrollOtherSourceLeases(database,write),...payrollOtherFinanceLeases(database,write)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);if(write)await database.batch(purchase.slice(1));
}
export async function withPayrollOtherRemittanceUnit<Result>(actor:WorkContext,runId:string,command:{kind:Kind;key?:string},run:(database:SqlDatabase,unit:Unit)=>Promise<Result>):Promise<Result>{
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'PAYROLL_TRANSACTION_REQUIRED','Payroll liability settlement requires its original Holding PostgreSQL transaction.');
 try{return await owner.transaction(async database=>{
  await leases(database,actor.companyId,command.kind!=='read');const current=await authority(database,actor,command.kind),operation=command.kind==='bank-import'?'finance.bank.import':command.kind==='bank-ignore'?'payroll.liability.bank-evidence.ignore':'payroll.liability.remit';
  const prior=command.key?await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,operation,actor.userId,actor.companyId).first():null,purpose=command.kind==='read'||prior?'history' as const:'settle-existing' as const;
  await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:'people.payroll',purpose});
  const boundaryKey=`payroll-other:${randomUUID()}`,boundary=command.kind==='read'?null:purchaseCommandBoundary(database,'enforced',boundaryKey,{companyId:actor.companyId,capability:'people.payroll',purpose})!;
  if(boundary)await database.batch(boundary.before);
  const source=await payrollOtherSource(database,actor.companyId,runId),before=await payrollOtherFinanceState(database,actor.companyId,source),unit:Unit={actor,current,runId,kind:command.kind,source,guards:[]};units.set(database,unit);
  try{
   const result=await run(database,unit);
   if(boundary)await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(boundaryKey)]);
   if(command.kind!=='read')await flushFinanceDeferredEffects(database);
   if((await payrollOtherSource(database,actor.companyId,runId)).image!==source.image||(await payrollOtherFinanceState(database,actor.companyId,source)).accrualImage!==before.accrualImage)throw new ApiError(409,'PAYROLL_REMITTANCE_SOURCE_CHANGED','The original approved Payroll accrual changed before completion.');
   for(const guard of unit.guards)await guard();
   if(image(current)!==image(await authority(database,actor,command.kind)))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original Company Treasury authority changed.');
   await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:'people.payroll',purpose});return result;
  }finally{units.delete(database);}
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
