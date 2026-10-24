import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {replayCommand} from '@erpx/server-workflow/public/commands';
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {manufacturingPostingCommerceReadLocks} from '@erpx/server-commerce/public/manufacturing-posting-setup-sources';
import {manufacturingPostingInventorySourceLocks} from '@erpx/server-inventory/public/manufacturing-posting-setup-sources';
import {requireManufacturingPostingFinanceAuthority,manufacturingPostingFinanceReadLocks,manufacturingPostingProfileReferenceSnapshot} from '@erpx/server-finance/public/manufacturing-posting-setup';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {manufacturingPostingSourceReadLocks} from '@erpx/server-manufacturing/adapters/posting-setup-sources';
import type {ManufacturingPostingProfilesView,ManufacturingPostingProfileInput,ManufacturingPostingProfileDecisionInput} from '@erpx/server-manufacturing/domain/posting-profiles';
type NativeProfile={id:string;company_id:string;chart_id:string;currency:string;inventory_account_id:string;wip_account_id:string;labor_account_id:string;overhead_account_id:string;scrap_account_id:string;status:'draft'|'approved'|'rejected'|'retired';version:number;created_by:string;source:string};
type Request=ManufacturingPostingProfileInput|ManufacturingPostingProfileDecisionInput;
const capabilities=['manufacturing.materials','manufacturing.costs','inventory.stock-core','finance.production-costing'] as const;
async function profiles(database:SqlDatabase,companyId:string){return (await database.prepare('SELECT p.*,to_jsonb(p)::text source FROM manufacturing_accounting_profiles p WHERE company_id=? ORDER BY id').bind(companyId).all<NativeProfile>()).results;}
function authorityImage(actor:WorkContext){return JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);}
function contextKey(actor:WorkContext,referenceImage:string,rows:NativeProfile[]){return createHash('sha256').update(JSON.stringify([authorityImage(actor),referenceImage,rows.map(row=>row.source)])).digest('hex');}
async function owner(actor:WorkContext){const db=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(db))throw new ApiError(503,'TRANSACTION_REQUIRED','Production profile continuation requires its owned Holding transaction.');return db;}
async function leases(database:SqlDatabase,companyId:string,write=false){
 const purchase=companyPurchaseReadLocks(database,companyId);if(write){await lockPackageSettlementSources(database);await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();}else await database.batch([purchase[0]]);
 await database.batch(manufacturingPostingCommerceReadLocks(database));
 if(write)await database.prepare('LOCK TABLE manufacturing_accounting_profiles IN SHARE ROW EXCLUSIVE MODE').run();
 await database.batch([...manufacturingPostingSourceReadLocks(database,false),...manufacturingPostingInventorySourceLocks(database),...manufacturingPostingFinanceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);if(write)await database.batch(purchase.slice(1));
}
const unavailable=(companyId:string,unavailableReason:ManufacturingPostingProfilesView['unavailableReason']):ManufacturingPostingProfilesView=>({companyId,available:false,contextKey:null,canPrepare:false,unsupportedExistingMapping:false,currency:null,chartId:null,charts:[],accounts:[],profiles:[],unavailableReason});
export async function manufacturingPostingProfiles(actor:WorkContext):Promise<ManufacturingPostingProfilesView>{
 if(actor.beta||actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read'))return unavailable(actor.companyId,'authority-required');
 return (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const current=await requireManufacturingPostingFinanceAuthority(database,actor,'finance.read'),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return unavailable(actor.companyId,'purchase-required');
  const refs=await manufacturingPostingProfileReferenceSnapshot(database,actor),rows=await profiles(database,actor.companyId),active=capabilities.every(capability=>access[capability].newWork),settle=capabilities.every(capability=>access[capability].settleExisting),supported=refs.state.configured&&!refs.state.unsupportedExistingMapping&&refs.rows.chart.some(row=>row.id===refs.state.chartId&&row.status==='active'),canReview=hasPermission(current,'finance.approve');
  const final=await requireManufacturingPostingFinanceAuthority(database,actor,'finance.read');if(authorityImage(current)!==authorityImage(final))throw new ApiError(409,'MANUFACTURING_POSTING_CONTEXT_CHANGED','Current Company Finance authority changed.');for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {companyId:actor.companyId,available:true,contextKey:contextKey(current,refs.image,rows),canPrepare:supported&&active&&hasPermission(current,'finance.write'),unsupportedExistingMapping:refs.state.unsupportedExistingMapping,currency:supported?'THB':null,chartId:refs.state.chartId,
   charts:refs.rows.chart.filter(row=>row.id===refs.state.chartId||rows.some(profile=>profile.chart_id===row.id)).map(row=>({id:row.id,name:row.name,baseCurrency:row.base_currency})),accounts:refs.rows.account.filter(row=>rows.some(profile=>[profile.inventory_account_id,profile.wip_account_id,profile.labor_account_id,profile.overhead_account_id,profile.scrap_account_id].includes(row.id))||row.chart_id===refs.state.chartId&&row.status==='active'&&row.control_kind==='none'&&['asset','liability','expense','other_expense'].includes(row.category)).map(row=>({id:row.id,chartId:row.chart_id,code:row.code,name:row.name,category:row.category,currency:row.currency,controlKind:row.control_kind,status:row.status})),
   profiles:rows.map(row=>({id:row.id,status:row.status,version:Number(row.version),chartId:row.chart_id,currency:row.currency,inventoryAccountId:row.inventory_account_id,wipAccountId:row.wip_account_id,laborAccountId:row.labor_account_id,overheadAccountId:row.overhead_account_id,scrapAccountId:row.scrap_account_id,canApprove:canReview&&row.created_by!==actor.userId&&row.status==='draft'&&active&&supported&&row.chart_id===refs.state.chartId&&row.inventory_account_id===refs.state.inventoryAccountId,canReject:canReview&&row.created_by!==actor.userId&&row.status==='draft'&&settle,canRetire:canReview&&row.created_by!==actor.userId&&row.status==='approved'&&settle})),unavailableReason:refs.state.unsupportedExistingMapping?'unsupported-existing-mapping':!refs.state.configured?'posting-mapping-required':null};
 });
}
/** Private same-owner composition; original accounting application still owns
 * all creates/reviews, independence, immutable fields, receipts and results. */
export async function withManufacturingPostingProfileCommand<Result>(actor:WorkContext,id:string|null,request:Request,key:string,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 const decision='action' in request.input?request.input:null,operation=decision?`manufacturing.accounting.${decision.action}`:'manufacturing.accounting.create',target=id??`mfg-accounting-${key}`,permission=decision?'finance.approve':'finance.write';
 try{return await (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId,true);const current=await requireManufacturingPostingFinanceAuthority(database,actor,permission),receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,target,actor.userId,actor.companyId).first(),purpose=receipt?'history' as const:decision&&decision.action!=='approve'?'settle-existing' as const:'new-work' as const;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const guard=`manufacturing-profile:${randomUUID()}`,boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const before=await profiles(database,actor.companyId),refs=await manufacturingPostingProfileReferenceSnapshot(database,actor);let result:Result;
  if(receipt){const prior=await replayCommand<Result>(database,actor,key,operation,target,request);if(!prior)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','Original profile receipt unavailable.');result=prior;}
  else{
   if(request.contextKey!==contextKey(current,refs.image,before))throw new ApiError(409,'MANUFACTURING_POSTING_CONTEXT_CHANGED','Keep the draft and review current profile/reference context.');
   if(purpose==='new-work'&&(!refs.state.configured||refs.state.unsupportedExistingMapping))throw new ApiError(422,'MANUFACTURING_CURRENCY_ADAPTER','Configure the actual THB production mapping before preparing or approving its profile.');
   if(decision?.action==='approve'){const selected=before.find(row=>row.id===target);if(selected&&(selected.chart_id!==refs.state.chartId||selected.inventory_account_id!==refs.state.inventoryAccountId))throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','The original profile must retain the actual production chart and inventory mapping.');}
   result=await run(database);
  }
  const written=await profiles(database,actor.companyId);
  if(!receipt){const actual=written.find(row=>row.id===target),prior=before.find(row=>row.id===target);if(!actual)throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','Original profile result missing.');
   if(JSON.stringify(written.filter(row=>row.id!==target).map(row=>row.source))!==JSON.stringify(before.filter(row=>row.id!==target).map(row=>row.source)))throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','An unrelated original profile changed.');
   if(decision){if(!prior||actual.version!==prior.version+1||actual.status!==(decision.action==='approve'?'approved':decision.action==='reject'?'rejected':'retired'))throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','Original profile decision mismatch.');const strip=(row:NativeProfile)=>{const value=JSON.parse(row.source) as Record<string,unknown>;for(const field of ['status','version','reviewed_by','updated_at'])delete value[field];return JSON.stringify(value);};if(strip(prior)!==strip(actual)||JSON.parse(actual.source).reviewed_by!==actor.userId)throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','Original profile identity changed.');}
   else{const input=request.input as ManufacturingPostingProfileInput['input'];if(prior||actual.created_by!==actor.userId||actual.status!=='draft'||actual.version!==1||actual.currency!=='THB'||actual.chart_id!==input.chartId||actual.inventory_account_id!==input.inventoryAccountId||actual.wip_account_id!==input.wipAccountId||actual.labor_account_id!==input.laborAccountId||actual.overhead_account_id!==input.overheadAccountId||actual.scrap_account_id!==input.scrapAccountId)throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','Original profile fields changed.');}
  }
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));await flushFinanceDeferredEffects(database);
  const final=await requireManufacturingPostingFinanceAuthority(database,actor,permission),after=await manufacturingPostingProfileReferenceSnapshot(database,actor);if(authorityImage(current)!==authorityImage(final))throw new ApiError(409,'MANUFACTURING_POSTING_CONTEXT_CHANGED','Current Company Finance authority changed.');
  // The reference snapshot includes profile rows via the private production
  // image: compare every Finance/mapping fact independently of our own row.
  if(JSON.stringify(refs.rows)!==JSON.stringify(after.rows)||refs.state.configured!==after.state.configured||refs.state.chartId!==after.state.chartId||refs.state.unsupportedExistingMapping!==after.state.unsupportedExistingMapping||refs.state.mappingImage!==after.state.mappingImage||refs.state.financialImage!==after.state.financialImage||refs.state.documentImage!==after.state.documentImage||refs.state.stockImage!==after.state.stockImage||refs.state.productionEconomicImage!==after.state.productionEconomicImage)throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','Original production posting references changed.');
  if(JSON.stringify((await profiles(database,actor.companyId)).map(row=>row.source))!==JSON.stringify((receipt?before:written).map(row=>row.source)))throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','Original profile changed after deferred effects.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
