import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {executeCommand,replayCommand,financialConstraintError} from '@erpx/server-workflow/public/commands';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {requireManufacturingPostingFinanceAuthority,manufacturingPostingFinanceReadLocks,manufacturingPostingFinanceState} from '@erpx/server-finance/public/manufacturing-posting-setup';
import {validateManufacturingExistingPostingFinanceState,manufacturingExistingPostingFinancialHistory} from '@erpx/server-finance/public/manufacturing-existing-posting-setup';
import {manufacturingPostingSourceReadLocks,manufacturingPostingSourceState} from '@erpx/server-manufacturing/public/posting-setup-sources';
import {manufacturingPostingInventorySourceLocks,manufacturingPostingInventoryState} from '@erpx/server-inventory/public/manufacturing-posting-setup-sources';
import {manufacturingPostingCommerceReadLocks,manufacturingPostingMappingState,manufacturingPostingDocumentState} from '@erpx/server-commerce/adapters/manufacturing-posting-setup-sources';
import {manufacturingExistingPostingMarker} from '@erpx/server-commerce/adapters/manufacturing-existing-posting-proof';
import {manufacturingExistingPostingSetupSchema,type ManufacturingExistingPostingSetupResult,type ManufacturingExistingPostingSetupView} from '@erpx/server-commerce/domain/manufacturing-existing-posting-setup';

