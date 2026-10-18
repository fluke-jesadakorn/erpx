import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@/lib/identity_access/permissions';
import {decimalToMinor} from '@/lib/finance/finance-money';
import type {WorkContext} from '../../identity_access/public/authorization';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled} from '../../pms/public/hospitality-rollout';
import {salesServiceProductFinanceLeases,salesServiceProductFinanceSource,salesServiceProductMoneySource,salesServiceProductMoneyMatches} from '../../finance/public/sales-service-product-sources';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {replayCommand,financialConstraintError} from '../../workflow/public/commands';
import {holdingResources} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {BusinessRuleError} from '../../shared';
import {createSalesServiceProductInTransaction} from './commerce';
import {salesServiceProductSchema,type SalesServiceProductInput,type SalesServiceProductResult,type SalesServiceProductsView} from '../domain/sales-service-products';
const capabilities=['sales.quotes','sales.orders','finance.sales-posting'] as const;
const changed=()=>new ApiError(409,'SALES_SERVICE_PRODUCT_SOURCE_CHANGED','The original Sales posting, product or price source changed before completion.');
const authorityImage=(actor:WorkContext)=>JSON.stringify([actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);
async function authority(database:SqlDatabase,actor:WorkContext,write=false){
 if(write&&actor.departmentId)throw new ApiError(403,'SALES_SERVICE_SCOPE_REQUIRED','Use a current Company or Branch Sales assignment.');
 let current=actor;for(const permission of ['commerce.read','commerce.sales',...(write?['inventory.prepare']:[])]){requirePermission(actor,permission);current=await requireQueuedCommandAuthority(database,actor,permission);}
 for(const feature of ['sales','finance',...(write?['inventory']:[])] as const)await assertProductionModuleEnabled(database,actor.companyId,feature);
 return current;
}
async function leases(database:SqlDatabase,actor:WorkContext,write=false){
 const purchase=companyPurchaseReadLocks(database,actor.companyId);await lockPackageSettlementSources(database);
 if(write)await database.prepare('LOCK TABLE active_work_contexts IN EXCLUSIVE MODE').run();
 await database.prepare(`LOCK TABLE command_receipts,commerce_products IN ${write?'SHARE ROW EXCLUSIVE':'SHARE'} MODE`).run();
 await database.prepare('LOCK TABLE commerce_settings IN SHARE MODE').run();await database.batch(salesServiceProductFinanceLeases(database,write));
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
}
async function owner(actor:WorkContext){const database=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(database))throw new ApiError(503,'TRANSACTION_REQUIRED','Sales service creation requires its original Holding transaction.');return database;}
async function product(database:SqlDatabase,actor:WorkContext,id:string){return database.prepare('SELECT to_jsonb(p)::text source FROM commerce_products p WHERE id=? AND company_id=?').bind(id,actor.companyId).first<string>('source');}
async function receipt(database:SqlDatabase,actor:WorkContext,key:string,id:string){return database.prepare("SELECT to_jsonb(r)::text source FROM command_receipts r WHERE command_id=? AND operation='commerce.products.create' AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,id,actor.userId,actor.companyId).first<string>('source');}
function matches(source:string|null,actor:WorkContext,input:SalesServiceProductInput,id:string){if(!source)return false;const p=JSON.parse(source) as Record<string,unknown>;return p.id===id&&p.company_id===actor.companyId&&Number(p.version)===1&&p.item_type==='service'&&p.tracking_mode==='none'&&p.sku===input.sku&&p.name===input.name&&p.unit===input.unit&&p.status===input.status&&Number(p.price_minor)===decimalToMinor(input.price)&&Number(p.tax_bps)===input.taxBps;}
export async function salesServiceProducts(actor:WorkContext):Promise<SalesServiceProductsView>{
 const empty=(unavailableReason:SalesServiceProductsView['unavailableReason']):SalesServiceProductsView=>({companyId:actor.companyId,available:false,contextKey:null,currency:null,canCreate:false,products:[],unavailableReason});
 if(!hasPermission(actor,'commerce.read')||!hasPermission(actor,'commerce.sales'))return empty('authority-required');
 return (await owner(actor)).transaction(async database=>{
  await leases(database,actor);const current=await authority(database,actor),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return empty('purchase-required');
  const finance=await salesServiceProductFinanceSource(database,actor.companyId),products=(await database.prepare("SELECT id,sku,name,unit,price_minor, tax_bps,status,version FROM commerce_products WHERE company_id=? AND item_type='service' AND tracking_mode='none' ORDER BY sku,id").bind(actor.companyId).all<{id:string;sku:string;name:string;unit:string;price_minor:number;tax_bps:number;status:'active'|'archived';version:number}>()).results;
  const canCreate=!actor.departmentId&&!!finance.effective&&hasPermission(current,'inventory.prepare')&&capabilities.every(capability=>access[capability].newWork);
  if(canCreate)await authority(database,actor,true);
  if(authorityImage(current)!==authorityImage(await authority(database,actor))||finance.image!==(await salesServiceProductFinanceSource(database,actor.companyId)).image)throw changed();
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {companyId:actor.companyId,available:true,contextKey:createHash('sha256').update(JSON.stringify([authorityImage(current),finance.image])).digest('hex'),currency:finance.effective?String(finance.effective.currency):null,canCreate,products:products.map(p=>({id:p.id,sku:p.sku,name:p.name,unit:p.unit,priceMinor:p.price_minor,taxBps:p.tax_bps,status:p.status,version:p.version})),unavailableReason:finance.effective?null:'sales-posting-required'};
 });
}
export async function createSalesServiceProduct(actor:WorkContext,raw:unknown,key:string):Promise<SalesServiceProductResult>{
 const input=salesServiceProductSchema.parse(raw),id=`products-${key}`,operation='commerce.products.create';
 try{return await (await owner(actor)).transaction(async database=>{
  await leases(database,actor,true);const current=await authority(database,actor,true),originalReceipt=await receipt(database,actor,key,id),purpose=originalReceipt?'history' as const:'new-work' as const,guard=`sales-service:${randomUUID()}`;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const finance=await salesServiceProductFinanceSource(database,actor.companyId),before=await product(database,actor,id),beforeMoney=await salesServiceProductMoneySource(database,actor.companyId,id);
  let result:{id:string;version:number};let captured:Awaited<ReturnType<typeof createSalesServiceProductInTransaction>>['money']=null;
  if(originalReceipt){const replay=await replayCommand<{id:string;version:number}>(database,actor,key,operation,id,input);if(!replay||!before||JSON.parse(before).item_type!=='service'||JSON.parse(before).tracking_mode!=='none')throw changed();result=replay;}
  else{
   if(!finance.effective)throw new ApiError(409,'COMMERCE_SETUP_REQUIRED','Prepare and independently approve service Sales posting defaults first.');if(before||beforeMoney.length)throw changed();
   const created=await createSalesServiceProductInTransaction(database,actor,input,key);result=created.result;captured=created.money;
   if(!matches(await product(database,actor,id),actor,input,id)||!captured||!salesServiceProductMoneyMatches(await salesServiceProductMoneySource(database,actor.companyId,id),actor.companyId,id,captured))throw changed();
  }
  if(result.id!==id||result.version!==1)throw changed();const savedProduct=await product(database,actor,id),savedMoney=await salesServiceProductMoneySource(database,actor.companyId,id),savedReceipt=await receipt(database,actor,key,id);if(!savedReceipt)throw changed();
  const bornReplay=await replayCommand<{id:string;version:number}>(database,actor,key,operation,id,input);if(!bornReplay||bornReplay.id!==id||bornReplay.version!==1||savedReceipt!==await receipt(database,actor,key,id))throw changed();
  const native=JSON.parse(savedReceipt) as {request_hash:string;result_json:string},nativeResult=JSON.parse(native.result_json) as {id?:unknown;version?:unknown};if(!native.request_hash||nativeResult.id!==id||nativeResult.version!==1)throw changed();
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));await flushFinanceDeferredEffects(database);
  if(savedProduct!==await product(database,actor,id)||savedReceipt!==await receipt(database,actor,key,id)||JSON.stringify(savedMoney)!==JSON.stringify(await salesServiceProductMoneySource(database,actor.companyId,id))||finance.image!==(await salesServiceProductFinanceSource(database,actor.companyId)).image)throw changed();
  if(originalReceipt&&(before!==savedProduct||JSON.stringify(beforeMoney)!==JSON.stringify(savedMoney)))throw changed();
  if(!originalReceipt&&(!matches(savedProduct,actor,input,id)||!captured||!salesServiceProductMoneyMatches(savedMoney,actor.companyId,id,captured)))throw changed();
  if(authorityImage(current)!==authorityImage(await authority(database,actor,true)))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original Sales product assignment changed before completion.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});return {id,version:1};
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
