import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {commerceCommandContext,commerceFormUnchanged} from '@erpx/feature-commerce/lib/commerce-command-fetch';
import type {SalesPostingSetupView} from '@erpx/server-finance/public/sales-posting-setup';

export type SalesPostingSource={base:string;companyId:string;contextKey:string|null};
export type SalesPostingInputs={
 charts:{name:string;baseCurrency:string;accountingBasis:string};
 accounts:{chartId:string;code:string;name:string;category:'asset'|'liability'|'equity'|'revenue'|'expense'|'other_income'|'other_expense';currency:string;controlKind:'none'|'receivable'|'payable'|'bank';isControl:boolean};
 periods:{name:string;startDate:string;endDate:string};
 rates:{currency:string;effectiveDate:string;currencyPerUsd:string;note:string};
 defaults:{chartId:string;currency:string;receivableAccountId:string;revenueAccountId:string;taxAccountId?:string;reason:string};
 decision:{expectedVersion:number;action:'approve'|'reject';reason:string};
};
export type SalesPostingKind=keyof SalesPostingInputs;
export type SalesPostingRequest={ [Kind in Exclude<SalesPostingKind,'decision'>]:{kind:Kind;body:{expectedVersion:0;contextKey:string;input:SalesPostingInputs[Kind]}} }[Exclude<SalesPostingKind,'decision'>]|{kind:'decision';id:string;body:{contextKey:string;input:SalesPostingInputs['decision']}};
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const integer=(value:unknown)=>Number.isSafeInteger(value)&&Number(value)>=0;
const flags=['canCreateChart','canCreateAccount','canCreatePeriod','canConfirmRate','canPrepareDefaults'];
export function validSalesPostingSetup(value:unknown,companyId:string):value is SalesPostingSetupView{
 if(!row(value)||value.companyId!==companyId||typeof value.available!=='boolean'||!flags.every(field=>typeof value[field]==='boolean'))return false;
 for(const field of ['charts','accounts','periods','rates','defaults'])if(!Array.isArray(value[field]))return false;
 if(!value.available)return value.contextKey===null&&['authority-required','purchase-required'].includes(String(value.unavailableReason))&&flags.every(field=>value[field]===false)&&['charts','accounts','periods','rates','defaults'].every(field=>(value[field] as unknown[]).length===0);
 if(typeof value.contextKey!=='string'||!/^[a-f0-9]{64}$/.test(value.contextKey)||value.unavailableReason!==null)return false;
 return (value.charts as unknown[]).every(item=>row(item)&&['id','name','baseCurrency'].every(field=>text(item[field])))
  &&(value.accounts as unknown[]).every(item=>row(item)&&['id','chartId','code','name','category','currency','controlKind'].every(field=>text(item[field])))
  &&(value.periods as unknown[]).every(item=>row(item)&&['id','name','startDate','endDate','state'].every(field=>text(item[field]))&&integer(item.version))
  &&(value.rates as unknown[]).every(item=>row(item)&&['id','currency','effectiveDate','currencyPerUsd'].every(field=>text(item[field]))&&typeof item.confirmedAt==='number'&&Number.isFinite(item.confirmedAt))
  &&(value.defaults as unknown[]).every(item=>row(item)&&['id','chartId','currency','receivableAccountId','revenueAccountId','reason'].every(field=>text(item[field]))&&(item.taxAccountId===null||text(item.taxAccountId))&&['draft','approved','rejected','superseded'].includes(String(item.status))&&integer(item.version)&&Number(item.version)>0&&(item.reviewReason===null||typeof item.reviewReason==='string')&&typeof item.canReview==='boolean'&&typeof item.canApprove==='boolean'&&typeof item.canReject==='boolean'&&typeof item.usable==='boolean');
}
/** Each receipt follows its original owner operation, including period version 0
 * and the rate confirmation's deliberately absent Company/version fields. */