const operation='commerce.manufacturing-posting-setup.confirm-existing',capabilities=['manufacturing.materials','manufacturing.costs','inventory.stock-core','finance.production-costing'] as const;
type StoredResult=ManufacturingExistingPostingSetupResult&{_manufacturingExistingPostingSetup:Record<string,unknown>};
function changed(){return new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','The original mapping, reviewed defaults or production/Stock/Finance source changed. Review the unchanged mapping again.');}
function authorityImage(actor:WorkContext){return JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);}
async function owner(actor:WorkContext){const database=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(database))throw new ApiError(503,'TRANSACTION_REQUIRED','Existing Manufacturing posting confirmation requires its original Holding transaction.');return database;}
async function leases(database:SqlDatabase,companyId:string,write=false){
 const purchase=companyPurchaseReadLocks(database,companyId);
 if(write){await lockPackageSettlementSources(database);await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();}else await database.batch([purchase[0]]);
 await database.batch([...manufacturingPostingCommerceReadLocks(database),...manufacturingPostingSourceReadLocks(database,false),...manufacturingPostingInventorySourceLocks(database),...manufacturingPostingFinanceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);if(write)await database.batch(purchase.slice(1));
}
async function schema(database:SqlDatabase){return !!await database.prepare("SELECT to_regclass('commerce_mfg_existing_posting_confirmations') IS NOT NULL ready").first<boolean>('ready');}
async function snapshot(database:SqlDatabase,actor:WorkContext){
 const [configuration,mapping,finance,documents,production,stock,financialHistory]=await Promise.all([
  manufacturingPostingMappingState(database,actor.companyId),database.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(actor.companyId).first<string>('image'),
  manufacturingPostingFinanceState(database,actor),manufacturingPostingDocumentState(database,actor.companyId),manufacturingPostingSourceState(database,actor.companyId),manufacturingPostingInventoryState(database,actor.companyId),
  manufacturingExistingPostingFinancialHistory(database,actor),
 ]);
 return {configuration,mapping,finance,image:JSON.stringify([mapping,finance.image,documents.image,production.image,stock.image,financialHistory.image])};
}
async function receiptImage(database:SqlDatabase,companyId:string,key?:string){return database.prepare(`SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.operation=? AND r.record_id=? AND r.authority_json::jsonb->>'companyId'=? AND (?::text IS NULL OR r.command_id=?) ORDER BY r.created_at,r.command_id LIMIT 1`).bind(operation,companyId,companyId,key??null,key??null).first<string>('image');}
async function nativeImage(database:SqlDatabase,companyId:string){return database.prepare('SELECT to_jsonb(s)::text image FROM commerce_mfg_existing_posting_confirmations s WHERE company_id=?').bind(companyId).first<string>('image');}
function contextKey(actor:WorkContext,state:Awaited<ReturnType<typeof snapshot>>,confirmation:string|null){return createHash('sha256').update(JSON.stringify([authorityImage(actor),state.image,confirmation])).digest('hex');}
const unavailable=(companyId:string,unavailableReason:ManufacturingExistingPostingSetupView['unavailableReason']):ManufacturingExistingPostingSetupView=>({companyId,available:false,schemaAvailable:false,configured:null,confirmed:null,version:null,canConfirm:false,contextKey:null,configuration:null,previewChart:null,previewAccounts:[],effectiveSalesConfiguration:null,expectedSalesDefaults:null,unavailableReason});
export async function manufacturingExistingPostingSetup(actor:WorkContext):Promise<ManufacturingExistingPostingSetupView>{
 if(actor.beta||actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read')||!hasPermission(actor,'finance.write'))return unavailable(actor.companyId,'authority-required');
 return (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const authority=await requireManufacturingPostingFinanceAuthority(database,actor),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return unavailable(actor.companyId,'purchase-required');
  const ready=await schema(database),state=await snapshot(database,actor),confirmation=await receiptImage(database,actor.companyId),initialized=!!await database.prepare("SELECT 1 FROM command_receipts WHERE operation='commerce.manufacturing-posting-setup.initialize' AND record_id=? AND authority_json::jsonb->>'companyId'=?").bind(actor.companyId,actor.companyId).first();
  let reason:ManufacturingExistingPostingSetupView['unavailableReason']=!ready?'schema-required':!state.configuration?'mapping-required':initialized?'already-manufacturing-configured':null;
  if(state.configuration&&!confirmation&&!reason){try{await validateManufacturingExistingPostingFinanceState(database,actor,state.configuration);}catch(error){if(error instanceof ApiError&&[409,422].includes(error.status))reason='invalid-existing-mapping';else throw error;}}
  if(authorityImage(authority)!==authorityImage(await requireManufacturingPostingFinanceAuthority(database,actor)))throw changed();
  if(state.image!==(await snapshot(database,actor)).image||confirmation!==await receiptImage(database,actor.companyId))throw changed();
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  const chart=state.finance.references.charts.find(row=>row.id===state.configuration?.chartId),selected=new Set(state.configuration?Object.entries(state.configuration).filter(([field])=>field.endsWith('AccountId')).map(([,value])=>value):[]),service=state.finance.service;
  return {companyId:actor.companyId,available:true,schemaAvailable:ready,configured:!!state.configuration,confirmed:!!confirmation,version:confirmation?1:0,canConfirm:!confirmation&&!reason&&capabilities.every(capability=>access[capability].newWork),contextKey:contextKey(authority,state,confirmation),configuration:state.configuration,previewChart:chart?{id:chart.id,name:chart.name,baseCurrency:chart.base_currency}:null,previewAccounts:state.finance.references.accounts.filter(row=>selected.has(row.id)).map(row=>({id:row.id,code:row.code,name:row.name,currency:row.currency})),effectiveSalesConfiguration:service?{chartId:String(service.chart_id),currency:String(service.currency),receivableAccountId:String(service.receivable_account_id),revenueAccountId:String(service.revenue_account_id),outputTaxAccountId:service.output_tax_account_id===null?null:String(service.output_tax_account_id)}:null,expectedSalesDefaults:state.finance.expectedSalesDefaults,unavailableReason:reason};
 });
}
export async function confirmExistingManufacturingPostingSetup(actor:WorkContext,raw:unknown,key:string):Promise<ManufacturingExistingPostingSetupResult>{
 const input=manufacturingExistingPostingSetupSchema.parse(raw);
 try{return await (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId,true);const authority=await requireManufacturingPostingFinanceAuthority(database,actor);
  if(!await schema(database))throw new ApiError(503,'MANUFACTURING_EXISTING_POSTING_SCHEMA_REQUIRED','Install the reviewed existing-mapping confirmation schema before confirming.');
  await database.prepare('LOCK TABLE commerce_mfg_existing_posting_confirmations IN SHARE ROW EXCLUSIVE MODE').run();
  const prior=await receiptImage(database,actor.companyId,key),purpose=prior?'history' as const:'new-work' as const;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const guard=`manufacturing-existing-posting:${randomUUID()}`,boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const before=await snapshot(database,actor),confirmation=await receiptImage(database,actor.companyId);let result:StoredResult;
  if(prior){const replay=await replayCommand<StoredResult>(database,actor,key,operation,actor.companyId,input);if(!replay)throw changed();result=replay;}
  else{
   if(confirmation)throw new ApiError(409,'MANUFACTURING_EXISTING_POSTING_ALREADY_CONFIRMED','Retain the original existing-mapping confirmation.');
   if(!before.configuration||!before.mapping)throw new ApiError(409,'MANUFACTURING_EXISTING_POSTING_MAPPING_REQUIRED','Configure the original nine real posting accounts first.');
   if(await database.prepare("SELECT 1 FROM command_receipts WHERE operation='commerce.manufacturing-posting-setup.initialize' AND record_id=? AND authority_json::jsonb->>'companyId'=?").bind(actor.companyId,actor.companyId).first())throw new ApiError(409,'MANUFACTURING_EXISTING_POSTING_ALREADY_CONFIRMED','The original Manufacturing initialization already supplies its source.');
   if(input.contextKey!==contextKey(authority,before,confirmation))throw new ApiError(409,'MANUFACTURING_POSTING_CONTEXT_CHANGED','Keep the draft and preview the actual existing mapping again.');
   await validateManufacturingExistingPostingFinanceState(database,actor,before.configuration);
   const proof={kind:'manufacturing-existing-posting-setup',companyId:actor.companyId,confirmationCommandId:key,mapping:JSON.parse(before.mapping) as Record<string,unknown>,expectedSalesDefaults:before.finance.expectedSalesDefaults,effectiveService:before.finance.service};
   result=await executeCommand(database,actor,key,operation,input,actor.companyId,1,{id:actor.companyId,status:'confirmed-existing' as const,version:1 as const,[manufacturingExistingPostingMarker]:proof},[
    database.prepare('INSERT INTO commerce_mfg_existing_posting_confirmations(company_id,command_id,request_hash,chart_id,inventory_account_id,version,mapping_json,proof_json,created_by,created_at) SELECT ?,r.command_id,r.request_hash,?,?,1,?::jsonb,?::jsonb,?,? FROM command_receipts r WHERE r.command_id=? AND r.operation=? AND r.record_id=?').bind(actor.companyId,before.configuration.chartId,before.configuration.inventoryAccountId,before.mapping,JSON.stringify(proof),actor.userId,Date.now(),key,operation,actor.companyId),
   ]);
  }
  const expectedReceipt=await receiptImage(database,actor.companyId,key),expectedNative=await nativeImage(database,actor.companyId);
  if(!expectedReceipt||!expectedNative||result.id!==actor.companyId||result.status!=='confirmed-existing'||result.version!==1)throw changed();
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));await flushFinanceDeferredEffects(database);
  if(authorityImage(authority)!==authorityImage(await requireManufacturingPostingFinanceAuthority(database,actor))||before.image!==(await snapshot(database,actor)).image||expectedReceipt!==await receiptImage(database,actor.companyId,key)||expectedNative!==await nativeImage(database,actor.companyId))throw changed();
  if(!prior)await validateManufacturingExistingPostingFinanceState(database,actor,before.configuration!);
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  return {id:result.id,status:result.status,version:result.version};
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
