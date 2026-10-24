import {effectiveWorkModules} from '@erpx/server-identity_access/public/rules/work-modules';
import {payrollPostingPrerequisitesPresentationAllowed} from '@erpx/server-finance/domain/payroll-posting-prerequisites-entry';
import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {decimalUnits} from '@erpx/feature-finance/lib/company-money';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {loadIamAuthority} from '@erpx/server-identity_access/public/iam-store';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {companyPurchaseReadLocks,checkPurchasedCapability,readPurchasedAccesses,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { assertProductionModuleEnabled, scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {payrollPostingPrerequisiteSourceLocks,payrollPostingPrerequisiteSources} from '@erpx/server-people_payroll/public/payroll-posting-prerequisite-sources';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {isTransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {replayCommand} from '@erpx/server-workflow/public/commands';
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
import {commercePostingFinanceReferences} from '@erpx/server-finance/adapters/commerce-posting-setup';
import {requireFinanceMoneyScale} from '@erpx/server-finance/adapters/finance-money-policy';
import {payrollPostingPrerequisiteConfirmationSchema,payrollPostingPrerequisiteNativeStatusSchema,payrollPostingPrerequisitesSelectionSchema,type PayrollPostingPrerequisitesView,type PayrollPostingPrerequisitesSelection,type PayrollPostingPrerequisiteConfirmationResult} from '@erpx/server-finance/domain/payroll-posting-prerequisites';

type Kind='chart'|'account'|'period'|'rate';
const operations={chart:'finance.chart.create',account:'finance.account.create',period:'finance.period.create',rate:'money.rate.confirm'} as const;
const writerTables={chart:'finance_charts',account:'finance_accounts',period:'finance_periods',rate:'company_fx_rates'} as const;
const capabilities=['people.directory','people.payroll'] as const;
type Request={payGroupId:string;expectedVersion:0;contextKey:string;input:unknown};
type Native={id:string;source:string};
type Period=Native&{name:string;start_date:string;end_date:string;state:string;version:number};
type Rate=Native&{currency:string;effective_date:string;currency_per_usd:string;confirmed_at:number};
async function owner(actor:WorkContext){const write=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED','Payroll prerequisites require their owned Holding transaction.');return write;}
async function authority(database:SqlDatabase,actor:WorkContext,permission:'finance.write'|'finance.approve'){
 requirePermission(actor,permission);
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_PAYROLL_SCOPE_REQUIRED','Use a current Company Finance assignment for Payroll prerequisites.');
 // Original Finance configuration also requires Finance read; the original
 // Payroll selector and manual-rate service require Finance approval instead.
 if(permission==='finance.write')requirePermission(actor,'finance.read');
 const current=await requireQueuedCommandAuthority(database,actor,permission);
 if(current.scope.kind!=='holding'||current.branchId||current.departmentId||current.companyId!==actor.companyId)throw new ApiError(403,'COMPANY_PAYROLL_SCOPE_REQUIRED','Use the original Company Finance assignment.');
 if(permission==='finance.write')requirePermission(current,'finance.read');
 for(const feature of ['people_payroll','finance'])await assertProductionModuleEnabled(database,actor.companyId,feature);
 return current;
}
async function leases(database:SqlDatabase,companyId:string,kind?:Kind){
 const purchase=companyPurchaseReadLocks(database,companyId);
 if(kind){await lockPackageSettlementSources(database);await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();await database.prepare(`LOCK TABLE ${writerTables[kind]} IN SHARE ROW EXCLUSIVE MODE`).run();}else await database.batch([purchase[0]]);
 await database.prepare('LOCK TABLE finance_charts,finance_accounts,finance_periods,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE').run();
 await database.batch(payrollPostingPrerequisiteSourceLocks(database));
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);if(kind)await database.batch(purchase.slice(1));
}
async function references(database:SqlDatabase,actor:WorkContext){
 const [finance,periods,rates,money,payroll]=await Promise.all([
  commercePostingFinanceReferences(database,actor),
  database.prepare('SELECT p.id,p.name,p.start_date,p.end_date,p.state,p.close_version version,to_jsonb(p)::text source FROM finance_periods p WHERE p.company_id=? ORDER BY p.id').bind(actor.companyId).all<Period>(),
  database.prepare('SELECT r.id,r.currency,r.effective_date,r.currency_per_usd::text,r.confirmed_at,to_jsonb(r)::text source FROM company_fx_rates r WHERE r.company_id=? ORDER BY r.id').bind(actor.companyId).all<Rate>(),
  database.prepare("SELECT 'settings' kind,to_jsonb(s)::text source FROM company_money_settings s WHERE s.company_id=? UNION ALL SELECT 'scale:'||s.currency kind,to_jsonb(s)::text source FROM company_currency_scales s WHERE s.company_id=? ORDER BY kind").bind(actor.companyId,actor.companyId).all<{kind:string;source:string}>(),
  payrollPostingPrerequisiteSources(database,actor.companyId),
 ]);
 return {chart:finance.charts,account:finance.accounts,period:periods.results,rate:rates.results,money:money.results,payroll};
}
type References=Awaited<ReturnType<typeof references>>;
const image=(rows:References)=>JSON.stringify([rows.chart,rows.account,rows.period,rows.rate,rows.money,rows.payroll.image]);
const stamp=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);
const contextKey=(actor:WorkContext,rows:References,payGroupId:string)=>createHash('sha256').update(JSON.stringify([stamp(actor),payGroupId,image(rows)])).digest('hex');
const changed=()=>new ApiError(409,'PAYROLL_POSTING_SOURCE_CHANGED','The original Payroll group, mapping, Finance reference or money source changed.');
const empty=(companyId:string,unavailableReason:PayrollPostingPrerequisitesView['unavailableReason']):PayrollPostingPrerequisitesView=>({companyId,available:false,contextKey:null,selection:null,groups:[],canCreateChart:false,canCreateAccount:false,canCreatePeriod:false,canConfirmRate:false,charts:[],accounts:[],periods:[],rates:[],unavailableReason});
export async function payrollPostingPrerequisites(actor:WorkContext,raw:PayrollPostingPrerequisitesSelection={}):Promise<PayrollPostingPrerequisitesView>{
 const selection=payrollPostingPrerequisitesSelectionSchema.parse(raw);
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.approve'))return empty(actor.companyId,'authority-required');
 return (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const current=await authority(database,actor,'finance.approve'),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return empty(actor.companyId,'purchase-required');
  const rows=await references(database,actor),groups=rows.payroll.groups.filter(group=>group.status==='active').map(({id,name,currency,status})=>({id,name,currency,status})),group=rows.payroll.groups.find(row=>row.id===selection.payGroupId);
  const base={...empty(actor.companyId,selection.payGroupId?'pay-group-unavailable':'pay-group-required'),available:true,groups};
  let supported=true;
  if(group){try{await requireFinanceMoneyScale(database,actor.companyId,group.currency);}catch(error){if(error instanceof ApiError&&['CURRENCY_SCALE_REQUIRED','FINANCE_CURRENCY_SCALE_UNSUPPORTED'].includes(error.code))supported=false;else throw error;}}
  const final=await authority(database,actor,'finance.approve');if(stamp(current)!==stamp(final))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The selected Company Finance authority changed.');
  if(image(rows)!==image(await references(database,actor)))throw changed();
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  if(!group)return base;
  const active=group.status==='active'&&supported&&capabilities.every(capability=>access[capability].newWork),canWrite=hasPermission(current,'finance.write')&&hasPermission(current,'finance.read'),charts=rows.chart.filter(row=>row.status==='active'&&row.base_currency===group.currency);
  return {...base,contextKey:contextKey(current,rows,group.id),selection:{id:group.id,name:group.name,currency:group.currency,status:group.status},canCreateChart:active&&canWrite,canCreateAccount:active&&canWrite,canCreatePeriod:active&&canWrite,canConfirmRate:active&&group.currency!=='USD',unavailableReason:!supported?'currency-unsupported':group.status!=='active'?'pay-group-unavailable':null,
   charts:charts.map(row=>({id:row.id,name:row.name,baseCurrency:row.base_currency})),accounts:rows.account.filter(row=>row.status==='active'&&row.currency===group.currency&&charts.some(chart=>chart.id===row.chart_id)&&['asset','liability','expense'].includes(row.category)).map(row=>({id:row.id,chartId:row.chart_id,code:row.code,name:row.name,category:row.category,currency:row.currency,controlKind:row.control_kind})),
   periods:rows.period.filter(row=>row.state==='open').map(row=>({id:row.id,name:row.name,startDate:String(row.start_date).slice(0,10),endDate:String(row.end_date).slice(0,10),state:row.state,version:Number(row.version)})),rates:rows.rate.filter(row=>row.currency===group.currency).map(row=>({id:row.id,currency:row.currency,effectiveDate:String(row.effective_date).slice(0,10),currencyPerUsd:row.currency_per_usd.includes(".")?row.currency_per_usd.replace(/0+$/,'').replace(/\.$/,''):row.currency_per_usd,confirmedAt:Number(row.confirmed_at)}))};
 });
}
async function validateFresh(database:SqlDatabase,actor:WorkContext,rows:References,kind:Kind,request:Request){
 const group=rows.payroll.groups.find(row=>row.id===request.payGroupId&&row.status==='active');if(!group)throw new ApiError(404,'PAY_GROUP_NOT_FOUND','Select an active original Company pay group.');
 await requireFinanceMoneyScale(database,actor.companyId,group.currency);
 if(!request.input||typeof request.input!=='object')throw new ApiError(422,'VALIDATION_FAILED','Use the original typed Finance input.');
 const input=request.input as Record<string,unknown>;
 if(kind==='chart'&&input.baseCurrency!==group.currency)throw new ApiError(422,'PAYROLL_CHART_CURRENCY','Payroll chart must use the pay-group currency.');
 if(kind==='account'){
  const chart=rows.chart.find(row=>row.id===input.chartId&&row.status==='active'&&row.base_currency===group.currency);
  if(!chart||(input.currency??chart.base_currency)!==group.currency||(input.parentId&&!rows.account.some(row=>row.id===input.parentId&&row.chart_id===chart.id)))throw new ApiError(422,'PAYROLL_ACCOUNT_INVALID','Use the original Company chart, currency and same-chart parent.');
 }
 if(kind==='rate'&&input.currency!==group.currency)throw new ApiError(422,'PAYROLL_CHART_CURRENCY','Confirm only the selected pay-group currency.');
}
function unchangedBeforeCreation(before:References,after:References,kind:Kind,id:string){return (Object.keys(operations) as Kind[]).every(key=>JSON.stringify(after[key].filter(row=>key!==kind||row.id!==id))===JSON.stringify(before[key]))&&JSON.stringify(before.money)===JSON.stringify(after.money)&&before.payroll.image===after.payroll.image;}
function createdReferenceMatches(rows:References,kind:Kind,id:string,actor:WorkContext,input:unknown){
 const row=rows[kind].find(item=>item.id===id);if(!row||!input||typeof input!=='object')return false;
 const source=JSON.parse(row.source) as Record<string,unknown>,value=input as Record<string,unknown>;if(source.company_id!==actor.companyId)return false;
 if(kind==='chart')return source.created_by===actor.userId&&source.name===value.name&&source.base_currency===value.baseCurrency&&source.accounting_basis===value.accountingBasis&&source.status==='active'&&source.version===1;
 if(kind==='account'){const chart=rows.chart.find(item=>item.id===value.chartId);return !!chart&&source.chart_id===value.chartId&&source.code===value.code&&source.name===value.name&&source.category===value.category&&source.parent_id===(value.parentId??null)&&source.currency===(value.currency??chart.base_currency)&&source.control_kind===value.controlKind&&Number(source.is_control)===(value.isControl||value.controlKind!=='none'?1:0)&&source.status==='active'&&source.version===1;}
 if(kind==='period')return source.name===value.name&&source.start_date===value.startDate&&source.end_date===value.endDate&&source.state==='open'&&source.close_version===0;
 const rate=rows.rate.find(item=>item.id===id);return !!rate&&source.confirmed_by===actor.userId&&source.currency===value.currency&&source.effective_date===value.effectiveDate&&source.note===value.note&&decimalUnits(rate.currency_per_usd,18)===decimalUnits(String(value.currencyPerUsd),18);
}
/** Fixed private Finance composition. The public port never accepts a database,
 * callback, operation, capability or authority supplied by a product caller. */
export async function withPayrollPostingPrerequisiteCommand<Result extends {id:string}>(actor:WorkContext,kind:Kind,request:Request,key:string,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 const operation=operations[kind],permission=kind==='rate'?'finance.approve':'finance.write';
 try{return await (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId,kind);const current=await authority(database,actor,permission),receipt=await database.prepare("SELECT to_jsonb(r)::text image,r.record_id FROM command_receipts r WHERE r.command_id=? AND r.operation=? AND r.actor_id=? AND r.authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId).first<{image:string;record_id:string}>(),purpose=receipt?'history' as const:'new-work' as const,guard=`payroll-posting-prerequisite:${randomUUID()}`;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const before=await references(database,actor);
  const boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));await database.batch(boundaries.flatMap(item=>item.boundary.before));
  let result:Result;
  if(receipt){const replay=await replayCommand<Result>(database,actor,key,operation,receipt.record_id,request);if(!replay)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','The original Payroll prerequisite receipt is unavailable.');if(replay.id!==receipt.record_id||!before[kind].some(row=>row.id===receipt.record_id)||!before.payroll.groups.some(row=>row.id===request.payGroupId))throw changed();result=replay;}
  else{if(request.contextKey!==contextKey(current,before,request.payGroupId))throw new ApiError(409,'PAYROLL_POSTING_CONTEXT_CHANGED','Refresh the current original Payroll group and Finance prerequisites.');await validateFresh(database,actor,before,kind,request);result=await run(database);}
  const created=await references(database,actor);if(receipt?image(created)!==image(before):!createdReferenceMatches(created,kind,result.id,actor,request.input)||!unchangedBeforeCreation(before,created,kind,result.id))throw changed();
  const expectedReceipt=await database.prepare('SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?').bind(key).first<string>('image');if(!expectedReceipt||receipt&&receipt.image!==expectedReceipt)throw changed();
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));await flushFinanceDeferredEffects(database);
  if(image(created)!==image(await references(database,actor))||expectedReceipt!==await database.prepare('SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?').bind(key).first<string>('image'))throw changed();
  const final=await authority(database,actor,permission);if(stamp(current)!==stamp(final))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original Company Finance authority changed.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export type PayrollPostingPrerequisitesProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string;beta?:boolean};
