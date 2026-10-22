import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {samePerson} from '@erpx/feature-identity_access/lib/iam';
import {decimalUnits} from '@erpx/feature-finance/lib/company-money';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {loadIamAuthority} from '@erpx/server-identity_access/public/iam-store';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {platformOwnerProjectionSources as identitySources} from '@erpx/server-identity_access/public/platform-owner-projection-sources';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure,financePackageEnforcementMode,commercePackageEnforcementMode,type PackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import {commerceCompleteOwnerSourceReadLocks} from '@erpx/server-commerce/public/owner-source-locks';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled,scheduledFeaturePredicate} from '@erpx/server-pms/public/hospitality-rollout';
import {financialConstraintError,replayCommand} from '@erpx/server-workflow/public/commands';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
import {commercePostingFinanceReferences} from '@erpx/server-finance/adapters/commerce-posting-setup';
import {commerceFinanceJoinedFacts} from '@erpx/server-finance/adapters/commerce-joined-facts';
import type {SalesDefaults} from '@erpx/server-finance/domain/sales';
import type {SalesPostingSetupView} from '@erpx/server-finance/domain/sales-posting-setup';

type Kind='chart'|'account'|'period'|'rate'|'defaults'|'decision';
type Permission='finance.read'|'finance.write'|'finance.approve';
const operations={chart:'finance.chart.create',account:'finance.account.create',period:'finance.period.create',rate:'money.rate.confirm',defaults:'finance.sales-defaults.prepare',decision:'finance.sales-defaults.decide'} as const;
const writerTables={chart:'finance_charts',account:'finance_accounts',period:'finance_periods',rate:'company_fx_rates',defaults:'finance_sales_defaults',decision:'finance_sales_defaults'} as const;
const capability='finance.sales-posting';
type Request={contextKey:string;input:unknown;expectedVersion?:0};
type Native={id:string;source:string};
type Period=Native&{name:string;start_date:string;end_date:string;state:string;version:number};
type Rate=Native&{currency:string;effective_date:string;currency_per_usd:string;confirmed_at:number};
type Edition=SalesDefaults&Native&{review_reason:string|null};
async function references(database:SqlDatabase,actor:WorkContext){
 const [finance,periods,rates,money,defaults]=await Promise.all([
  commercePostingFinanceReferences(database,actor),
  database.prepare('SELECT p.id,p.name,p.start_date,p.end_date,p.state,p.close_version version,to_jsonb(p)::text source FROM finance_periods p WHERE p.company_id=? ORDER BY p.id').bind(actor.companyId).all<Period>(),
  database.prepare('SELECT r.id,r.currency,r.effective_date,r.currency_per_usd::text,r.confirmed_at,to_jsonb(r)::text source FROM company_fx_rates r WHERE r.company_id=? ORDER BY r.id').bind(actor.companyId).all<Rate>(),
  database.prepare("SELECT 'settings' kind,to_jsonb(s)::text source FROM company_money_settings s WHERE s.company_id=? UNION ALL SELECT 'scale:'||s.currency kind,to_jsonb(s)::text source FROM company_currency_scales s WHERE s.company_id=? ORDER BY kind").bind(actor.companyId,actor.companyId).all<{kind:string;source:string}>(),
  database.prepare('SELECT d.*,to_jsonb(d)::text source FROM finance_sales_defaults d WHERE d.company_id=? ORDER BY d.id').bind(actor.companyId).all<Edition>(),
 ]);
 return {chart:finance.charts,account:finance.accounts,period:periods.results,rate:rates.results,money:money.results,defaults:defaults.results};
}
type References=Awaited<ReturnType<typeof references>>;
const image=(rows:References)=>JSON.stringify(rows);
function authorityImage(actor:WorkContext){return JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam?.queuedAssignmentVersion,actor.iam?.identityVersion,actor.iam?.roleRevision,actor.iam?.hrAppointmentVersion]);}
function contextKey(actor:WorkContext,rows:References){return createHash('sha256').update(authorityImage(actor)).update(image(rows)).digest('hex');}
function companyAuthority(actor:WorkContext){if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_SCOPE_REQUIRED','Use a Company Finance assignment for sales configuration.');}
async function authority(database:SqlDatabase,actor:WorkContext,permission:Permission,sales:boolean){
 requirePermission(actor,'finance.read');requirePermission(actor,permission);companyAuthority(actor);
 const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'finance.read');companyAuthority(current);
 await assertProductionModuleEnabled(database,actor.companyId,'finance');if(sales)await assertProductionModuleEnabled(database,actor.companyId,'sales');return current;
}
async function owner(actor:WorkContext){const write=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED','Sales posting setup requires its owned Holding transaction.');return write;}
async function leases(database:SqlDatabase,companyId:string,kind?:Kind){
 const purchase=companyPurchaseReadLocks(database,companyId);
 await database.batch([purchase[0],...commerceCompleteOwnerSourceReadLocks(database),database.prepare('LOCK TABLE finance_sales_defaults IN SHARE ROW EXCLUSIVE MODE'),...(kind?[database.prepare(`LOCK TABLE ${writerTables[kind]} IN ROW EXCLUSIVE MODE`)]:[])]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
}
async function accessFor(database:SqlDatabase,actor:WorkContext,mode:PackageEnforcementMode,strict:boolean,purpose:'history'|'new-work'|'settle-existing'){
 const access=await readPurchasedAccesses(database,mode,actor.companyId,strict?[capability]:['finance.accounting',capability]);
 const selected=strict?capability:Object.keys(access).find(key=>access[key].codes[purpose]==='GRANTED')??Object.keys(access).find(key=>access[key].codes.history==='GRANTED')??'finance.accounting';
 return {capability:selected,access:access[selected],sales:strict||selected===capability};
}
function usable(rows:References,row:Edition){
 const chart=rows.chart.find(item=>item.id===row.chart_id&&item.status==='active'),ar=rows.account.find(item=>item.id===row.receivable_account_id),revenue=rows.account.find(item=>item.id===row.revenue_account_id),tax=rows.account.find(item=>item.id===row.tax_account_id);
 return !!chart&&[ar,revenue,...(row.tax_account_id?[tax]:[])].every(item=>!!item&&item.status==='active'&&item.chart_id===row.chart_id&&item.currency===row.currency)&&ar?.control_kind==='receivable'&&revenue?.category==='revenue'&&(!row.tax_account_id||tax?.category==='liability'&&tax.control_kind==='none');
}
const unavailable=(companyId:string,reason:SalesPostingSetupView['unavailableReason']):SalesPostingSetupView=>({companyId,available:false,contextKey:null,unavailableReason:reason,canCreateChart:false,canCreateAccount:false,canCreatePeriod:false,canConfirmRate:false,canPrepareDefaults:false,charts:[],accounts:[],periods:[],rates:[],defaults:[]});
export async function salesPostingSetup(actor:WorkContext):Promise<SalesPostingSetupView>{
 const permission=hasPermission(actor,'finance.write')?'finance.write':'finance.approve';
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read')||!hasPermission(actor,permission))return unavailable(actor.companyId,'authority-required');
 return (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const current=await authority(database,actor,permission,true),access=(await readPurchasedAccesses(database,'enforced',actor.companyId,[capability]))[capability];
  if(!access.history)return unavailable(actor.companyId,'purchase-required');
  const rows=await references(database,actor),final=await authority(database,actor,permission,true);
  if(authorityImage(current)!==authorityImage(final))throw new ApiError(409,'SALES_POSTING_CONTEXT_CHANGED','The selected Finance authority changed during Sales setup discovery.');
  await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  const write=hasPermission(current,'finance.write'),approve=hasPermission(current,'finance.approve');
  return {companyId:actor.companyId,available:true,contextKey:contextKey(current,rows),unavailableReason:null,canCreateChart:access.newWork&&write,canCreateAccount:access.newWork&&write,canCreatePeriod:access.newWork&&write,canConfirmRate:access.newWork&&approve,canPrepareDefaults:access.newWork&&write,
   charts:rows.chart.filter(row=>row.status==='active').map(row=>({id:row.id,name:row.name,baseCurrency:row.base_currency})),accounts:rows.account.filter(row=>row.status==='active').map(row=>({id:row.id,chartId:row.chart_id,code:row.code,name:row.name,category:row.category,currency:row.currency,controlKind:row.control_kind})),
   periods:rows.period.map(row=>({id:row.id,name:row.name,startDate:String(row.start_date).slice(0,10),endDate:String(row.end_date).slice(0,10),state:row.state,version:Number(row.version)})),rates:rows.rate.map(row=>({id:row.id,currency:row.currency,effectiveDate:String(row.effective_date).slice(0,10),currencyPerUsd:row.currency_per_usd.includes(".")?row.currency_per_usd.replace(/0+$/,'').replace(/\.$/,''):row.currency_per_usd,confirmedAt:Number(row.confirmed_at)})),
   defaults:rows.defaults.map(row=>({id:row.id,chartId:row.chart_id,currency:row.currency,receivableAccountId:row.receivable_account_id,revenueAccountId:row.revenue_account_id,taxAccountId:row.tax_account_id,status:row.status as SalesPostingSetupView['defaults'][number]['status'],version:Number(row.version),reason:row.reason,reviewReason:row.review_reason,canReview:row.status==='draft'&&(access.newWork||access.settleExisting)&&approve&&!samePerson(current,row.created_by),canApprove:row.status==='draft'&&access.newWork&&approve&&!samePerson(current,row.created_by),canReject:row.status==='draft'&&access.settleExisting&&approve&&!samePerson(current,row.created_by),usable:row.status==='approved'&&usable(rows,row)}))};
 });
}
/** Legacy Console readers retain their Finance accounting OR actual Sales
 * foundation decision and disabled/shadow behavior, without loading invoices. */
export async function salesDefaultsReferences(actor:WorkContext){
 const mode=await financePackageEnforcementMode();return (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const selected=await accessFor(database,actor,mode,false,'history'),current=await authority(database,actor,'finance.read',selected.sales);
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:selected.capability,purpose:'history'});
  const rows=await references(database,actor),parties=(await database.prepare("SELECT id,name,kind,status FROM finance_parties WHERE company_id=? AND kind='customer' AND person_id IS NULL ORDER BY name,id").bind(actor.companyId).all<{id:string;name:string;kind:string;status:string}>()).results;
  const final=await authority(database,actor,'finance.read',selected.sales);if(authorityImage(current)!==authorityImage(final))throw new ApiError(409,'SALES_POSTING_CONTEXT_CHANGED','The selected Finance authority changed during defaults discovery.');
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:selected.capability,purpose:'history'});
  return {editions:rows.defaults.map(row=>({id:row.id,company_id:row.company_id,chart_id:row.chart_id,currency:row.currency,receivable_account_id:row.receivable_account_id,revenue_account_id:row.revenue_account_id,tax_account_id:row.tax_account_id,status:row.status,version:row.version,reason:row.reason,created_by:row.created_by,reviewed_by:row.reviewed_by,created_at:row.created_at,updated_at:row.updated_at,review_reason:row.review_reason})),options:{charts:rows.chart,accounts:rows.account,parties},usableDefaults:rows.defaults.filter(row=>row.status==='approved'&&usable(rows,row)).map(row=>row.id),issues:rows.defaults.filter(row=>row.status==='approved'&&!usable(rows,row)).map(row=>({currency:row.currency,message:'Restore active receivable, revenue and VAT accounts on the approved ledger, or prepare new sales defaults.'}))};
 });
}
function sourceChanged(){return new ApiError(409,'SALES_POSTING_SOURCE_CHANGED','The original Company posting references changed before completion.');}
function unchangedExcept(before:References,after:References,kind:Kind,id:string){
 return (['chart','account','period','rate','defaults','money'] as const).every(key=>JSON.stringify(after[key].filter(row=>key!==kind||!('id' in row)||row.id!==id))===JSON.stringify(before[key]));
}
function createdReferenceMatches(rows:References,kind:Kind,id:string,actor:WorkContext,input:unknown){
 if(kind==='decision')return false;
 const row=rows[kind].find(item=>item.id===id);if(!row||!input||typeof input!=='object')return false;
 const source=JSON.parse(row.source) as Record<string,unknown>,value=input as Record<string,unknown>;
 if(source.company_id!==actor.companyId)return false;
 if(kind==='chart')return source.created_by===actor.userId&&source.name===value.name&&source.base_currency===value.baseCurrency&&source.accounting_basis===value.accountingBasis&&source.status==='active'&&source.version===1;
 if(kind==='account'){const chart=rows.chart.find(item=>item.id===value.chartId);return !!chart&&source.chart_id===value.chartId&&source.code===value.code&&source.name===value.name&&source.category===value.category&&source.parent_id===(value.parentId??null)&&source.currency===(value.currency??chart.base_currency)&&source.control_kind===value.controlKind&&Number(source.is_control)===(value.isControl||value.controlKind!=='none'?1:0)&&source.status==='active'&&source.version===1;}
 if(kind==='period')return source.name===value.name&&source.start_date===value.startDate&&source.end_date===value.endDate&&source.state==='open'&&source.close_version===0;
 if(kind==='defaults')return source.created_by===actor.userId&&source.chart_id===value.chartId&&source.currency===value.currency&&source.receivable_account_id===value.receivableAccountId&&source.revenue_account_id===value.revenueAccountId&&source.tax_account_id===(value.taxAccountId??null)&&source.reason===value.reason&&source.status==='draft'&&source.version===1&&source.reviewed_by===null;
 const rate=rows.rate.find(item=>item.id===id);return !!rate&&source.confirmed_by===actor.userId&&source.currency===value.currency&&source.effective_date===value.effectiveDate&&source.note===value.note&&decimalUnits(rate.currency_per_usd,18)===decimalUnits(String(value.currencyPerUsd),18);
}
function decisionMatches(before:References,after:References,id:string,actor:WorkContext,input:unknown){
 if(!input||typeof input!=='object')return false;const value=input as Record<string,unknown>,target=before.defaults.find(row=>row.id===id);if(!target)return false;
 if(JSON.stringify({...before,defaults:[]})!==JSON.stringify({...after,defaults:[]})||before.defaults.length!==after.defaults.length)return false;
 return before.defaults.every(row=>{const next=after.defaults.find(item=>item.id===row.id);if(!next)return false;const old=JSON.parse(row.source) as Record<string,unknown>,actual=JSON.parse(next.source) as Record<string,unknown>;
  if(row.id===id){old.status=value.action==='approve'?'approved':'rejected';old.reviewed_by=actor.userId;old.review_reason=value.reason;old.version=Number(old.version)+1;old.updated_at=actual.updated_at;}
  else if(value.action==='approve'&&row.currency===target.currency&&row.status==='approved'){old.status='superseded';old.version=Number(old.version)+1;old.updated_at=actual.updated_at;}
  return JSON.stringify(old)===JSON.stringify(actual);
 });
}
/** Private composition of the original Finance services. No caller SQL,
 * purchase label or supplied authority snapshot is exposed publicly. */
