import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand,financialConstraintError} from '@erpx/server-workflow/public/commands';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {requireSalesGoodsPostingFinanceAuthority,salesGoodsPostingFinanceSourceLocks,salesGoodsPostingFinanceState,validateSalesGoodsPostingFinanceState,salesGoodsPostingEffectiveService} from '@erpx/server-finance/public/sales-goods-posting-setup';
import {commercialOwnerProjectionSources as inventorySources} from '@erpx/server-inventory/public/commercial-owner-projection-sources';
import {commerceCompleteOwnerSourceReadLocks} from '@erpx/server-commerce/adapters/owner-source-locks';
import {salesGoodsPostingSetupSchema,type SalesGoodsPostingSetupInput,type SalesGoodsPostingSetupResult,type SalesGoodsPostingSetupView} from '@erpx/server-commerce/domain/sales-goods-posting-setup';
import type {CommercePostingSetupConfiguration} from '@erpx/server-commerce/domain/posting-setup';

export const salesGoodsPostingFulfillmentSources="SELECT f.id,f.invoice_id,o.company_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE o.kind='sales'";
const operation='commerce.sales-goods-posting-setup.initialize';
const capabilities=['sales.orders','inventory.stock-core','finance.sales-posting'] as const;
const columns={chartId:'chart_id',currency:'currency',receivableAccountId:'receivable_account_id',payableAccountId:'payable_account_id',revenueAccountId:'revenue_account_id',inventoryAccountId:'inventory_account_id',clearingAccountId:'clearing_account_id',cogsAccountId:'cogs_account_id',inputTaxAccountId:'input_tax_account_id',outputTaxAccountId:'output_tax_account_id',cashAccountId:'cash_account_id'} as const;
const fields=Object.keys(columns) as Array<keyof typeof columns>;
async function current(database:SqlDatabase,companyId:string){return database.prepare(`SELECT ${fields.map(field=>`${columns[field]} "${field}"`).join(',')} FROM commerce_settings WHERE company_id=?`).bind(companyId).first<CommercePostingSetupConfiguration>();}
function mapping(input:SalesGoodsPostingSetupInput):CommercePostingSetupConfiguration{return Object.fromEntries(fields.map(field=>[field,input[field]])) as CommercePostingSetupConfiguration;}
function sameMapping(a:CommercePostingSetupConfiguration|null,b:CommercePostingSetupConfiguration){return !!a&&fields.every(field=>a[field]===b[field]);}
/** Native Commerce intent and economics stay separate: an unfulfilled order
 * does not invent prior goods valuation. Original Supply's ANY-history rule
 * is unchanged; this is a distinct Sales owner decision. */