export function validSalesPostingReceipt(value:unknown,request:SalesPostingRequest,companyId:string,key:string):value is Record<string,unknown>&{id:string}{
 if(!row(value)||!text(value.id))return false;
 const input=request.body.input;
 if(request.kind==='decision')return 'action' in input&&value.id===request.id&&value.version===input.expectedVersion+1&&value.status===(input.action==='approve'?'approved':'rejected');
 if(request.kind==='defaults')return value.id===`sales-defaults-${key}`&&value.status==='draft'&&value.version===1;
 if(request.kind==='rates')return 'effectiveDate' in input&&value.currency===input.currency&&value.effectiveDate===input.effectiveDate&&value.currencyPerUsd===input.currencyPerUsd&&typeof value.confirmedAt==='number'&&Number.isFinite(value.confirmedAt);
 if(value.companyId!==companyId)return false;
 if(request.kind==='charts')return 'baseCurrency' in input&&value.version===1&&value.name===input.name.trim()&&value.baseCurrency===input.baseCurrency.toUpperCase()&&value.accountingBasis===input.accountingBasis.trim();
 if(request.kind==='periods')return 'startDate' in input&&value.version===0&&value.state==='open'&&value.name===input.name.trim()&&value.startDate===input.startDate&&value.endDate===input.endDate;
 return 'code' in input&&value.version===1&&value.chartId===input.chartId&&value.code===input.code.trim()&&value.name===input.name.trim()&&value.category===input.category&&value.currency===input.currency.toUpperCase()&&value.isControl===input.isControl&&value.controlKind===input.controlKind;
}
type Attempt={source:SalesPostingSource;context:string;request:SalesPostingRequest;body:string;key:string;uncertain:boolean;applied:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
export function createSalesPostingCommands(current:()=>SalesPostingSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 const same=(a:SalesPostingSource,b:SalesPostingSource)=>a.base===b.base&&a.companyId===b.companyId;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original Sales setup request.');
  const before=current();if(!before||!same(before,captured.source)||commerceCommandContext()!==captured.context)throw Error('Return to the original Company and work role to confirm the original request.');
  const source={...before},assertWork=captureWorkCommandContext(),wasUncertain=captured.uncertain,url=`/api/v1${source.base}/commerce/sales-posting-setup`;
  const assertCurrent=()=>{assertWork();const live=current();if(!live||!same(source,live)||source.contextKey!==live.contextKey||commerceCommandContext()!==captured.context)throw Object.assign(Error('Sales setup context changed. Your original draft and request are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  busy=true;
  try{
   const suffix=captured.request.kind==='decision'?`defaults/${encodeURIComponent(captured.request.id)}/decide`:captured.request.kind;
   const response=await send(`${url}/${suffix}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   if(!response.ok){if(!wasUncertain&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED'){assertCurrent();attempt=null;}throw Object.assign(Error(body.error?.message??'Unable to confirm Sales setup.'),{code:body.error?.code});}
   if(!validSalesPostingReceipt(body.data,captured.request,source.companyId,captured.key))throw Error('The Sales setup receipt is incomplete. Retry the original request.');
   const receipt=body.data;captured.applied=true;assertCurrent();
   const refreshed=await send(url,{cache:'no-store'}),decoded=await refreshed.json() as {data?:unknown;error?:{message?:string}};
   if(!refreshed.ok||!validSalesPostingSetup(decoded.data,source.companyId)||!decoded.data.available)throw Error(decoded.error?.message??'The command was saved. Reload its current status with the original request.');
   const collection=captured.request.kind==='decision'?decoded.data.defaults:decoded.data[captured.request.kind];
   if(!collection.some(item=>item.id===receipt.id))throw Error('The saved result is not in the current owner status. Retry the original confirmation.');
   assertCurrent();captured.uncertain=true;const view=decoded.data;
   return {view,receipt,submitted:captured.request,unchanged:()=>{const completion=captured.completion();return captured.hadCompletion?!!completion&&commerceFormUnchanged(completion):true;},acknowledge(){
    assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();
    if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,commerceAcknowledged:true}}));announceWorkChange();return true;
   }};
  }catch(error){if(attempt===captured)captured.uncertain=true;throw error;}finally{busy=false;}
 }
 return {submit(request:SalesPostingRequest,completion?:WorkCommandCompletion){
  const source=current();if(!source||request.body.contextKey!==source.contextKey)throw Error('Review current Sales setup references before submitting. Your draft is retained.');const body=JSON.stringify(request.body);
  if(attempt&&(attempt.body!==body||attempt.request.kind!==request.kind||!same(source,attempt.source)||attempt.context!==commerceCommandContext()||attempt.request.kind==='decision'&&request.kind==='decision'&&attempt.request.id!==request.id))throw Error('Confirm the original request before starting another Sales setup command.');
  attempt??={source:{...source},context:commerceCommandContext(),request,body,key:crypto.randomUUID(),uncertain:false,applied:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);
 },retry(){if(!attempt)throw Error('There is no original Sales setup request to retry.');return run(attempt);},pending:()=>attempt!==null,applied:()=>attempt?.applied??false};
}
