import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';
import {hasPermission} from '@/lib/identity_access/permissions';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {loadIamAuthority} from '../../identity_access/public/iam-store';
import {assignmentReadSql} from '../../identity_access/public/record-source-authority';
import {platformOwnerProjectionSources as identitySources} from '../../identity_access/public/platform-owner-projection-sources';
import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {holdingResources} from '../../runtime/public/runtime';
import {ApiError} from '../../runtime/public/http';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '../../commercial/public/purchased-capability-sql';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {requireCommercePostingFinanceAuthority,commercePostingFinanceSourceReadLocks,commercePostingFinanceReferences,validateCommercePostingFinanceReferences} from '../../finance/public/commerce-posting-setup';
import {executeCommand,replayCommand,assertion,financialConstraintError} from '../../workflow/public/commands';
import {BusinessRuleError} from '../../shared';
import {commerceCompleteOwnerSourceReadLocks} from './owner-source-locks';
import {commercePostingPrerequisiteState} from './posting-prerequisite-sources';
import {createCommercePostingSetup} from '../application/posting-setup';
import {commercePostingSetupSchema,type CommercePostingSetupInput,type CommercePostingSetupConfiguration,type CommercePostingSetupResult,type CommercePostingSetupView} from '../domain/posting-setup';

const operation='commerce.posting-setup.initialize',capabilities=['supply.receiving','finance.inventory-valuation'] as const;
const columns={chartId:'chart_id',currency:'currency',receivableAccountId:'receivable_account_id',payableAccountId:'payable_account_id',revenueAccountId:'revenue_account_id',inventoryAccountId:'inventory_account_id',clearingAccountId:'clearing_account_id',cogsAccountId:'cogs_account_id',inputTaxAccountId:'input_tax_account_id',outputTaxAccountId:'output_tax_account_id',cashAccountId:'cash_account_id'} as const;
const fields=Object.keys(columns) as Array<keyof typeof columns>;
async function current(database:SqlDatabase,companyId:string):Promise<CommercePostingSetupConfiguration|null>{
 const row=await database.prepare(`SELECT ${fields.map(field=>`${columns[field]} "${field}"`).join(',')} FROM commerce_settings WHERE company_id=?`).bind(companyId).first<CommercePostingSetupConfiguration>();return row;
}
function mapping(input:CommercePostingSetupInput):CommercePostingSetupConfiguration{return Object.fromEntries(fields.map(field=>[field,input[field]])) as CommercePostingSetupConfiguration;}
function sameMapping(a:CommercePostingSetupConfiguration|null,b:CommercePostingSetupConfiguration){return a!==null&&fields.every(field=>a[field]===b[field]);}
function contextKey(actor:WorkContext,image:string){return createHash('sha256').update(JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam?.queuedAssignmentVersion??null,actor.iam?.identityVersion??null,actor.iam?.roleRevision??null,actor.iam?.hrAppointmentVersion??null,image])).digest('hex');}
async function leases(database:SqlDatabase,companyId:string){
 const purchase=companyPurchaseReadLocks(database,companyId);
 await database.batch([purchase[0],...commerceCompleteOwnerSourceReadLocks(database),database.prepare('LOCK TABLE commerce_settings IN SHARE ROW EXCLUSIVE MODE'),...commercePostingFinanceSourceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
}
const unavailable=(companyId:string,reason:'authority-required'|'purchase-required'):CommercePostingSetupView=>({companyId,available:false,configured:null,version:null,canConfigure:false,contextKey:null,configuration:null,charts:[],accounts:[],unavailableReason:reason});
async function owner(actor:WorkContext){const write=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED','Initial posting configuration requires its original Holding transaction.');return write;}
export async function commercePostingSetup(actor:WorkContext):Promise<CommercePostingSetupView>{
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read')||!hasPermission(actor,'finance.write'))return unavailable(actor.companyId,'authority-required');
 const write=await owner(actor);return write.transaction(async database=>{
  await leases(database,actor.companyId);const authority=await requireCommercePostingFinanceAuthority(database,actor);
  const access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return unavailable(actor.companyId,'purchase-required');
  const configuration=await current(database,actor.companyId),references=await commercePostingFinanceReferences(database,actor),canConfigure=!configuration&&!(await commercePostingPrerequisiteState(database,actor.companyId)).history&&capabilities.every(capability=>access[capability].newWork);
  const finalAuthority=await requireCommercePostingFinanceAuthority(database,actor);if(contextKey(finalAuthority,'')!==contextKey(authority,''))throw new ApiError(409,'COMMERCE_POSTING_CONTEXT_CHANGED','The selected Finance authority changed during setup discovery.');for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {companyId:actor.companyId,available:true,configured:!!configuration,version:configuration?1:0,canConfigure,contextKey:contextKey(authority,references.image),configuration,unavailableReason:null,
   charts:references.charts.filter(row=>row.status==='active').map(row=>({id:row.id,name:row.name,baseCurrency:row.base_currency})),
   accounts:references.accounts.filter(row=>row.status==='active').map(row=>({id:row.id,chartId:row.chart_id,code:row.code,name:row.name,category:row.category,currency:row.currency,controlKind:row.control_kind}))};
 });
}
async function assertNoHistory(database:SqlDatabase,companyId:string){
 const found=await database.prepare(`SELECT EXISTS(SELECT 1 FROM commerce_orders WHERE company_id=?) OR EXISTS(SELECT 1 FROM commerce_stock_moves m JOIN commerce_warehouses w ON w.id=m.warehouse_id WHERE w.company_id=?) OR EXISTS(SELECT 1 FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND (s.quantity_milli<>0 OR s.value_minor<>0)) OR EXISTS(SELECT 1 FROM commerce_goods_valuations WHERE company_id=?) history`).bind(companyId,companyId,companyId,companyId).first<boolean>('history');
 if(found)throw new ApiError(409,'COMMERCE_POSTING_HISTORY_REQUIRES_REVIEW','Existing Commerce source history needs its original owner reconciliation; initial setup cannot rebind it.');
}
export async function initializeCommercePostingSetup(actor:WorkContext,raw:unknown,key:string):Promise<CommercePostingSetupResult>{
 const write=await owner(actor);
 try{return await write.transaction(async database=>{
  await leases(database,actor.companyId);const authority=await requireCommercePostingFinanceAuthority(database,actor);
  const receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId).first();
  const purpose=receipt?'history' as const:'new-work' as const,boundaryKey=`commerce-posting-setup:${randomUUID()}`;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const boundaries=capabilities.map((capability,index)=>({key:`${boundaryKey}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${boundaryKey}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const references=await commercePostingFinanceReferences(database,actor);
  let applied:CommercePostingSetupInput|undefined;
  const run=createCommercePostingSetup({
   replay:(input,command)=>replayCommand<CommercePostingSetupResult>(database,actor,command,operation,actor.companyId,input),current:()=>current(database,actor.companyId),contextKey:async()=>contextKey(authority,references.image),
   validate:async input=>{await validateCommercePostingFinanceReferences(database,actor,input);},assertNoHistory:()=>assertNoHistory(database,actor.companyId),
   commit:async(input,command)=>{applied=input;return executeCommand(database,actor,command,operation,input,actor.companyId,1,{id:actor.companyId,status:'configured' as const,version:1 as const},[
    assertion(database,command,'commerce-posting-absent','NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)',[actor.companyId]),
    database.prepare(`INSERT INTO commerce_settings(company_id,${fields.map(field=>columns[field]).join(',')}) VALUES(${Array.from({length:fields.length+1},()=>'?').join(',')})`).bind(actor.companyId,...fields.map(field=>input[field])),
   ]);},
  });
  const result=await run(raw,key),input=applied??commercePostingSetupSchema.parse(raw);
  await flushFinanceDeferredEffects(database);const finalAuthority=await requireCommercePostingFinanceAuthority(database,actor);
  if(contextKey(finalAuthority,'')!==contextKey(authority,''))throw new ApiError(409,'COMMERCE_POSTING_CONTEXT_CHANGED','The selected Finance authority changed before configuration completed.');
  if(!sameMapping(await current(database,actor.companyId),mapping(input)))throw new ApiError(409,'COMMERCE_POSTING_SOURCE_CHANGED','The original Company posting mapping changed before completion.');
  if(applied){const after=await validateCommercePostingFinanceReferences(database,actor,input);if(after.image!==references.image)throw new ApiError(409,'COMMERCE_POSTING_SOURCE_CHANGED','The original accounting references changed before completion.');await assertNoHistory(database,actor.companyId);}
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));
  return result;
 });}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

export type CommercePostingSetupProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string};
/** Exact page discovery before Work role selection. This reads current native
 * authority and releases; it creates neither a WorkContext nor an assignment. */
export async function commercePostingSetupProductEntry(database:SqlDatabase,scope:CommercePostingSetupProductScope){
 if(scope.branchId||scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 let iam:Awaited<ReturnType<typeof loadIamAuthority>>;
 try{iam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))return null;throw error;}
 const access=await readPurchasedAccesses(database,'enforced',scope.companyId,capabilities);
 if(!capabilities.every(capability=>access[capability].history))return null;
 const current=await database.prepare(`WITH posting_entry_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)
  SELECT a.id FROM (${identitySources.assignments}) a CROSS JOIN (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id) posting_entry_scope CROSS JOIN posting_entry_clock
  WHERE a.id=? AND a.user_id=? AND a.company_id=posting_entry_scope.company_id AND a.role=? AND a.version=? AND a.scope_kind='holding'
   AND ${assignmentReadSql('posting_entry_scope','finance.read')} AND (${assignmentReadSql('posting_entry_scope','finance.write')} OR ${assignmentReadSql('posting_entry_scope','finance.approve')})
   AND ${scheduledFeaturePredicate('posting_entry_scope.company_id','procurement')} AND ${scheduledFeaturePredicate('posting_entry_scope.company_id','finance')}
   AND ${capabilities.map(capability=>purchasedCapabilityPredicate('posting_entry_scope.company_id',capability,'history','posting_entry_clock.now_ms')).join(' AND ')}`)
  .bind(scope.companyId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.userId,scope.role).first();
 return current?{revision:createHash('sha256').update(JSON.stringify([scope.companyId,scope.assignmentId,scope.assignmentVersion,scope.role,iam.identityVersion,iam.roleRevision,iam.hrAppointmentVersion,access[capabilities[0]].revision])).digest('hex'),purchasePurpose:'history' as const}:null;
}