/** Exact setup entry retains the original Company finance.approve selector. */
export async function payrollPostingPrerequisitesProductEntry(database:SqlDatabase,scope:PayrollPostingPrerequisitesProductScope){
 if(scope.beta||scope.branchId||scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 let iam:Awaited<ReturnType<typeof loadIamAuthority>>;try{iam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&[401,403].includes(error.status))return null;throw error;}
 const access=await readPurchasedAccesses(database,'enforced',scope.companyId,capabilities);if(!capabilities.every(capability=>access[capability].history))return null;
 const query=`WITH payroll_setup_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT a.id,a.modules_json FROM assignments a CROSS JOIN(SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id)payroll_setup_scope CROSS JOIN payroll_setup_clock
 WHERE a.id=? AND a.user_id=? AND a.company_id=payroll_setup_scope.company_id AND a.role=? AND a.version=? AND a.scope_kind='holding' AND ${assignmentReadSql('payroll_setup_scope','finance.approve')}
 AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id=payroll_setup_scope.company_id AND r.user_id=a.user_id AND r.session_id=?)
 AND ${['people_payroll','finance'].map(feature=>scheduledFeaturePredicate('payroll_setup_scope.company_id',feature)).join(' AND ')} AND ${capabilities.map(capability=>purchasedCapabilityPredicate('payroll_setup_scope.company_id',capability,'history','payroll_setup_clock.now_ms')).join(' AND ')}`;
 const bind=()=>database.prepare(query).bind(scope.companyId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,scope.assignmentId,scope.userId,scope.role,scope.sessionId);
 const row=await bind().first<{id:string;modules_json:string|string[]}>();if(!row)return null;
 const assigned=typeof row.modules_json==='string'?JSON.parse(row.modules_json) as string[]:row.modules_json,modules=effectiveWorkModules(scope.role,assigned,iam.permissions,!!scope.beta);
 if(!payrollPostingPrerequisitesPresentationAllowed({scopeKind:'holding',beta:!!scope.beta,modules,permissions:iam.permissions}))return null;
 let finalIam:typeof iam;try{finalIam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&[401,403].includes(error.status))return null;throw error;}
 const finalAccess=await readPurchasedAccesses(database,'enforced',scope.companyId,capabilities);
 if(JSON.stringify([iam.identityVersion,iam.roleRevision,iam.hrAppointmentVersion,iam.emergencyUntil])!==JSON.stringify([finalIam.identityVersion,finalIam.roleRevision,finalIam.hrAppointmentVersion,finalIam.emergencyUntil])||!capabilities.every(capability=>finalAccess[capability].history&&finalAccess[capability].revision===access[capability].revision))return null;
 const finalRow=await bind().first<{id:string;modules_json:string|string[]}>();if(!finalRow||JSON.stringify(finalRow.modules_json)!==JSON.stringify(row.modules_json))return null;
 return {revision:createHash('sha256').update(JSON.stringify([scope,modules,iam.identityVersion,iam.roleRevision,iam.hrAppointmentVersion,capabilities.map(capability=>access[capability].revision)])).digest('hex'),purchasePurpose:'history' as const};
}