async function documentState(database:SqlDatabase,companyId:string){
 const row=await database.prepare(`WITH scope AS(SELECT ?::text company_id),warehouses AS MATERIALIZED(SELECT w.id FROM commerce_warehouses w,scope WHERE w.company_id=scope.company_id),orders AS MATERIALIZED(SELECT o.* FROM commerce_orders o,scope WHERE o.company_id=scope.company_id),fulfillments AS MATERIALIZED(SELECT f.* FROM commerce_fulfillments f JOIN orders o ON o.id=f.order_id),lines AS MATERIALIZED(SELECT l.* FROM commerce_order_lines l JOIN orders o ON o.id=l.order_id)
 SELECT EXISTS(SELECT 1 FROM orders) history,
 (EXISTS(SELECT 1 FROM commerce_fulfillment_lines fl JOIN fulfillments f ON f.id=fl.fulfillment_id JOIN lines l ON l.id=fl.order_line_id WHERE l.item_type='stock')
 OR EXISTS(SELECT 1 FROM commerce_goods_valuations v,scope WHERE v.company_id=scope.company_id)
 OR EXISTS(SELECT 1 FROM commerce_stock_moves m JOIN warehouses w ON w.id=m.warehouse_id)
 OR EXISTS(SELECT 1 FROM (${inventorySources.commerce_stock}) s JOIN warehouses w ON w.id=s.warehouse_id WHERE s.quantity_milli<>0 OR s.value_minor<>0)
 OR EXISTS(SELECT 1 FROM (${inventorySources.inventory_ledger}) l JOIN warehouses w ON w.id=l.warehouse_id)
 OR EXISTS(SELECT 1 FROM (${inventorySources.inventory_lot_balances}) b JOIN warehouses w ON w.id=b.warehouse_id WHERE b.quantity_milli<>0 OR b.value_minor<>0)) goods,
 jsonb_build_array(
 (SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM orders o),
 (SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM lines l),
 (SELECT jsonb_agg(to_jsonb(f) ORDER BY f.id) FROM fulfillments f),
 (SELECT jsonb_agg(to_jsonb(fl) ORDER BY fl.id) FROM commerce_fulfillment_lines fl JOIN fulfillments f ON f.id=fl.fulfillment_id),
 (SELECT jsonb_agg(to_jsonb(v) ORDER BY v.fulfillment_id) FROM commerce_goods_valuations v,scope WHERE v.company_id=scope.company_id),
 (SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM commerce_stock_moves m JOIN warehouses w ON w.id=m.warehouse_id),
 (SELECT jsonb_agg(to_jsonb(s) ORDER BY s.warehouse_id,s.product_id) FROM (${inventorySources.commerce_stock}) s JOIN warehouses w ON w.id=s.warehouse_id),
 (SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM (${inventorySources.inventory_ledger}) l JOIN warehouses w ON w.id=l.warehouse_id),
 (SELECT jsonb_agg(to_jsonb(b) ORDER BY b.warehouse_id,b.lot_id) FROM (${inventorySources.inventory_lot_balances}) b JOIN warehouses w ON w.id=b.warehouse_id))::text image`).bind(companyId).first<{history:boolean;goods:boolean;image:string}>();
 if(!row)throw new ApiError(503,'SALES_GOODS_POSTING_SOURCE_UNAVAILABLE','The original Sales posting source is unavailable.');return row;
}
function authorityImage(actor:WorkContext){return JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);}
function contextKey(actor:WorkContext,finance:Awaited<ReturnType<typeof salesGoodsPostingFinanceState>>,documents:Awaited<ReturnType<typeof documentState>>,configuration:CommercePostingSetupConfiguration|null){return createHash('sha256').update(JSON.stringify([authorityImage(actor),finance.image,finance.service,finance.expectedSalesDefaults,documents,configuration])).digest('hex');}
async function owner(actor:WorkContext){const write=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED','Sales goods setup requires its original owned Holding transaction.');return write;}
async function leases(database:SqlDatabase,companyId:string){
 const purchase=companyPurchaseReadLocks(database,companyId);
 await database.batch([purchase[0],...commerceCompleteOwnerSourceReadLocks(database),database.prepare('LOCK TABLE commerce_settings IN SHARE ROW EXCLUSIVE MODE'),...salesGoodsPostingFinanceSourceLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
}
const unavailable=(companyId:string,reason:'authority-required'|'purchase-required'):SalesGoodsPostingSetupView=>({companyId,available:false,configured:null,version:null,canConfigure:false,contextKey:null,configuration:null,charts:[],accounts:[],expectedSalesDefaults:null,requiresApprovedDefaults:false,unavailableReason:reason});
export async function salesGoodsPostingSetup(actor:WorkContext):Promise<SalesGoodsPostingSetupView>{
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read')||!hasPermission(actor,'finance.write'))return unavailable(actor.companyId,'authority-required');
 return (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const authority=await requireSalesGoodsPostingFinanceAuthority(database,actor),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return unavailable(actor.companyId,'purchase-required');
  const configuration=await current(database,actor.companyId),finance=await salesGoodsPostingFinanceState(database,actor),documents=await documentState(database,actor.companyId);
  const reason=configuration?null:documents.goods||finance.historyInvalid?'goods-history-requires-review' as const:finance.defaultsBlocked||documents.history&&!finance.defaults?'sales-defaults-required' as const:null;
  if(authorityImage(authority)!==authorityImage(await requireSalesGoodsPostingFinanceAuthority(database,actor)))throw new ApiError(409,'SALES_GOODS_POSTING_CONTEXT_CHANGED','The selected Finance authority changed during Sales goods setup discovery.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {companyId:actor.companyId,available:true,configured:!!configuration,version:configuration?1:0,canConfigure:!configuration&&!reason&&capabilities.every(capability=>access[capability].newWork),contextKey:contextKey(authority,finance,documents,configuration),configuration,expectedSalesDefaults:finance.expectedSalesDefaults,requiresApprovedDefaults:documents.history,
   charts:finance.references.charts.filter(row=>row.status==='active').map(row=>({id:row.id,name:row.name,baseCurrency:row.base_currency})),accounts:finance.references.accounts.filter(row=>row.status==='active').map(row=>({id:row.id,chartId:row.chart_id,code:row.code,name:row.name,category:row.category,currency:row.currency,controlKind:row.control_kind})),unavailableReason:reason};
 });
}
export async function initializeSalesGoodsPostingSetup(actor:WorkContext,raw:unknown,key:string):Promise<SalesGoodsPostingSetupResult>{
 const input=salesGoodsPostingSetupSchema.parse(raw);
 try{return await (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const authority=await requireSalesGoodsPostingFinanceAuthority(database,actor);
  const receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId).first(),purpose=receipt?'history' as const:'new-work' as const;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const guard=`sales-goods-posting:${randomUUID()}`,boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const configuration=await current(database,actor.companyId),finance=await salesGoodsPostingFinanceState(database,actor),documents=await documentState(database,actor.companyId);
  const replay=await replayCommand<SalesGoodsPostingSetupResult>(database,actor,key,operation,actor.companyId,input);let result:SalesGoodsPostingSetupResult;
  if(replay)result=replay;
  else{
   if(configuration)throw new ApiError(409,'COMMERCE_POSTING_ALREADY_CONFIGURED','The existing Company posting mapping is retained. Initial setup cannot replace it.');
   if(input.contextKey!==contextKey(authority,finance,documents,configuration))throw new ApiError(409,'SALES_GOODS_POSTING_CONTEXT_CHANGED','The selected context, reviewed defaults or original documents changed. Keep the draft and review the current setup.');
   if(documents.goods)throw new ApiError(409,'SALES_GOODS_POSTING_HISTORY_REQUIRES_REVIEW','Goods economics without their original mapping require owner reconciliation. Initial Sales setup cannot repair them.');
   await validateSalesGoodsPostingFinanceState(database,actor,input,documents.history);
   result=await executeCommand(database,actor,key,operation,input,actor.companyId,1,{id:actor.companyId,status:'configured' as const,version:1 as const},[
    assertion(database,key,'sales-goods-posting-absent','NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)',[actor.companyId]),
    database.prepare(`INSERT INTO commerce_settings(company_id,${fields.map(field=>columns[field]).join(',')}) VALUES(${Array.from({length:fields.length+1},()=>'?').join(',')})`).bind(actor.companyId,...fields.map(field=>input[field])),
   ]);
  }
  await flushFinanceDeferredEffects(database);
  if(authorityImage(authority)!==authorityImage(await requireSalesGoodsPostingFinanceAuthority(database,actor)))throw new ApiError(409,'SALES_GOODS_POSTING_CONTEXT_CHANGED','The selected Finance authority changed before Sales goods setup completed.');
  if(!sameMapping(await current(database,actor.companyId),mapping(input)))throw new ApiError(409,'SALES_GOODS_POSTING_SOURCE_CHANGED','The original goods posting mapping changed before completion.');
  const finalFinance=await salesGoodsPostingFinanceState(database,actor),finalDocuments=await documentState(database,actor.companyId);
  if(finance.image!==finalFinance.image||documents.image!==finalDocuments.image||!replay&&finance.defaults&&JSON.stringify(finance.service)!==JSON.stringify(await salesGoodsPostingEffectiveService(database,actor)))throw new ApiError(409,'SALES_GOODS_POSTING_SOURCE_CHANGED','The original reviewed Sales settings or document economics changed before completion.');
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

// Closed owner-local source reuse; no additional purchase or command port.
export {documentState as manufacturingPostingDocumentState};
