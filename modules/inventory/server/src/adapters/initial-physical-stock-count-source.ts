import 'server-only';
import {commandReplayExists} from './native-replay-probe';
import {countStatus} from './native-count-status';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/identity-access-server/public/queued-command-authority';
import {requirePermission} from '@erpx/identity-access-server/public/iam-permissions';
import {lockPackageSettlementSources} from '@erpx/commercial-server/public/package-locks';
import {companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/commercial-server/public/purchase-command-boundary';
import type {CapabilityPurpose} from '@erpx/commercial-server/public/rules/packages';
import {initialPhysicalStockCommerceSourceLocks} from '@erpx/commerce-server/public/initial-physical-stock-count-sources';
import {initialPhysicalStockCountFinanceLeases} from '@erpx/finance-server/public/initial-physical-stock-count-sources';
import {flushFinanceDeferredEffects} from '@erpx/finance-server/public/finance-transactions';
import {lockScheduledFeatureSources} from '@erpx/pms-server/public/scheduled-feature-source-locks';
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import {financialConstraintError} from '@erpx/workflow-server/public/command-guards';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {initialPhysicalStockCapabilities,initialPhysicalStockCountSourceRows,initialPhysicalStockMarkedRows} from '@erpx/inventory-server/adapters/initial-physical-stock-count-source-sql';
import {clearWorkflowCommandAssertions} from "@erpx/workflow-server/public/command-assertion-effects";
export {initialPhysicalStockCapabilities,initialPhysicalStockMarker,initialPhysicalStockCountSourceRows,initialPhysicalStockMarkedRows,type InitialPhysicalStockProof} from '@erpx/inventory-server/adapters/initial-physical-stock-count-source-sql';
import {countMarkedExistsSql,countSourceProofSql} from './read-tail-sql';
export async function initialPhysicalStockCountMarked(database:SqlDatabase,id:string){return !!await database.prepare(countMarkedExistsSql(initialPhysicalStockMarkedRows)).bind(id).first();}
export async function initialPhysicalStockCountSource(database:SqlDatabase,scope:{companyId:string;branchId:string|null},id:string){return database.prepare(countSourceProofSql(initialPhysicalStockCountSourceRows)).bind(id,scope.companyId,scope.branchId,scope.branchId).first<string>('proof');}
export function initialPhysicalStockSourceLeases(database:SqlDatabase){return [database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves,inventory_lots,inventory_lot_balances,inventory_lot_moves,inventory_order_reservations,inventory_quality_holds IN SHARE ROW EXCLUSIVE MODE'),...initialPhysicalStockCommerceSourceLocks(database),...initialPhysicalStockCountFinanceLeases(database)];}
export async function requireInitialPhysicalStockAuthority(database:SqlDatabase,actor:WorkContext,permission='inventory.read'){
 if(actor.departmentId)throw new ApiError(403,'INVENTORY_SCOPE_REQUIRED','Use a current Company or Branch Stock assignment.');
 requirePermission(actor,'inventory.read');requirePermission(actor,permission);
 const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'inventory.read');
 for(const moduleId of ['sales','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,moduleId);
 return current;
}
export async function requireInitialPhysicalStockHistory(database:SqlDatabase,actor:WorkContext,id:string){
 await requireInitialPhysicalStockAuthority(database,actor);
 const source=await initialPhysicalStockCountSource(database,actor,id);if(!source)throw new ApiError(404,'COUNT_NOT_FOUND','The original Initial Physical Stock Count is unavailable.');
 for(const capability of initialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});return source;
}
type InitialPhysicalUnitCommand={key:string;operation:string;id?:string;purpose:CapabilityPurpose;permission?:string;readOnly?:boolean;flowStep?:string;saveDefault?:boolean};
const initialPhysicalUnits=new WeakMap<SqlDatabase,{binding:string;source:string|null;replay:boolean;current:WorkContext;register:(guard:()=>Promise<void>)=>void}>();
const binding=(actor:WorkContext,command:InitialPhysicalUnitCommand)=>JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam??null,command.key,command.operation,command.id??null,command.flowStep??null,command.saveDefault??false]);
export async function withInitialPhysicalStockCountUnit<Result>(owner:SqlDatabase,actor:WorkContext,command:InitialPhysicalUnitCommand,run:(database:SqlDatabase,source:string|null,replay:boolean,current:WorkContext,finalGuard:(guard:()=>Promise<void>)=>void)=>Promise<Result>){
 const borrowed=initialPhysicalUnits.get(owner);
 if(borrowed){if(borrowed.binding!==binding(actor,command))throw new ApiError(503,'INITIAL_PHYSICAL_STOCK_UNIT_MISMATCH','A different command cannot borrow the Initial Physical Stock Count transaction.');return run(owner,borrowed.source,borrowed.replay,borrowed.current,borrowed.register);}
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Initial Physical Stock Counts require their owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  const purchase=companyPurchaseReadLocks(database,actor.companyId);await lockPackageSettlementSources(database);await database.batch(initialPhysicalStockSourceLeases(database));
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
  const current=await requireInitialPhysicalStockAuthority(database,actor,command.permission);
  const replay=!command.readOnly&&await commandReplayExists(database,command.key,command.operation,actor.userId,actor.companyId);
  const before=command.id?await initialPhysicalStockCountSource(database,actor,command.id):null;
  if(command.id&&!before)throw new ApiError(404,'COUNT_NOT_FOUND','The original Initial Physical Stock Count is unavailable.');
  const flow=command.flowStep&&command.id?await countStatus(database,command.id,actor.companyId):null;
  const flowNewWork=!!command.flowStep&&(command.saveDefault||command.flowStep==='draft'||['draft','rejected'].includes(flow?.status??''));
  const purpose=replay?'history':flowNewWork?'new-work':command.purpose,key=`initial-stock:${randomUUID()}`;
  for(const capability of initialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const boundaries=initialPhysicalStockCapabilities.map((capability,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${key}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  if(!command.readOnly)await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const finalGuards:Array<()=>Promise<void>>=[],register=(guard:()=>Promise<void>)=>{finalGuards.push(guard);};
  initialPhysicalUnits.set(database,{binding:binding(actor,command),source:before,replay,current,register});
  let result:Result;try{result=await run(database,before,replay,current,register);}finally{initialPhysicalUnits.delete(database);}
  if(!command.readOnly)await database.batch(boundaries.flatMap(item=>[...item.boundary.after,clearWorkflowCommandAssertions(database, item.key)]));
  if(!command.readOnly)await flushFinanceDeferredEffects(database);
  for(const guard of finalGuards)await guard();
  if(before!==null&&before!==await initialPhysicalStockCountSource(database,actor,command.id!))throw new ApiError(409,'INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original Count source changed before completion.');
  await requireInitialPhysicalStockAuthority(database,actor,command.permission);
  for(const capability of initialPhysicalStockCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  return result;
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