/** A recovery confirmation runs original replay validation, including its
 * transient Work/IAM assertions. No new business command or receipt is made.
 * The strong package/receipt fence also covers reference writers that prelease
 * their table when enabled and original receipt-first writers when disabled. */
export async function payrollPostingPrerequisiteConfirmation(actor:WorkContext,raw:unknown,key:string):Promise<PayrollPostingPrerequisiteConfirmationResult>{
 const input=payrollPostingPrerequisiteConfirmationSchema.parse(raw),kind=input.kind,permission=kind==='rate'?'finance.approve':'finance.write',operation=operations[kind];
 if(!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,'COMMAND_KEY_REQUIRED','Use the original Idempotency-Key.');
 try{return await (await owner(actor)).transaction(async database=>{
  await lockPackageSettlementSources(database);await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();
  await database.prepare('LOCK TABLE finance_charts,finance_accounts,finance_periods,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE').run();
  await database.batch(payrollPostingPrerequisiteSourceLocks(database));await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);
  const current=await authority(database,actor,permission);for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  const before=await references(database,actor),receipt=await database.prepare("SELECT r.record_id,to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=? AND r.operation=? AND r.actor_id=? AND r.authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId).first<{record_id:string;image:string}>();
  if(!receipt)throw new ApiError(404,'PAYROLL_POSTING_CONFIRMATION_NOT_FOUND','The original Payroll prerequisite receipt is unavailable.');
  const original=await replayCommand<{id:string}>(database,actor,key,operation,receipt.record_id,input.request),native=before[kind].find(row=>row.id===receipt.record_id);
  if(!original||original.id!==receipt.record_id||!native||!before.payroll.groups.some(row=>row.id===input.request.payGroupId))throw changed();
  const source=JSON.parse(native.source) as Record<string,unknown>,state=payrollPostingPrerequisiteNativeStatusSchema.safeParse(kind==='rate'?'confirmed':kind==='period'?source.state:source.status),allowed:Record<Kind,readonly string[]>={chart:['draft','active','archived'],account:['active','inactive'],period:['open','closing','closed'],rate:['confirmed']};
  if(!state.success||!allowed[kind].includes(state.data))throw changed();
  let version:number|null=null;if(kind!=='rate'){const value=kind==='period'?source.close_version:source.version;if(typeof value!=='number'||!Number.isSafeInteger(value)||value<(kind==='period'?0:1))throw changed();version=value;}
  // Original replay assertion cleanup can defer owner effects. Flush them all
  // before final read-only source, authority and purchase observations.
  await flushFinanceDeferredEffects(database);
  if(image(before)!==image(await references(database,actor))||receipt.image!==await database.prepare('SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?').bind(key).first<string>('image'))throw changed();
  const final=await authority(database,actor,permission);if(stamp(current)!==stamp(final))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original Company Finance authority changed.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {kind,commandId:key,payGroupId:input.request.payGroupId,id:receipt.record_id,status:state.data,version};
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
