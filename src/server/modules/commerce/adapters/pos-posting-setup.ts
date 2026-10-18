import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@/lib/identity_access/permissions';
import type {WorkContext} from '../../identity_access/public/authorization';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {holdingResources} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {BusinessRuleError} from '../../shared';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {posStartupCommerceReadLocks} from './pos-posting-setup-sources';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import {executeCommand,replayCommand,assertion,financialConstraintError} from '../../workflow/public/commands';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {requirePosStartupFinanceAuthority,posStartupFinanceReadLocks,posStartupFinanceState,validatePosStartupFinanceState,posStartupEffectiveService} from '../../finance/public/pos-posting-setup';
import {posStartupSourceReadLocks,posStartupSourceState} from '../../pos/public/startup-sources';
import {posStartupInventorySourceLocks,posStartupInventoryState} from '../../inventory/public/pos-startup-sources';
import {posStartupDocumentState} from './pos-posting-setup-sources';
import {createPosPostingSetup} from '../application/pos-posting-setup';
import {posPostingSetupSchema,type PosPostingSetupInput,type PosPostingSetupResult,type PosPostingSetupView} from '../domain/pos-posting-setup';
import type {CommercePostingSetupConfiguration} from '../domain/posting-setup';

const operation='commerce.pos-posting-setup.initialize',capabilities=['pos.sales','inventory.stock-core','finance.pos-posting'] as const;
const columns={chartId:'chart_id',currency:'currency',receivableAccountId:'receivable_account_id',payableAccountId:'payable_account_id',revenueAccountId:'revenue_account_id',inventoryAccountId:'inventory_account_id',clearingAccountId:'clearing_account_id',cogsAccountId:'cogs_account_id',inputTaxAccountId:'input_tax_account_id',outputTaxAccountId:'output_tax_account_id',cashAccountId:'cash_account_id'} as const;
const fields=Object.keys(columns) as Array<keyof typeof columns>;
function mapping(input:PosPostingSetupInput):CommercePostingSetupConfiguration{return Object.fromEntries(fields.map(field=>[field,input[field]])) as CommercePostingSetupConfiguration;}
async function current(database:SqlDatabase,companyId:string){return database.prepare(`SELECT ${fields.map(field=>`${columns[field]} "${field}"`).join(',')} FROM commerce_settings WHERE company_id=?`).bind(companyId).first<CommercePostingSetupConfiguration>();}
function authorityImage(actor:WorkContext){return JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);}
async function owner(actor:WorkContext){const database=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(database))throw new ApiError(503,'TRANSACTION_REQUIRED','POS posting setup requires the original owned Holding transaction.');return database;}
async function leases(database:SqlDatabase,companyId:string,write=false){
 const purchase=companyPurchaseReadLocks(database,companyId);
 if(write){await lockPackageSettlementSources(database);await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();await database.prepare('LOCK TABLE commerce_settings IN SHARE ROW EXCLUSIVE MODE').run();}
 else await database.batch([purchase[0]]);
 await database.batch([...posStartupCommerceReadLocks(database),...posStartupSourceReadLocks(database),...posStartupInventorySourceLocks(database),...posStartupFinanceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);if(write)await database.batch(purchase.slice(1));
}

async function sources(database:SqlDatabase,actor:WorkContext){
 const [finance,documents,production,stock]=await Promise.all([posStartupFinanceState(database,actor),posStartupDocumentState(database,actor.companyId),posStartupSourceState(database,actor.companyId),posStartupInventoryState(database,actor.companyId)]);
 return {finance,documents,production,stock,image:JSON.stringify([finance.image,documents.image,production.image,stock.image])};
}
type Sources=Awaited<ReturnType<typeof sources>>;
function contextKey(actor:WorkContext,state:Sources,configuration:CommercePostingSetupConfiguration|null){return createHash('sha256').update(JSON.stringify([authorityImage(actor),state.image,state.finance.expectedSalesDefaults,configuration])).digest('hex');}
function refusal(state:Sources){return state.documents.goods||state.production.economics||state.stock.economics||state.finance.historyInvalid?'history-requires-review' as const:state.finance.defaultsBlocked||state.documents.history&&!state.finance.defaults?'sales-defaults-required' as const:null;}
const unavailable=(companyId:string,unavailableReason:'authority-required'|'purchase-required'):PosPostingSetupView=>({companyId,available:false,configured:null,version:null,canConfigure:false,contextKey:null,configuration:null,charts:[],accounts:[],expectedSalesDefaults:null,requiresApprovedDefaults:false,unsupportedExistingMapping:false,unavailableReason});
export async function posPostingSetup(actor:WorkContext):Promise<PosPostingSetupView>{
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read')||!hasPermission(actor,'finance.write'))return unavailable(actor.companyId,'authority-required');
 return (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const authority=await requirePosStartupFinanceAuthority(database,actor),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return unavailable(actor.companyId,'purchase-required');
  const configuration=await current(database,actor.companyId),state=await sources(database,actor),unsupportedExistingMapping=!!configuration&&(state.finance.references.charts.find(row=>row.id===configuration.chartId)?.base_currency!==configuration.currency),reason=configuration?null:refusal(state);
  if(authorityImage(authority)!==authorityImage(await requirePosStartupFinanceAuthority(database,actor)))throw new ApiError(409,'POS_POSTING_CONTEXT_CHANGED','Current Company Finance authority changed while reading POS posting setup.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {companyId:actor.companyId,available:true,configured:!!configuration,version:configuration?1:0,unsupportedExistingMapping,canConfigure:!configuration&&!reason&&capabilities.every(capability=>access[capability].newWork),contextKey:contextKey(authority,state,configuration),configuration,expectedSalesDefaults:state.finance.expectedSalesDefaults,requiresApprovedDefaults:state.documents.history,
   charts:state.finance.references.charts.filter(row=>row.status==='active').map(row=>({id:row.id,name:row.name,baseCurrency:row.base_currency})),accounts:state.finance.references.accounts.filter(row=>row.status==='active'&&typeof row.currency==='string').map(row=>({id:row.id,chartId:row.chart_id,code:row.code,name:row.name,category:row.category,currency:row.currency,controlKind:row.control_kind})),unavailableReason:reason};
 });
}
export async function initializePosPostingSetup(actor:WorkContext,raw:unknown,key:string):Promise<PosPostingSetupResult>{
 const input=posPostingSetupSchema.parse(raw);
 try{return await (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId,true);const authority=await requirePosStartupFinanceAuthority(database,actor),receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.companyId,actor.userId,actor.companyId).first(),purpose=receipt?'history' as const:'new-work' as const;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const guard=`pos-posting:${randomUUID()}`,boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  await database.batch(boundaries.flatMap(item=>item.boundary.before));const configuration=await current(database,actor.companyId),before=await sources(database,actor);let applied=false;
  const run=createPosPostingSetup({
   replay:(body,command)=>replayCommand<PosPostingSetupResult>(database,actor,command,operation,actor.companyId,body),configured:async()=>!!configuration,contextKey:()=>contextKey(authority,before,configuration),
   assertNoEconomics:async()=>{if(refusal(before)==='history-requires-review')throw new ApiError(409,'POS_POSTING_HISTORY_REQUIRES_REVIEW','Original Stock/POS economics or invalid financial lineage require their original owner reconciliation. Initial setup cannot rebind them.');},
   validate:body=>validatePosStartupFinanceState(database,actor,body,before.documents.history).then(()=>undefined),
   commit:async(body,command)=>{applied=true;return executeCommand(database,actor,command,operation,body,actor.companyId,1,{id:actor.companyId,status:'configured' as const,version:1 as const},[
    assertion(database,command,'pos-posting-absent','NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)',[actor.companyId]),
    database.prepare(`INSERT INTO commerce_settings(company_id,${fields.map(field=>columns[field]).join(',')}) VALUES(${Array.from({length:fields.length+1},()=>'?').join(',')})`).bind(actor.companyId,...fields.map(field=>body[field])),
   ]);},
  });
  const result=await run(input,key);
  // Original boundary effects/cleanup and every deferred owner write finish
  // before the final source/current-authority/purchase reads; no later writes.
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));await flushFinanceDeferredEffects(database);
  if(authorityImage(authority)!==authorityImage(await requirePosStartupFinanceAuthority(database,actor)))throw new ApiError(409,'POS_POSTING_CONTEXT_CHANGED','Current Company Finance authority changed before initial posting setup completed.');
  if(JSON.stringify(await current(database,actor.companyId))!==JSON.stringify(mapping(input)))throw new ApiError(409,'POS_POSTING_SOURCE_CHANGED','The original Company posting mapping changed before completion.');
  if(applied){const after=await sources(database,actor);if(after.image!==before.image||refusal(after)==='history-requires-review')throw new ApiError(409,'POS_POSTING_SOURCE_CHANGED','Original POS/Stock/Finance sources changed after initial setup effects.');await validatePosStartupFinanceState(database,actor,input,before.documents.history);
   if(before.finance.service&&JSON.stringify(await posStartupEffectiveService(database,actor))!==JSON.stringify(before.finance.service))throw new ApiError(409,'POS_POSTING_SOURCE_CHANGED','Initial POS setup cannot alter the original reviewed service posting semantics.');}
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});return result;
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
