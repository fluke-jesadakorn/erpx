import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
import {posInitialPhysicalStockCommerceLocks} from '@erpx/server-commerce/public/pos-initial-physical-stock-sources';
import {initialPhysicalStockCountFinanceLeases} from '@erpx/server-finance/public/initial-physical-stock-count-sources';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled} from '@erpx/server-pms/public/hospitality-rollout';
import {financialConstraintError} from '@erpx/server-workflow/public/commands';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {posInitialPhysicalStockCapabilities,posInitialPhysicalStockCountSourceRows,posInitialPhysicalStockMarkedRows,posInitialPhysicalStockSourceImages} from '@erpx/server-inventory/adapters/pos-initial-physical-stock-count-source-sql';
export {posInitialPhysicalStockCapabilities,posInitialPhysicalStockMarker,posInitialPhysicalStockCountSourceRows,posInitialPhysicalStockMarkedRows,type PosInitialPhysicalStockProof} from '@erpx/server-inventory/adapters/pos-initial-physical-stock-count-source-sql';
export async function posInitialPhysicalStockCountMarked(database:SqlDatabase,id:string){return !!await database.prepare(`SELECT 1 FROM (${posInitialPhysicalStockMarkedRows}) marker WHERE id=?`).bind(id).first();}
export async function posInitialPhysicalStockCountSource(database:SqlDatabase,scope:{companyId:string;branchId:string|null},id:string){return database.prepare(`SELECT source_proof::text proof FROM (${posInitialPhysicalStockCountSourceRows}) source WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)`).bind(id,scope.companyId,scope.branchId,scope.branchId).first<string>('proof');}
export async function posInitialPhysicalStockCountImage(database:SqlDatabase,scope:{companyId:string;branchId:string|null},id:string){return database.prepare(`SELECT image FROM (${posInitialPhysicalStockSourceImages}) source WHERE id=? AND company_id=? AND EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=source.id AND (?::text IS NULL OR c.branch_id=?))`).bind(id,scope.companyId,scope.branchId,scope.branchId).first<string>('image');}
export function posInitialPhysicalStockSourceLeases(database:SqlDatabase){return [database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE inventory_ledger,inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves,inventory_lots,inventory_lot_balances,inventory_lot_moves,inventory_order_reservations,inventory_quality_holds IN SHARE ROW EXCLUSIVE MODE'),...posInitialPhysicalStockCommerceLocks(database),...initialPhysicalStockCountFinanceLeases(database)];}
export async function requirePosInitialPhysicalStockAuthority(database:SqlDatabase,actor:WorkContext,permission='inventory.read'){
 if(actor.departmentId)throw new ApiError(403,'INVENTORY_SCOPE_REQUIRED','Use a current Company or Branch Stock assignment.');
 requirePermission(actor,'inventory.read');requirePermission(actor,permission);
 const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'inventory.read');
 for(const moduleId of ['pos','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,moduleId);
 return current;
}
export async function requirePosInitialPhysicalStockHistory(database:SqlDatabase,actor:WorkContext,id:string){
 await requirePosInitialPhysicalStockAuthority(database,actor);
 const source=await posInitialPhysicalStockCountSource(database,actor,id);if(!source)throw new ApiError(404,'COUNT_NOT_FOUND','The original Initial Physical Stock Count is unavailable.');
 for(const capability of posInitialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});return source;
}
type PosInitialPhysicalUnitCommand={key:string;operation:string;id?:string;purpose:CapabilityPurpose;permission?:string;readOnly?:boolean;flowStep?:string;saveDefault?:boolean};
const posInitialUnits=new WeakMap<SqlDatabase,{binding:string;source:string|null;replay:boolean;current:WorkContext;register:(guard:()=>Promise<void>)=>void}>();
const binding=(actor:WorkContext,command:PosInitialPhysicalUnitCommand)=>JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam??null,command.key,command.operation,command.id??null,command.flowStep??null,command.saveDefault??false]);
export async function withPosInitialPhysicalStockCountUnit<Result>(owner:SqlDatabase,actor:WorkContext,command:PosInitialPhysicalUnitCommand,run:(database:SqlDatabase,source:string|null,replay:boolean,current:WorkContext,finalGuard:(guard:()=>Promise<void>)=>void)=>Promise<Result>){
 const borrowed=posInitialUnits.get(owner);
 if(borrowed){if(borrowed.binding!==binding(actor,command))throw new ApiError(503,'POS_INITIAL_PHYSICAL_STOCK_UNIT_MISMATCH','A different command cannot borrow the Initial Physical Stock Count transaction.');return run(owner,borrowed.source,borrowed.replay,borrowed.current,borrowed.register);}
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Initial Physical Stock Counts require their owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  const purchase=companyPurchaseReadLocks(database,actor.companyId);await lockPackageSettlementSources(database);await database.batch(posInitialPhysicalStockSourceLeases(database));
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
  const current=await requirePosInitialPhysicalStockAuthority(database,actor,command.permission);
  const replay=!command.readOnly&&!!await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first();
  const before=command.id?await posInitialPhysicalStockCountSource(database,actor,command.id):null;
  if(command.id&&!before)throw new ApiError(404,'COUNT_NOT_FOUND','The original Initial Physical Stock Count is unavailable.');
  const readImage=command.id&&command.readOnly?await posInitialPhysicalStockCountImage(database,actor,command.id):null;
  const flow=command.flowStep&&command.id?await database.prepare('SELECT status FROM inventory_counts WHERE id=? AND company_id=?').bind(command.id,actor.companyId).first<{status:string}>():null;
  const flowNewWork=!!command.flowStep&&(command.saveDefault||command.flowStep==='draft'||['draft','rejected'].includes(flow?.status??''));
  const purpose=replay?'history':flowNewWork?'new-work':command.purpose,key=`pos-initial-stock:${randomUUID()}`;
  for(const capability of posInitialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const boundaries=posInitialPhysicalStockCapabilities.map((capability,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${key}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  if(!command.readOnly)await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const finalGuards:Array<()=>Promise<void>>=[],register=(guard:()=>Promise<void>)=>{finalGuards.push(guard);};
  posInitialUnits.set(database,{binding:binding(actor,command),source:before,replay,current,register});
  let result:Result;try{result=await run(database,before,replay,current,register);}finally{posInitialUnits.delete(database);}
  const completedImage=command.id&&!command.readOnly?await posInitialPhysicalStockCountImage(database,actor,command.id):readImage;
  if(!command.readOnly)await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));
  if(!command.readOnly)await flushFinanceDeferredEffects(database);
  for(const guard of finalGuards)await guard();
  if(command.id&&completedImage!==await posInitialPhysicalStockCountImage(database,actor,command.id))throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original Count image changed before completion.');
  if(before!==null&&before!==await posInitialPhysicalStockCountSource(database,actor,command.id!))throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original Count source changed before completion.');
  await requirePosInitialPhysicalStockAuthority(database,actor,command.permission);
  for(const capability of posInitialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  return result;
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