export async function withSalesPostingCommand<Result extends {id:string}>(actor:WorkContext,kind:Kind,request:unknown,key:string,run:(database:SqlDatabase)=>Promise<Result>,legacy=false,recordId?:string):Promise<Result>{
 const operation=operations[kind],permission=kind==='rate'||kind==='decision'?'finance.approve':'finance.write',mode=legacy?await financePackageEnforcementMode():'enforced';
 const decisionInput=(legacy?request:(request as Request).input) as {action?:string};
 try{return await (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId,kind);
  const receipt=await database.prepare("SELECT record_id FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId).first<{record_id:string}>(),purpose=receipt?'history' as const:kind==='decision'&&decisionInput.action==='reject'?'settle-existing' as const:'new-work' as const;
  const selected=await accessFor(database,actor,mode,!legacy,purpose),current=await authority(database,actor,permission,selected.sales);
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:selected.capability,purpose});
  const guard=`sales-posting:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,guard,{companyId:actor.companyId,capability:selected.capability,purpose});if(boundary)await database.batch(boundary.before);
  const before=await references(database,actor);let result:Result;
  if(receipt){const replay=await replayCommand<Result>(database,actor,key,operation,recordId??receipt.record_id,request);if(!replay)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','The original Sales setup receipt is unavailable.');result=replay;}
  else{
   if(!legacy&&(request as Request).contextKey!==contextKey(current,before))throw new ApiError(409,'SALES_POSTING_CONTEXT_CHANGED','The selected Finance context or posting references changed. Keep the draft and review the current setup.');
   result=await run(database);
  }
  const created=await references(database,actor),input=legacy?request:(request as Request).input;
  if(!receipt&&!(kind==='decision'?decisionMatches(before,created,result.id,actor,input):createdReferenceMatches(created,kind,result.id,actor,input)&&unchangedExcept(before,created,kind,result.id)))throw sourceChanged();
  await flushFinanceDeferredEffects(database);const final=await authority(database,actor,permission,selected.sales),after=await references(database,actor);
  if(authorityImage(current)!==authorityImage(final))throw new ApiError(409,'SALES_POSTING_CONTEXT_CHANGED','The selected Finance authority changed before Sales setup completed.');
  if(image(created)!==image(after))throw sourceChanged();
  if(boundary){const observed=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(guard)]);boundary.observe?.(observed);}
  return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export type SalesPostingSetupProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string};
/** Exact source page eligibility before selecting a Work role. */
export async function salesPostingSetupProductEntry(database:SqlDatabase,scope:SalesPostingSetupProductScope){
 if(scope.branchId||scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 let iam:Awaited<ReturnType<typeof loadIamAuthority>>;try{iam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))return null;throw error;}
 const access=(await readPurchasedAccesses(database,'enforced',scope.companyId,[capability]))[capability];if(!access.history)return null;
 const current=await database.prepare(`WITH sales_setup_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)
 SELECT a.id FROM (${identitySources.assignments}) a CROSS JOIN (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id) sales_setup_scope CROSS JOIN sales_setup_clock
 WHERE a.id=? AND a.user_id=? AND a.company_id=sales_setup_scope.company_id AND a.role=? AND a.version=? AND a.scope_kind='holding'
 AND ${assignmentReadSql('sales_setup_scope','finance.read')} AND (${assignmentReadSql('sales_setup_scope','finance.write')} OR ${assignmentReadSql('sales_setup_scope','finance.approve')})
 AND ${scheduledFeaturePredicate('sales_setup_scope.company_id','sales')} AND ${scheduledFeaturePredicate('sales_setup_scope.company_id','finance')}
 AND ${purchasedCapabilityPredicate('sales_setup_scope.company_id',capability,'history','sales_setup_clock.now_ms')}`)
 .bind(scope.companyId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.userId,scope.role).first();
 return current?{revision:createHash('sha256').update(JSON.stringify([scope.companyId,scope.assignmentId,scope.assignmentVersion,scope.role,iam.identityVersion,iam.roleRevision,iam.hrAppointmentVersion,access.revision])).digest('hex'),purchasePurpose:'history' as const}:null;
}
/** Ordinary Sales discovery gets no Finance reference or edition identities. */
export async function salesPostingDocumentReadiness(actor:WorkContext,kind:'quote'|'sales'):Promise<{ready:boolean;reason:'sales-defaults-required'|null}> {
 requirePermission(actor,'commerce.read');const mode=await commercePackageEnforcementMode(),documentCapability=kind==='quote'?'sales.quotes':'sales.orders';return (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const current=await requireQueuedCommandAuthority(database,actor,'commerce.read');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:documentCapability,purpose:'history'});
  await assertProductionModuleEnabled(database,actor.companyId,'sales');await assertProductionModuleEnabled(database,actor.companyId,'finance');
  const ready=!!await commerceFinanceJoinedFacts(database).readCommerceSettings1({companyId:actor.companyId});
  const final=await requireQueuedCommandAuthority(database,actor,'commerce.read');if(authorityImage(current)!==authorityImage(final))throw new ApiError(409,'SALES_POSTING_CONTEXT_CHANGED','The selected Sales authority changed.');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:documentCapability,purpose:'history'});
  await assertProductionModuleEnabled(database,actor.companyId,'sales');await assertProductionModuleEnabled(database,actor.companyId,'finance');return {ready,reason:ready?null:'sales-defaults-required' as const};
 });
}
