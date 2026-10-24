import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {decimalUnits} from '@erpx/feature-finance/lib/company-money';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {servicePartsPostingSourceLocks,servicePartsPostingState} from '@erpx/server-commerce/public/service-parts-posting-sources';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {replayCommand} from '@erpx/server-workflow/public/commands';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
import {commercePostingFinanceReferences} from '@erpx/server-finance/adapters/commerce-posting-setup';
import type {ServicePartsPrerequisitesView} from '@erpx/server-finance/domain/service-parts-prerequisites';

type Kind='chart'|'account'|'period'|'rate';
const operations={chart:'finance.chart.create',account:'finance.account.create',period:'finance.period.create',rate:'money.rate.confirm'} as const;
const writerTables={chart:'finance_charts',account:'finance_accounts',period:'finance_periods',rate:'company_fx_rates'} as const;
const capabilities=['service.orders','service.parts-assets','finance.service-invoice'] as const;
type Request={expectedVersion:0;contextKey:string;input:unknown};
type Native={id:string;source:string};
type Period=Native&{name:string;start_date:string;end_date:string;state:string;version:number};
type Rate=Native&{currency:string;effective_date:string;currency_per_usd:string;confirmed_at:number};
async function references(database:SqlDatabase,actor:WorkContext){
 const [finance,periods,rates,money]=await Promise.all([
  commercePostingFinanceReferences(database,actor),
  database.prepare('SELECT p.id,p.name,p.start_date,p.end_date,p.state,p.close_version version,to_jsonb(p)::text source FROM finance_periods p WHERE p.company_id=? ORDER BY p.id').bind(actor.companyId).all<Period>(),
  database.prepare('SELECT r.id,r.currency,r.effective_date,r.currency_per_usd::text,r.confirmed_at,to_jsonb(r)::text source FROM company_fx_rates r WHERE r.company_id=? ORDER BY r.id').bind(actor.companyId).all<Rate>(),
  database.prepare("SELECT 'settings' kind,to_jsonb(s)::text source FROM company_money_settings s WHERE s.company_id=? UNION ALL SELECT 'scale:'||s.currency kind,to_jsonb(s)::text source FROM company_currency_scales s WHERE s.company_id=? ORDER BY kind").bind(actor.companyId,actor.companyId).all<{kind:string;source:string}>(),
 ]);
 return {chart:finance.charts,account:finance.accounts,period:periods.results,rate:rates.results,money:money.results};
}
type References=Awaited<ReturnType<typeof references>>;
const image=(rows:References)=>JSON.stringify([rows.chart,rows.account,rows.period,rows.rate,rows.money]);
function contextKey(actor:WorkContext,rows:References,state:{configured:boolean;history:boolean}){return createHash('sha256').update(JSON.stringify([actor.holdingId,actor.companyId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam?.queuedAssignmentVersion,actor.iam?.identityVersion,actor.iam?.roleRevision,actor.iam?.hrAppointmentVersion,image(rows),state])).digest('hex');}
function authorityImage(actor:WorkContext){return JSON.stringify([actor.userId,actor.assignmentId,actor.role,actor.companyId,actor.branchId,actor.departmentId,actor.authority??null,actor.iam?.queuedAssignmentVersion,actor.iam?.identityVersion,actor.iam?.roleRevision,actor.iam?.hrAppointmentVersion]);}
async function authority(database:SqlDatabase,actor:WorkContext,permission:'finance.write'|'finance.approve'){if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use the original Company Finance assignment.');requirePermission(actor,'finance.read');requirePermission(actor,permission);const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'finance.read');for(const moduleId of ['field_service','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,moduleId);return current;}
async function owner(actor:WorkContext){const write=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED','Service parts posting prerequisites require their owned Holding transaction.');return write;}
async function leases(database:SqlDatabase,companyId:string,kind?:Kind){
 const purchase=companyPurchaseReadLocks(database,companyId);
 if(kind){await lockPackageSettlementSources(database);await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();await database.prepare(`LOCK TABLE ${writerTables[kind]} IN SHARE ROW EXCLUSIVE MODE`).run();}else await database.batch([purchase[0]]);
 await database.batch([...servicePartsPostingSourceLocks(database),database.prepare('LOCK TABLE finance_charts,finance_accounts,finance_periods,company_money_settings,company_currency_scales,company_fx_rates,finance_invoices,finance_payments,finance_journals IN SHARE MODE')]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);if(kind)await database.batch(purchase.slice(1));
}
async function postingState(database:SqlDatabase,actor:WorkContext){const source=await servicePartsPostingState(database,actor.companyId),refs=await commercePostingFinanceReferences(database,actor);return {configured:!!source.mapping,history:source.history,chartId:source.mapping?String(source.mapping.chart_id):null,mappingImage:JSON.stringify(source.mapping),proofImage:JSON.stringify(source.proof),image:source.image,unsupportedExistingMapping:!!source.mapping&&refs.charts.find(row=>row.id===source.mapping!.chart_id)?.base_currency!==source.mapping.currency};}

const unavailable=(companyId:string,reason:ServicePartsPrerequisitesView['unavailableReason']):ServicePartsPrerequisitesView=>({companyId,available:false,configured:null,contextKey:null,canCreateChart:false,canCreateAccount:false,canCreatePeriod:false,canConfirmRate:false,charts:[],accounts:[],periods:[],rates:[],unavailableReason:reason});
export async function servicePartsPrerequisites(actor:WorkContext):Promise<ServicePartsPrerequisitesView>{
 const permission=hasPermission(actor,'finance.write')?'finance.write':'finance.approve';
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read')||!hasPermission(actor,permission))return unavailable(actor.companyId,'authority-required');
 const write=await owner(actor);return write.transaction(async database=>{
  await leases(database,actor.companyId);const current=await authority(database,actor,permission),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return unavailable(actor.companyId,'purchase-required');
  const rows=await references(database,actor),state=await postingState(database,actor),active=capabilities.every(capability=>access[capability].newWork),canWrite=hasPermission(current,'finance.write'),initial=!state.configured&&!state.history;
  const final=await authority(database,actor,permission);if(authorityImage(current)!==authorityImage(final))throw new ApiError(409,'SERVICE_PARTS_POSTING_CONTEXT_CHANGED','The selected Finance authority changed during prerequisite discovery.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {companyId:actor.companyId,available:true,configured:state.configured,contextKey:contextKey(current,rows,state),canCreateChart:active&&canWrite&&initial,canCreateAccount:active&&canWrite&&!state.unsupportedExistingMapping,canCreatePeriod:active&&canWrite,canConfirmRate:active&&hasPermission(current,'finance.approve'),unavailableReason:!state.configured&&state.history?'history-requires-review':null,
   charts:rows.chart.filter(row=>row.status==='active').map(row=>({id:row.id,name:row.name,baseCurrency:row.base_currency})),accounts:rows.account.filter(row=>row.status==='active'&&typeof row.currency==='string').map(row=>({id:row.id,chartId:row.chart_id,code:row.code,name:row.name,category:row.category,currency:row.currency,controlKind:row.control_kind})),
   periods:rows.period.map(row=>({id:row.id,name:row.name,startDate:String(row.start_date).slice(0,10),endDate:String(row.end_date).slice(0,10),state:row.state,version:Number(row.version)})),
   rates:rows.rate.map(row=>({id:row.id,currency:row.currency,effectiveDate:String(row.effective_date).slice(0,10),currencyPerUsd:row.currency_per_usd.includes(".")?row.currency_per_usd.replace(/0+$/,'').replace(/\.$/,''):row.currency_per_usd,confirmedAt:Number(row.confirmed_at)}))};
 });
}
function unchangedBeforeCreation(before:References,after:References,kind:Kind,id:string){
 return (Object.keys(operations) as Kind[]).every(key=>JSON.stringify(after[key].filter(row=>key!==kind||row.id!==id))===JSON.stringify(before[key]))&&JSON.stringify(before.money)===JSON.stringify(after.money);
}
function createdReferenceMatches(rows:References,kind:Kind,id:string,actor:WorkContext,input:unknown){
 const row=rows[kind].find(item=>item.id===id);if(!row||!input||typeof input!=='object')return false;
 const source=JSON.parse(row.source) as Record<string,unknown>,value=input as Record<string,unknown>;
 if(source.company_id!==actor.companyId)return false;
 if(kind==='chart')return source.created_by===actor.userId&&source.name===value.name&&source.base_currency===value.baseCurrency&&source.accounting_basis===value.accountingBasis&&source.status==='active'&&source.version===1;
 if(kind==='account'){
  const chart=rows.chart.find(item=>item.id===value.chartId);
  return !!chart&&source.chart_id===value.chartId&&source.code===value.code&&source.name===value.name&&source.category===value.category&&source.parent_id===(value.parentId??null)&&source.currency===(value.currency??chart.base_currency)&&source.control_kind===value.controlKind&&Number(source.is_control)===(value.isControl||value.controlKind!=='none'?1:0)&&source.status==='active'&&source.version===1;
 }
 if(kind==='period')return source.name===value.name&&source.start_date===value.startDate&&source.end_date===value.endDate&&source.state==='open'&&source.close_version===0;
 const rate=rows.rate.find(item=>item.id===id);
 return !!rate&&source.confirmed_by===actor.userId&&source.currency===value.currency&&source.effective_date===value.effectiveDate&&source.note===value.note&&decimalUnits(rate.currency_per_usd,18)===decimalUnits(String(value.currencyPerUsd),18);
}
/** Private same-owner composition. The public operations expose no SQL port,
 * authority facts or arbitrary operation selector. Original services own every
 * business INSERT, result, receipt, audit and idempotency hash. */
export async function withServicePartsPrerequisiteCommand<Result extends {id:string}>(actor:WorkContext,kind:Kind,request:Request,key:string,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 const write=await owner(actor),operation=operations[kind],permission=kind==='rate'?'finance.approve':'finance.write';
 try{return await write.transaction(async database=>{
  await leases(database,actor.companyId,kind);const current=await authority(database,actor,permission);
  const receipt=await database.prepare("SELECT record_id FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId).first<{record_id:string}>();
  const purpose=receipt?'history' as const:'new-work' as const,guard=`service-parts-prerequisite:${randomUUID()}`;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const before=await references(database,actor),state=await postingState(database,actor);
  let result:Result;
  if(receipt){const replay=await replayCommand<Result>(database,actor,key,operation,receipt.record_id,request);if(!replay)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','The original prerequisite receipt is unavailable.');result=replay;}
  else{
   if(kind==='chart'&&(state.configured||state.history))throw new ApiError(409,'SERVICE_PARTS_POSTING_HISTORY_REQUIRES_REVIEW','Initial chart and account setup cannot replace an existing mapping or historical source.');
   if(kind==='account'&&request.input&&typeof request.input==='object'){const chartId='chartId' in request.input?request.input.chartId:null,chart=before.chart.find(row=>row.id===chartId);if(!chart||chart.status!=='active'||state.configured&&chart.id!==state.chartId)throw new ApiError(422,'SERVICE_PARTS_CURRENCY_MISMATCH','Use the actual retained active Service parts chart.');}
   if(request.contextKey!==contextKey(current,before,state))throw new ApiError(409,'SERVICE_PARTS_POSTING_CONTEXT_CHANGED','The selected work context or posting references changed. Keep the draft and review the current prerequisites.');
   result=await run(database);
  }
  const created=await references(database,actor);
  if(!receipt&&(!createdReferenceMatches(created,kind,result.id,actor,request.input)||!unchangedBeforeCreation(before,created,kind,result.id)))throw new ApiError(409,'SERVICE_PARTS_POSTING_SOURCE_CHANGED','The original posting references changed while creating a prerequisite.');
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));await flushFinanceDeferredEffects(database);const final=await authority(database,actor,permission),after=await references(database,actor),finalState=await postingState(database,actor);
  if(authorityImage(current)!==authorityImage(final))throw new ApiError(409,'SERVICE_PARTS_POSTING_CONTEXT_CHANGED','The selected Finance authority changed before prerequisite completion.');
  if(image(created)!==image(after)||JSON.stringify(state)!==JSON.stringify(finalState))throw new ApiError(409,'SERVICE_PARTS_POSTING_SOURCE_CHANGED','The original posting references changed before prerequisite completion.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
