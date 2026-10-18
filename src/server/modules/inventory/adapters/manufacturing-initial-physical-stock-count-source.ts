import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '../../commercial/public/rules/packages';
import {manufacturingInitialPhysicalStockCommerceLocks} from '../../commerce/public/manufacturing-initial-physical-stock-sources';
import {manufacturingInitialPhysicalStockSourceLocks} from '../../manufacturing/public/initial-physical-stock-sources';
import {initialPhysicalStockCountFinanceLeases} from '../../finance/public/initial-physical-stock-count-sources';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled} from '../../pms/public/hospitality-rollout';
import {financialConstraintError} from '../../workflow/public/commands';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {BusinessRuleError} from '../../shared';
import {manufacturingInitialPhysicalStockCapabilities,manufacturingInitialPhysicalStockCountSourceRows,manufacturingInitialPhysicalStockMarkedRows,manufacturingInitialPhysicalStockSourceImages} from './manufacturing-initial-physical-stock-count-source-sql';
export {manufacturingInitialPhysicalStockCapabilities,manufacturingInitialPhysicalStockMarker,manufacturingInitialPhysicalStockCountSourceRows,manufacturingInitialPhysicalStockMarkedRows,type ManufacturingInitialPhysicalStockProof} from './manufacturing-initial-physical-stock-count-source-sql';
export async function manufacturingInitialPhysicalStockCountMarked(database:SqlDatabase,id:string){return !!await database.prepare(`SELECT 1 FROM (${manufacturingInitialPhysicalStockMarkedRows}) marker WHERE id=?`).bind(id).first();}
export async function manufacturingInitialPhysicalStockCountSource(database:SqlDatabase,scope:{companyId:string;branchId:string|null},id:string){return database.prepare(`SELECT source_proof::text proof FROM (${manufacturingInitialPhysicalStockCountSourceRows}) source WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)`).bind(id,scope.companyId,scope.branchId,scope.branchId).first<string>('proof');}
export async function manufacturingInitialPhysicalStockCountImage(database:SqlDatabase,scope:{companyId:string;branchId:string|null},id:string){return database.prepare(`SELECT image FROM (${manufacturingInitialPhysicalStockSourceImages}) source WHERE id=? AND company_id=? AND EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=source.id AND (?::text IS NULL OR c.branch_id=?))`).bind(id,scope.companyId,scope.branchId,scope.branchId).first<string>('image');}
export function manufacturingInitialPhysicalStockSourceLeases(database:SqlDatabase){return [database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves,inventory_lots,inventory_lot_balances,inventory_lot_moves,inventory_order_reservations,inventory_quality_holds IN SHARE ROW EXCLUSIVE MODE'),...manufacturingInitialPhysicalStockCommerceLocks(database),...manufacturingInitialPhysicalStockSourceLocks(database),...initialPhysicalStockCountFinanceLeases(database)];}
export async function requireManufacturingInitialPhysicalStockAuthority(database:SqlDatabase,actor:WorkContext,permission='inventory.read'){
 if(actor.departmentId)throw new ApiError(403,'INVENTORY_SCOPE_REQUIRED','Use a current Company or Branch Stock assignment.');
 requirePermission(actor,'inventory.read');requirePermission(actor,permission);
 const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'inventory.read');
 for(const moduleId of ['manufacturing','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,moduleId);
 return current;
}
export async function requireManufacturingInitialPhysicalStockHistory(database:SqlDatabase,actor:WorkContext,id:string){
 await requireManufacturingInitialPhysicalStockAuthority(database,actor);
 const source=await manufacturingInitialPhysicalStockCountSource(database,actor,id);if(!source)throw new ApiError(404,'COUNT_NOT_FOUND','The original Initial Physical Stock Count is unavailable.');
 for(const capability of manufacturingInitialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});return source;
}
type ManufacturingInitialPhysicalUnitCommand={key:string;operation:string;id?:string;purpose:CapabilityPurpose;permission?:string;readOnly?:boolean;flowStep?:string;saveDefault?:boolean};
const manufacturingInitialUnits=new WeakMap<SqlDatabase,{binding:string;source:string|null;replay:boolean;current:WorkContext;register:(guard:()=>Promise<void>)=>void}>();
const binding=(actor:WorkContext,command:ManufacturingInitialPhysicalUnitCommand)=>JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam??null,command.key,command.operation,command.id??null,command.flowStep??null,command.saveDefault??false]);
export async function withManufacturingInitialPhysicalStockCountUnit<Result>(owner:SqlDatabase,actor:WorkContext,command:ManufacturingInitialPhysicalUnitCommand,run:(database:SqlDatabase,source:string|null,replay:boolean,current:WorkContext,finalGuard:(guard:()=>Promise<void>)=>void)=>Promise<Result>){
 const borrowed=manufacturingInitialUnits.get(owner);
 if(borrowed){if(borrowed.binding!==binding(actor,command))throw new ApiError(503,'MANUFACTURING_INITIAL_PHYSICAL_STOCK_UNIT_MISMATCH','A different command cannot borrow the Initial Physical Stock Count transaction.');return run(owner,borrowed.source,borrowed.replay,borrowed.current,borrowed.register);}
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Initial Physical Stock Counts require their owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  const purchase=companyPurchaseReadLocks(database,actor.companyId);await lockPackageSettlementSources(database);await database.batch(manufacturingInitialPhysicalStockSourceLeases(database));
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
  const current=await requireManufacturingInitialPhysicalStockAuthority(database,actor,command.permission);
  const replay=!command.readOnly&&!!await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first();
  const before=command.id?await manufacturingInitialPhysicalStockCountSource(database,actor,command.id):null;
  if(command.id&&!before)throw new ApiError(404,'COUNT_NOT_FOUND','The original Initial Physical Stock Count is unavailable.');
  const readImage=command.id&&command.readOnly?await manufacturingInitialPhysicalStockCountImage(database,actor,command.id):null;
  const flow=command.flowStep&&command.id?await database.prepare('SELECT status FROM inventory_counts WHERE id=? AND company_id=?').bind(command.id,actor.companyId).first<{status:string}>():null;
  const flowNewWork=!!command.flowStep&&(command.saveDefault||command.flowStep==='draft'||['draft','rejected'].includes(flow?.status??''));
  const purpose=replay?'history':flowNewWork?'new-work':command.purpose,key=`manufacturing-initial-stock:${randomUUID()}`;
  for(const capability of manufacturingInitialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const boundaries=manufacturingInitialPhysicalStockCapabilities.map((capability,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${key}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  if(!command.readOnly)await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const finalGuards:Array<()=>Promise<void>>=[],register=(guard:()=>Promise<void>)=>{finalGuards.push(guard);};
  manufacturingInitialUnits.set(database,{binding:binding(actor,command),source:before,replay,current,register});
  let result:Result;try{result=await run(database,before,replay,current,register);}finally{manufacturingInitialUnits.delete(database);}
  const completedImage=command.id&&!command.readOnly?await manufacturingInitialPhysicalStockCountImage(database,actor,command.id):readImage;
  if(!command.readOnly)await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));
  if(!command.readOnly)await flushFinanceDeferredEffects(database);
  for(const guard of finalGuards)await guard();
  if(command.id&&completedImage!==await manufacturingInitialPhysicalStockCountImage(database,actor,command.id))throw new ApiError(409,'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original Count image changed before completion.');
  if(before!==null&&before!==await manufacturingInitialPhysicalStockCountSource(database,actor,command.id!))throw new ApiError(409,'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original Count source changed before completion.');
  await requireManufacturingInitialPhysicalStockAuthority(database,actor,command.permission);
  for(const capability of manufacturingInitialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  return result;
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
