import 'server-only';
import {randomUUID} from 'node:crypto';
import {decimalToMinor} from '@/lib/finance/finance-money';
import type {WorkContext} from '../../identity_access/public/authorization';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled} from '../../pms/public/hospitality-rollout';
import {financeStarterOrganizationNodes} from '../../organization/public/finance-starter-sources';
import {posStartupMasterFinanceLeases,posStartupMasterFinanceSource,posStartupProductMoneySource,posStartupProductMoneyMatches} from '../../finance/public/pos-startup-master-sources';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {replayCommand,financialConstraintError} from '../../workflow/public/commands';
import {holdingResources} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {BusinessRuleError} from '../../shared';
import {posStartupMappingState} from './pos-posting-setup-sources';
import {createPosStartupMasterInTransaction} from './commerce';
import {posStartupProductSchema,posStartupWarehouseSchema,type PosStartupProductInput,type PosStartupWarehouseInput,type PosStartupMasterResult} from '../domain/pos-startup-masters';
const capabilities=['pos.sales','inventory.stock-core','finance.pos-posting'] as const;
type Kind='products'|'warehouses';
const changed=()=>new ApiError(409,'POS_STARTUP_SOURCE_CHANGED','The original POS mapping, master or price source changed before completion.');
const authorityImage=(actor:WorkContext)=>JSON.stringify([actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);
async function authority(database:SqlDatabase,actor:WorkContext){
 if(actor.departmentId)throw new ApiError(403,'INVENTORY_SCOPE_REQUIRED','Use a current Company or Branch Stock assignment.');
 requirePermission(actor,'inventory.read');requirePermission(actor,'inventory.prepare');
 const current=await requireQueuedCommandAuthority(database,actor,'inventory.prepare');requirePermission(current,'inventory.read');
 for(const moduleId of ['pos','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,moduleId);
 return current;
}
async function leases(database:SqlDatabase,actor:WorkContext){
 const purchase=companyPurchaseReadLocks(database,actor.companyId);
 await lockPackageSettlementSources(database);
 // Original generic masters take their receipt before business effects, even
 // with legacy enforcement disabled. Acquire this exclusion before any source.
 await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();
 await database.prepare('LOCK TABLE commerce_products,commerce_warehouses IN SHARE ROW EXCLUSIVE MODE').run();
 await database.prepare('LOCK TABLE commerce_settings IN SHARE MODE').run();
 await database.batch(posStartupMasterFinanceLeases(database));
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
}
async function master(database:SqlDatabase,actor:WorkContext,kind:Kind,id:string){
 const table=kind==='products'?'commerce_products':'commerce_warehouses';
 return database.prepare(`SELECT to_jsonb(master)::text source FROM ${table} master WHERE id=? AND company_id=? ${kind==='warehouses'?'AND (?::text IS NULL OR branch_id=?)':''}`).bind(id,actor.companyId,...(kind==='warehouses'?[actor.branchId,actor.branchId]:[])).first<string>('source');
}
async function ownReceipt(database:SqlDatabase,actor:WorkContext,operation:string,key:string,id:string){
 return database.prepare("SELECT to_jsonb(receipt)::text source FROM command_receipts receipt WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,id,actor.userId,actor.companyId).first<string>('source');
}
function matches(source:string|null,actor:WorkContext,kind:Kind,input:PosStartupProductInput|PosStartupWarehouseInput,id:string){
 if(!source)return false;const row=JSON.parse(source) as Record<string,unknown>;
 if(row.id!==id||row.company_id!==actor.companyId||Number(row.version)!==1||row.status!==input.status)return false;
 if(kind==='products'){const value=input as PosStartupProductInput;return row.item_type===value.itemType&&row.sku===value.sku&&row.name===value.name&&row.unit===value.unit&&row.tracking_mode===value.trackingMode&&Number(row.price_minor)===decimalToMinor(value.price)&&Number(row.tax_bps)===value.taxBps;}
 const value=input as PosStartupWarehouseInput;return row.branch_id===value.branchId&&row.code===value.code&&row.name===value.name;
}
async function branch(database:SqlDatabase,actor:WorkContext,input:PosStartupProductInput|PosStartupWarehouseInput,final=false){
 if(!('branchId' in input))return null;
 if(actor.branchId&&actor.branchId!==input.branchId)throw new ApiError(403,'SCOPE_FORBIDDEN','Choose your assigned branch.');
 const selected=(await financeStarterOrganizationNodes(database,actor.companyId)).find(row=>row.kind==='branch'&&row.id===input.branchId);
 if(!selected&&!final)throw new ApiError(409,'CONFLICT','Choose an active Branch in this Company.');return selected??null;
}
async function create(actor:WorkContext,kind:Kind,raw:unknown,key:string):Promise<PosStartupMasterResult>{
 const input=kind==='products'?posStartupProductSchema.parse(raw):posStartupWarehouseSchema.parse(raw),operation=`commerce.${kind}.create`,id=`${kind}-${key}`;
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','POS Stock master creation requires its original Holding transaction.');
 try{return await owner.transaction(async database=>{
  await leases(database,actor);const current=await authority(database,actor);
  const receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId).first();
  const purpose=receipt?'history' as const:'new-work' as const,guard=`pos-startup-master:${randomUUID()}`;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const mapping=await posStartupMappingState(database,actor.companyId);
  if(!mapping)throw new ApiError(409,'POS_STARTUP_SETUP_REQUIRED','Configure the original POS posting mapping first.');
  const finance=await posStartupMasterFinanceSource(database,actor.companyId,String(mapping.chartId),String(mapping.currency)),before=await master(database,actor,kind,id),beforeMoney=kind==='products'?await posStartupProductMoneySource(database,actor.companyId,id):null;
  const selectedBranch=receipt?null:await branch(database,actor,input);
  let result:{id:string;version:number};let createdMoney:Awaited<ReturnType<typeof createPosStartupMasterInTransaction>>['money']=null;
  if(receipt){const replay=await replayCommand<{id:string;version:number}>(database,actor,key,operation,id,input);if(!replay||!before)throw changed();result=replay;}
  else{
   if(before||beforeMoney?.length)throw changed();
   if(!finance.effective||finance.effective.chart_id!==mapping.chartId||finance.effective.currency!==mapping.currency||finance.effective.base_currency!==mapping.currency)throw new ApiError(409,'POS_STARTUP_SETUP_REQUIRED','Use the active original POS posting chart and document currency.');
   const created=await createPosStartupMasterInTransaction(database,actor,kind,input,key);result=created.result;createdMoney=created.money;
   if(!matches(await master(database,actor,kind,id),actor,kind,input,id))throw changed();
   if(kind==='products'&&(!createdMoney||!posStartupProductMoneyMatches(await posStartupProductMoneySource(database,actor.companyId,id),actor.companyId,id,createdMoney)))throw changed();
  }
  if(result.id!==id||result.version!==1)throw changed();
  const expectedMaster=await master(database,actor,kind,id),expectedMoney=kind==='products'?await posStartupProductMoneySource(database,actor.companyId,id):null,expectedReceipt=await ownReceipt(database,actor,operation,key,id);
  if(!expectedReceipt)throw changed();
  const saved=JSON.parse(expectedReceipt) as {result_json:string;request_hash:string},savedResult=JSON.parse(saved.result_json) as {id?:unknown;version?:unknown};
  if(!saved.request_hash||savedResult.id!==id||savedResult.version!==1)throw changed();
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));
  await flushFinanceDeferredEffects(database);
  if(expectedReceipt!==await ownReceipt(database,actor,operation,key,id)||JSON.stringify(mapping)!==JSON.stringify(await posStartupMappingState(database,actor.companyId))||finance.image!==(await posStartupMasterFinanceSource(database,actor.companyId,String(mapping.chartId),String(mapping.currency))).image||expectedMaster!==await master(database,actor,kind,id)||JSON.stringify(expectedMoney)!==JSON.stringify(kind==='products'?await posStartupProductMoneySource(database,actor.companyId,id):null))throw changed();
  if(receipt&&(before!==expectedMaster||JSON.stringify(beforeMoney)!==JSON.stringify(expectedMoney)))throw changed();
  if(!receipt&&(!matches(expectedMaster,actor,kind,input,id)||JSON.stringify(selectedBranch)!==JSON.stringify(await branch(database,actor,input,true))||kind==='products'&&(!createdMoney||!posStartupProductMoneyMatches(expectedMoney!,actor.companyId,id,createdMoney))))throw changed();
  const finalAuthority=await authority(database,actor);if(authorityImage(current)!==authorityImage(finalAuthority))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original Stock assignment changed before completion.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  return {id:result.id,version:1};
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export function createPosStartupProduct(actor:WorkContext,raw:unknown,key:string){return create(actor,'products',raw,key);}
export function createPosStartupWarehouse(actor:WorkContext,raw:unknown,key:string){return create(actor,'warehouses',raw,key);}
