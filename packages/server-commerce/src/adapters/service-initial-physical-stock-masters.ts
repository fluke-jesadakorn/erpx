import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import type {ServicePartsStockWorkspace} from '@erpx/server-commerce/domain/service-parts-posting-setup';
import {readPurchasedAccesses} from '@erpx/server-commercial/public/purchase-command-boundary';
import 'server-only';
import {randomUUID} from 'node:crypto';
import {decimalToMinor} from '@erpx/feature-finance/lib/finance-money';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {financeStarterOrganizationNodes} from '@erpx/server-organization/public/finance-starter-sources';
import {serviceInitialPhysicalStockMasterFinanceLeases,serviceInitialPhysicalStockMasterFinanceSource,serviceInitialPhysicalStockProductMoneySource,serviceInitialPhysicalStockProductMoneyMatches} from '@erpx/server-finance/public/service-initial-physical-stock-master-sources';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {replayCommand,financialConstraintError} from '@erpx/server-workflow/public/commands';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {serviceInitialPhysicalStockCommerceSource} from '@erpx/server-commerce/adapters/service-initial-physical-stock-sources';
import {createServiceInitialPhysicalStockMasterInTransaction} from '@erpx/server-commerce/adapters/commerce';
import {serviceInitialPhysicalStockProductSchema,serviceInitialPhysicalStockWarehouseSchema,type ServiceInitialPhysicalStockProductInput,type ServiceInitialPhysicalStockWarehouseInput,type ServiceInitialPhysicalStockMasterResult} from '@erpx/server-commerce/domain/service-initial-physical-stock-masters';
const capabilities=['service.orders','service.parts-assets','finance.service-invoice'] as const;
type Kind='products'|'warehouses';
const changed=()=>new ApiError(409,'SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original Service mapping, master or price source changed before completion.');
const authorityImage=(actor:WorkContext)=>JSON.stringify([actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);
async function authority(database:SqlDatabase,actor:WorkContext){
 if(actor.departmentId)throw new ApiError(403,'INVENTORY_SCOPE_REQUIRED','Use a current Company or Branch Stock assignment.');
 requirePermission(actor,'inventory.read');requirePermission(actor,'inventory.prepare');
 const current=await requireQueuedCommandAuthority(database,actor,'inventory.prepare');requirePermission(current,'inventory.read');
 for(const moduleId of ['field_service','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,moduleId);
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
 await database.batch(serviceInitialPhysicalStockMasterFinanceLeases(database));
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
}
async function master(database:SqlDatabase,actor:WorkContext,kind:Kind,id:string){
 const table=kind==='products'?'commerce_products':'commerce_warehouses';
 return database.prepare(`SELECT to_jsonb(master)::text source FROM ${table} master WHERE id=? AND company_id=? ${kind==='warehouses'?'AND (?::text IS NULL OR branch_id=?)':''}`).bind(id,actor.companyId,...(kind==='warehouses'?[actor.branchId,actor.branchId]:[])).first<string>('source');
}
async function ownReceipt(database:SqlDatabase,actor:WorkContext,operation:string,key:string,id:string){
 return database.prepare("SELECT to_jsonb(receipt)::text source FROM command_receipts receipt WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,id,actor.userId,actor.companyId).first<string>('source');
}
function matches(source:string|null,actor:WorkContext,kind:Kind,input:ServiceInitialPhysicalStockProductInput|ServiceInitialPhysicalStockWarehouseInput,id:string){
 if(!source)return false;const row=JSON.parse(source) as Record<string,unknown>;
 if(row.id!==id||row.company_id!==actor.companyId||Number(row.version)!==1||row.status!==input.status)return false;
 if(kind==='products'){const value=input as ServiceInitialPhysicalStockProductInput;return row.item_type==='stock'&&row.sku===value.sku&&row.name===value.name&&row.unit===value.unit&&row.tracking_mode===value.trackingMode&&Number(row.price_minor)===decimalToMinor(value.price)&&Number(row.tax_bps)===value.taxBps;}
 const value=input as ServiceInitialPhysicalStockWarehouseInput;return row.branch_id===value.branchId&&row.code===value.code&&row.name===value.name;
}
async function branch(database:SqlDatabase,actor:WorkContext,input:ServiceInitialPhysicalStockProductInput|ServiceInitialPhysicalStockWarehouseInput,final=false){
 if(!('branchId' in input))return null;
 if(actor.branchId&&actor.branchId!==input.branchId)throw new ApiError(403,'SCOPE_FORBIDDEN','Choose your assigned branch.');
 const selected=(await financeStarterOrganizationNodes(database,actor.companyId)).find(row=>row.kind==='branch'&&row.id===input.branchId);
 if(!selected&&!final)throw new ApiError(409,'CONFLICT','Choose an active Branch in this Company.');return selected??null;
}
async function create(actor:WorkContext,kind:Kind,raw:unknown,key:string):Promise<ServiceInitialPhysicalStockMasterResult>{
 const input=kind==='products'?serviceInitialPhysicalStockProductSchema.parse(raw):serviceInitialPhysicalStockWarehouseSchema.parse(raw),operation=`commerce.${kind}.create`,id=`${kind}-${key}`;
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Service Stock master creation requires its original Holding transaction.');
 try{return await owner.transaction(async database=>{
  await leases(database,actor);const current=await authority(database,actor);
  const receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId).first();
  const purpose=receipt?'history' as const:'new-work' as const,guard=`service-stock-master:${randomUUID()}`;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const mapping=await serviceInitialPhysicalStockCommerceSource(database,actor.companyId);
  if(!mapping)throw new ApiError(409,'SERVICE_INITIAL_PHYSICAL_STOCK_SETUP_REQUIRED','Configure the original Service posting mapping first.');
  const finance=await serviceInitialPhysicalStockMasterFinanceSource(database,actor.companyId,String(mapping.chart_id),String(mapping.currency)),before=await master(database,actor,kind,id),beforeMoney=kind==='products'?await serviceInitialPhysicalStockProductMoneySource(database,actor.companyId,id):null;
  const selectedBranch=receipt?null:await branch(database,actor,input);
  let result:{id:string;version:number};let createdMoney:Awaited<ReturnType<typeof createServiceInitialPhysicalStockMasterInTransaction>>['money']=null;
  if(receipt){const replay=await replayCommand<{id:string;version:number}>(database,actor,key,operation,id,input);if(!replay||!before)throw changed();result=replay;}
  else{
   if(before||beforeMoney?.length)throw changed();
   if(!finance.effective||finance.effective.chart_id!==mapping.chart_id||finance.effective.currency!==mapping.currency)throw new ApiError(409,'SERVICE_INITIAL_PHYSICAL_STOCK_SETUP_REQUIRED','Use the active original Service posting chart and document currency.');
   const created=await createServiceInitialPhysicalStockMasterInTransaction(database,actor,kind,input,key);result=created.result;createdMoney=created.money;
   if(!matches(await master(database,actor,kind,id),actor,kind,input,id))throw changed();
   if(kind==='products'&&(!createdMoney||!serviceInitialPhysicalStockProductMoneyMatches(await serviceInitialPhysicalStockProductMoneySource(database,actor.companyId,id),actor.companyId,id,createdMoney)))throw changed();
  }
  if(result.id!==id||result.version!==1)throw changed();
  const expectedMaster=await master(database,actor,kind,id),expectedMoney=kind==='products'?await serviceInitialPhysicalStockProductMoneySource(database,actor.companyId,id):null,expectedReceipt=await ownReceipt(database,actor,operation,key,id);
  if(!expectedReceipt)throw changed();
  const saved=JSON.parse(expectedReceipt) as {result_json:string;request_hash:string},savedResult=JSON.parse(saved.result_json) as {id?:unknown;version?:unknown};
  if(!saved.request_hash||savedResult.id!==id||savedResult.version!==1)throw changed();
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));
  await flushFinanceDeferredEffects(database);
  if(expectedReceipt!==await ownReceipt(database,actor,operation,key,id)||JSON.stringify(mapping)!==JSON.stringify(await serviceInitialPhysicalStockCommerceSource(database,actor.companyId))||finance.image!==(await serviceInitialPhysicalStockMasterFinanceSource(database,actor.companyId,String(mapping.chart_id),String(mapping.currency))).image||expectedMaster!==await master(database,actor,kind,id)||JSON.stringify(expectedMoney)!==JSON.stringify(kind==='products'?await serviceInitialPhysicalStockProductMoneySource(database,actor.companyId,id):null))throw changed();
  if(receipt&&(before!==expectedMaster||JSON.stringify(beforeMoney)!==JSON.stringify(expectedMoney)))throw changed();
  if(!receipt&&(!matches(expectedMaster,actor,kind,input,id)||JSON.stringify(selectedBranch)!==JSON.stringify(await branch(database,actor,input,true))||kind==='products'&&(!createdMoney||!serviceInitialPhysicalStockProductMoneyMatches(expectedMoney!,actor.companyId,id,createdMoney))))throw changed();
  const finalAuthority=await authority(database,actor);if(authorityImage(current)!==authorityImage(finalAuthority))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original Stock assignment changed before completion.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  return {id:result.id,version:1};
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export function createServiceInitialPhysicalStockProduct(actor:WorkContext,raw:unknown,key:string){return create(actor,'products',raw,key);}
export function createServiceInitialPhysicalStockWarehouse(actor:WorkContext,raw:unknown,key:string){return create(actor,'warehouses',raw,key);}

/** Stock-only discovery never projects the Finance mapping or account rows. */
export async function servicePartsStockWorkspace(actor:WorkContext):Promise<ServicePartsStockWorkspace>{
 const empty=(unavailableReason:ServicePartsStockWorkspace['unavailableReason']):ServicePartsStockWorkspace=>({companyId:actor.companyId,available:false,schemaAvailable:false,sourceReady:false,currency:null,canCreateWarehouse:false,canCreateProduct:false,branches:[],unavailableReason});
 if(actor.departmentId||!hasPermission(actor,'inventory.read')||!hasPermission(actor,'inventory.prepare'))return empty('stock-authority-required');
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Service Stock discovery requires the original Holding transaction.');
 return owner.transaction(async database=>{
  await leases(database,actor);const current=await authority(database,actor),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(cap=>access[cap].history))return empty('purchase-required');
  const schemaAvailable=!!await database.prepare("SELECT to_regclass('commerce_service_parts_posting_sources') IS NOT NULL AND to_regclass('inventory_service_initial_physical_sources') IS NOT NULL ready").first<boolean>('ready'),source=await serviceInitialPhysicalStockCommerceSource(database,actor.companyId),nodes=await financeStarterOrganizationNodes(database,actor.companyId),active=capabilities.every(cap=>access[cap].newWork),ready=schemaAvailable&&!!source;
  const final=await authority(database,actor);if(authorityImage(current)!==authorityImage(final)||JSON.stringify(source)!==JSON.stringify(await serviceInitialPhysicalStockCommerceSource(database,actor.companyId)))throw changed();
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {companyId:actor.companyId,available:true,schemaAvailable,sourceReady:!!source,currency:source?String(source.currency):null,canCreateWarehouse:active&&ready,canCreateProduct:active&&ready,branches:nodes.filter(row=>row.kind==='branch'&&(!actor.branchId||row.id===actor.branchId)).map(row=>({id:row.id,name:row.display_name})),unavailableReason:!schemaAvailable?'schema-required':!source?'service-posting-required':null};
 });
}
