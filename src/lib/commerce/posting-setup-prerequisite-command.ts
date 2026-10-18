import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {commerceCommandContext,commerceFormUnchanged} from './commerce-command-fetch';
import type {PostingSetupSource} from './posting-setup-command';
import type {CommercePostingSetupView} from '@/server/modules/commerce/public/posting-setup';
import type {SupplyPostingPrerequisitesView} from '@/server/modules/finance/public/supply-posting-prerequisites';

export type PostingPrerequisiteInputs={
 charts:{name:string;baseCurrency:string;accountingBasis:string};
 accounts:{chartId:string;code:string;name:string;category:'asset'|'liability'|'equity'|'revenue'|'expense'|'other_income'|'other_expense';currency:string;controlKind:'none'|'receivable'|'payable'|'bank';isControl:boolean};
 periods:{name:string;startDate:string;endDate:string};
 rates:{currency:string;effectiveDate:string;currencyPerUsd:string;note:string};
};
export type PostingPrerequisiteKind=keyof PostingPrerequisiteInputs;
export type PostingPrerequisiteRequest={[Kind in PostingPrerequisiteKind]:{kind:Kind;body:{expectedVersion:0;contextKey:string;input:PostingPrerequisiteInputs[Kind]}}}[PostingPrerequisiteKind];
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const key=(value:unknown)=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const charts=(value:unknown):value is SupplyPostingPrerequisitesView['charts']=>Array.isArray(value)&&value.every(item=>row(item)&&text(item.id)&&text(item.name)&&text(item.baseCurrency));
const accounts=(value:unknown):value is SupplyPostingPrerequisitesView['accounts']=>Array.isArray(value)&&value.every(item=>row(item)&&['id','chartId','code','name','category','currency'].every(field=>text(item[field]))&&typeof item.controlKind==='string');
export function validPostingPrerequisites(value:unknown,companyId:string):value is SupplyPostingPrerequisitesView{
 if(!row(value)||value.companyId!==companyId||typeof value.available!=='boolean'||!['canCreateChart','canCreateAccount','canCreatePeriod','canConfirmRate'].every(field=>typeof value[field]==='boolean')||!charts(value.charts)||!accounts(value.accounts)||!Array.isArray(value.periods)||!Array.isArray(value.rates))return false;
 if(!value.periods.every(item=>row(item)&&['id','name','startDate','endDate','state'].every(field=>text(item[field]))&&Number.isSafeInteger(item.version))||!value.rates.every(item=>row(item)&&['id','currency','effectiveDate','currencyPerUsd'].every(field=>text(item[field]))&&typeof item.confirmedAt==='number'&&Number.isFinite(item.confirmedAt)))return false;
 return value.available?typeof value.configured==='boolean'&&key(value.contextKey)&&(value.unavailableReason===null||value.unavailableReason==='history-requires-review'):value.configured===null&&value.contextKey===null&&['authority-required','purchase-required','history-requires-review'].includes(String(value.unavailableReason))&&value.charts.length===0&&value.accounts.length===0&&value.periods.length===0&&value.rates.length===0&&['canCreateChart','canCreateAccount','canCreatePeriod','canConfirmRate'].every(field=>value[field]===false);
}
const postingFields=['chartId','currency','receivableAccountId','payableAccountId','revenueAccountId','inventoryAccountId','clearingAccountId','cogsAccountId','inputTaxAccountId','outputTaxAccountId','cashAccountId'];
export function validPostingSetupSeed(value:unknown,companyId:string):value is CommercePostingSetupView{
 if(!row(value)||value.companyId!==companyId||typeof value.available!=='boolean'||typeof value.canConfigure!=='boolean'||!charts(value.charts)||!accounts(value.accounts))return false;
 if(!value.available)return value.configured===null&&value.version===null&&value.contextKey===null&&value.configuration===null&&value.canConfigure===false&&value.charts.length===0&&value.accounts.length===0&&['authority-required','purchase-required'].includes(String(value.unavailableReason));
 return key(value.contextKey)&&value.unavailableReason===null&&(value.configured===false&&value.version===0&&value.configuration===null||value.configured===true&&value.version===1&&row(value.configuration)&&postingFields.every(field=>text(value.configuration&&row(value.configuration)?value.configuration[field]:null)));
}
/** The original four Finance operations deliberately have different receipts. */
export function validPostingPrerequisiteReceipt(value:unknown,request:PostingPrerequisiteRequest,companyId:string):value is Record<string,unknown>&{id:string}{
 if(!row(value)||!text(value.id))return false;
 const input=request.body.input;
 if(request.kind==='rates')return 'effectiveDate' in input&&value.currency===input.currency&&value.effectiveDate===input.effectiveDate&&value.currencyPerUsd===input.currencyPerUsd&&typeof value.confirmedAt==='number'&&Number.isFinite(value.confirmedAt);
 if(value.companyId!==companyId)return false;
 if(request.kind==='charts')return 'baseCurrency' in input&&value.version===1&&value.name===input.name.trim()&&value.baseCurrency===input.baseCurrency.toUpperCase()&&value.accountingBasis===input.accountingBasis.trim();
 if(request.kind==='periods')return 'startDate' in input&&value.version===0&&value.state==='open'&&value.name===input.name.trim()&&value.startDate===input.startDate&&value.endDate===input.endDate;
 return 'chartId' in input&&value.version===1&&value.chartId===input.chartId&&value.code===input.code.trim()&&value.name===input.name.trim()&&value.category===input.category&&value.currency===input.currency.toUpperCase()&&value.controlKind===input.controlKind&&value.isControl===input.isControl;
}
type Attempt={request:PostingPrerequisiteRequest;source:PostingSetupSource;workContext:string;body:string;key:string;uncertain:boolean;applied:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
/** One original creation at a time. No key, draft or event is finalized before
 * the original receipt and both fresh, minimal owner reads are confirmed. */
export function createPostingPrerequisiteCommands(current:()=>PostingSetupSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 const same=(a:PostingSetupSource,b:PostingSetupSource)=>a.base===b.base&&a.companyId===b.companyId;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the submitted preparation command.');
  const before=current();if(!before||!same(before,captured.source)||commerceCommandContext()!==captured.workContext)throw Error('Return to the original Company and work role before retrying preparation.');
  const source={...before},assertWork=captureWorkCommandContext(),wasUncertain=captured.uncertain,url=`/api/v1${source.base}/commerce/posting-setup`;
  const assertCurrent=()=>{assertWork();const live=current();if(!live||!same(source,live)||source.contextKey!==live.contextKey||commerceCommandContext()!==captured.workContext)throw Object.assign(Error('Preparation context changed. Your original request and draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  busy=true;
  try{
   const response=await send(`${url}/${captured.request.kind}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   if(!response.ok){if(!wasUncertain&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED'){assertCurrent();attempt=null;}throw Object.assign(Error(body.error?.message??'Preparation could not be confirmed.'),{code:body.error?.code});}
   if(!validPostingPrerequisiteReceipt(body.data,captured.request,source.companyId))throw Error('Preparation receipt is incomplete. Retry the original request.');
   const receipt=body.data;captured.applied=true;assertCurrent();
   const fresh=await send(`${url}/prerequisites`,{cache:'no-store'}),prepared=await fresh.json() as {data?:unknown;error?:{message?:string}};
   if(!fresh.ok||!validPostingPrerequisites(prepared.data,source.companyId)||!prepared.data.available)throw Error(prepared.error?.message??'The command was saved. Reload its current preparation status using the same request.');
   const collections={charts:prepared.data.charts,accounts:prepared.data.accounts,periods:prepared.data.periods,rates:prepared.data.rates};
   if(!collections[captured.request.kind].some(item=>item.id===receipt.id))throw Error('The saved reference is not in the current owner status. Retry the original confirmation.');
   assertCurrent();
   const mappingResponse=await send(url,{cache:'no-store'}),mapping=await mappingResponse.json() as {data?:unknown;error?:{message?:string}};
   if(!mappingResponse.ok||!validPostingSetupSeed(mapping.data,source.companyId))throw Error(mapping.error?.message??'The command was saved. Reload the current mapping choices using the same request.');
   assertCurrent();captured.uncertain=true;const prerequisites=prepared.data,posting=mapping.data;
   return {prerequisites,posting,receipt,submitted:captured.request,unchanged:()=>{const completion=captured.completion();return captured.hadCompletion?!!completion&&commerceFormUnchanged(completion):true;},acknowledge(){
    assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();
    if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,commerceAcknowledged:true}}));announceWorkChange();return true;
   }};
  }catch(error){if(attempt===captured)captured.uncertain=true;throw error;}finally{busy=false;}
 }
 return {submit(request:PostingPrerequisiteRequest,completion?:WorkCommandCompletion){
  const source=current();if(!source||request.body.contextKey!==source.contextKey)throw Error('Reload preparation before starting a new request. Your draft is retained.');const body=JSON.stringify(request.body);
  if(attempt&&(attempt.body!==body||attempt.request.kind!==request.kind||!same(source,attempt.source)||attempt.workContext!==commerceCommandContext()))throw Error('Retry the submitted preparation first. Your draft is retained.');
  attempt??={request,source:{...source},workContext:commerceCommandContext(),body,key:crypto.randomUUID(),uncertain:false,applied:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);
 },retry(){if(!attempt)throw Error('There is no submitted preparation to retry.');return run(attempt);},pending:()=>attempt!==null,applied:()=>attempt?.applied??false};
}
