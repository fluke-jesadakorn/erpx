import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {commerceCommandContext,commerceFormUnchanged} from '@erpx/feature-commerce/lib/commerce-command-fetch';
import type {SalesGoodsPostingSetupInput,SalesGoodsPostingSetupView} from '@erpx/server-commerce/public/sales-goods-posting-setup';

export type SalesGoodsSource={base:string;companyId:string;contextKey:string|null};
export const salesGoodsAccountFields=['receivableAccountId','payableAccountId','revenueAccountId','inventoryAccountId','clearingAccountId','cogsAccountId','inputTaxAccountId','outputTaxAccountId','cashAccountId'] as const;
const mappingFields=['chartId','currency',...salesGoodsAccountFields] as const;
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown)=>typeof value==='string'&&value.length>0;
export function validSalesGoodsSetup(value:unknown,companyId:string):value is SalesGoodsPostingSetupView{
 if(!row(value)||value.companyId!==companyId||typeof value.available!=='boolean'||typeof value.canConfigure!=='boolean'||typeof value.requiresApprovedDefaults!=='boolean'||!Array.isArray(value.charts)||!Array.isArray(value.accounts))return false;
 if(value.expectedSalesDefaults!==null&&(!row(value.expectedSalesDefaults)||!text(value.expectedSalesDefaults.id)||!Number.isSafeInteger(value.expectedSalesDefaults.version)||Number(value.expectedSalesDefaults.version)<1))return false;
 if(!value.charts.every(item=>row(item)&&['id','name','baseCurrency'].every(field=>text(item[field])))||!value.accounts.every(item=>row(item)&&['id','chartId','code','name','category','currency','controlKind'].every(field=>text(item[field]))))return false;
 if(!value.available)return value.contextKey===null&&value.configuration===null&&value.configured===null&&value.version===null&&!value.canConfigure&&value.expectedSalesDefaults===null&&value.charts.length===0&&value.accounts.length===0&&['authority-required','purchase-required'].includes(String(value.unavailableReason));
 if(typeof value.contextKey!=='string'||!/^[a-f0-9]{64}$/.test(value.contextKey)||![null,'goods-history-requires-review','sales-defaults-required'].includes(value.unavailableReason as string|null))return false;
 if(value.canConfigure&&(value.unavailableReason!==null||value.requiresApprovedDefaults&&value.expectedSalesDefaults===null))return false;
 return value.configured===true&&value.version===1&&row(value.configuration)&&mappingFields.every(field=>text((value.configuration as Record<string,unknown>)[field]))&&!value.canConfigure||value.configured===false&&value.version===0&&value.configuration===null;
}
const same=(a:SalesGoodsSource,b:SalesGoodsSource)=>a.base===b.base&&a.companyId===b.companyId;
/** Explicit refresh only. A parent reference command or a user request may
 * reload this seed; matching SSR content never causes a mount GET. */
export async function readSalesGoodsSetup(current:()=>SalesGoodsSource|null,send:typeof workFetch=workFetch,signal?:AbortSignal){
 const source=current();if(!source)throw Error('Return to the original Sales posting page.');const captured={...source},assertWork=captureWorkCommandContext();
 const response=await send(`/api/v1${captured.base}/commerce/sales-goods-posting-setup`,{cache:'no-store',signal}),body=await response.json() as {data?:unknown;error?:{message?:string}};
 assertWork();const live=current();if(signal?.aborted||!live||!same(captured,live)||captured.contextKey!==live.contextKey)throw Object.assign(Error('Your Goods setup context changed. The current draft is retained.'),{code:'WORK_CONTEXT_CHANGED'});
 if(!response.ok||!validSalesGoodsSetup(body.data,captured.companyId))throw Error(body.error?.message??'Current Goods posting status could not be loaded.');return body.data;
}
type Attempt={source:SalesGoodsSource;work:string;input:SalesGoodsPostingSetupInput;body:string;key:string;uncertain:boolean;applied:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
export function createSalesGoodsCommands(current:()=>SalesGoodsSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original Goods posting setup request.');
  const source=current();if(!source||!same(source,captured.source)||commerceCommandContext()!==captured.work)throw Error('Return to the original Company and work role before confirming Goods setup.');
  const before={...source},assertWork=captureWorkCommandContext(),wasUncertain=captured.uncertain,url=`/api/v1${before.base}/commerce/sales-goods-posting-setup`;
  const assertCurrent=()=>{assertWork();const live=current();if(!live||!same(before,live)||before.contextKey!==live.contextKey||commerceCommandContext()!==captured.work)throw Object.assign(Error('Goods setup context changed. Your original body, key and draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  busy=true;
  try{
   const response=await send(url,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   if(!response.ok){if(!wasUncertain&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED'){assertCurrent();attempt=null;}throw Object.assign(Error(body.error?.message??'Goods posting setup was not confirmed.'),{code:body.error?.code});}
   if(!row(body.data)||body.data.id!==before.companyId||body.data.status!=='configured'||body.data.version!==1)throw Error('Goods posting receipt is incomplete. Confirm the original setup.');
   captured.applied=true;assertCurrent();const refreshed=await send(url,{cache:'no-store'}),decoded=await refreshed.json() as {data?:unknown;error?:{message?:string}};
   if(!refreshed.ok||!validSalesGoodsSetup(decoded.data,before.companyId)||!decoded.data.available||!decoded.data.configured||!decoded.data.configuration||!mappingFields.every(field=>decoded.data&&row(decoded.data)&&row(decoded.data.configuration)&&decoded.data.configuration[field]===captured.input[field]))throw Error(decoded.error?.message??'The mapping was saved. Reload its current status using the original request.');
   assertCurrent();captured.uncertain=true;const view=decoded.data;
   return {view,submitted:captured.input,unchanged:()=>{const completion=captured.completion();return captured.hadCompletion?!!completion&&commerceFormUnchanged(completion):true;},acknowledge(){
    assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,commerceAcknowledged:true}}));announceWorkChange();return true;
   }};
  }catch(error){if(attempt===captured)captured.uncertain=true;throw error;}finally{busy=false;}
 }
 return {submit(input:SalesGoodsPostingSetupInput,completion?:WorkCommandCompletion){
  const source=current();if(!source||input.contextKey!==source.contextKey)throw Error('Review current Goods posting references before starting a new setup.');const body=JSON.stringify(input);
  if(attempt&&(attempt.body!==body||!same(source,attempt.source)||commerceCommandContext()!==attempt.work))throw Error('Confirm the original Goods setup first. Your newer selections are retained.');
  attempt??={source:{...source},work:commerceCommandContext(),input,body,key:crypto.randomUUID(),uncertain:false,applied:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);
 },retry(){if(!attempt)throw Error('There is no original Goods setup to confirm.');return run(attempt);},pending:()=>attempt!==null,applied:()=>attempt?.applied??false};
}
