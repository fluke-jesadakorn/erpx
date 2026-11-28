import 'server-only';
import { COMMERCE_POSTING_ABSENT_SQL, COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL } from './guarded-source-dedupe-sql';
import { nativeCommerceReadFirst, RD_POSTING_MAPPING_STATE } from './native-commerce-read';

import { nativeCommerceOwnerWrite, NativeCommerceOwnerWriteError } from "@erpx/commerce-server/adapters/native-owner-write";
async function nativeSettingsWrite(actor: any, input: any, key: string, fields: readonly string[]) {
  try { return await nativeCommerceOwnerWrite('settings.initialize', [actor.companyId, ...fields.map((f: string) => input[f])], actor.companyId, actor.userId, key); }
  catch (error) { if (error instanceof NativeCommerceOwnerWriteError) throw new ApiError(error.status, error.code, error.message); throw error; }
}
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@erpx/identity-access-ui/lib/permissions';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {lockQueuedCommandAuthority} from '@erpx/identity-access-server/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/pms-server/public/scheduled-feature-source-locks';
import {holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {lockPackageSettlementSources} from '@erpx/commercial-server/public/package-locks';
import {posStartupCommerceReadLocks} from '@erpx/commerce-server/adapters/pos-posting-setup-sources';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/commercial-server/public/purchase-command-boundary';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/workflow-server/public/commands';
import {financialConstraintError} from '@erpx/workflow-server/public/command-guards';
import {flushFinanceDeferredEffects} from '@erpx/finance-server/public/finance-transactions';
import {requirePosStartupFinanceAuthority,posStartupFinanceReadLocks,posStartupFinanceState,validatePosStartupFinanceState,posStartupEffectiveService} from '@erpx/finance-server/public/pos-posting-setup';
import {posStartupSourceReadLocks,posStartupSourceState} from '@erpx/pos-server/public/startup-sources';
import {posStartupInventorySourceLocks,posStartupInventoryState} from '@erpx/inventory-server/public/pos-startup-sources';
import {posStartupDocumentState} from '@erpx/commerce-server/adapters/pos-posting-setup-sources';
import {createPosPostingSetup} from '@erpx/commerce-server/application/pos-posting-setup';
import {posPostingSetupSchema,type PosPostingSetupInput,type PosPostingSetupResult,type PosPostingSetupView} from '@erpx/commerce-server/domain/pos-posting-setup';
import type {CommercePostingSetupConfiguration} from '@erpx/commerce-server/domain/posting-setup';
import {clearWorkflowCommandAssertions} from "@erpx/workflow-server/public/command-assertion-effects";

const operation='commerce.pos-posting-setup.initialize',capabilities=['pos.sales','inventory.stock-core','finance.pos-posting'] as const;
const columns={chartId:'chart_id',currency:'currency',receivableAccountId:'receivable_account_id',payableAccountId:'payable_account_id',revenueAccountId:'revenue_account_id',inventoryAccountId:'inventory_account_id',clearingAccountId:'clearing_account_id',cogsAccountId:'cogs_account_id',inputTaxAccountId:'input_tax_account_id',outputTaxAccountId:'output_tax_account_id',cashAccountId:'cash_account_id'} as const;
const fields=Object.keys(columns) as Array<keyof typeof columns>;
function mapping(input:PosPostingSetupInput):CommercePostingSetupConfiguration{return Object.fromEntries(fields.map(field=>[field,input[field]])) as CommercePostingSetupConfiguration;}
async function current(database:SqlDatabase,companyId:string){return nativeCommerceReadFirst<CommercePostingSetupConfiguration>(RD_POSTING_MAPPING_STATE, [companyId], database);}
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
  await leases(database,actor.companyId,true);const authority=await requirePosStartupFinanceAuthority(database,actor),receipt=await database.prepare(COMMERCE_POSTING_RECEIPT_RECORD_GUARD_SQL).bind(key,operation,actor.companyId,actor.userId,actor.companyId).first(),purpose=receipt?'history' as const:'new-work' as const;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const guard=`pos-posting:${randomUUID()}`,boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  await database.batch(boundaries.flatMap(item=>item.boundary.before));const configuration=await current(database,actor.companyId),before=await sources(database,actor);let applied=false;
  const run=createPosPostingSetup({
   replay:(body,command)=>replayCommand<PosPostingSetupResult>(database,actor,command,operation,actor.companyId,body),configured:async()=>!!configuration,contextKey:()=>contextKey(authority,before,configuration),
   assertNoEconomics:async()=>{if(refusal(before)==='history-requires-review')throw new ApiError(409,'POS_POSTING_HISTORY_REQUIRES_REVIEW','Original Stock/POS economics or invalid financial lineage require their original owner reconciliation. Initial setup cannot rebind them.');},
   validate:body=>validatePosStartupFinanceState(database,actor,body,before.documents.history).then(()=>undefined),
   commit:async(body,command)=>{applied=true;await nativeSettingsWrite(actor, body, key, fields);
   return executeCommand(database,actor,command,operation,body,actor.companyId,1,{id:actor.companyId,status:'configured' as const,version:1 as const},[
    assertion(database,command,'pos-posting-absent',COMMERCE_POSTING_ABSENT_SQL,[actor.companyId]),
   ]);},
  });
  const result=await run(input,key);
  // Original boundary effects/cleanup and every deferred owner write finish
  // before the final source/current-authority/purchase reads; no later writes.
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,clearWorkflowCommandAssertions(database, item.key)]));await flushFinanceDeferredEffects(database);
  if(authorityImage(authority)!==authorityImage(await requirePosStartupFinanceAuthority(database,actor)))throw new ApiError(409,'POS_POSTING_CONTEXT_CHANGED','Current Company Finance authority changed before initial posting setup completed.');
  if(JSON.stringify(await current(database,actor.companyId))!==JSON.stringify(mapping(input)))throw new ApiError(409,'POS_POSTING_SOURCE_CHANGED','The original Company posting mapping changed before completion.');
  if(applied){const after=await sources(database,actor);if(after.image!==before.image||refusal(after)==='history-requires-review')throw new ApiError(409,'POS_POSTING_SOURCE_CHANGED','Original POS/Stock/Finance sources changed after initial setup effects.');await validatePosStartupFinanceState(database,actor,input,before.documents.history);
   if(before.finance.service&&JSON.stringify(await posStartupEffectiveService(database,actor))!==JSON.stringify(before.finance.service))throw new ApiError(409,'POS_POSTING_SOURCE_CHANGED','Initial POS setup cannot alter the original reviewed service posting semantics.');}
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});return result;
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
