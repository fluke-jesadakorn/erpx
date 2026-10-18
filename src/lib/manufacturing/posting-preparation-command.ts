import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {manufacturingCommandContext,manufacturingFormUnchanged} from './command-fetch';
import type {ManufacturingPostingSource} from './posting-setup-command';
import type {ManufacturingPostingPrerequisitesView,ManufacturingPostingChartInput,ManufacturingPostingAccountInput,ManufacturingPostingPeriodInput,ManufacturingPostingRateInput} from '@/server/modules/finance/public/manufacturing-posting-prerequisites';
import type {ManufacturingPostingProfilesView,ManufacturingPostingProfileInput,ManufacturingPostingProfileDecisionInput} from '@/server/modules/manufacturing/public/posting-profiles';
export type ManufacturingPreparationInputs={charts:ManufacturingPostingChartInput['input'];accounts:ManufacturingPostingAccountInput['input'];periods:ManufacturingPostingPeriodInput['input'];rates:ManufacturingPostingRateInput['input'];profiles:ManufacturingPostingProfileInput['input'];decision:ManufacturingPostingProfileDecisionInput['input']};
export type ManufacturingPreparationKind=keyof ManufacturingPreparationInputs;
export type ManufacturingPreparationRequest={kind:'charts';body:ManufacturingPostingChartInput}|{kind:'accounts';body:ManufacturingPostingAccountInput}|{kind:'periods';body:ManufacturingPostingPeriodInput}|{kind:'rates';body:ManufacturingPostingRateInput}|{kind:'profiles';body:ManufacturingPostingProfileInput}|{kind:'decision';id:string;body:ManufacturingPostingProfileDecisionInput};
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown)=>typeof value==='string'&&value.length>0;
const context=(value:unknown)=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const charts=(value:unknown):value is unknown[]=>Array.isArray(value)&&value.every(item=>row(item)&&['id','name','baseCurrency'].every(field=>text(item[field])));
const accounts=(value:unknown):value is unknown[]=>Array.isArray(value)&&value.every(item=>row(item)&&['id','chartId','code','name','category','currency','controlKind'].every(field=>text(item[field])));
const flags=['canCreateChart','canCreateAccount','canCreatePeriod','canConfirmRate'];
export function validManufacturingPrerequisites(value:unknown,companyId:string):value is ManufacturingPostingPrerequisitesView{
 if(!row(value)||value.companyId!==companyId||typeof value.available!=='boolean'||value.functionalCurrency!=='THB'||typeof value.unsupportedExistingMapping!=='boolean'||!flags.every(field=>typeof value[field]==='boolean')||!charts(value.charts)||!accounts(value.accounts)||!Array.isArray(value.periods)||!Array.isArray(value.rates))return false;
 if(!value.periods.every(item=>row(item)&&['id','name','startDate','endDate','state'].every(field=>text(item[field]))&&Number.isSafeInteger(item.version))||!value.rates.every(item=>row(item)&&['id','currency','effectiveDate','currencyPerUsd'].every(field=>text(item[field]))&&typeof item.confirmedAt==='number'&&Number.isFinite(item.confirmedAt)))return false;
 return value.available?typeof value.configured==='boolean'&&context(value.contextKey)&&[null,'history-requires-review'].includes(value.unavailableReason as string|null):value.configured===null&&value.contextKey===null&&['authority-required','purchase-required','history-requires-review'].includes(String(value.unavailableReason))&&['charts','accounts','periods','rates'].every(field=>(value[field] as unknown[]).length===0)&&flags.every(field=>value[field]===false);
}
const profileFields=['chartId','inventoryAccountId','wipAccountId','laborAccountId','overheadAccountId','scrapAccountId'] as const;
export function validManufacturingProfiles(value:unknown,companyId:string):value is ManufacturingPostingProfilesView{
 if(!row(value)||value.companyId!==companyId||typeof value.available!=='boolean'||typeof value.canPrepare!=='boolean'||typeof value.unsupportedExistingMapping!=='boolean'||!charts(value.charts)||!accounts(value.accounts)||!Array.isArray(value.profiles)||![null,'THB'].includes(value.currency as string|null)||(value.chartId!==null&&!text(value.chartId)))return false;
 if(!(value.accounts as unknown[]).every(item=>row(item)&&text(item.status))||!value.profiles.every(item=>row(item)&&text(item.id)&&text(item.currency)&&profileFields.every(field=>text(item[field]))&&['draft','approved','rejected','retired'].includes(String(item.status))&&Number.isSafeInteger(item.version)&&Number(item.version)>0&&['canApprove','canReject','canRetire'].every(field=>typeof item[field]==='boolean')))return false;
 if(!value.available)return value.contextKey===null&&!value.canPrepare&&value.currency===null&&value.chartId===null&&value.charts.length===0&&value.accounts.length===0&&value.profiles.length===0&&['authority-required','purchase-required'].includes(String(value.unavailableReason));
 return context(value.contextKey)&&[null,'posting-mapping-required','unsupported-existing-mapping'].includes(value.unavailableReason as string|null)&&(!value.canPrepare||value.unavailableReason===null&&!value.unsupportedExistingMapping&&value.currency==='THB'&&text(value.chartId));
}
export function validManufacturingPreparationReceipt(value:unknown,request:ManufacturingPreparationRequest,companyId:string,key:string):value is Record<string,unknown>&{id:string}{
 if(!row(value)||!text(value.id))return false;
 if(request.kind==='profiles')return value.id===`mfg-accounting-${key}`&&value.status==='draft'&&value.version===1;
 if(request.kind==='decision')return value.id===request.id&&value.version===request.body.input.expectedVersion+1&&value.status===({approve:'approved',reject:'rejected',retire:'retired'} as const)[request.body.input.action];
 if(request.kind==='rates'){const input=request.body.input;return value.currency===input.currency&&value.effectiveDate===input.effectiveDate&&value.currencyPerUsd===input.currencyPerUsd&&typeof value.confirmedAt==='number'&&Number.isFinite(value.confirmedAt);}
 if(value.companyId!==companyId)return false;
 if(request.kind==='charts'){const input=request.body.input;return value.version===1&&value.name===input.name.trim()&&value.baseCurrency===input.baseCurrency.toUpperCase()&&value.accountingBasis===input.accountingBasis.trim();}
 if(request.kind==='periods'){const input=request.body.input;return value.version===0&&value.state==='open'&&value.name===input.name.trim()&&value.startDate===input.startDate&&value.endDate===input.endDate;}
 const input=request.body.input;return value.version===1&&value.chartId===input.chartId&&value.code===input.code.trim()&&value.name===input.name.trim()&&value.category===input.category&&value.currency===(input.currency??'THB').toUpperCase()&&value.isControl===input.isControl&&value.controlKind===input.controlKind;
}
type Attempt={source:ManufacturingPostingSource;work:string;request:ManufacturingPreparationRequest;body:string;key:string;uncertain:boolean;applied:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
const identity=(request:ManufacturingPreparationRequest)=>JSON.stringify([request.kind,request.kind==='decision'?request.id:null]);
const same=(a:ManufacturingPostingSource,b:ManufacturingPostingSource)=>a.base===b.base&&a.companyId===b.companyId;
export function createManufacturingPreparationCommands(current:(kind:ManufacturingPreparationKind)=>ManufacturingPostingSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original Manufacturing preparation request.');
  const before=current(captured.request.kind);if(!before||!same(before,captured.source)||manufacturingCommandContext()!==captured.work)throw Error('Return to the original Company and work role to confirm preparation.');
  const source={...before},assertWork=captureWorkCommandContext(),wasUncertain=captured.uncertain,url=`/api/v1${source.base}/production/posting-setup`,profile=captured.request.kind==='profiles'||captured.request.kind==='decision';
  const assertCurrent=()=>{assertWork();const live=current(captured.request.kind);if(!live||!same(source,live)||source.contextKey!==live.contextKey||manufacturingCommandContext()!==captured.work)throw Object.assign(Error('The preparation context changed. Your original body, key and draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  assertCurrent();busy=true;
  try{
   const suffix=captured.request.kind==='decision'?`profiles/${encodeURIComponent(captured.request.id)}/decide`:captured.request.kind;
   const response=await send(`${url}/${suffix}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   if(!response.ok){if(!wasUncertain&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED'){assertCurrent();attempt=null;}throw Object.assign(Error(body.error?.message??'Manufacturing preparation was not confirmed.'),{code:body.error?.code});}
   if(!validManufacturingPreparationReceipt(body.data,captured.request,source.companyId,captured.key))throw Error('The original owner receipt is incomplete. Retry its confirmation.');
   const receipt=body.data;captured.applied=true;assertCurrent();const responseView=await send(`${url}/${profile?'profiles':'prerequisites'}`,{cache:'no-store'}),decoded=await responseView.json() as {data?:unknown;error?:{message?:string}};
   const view=decoded.data;if(!responseView.ok||!(profile?validManufacturingProfiles(view,source.companyId):validManufacturingPrerequisites(view,source.companyId)))throw Error(decoded.error?.message??'The command was saved. Reload its current status with the original request.');
   if(validManufacturingProfiles(view,source.companyId)){
    const saved=view.profiles.find(item=>item.id===receipt.id),input=captured.request.kind==='profiles'?captured.request.body.input:null;if(!saved||saved.version<Number(receipt.version)||input&&!profileFields.every(field=>saved[field]===input[field]))throw Error('The saved profile is not in the current owner status. Retry the original confirmation.');
   }else if(validManufacturingPrerequisites(view,source.companyId)&&captured.request.kind!=='profiles'&&captured.request.kind!=='decision'){
    if(!view[captured.request.kind].some(item=>item.id===receipt.id))throw Error('The saved reference is not in the current owner status. Retry the original confirmation.');
   }else throw Error('The refreshed owner status does not match the submitted command.');
   assertCurrent();captured.uncertain=true;return{view,receipt,submitted:captured.request,unchanged:()=>{const completion=captured.completion();return captured.hadCompletion?!!completion&&manufacturingFormUnchanged(completion):true;},acknowledge(){
    assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,manufacturingAcknowledged:true}}));announceWorkChange();return true;
   }};
  }catch(error){if(attempt===captured)captured.uncertain=true;throw error;}finally{busy=false;}
 }
 return {submit(request:ManufacturingPreparationRequest,completion?:WorkCommandCompletion){
  const source=current(request.kind);if(!source||request.body.contextKey!==source.contextKey)throw Error('Review current Manufacturing preparation references before submitting.');const body=JSON.stringify(request.body);
  if(attempt&&(attempt.body!==body||identity(attempt.request)!==identity(request)||!same(source,attempt.source)||manufacturingCommandContext()!==attempt.work))throw Error('Confirm the original preparation before starting another command.');
  attempt??={source:{...source},work:manufacturingCommandContext(),request,body,key:crypto.randomUUID(),uncertain:false,applied:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);
 },retry(){if(!attempt)throw Error('There is no original preparation to confirm.');return run(attempt);},pending:()=>attempt!==null,applied:()=>attempt?.applied??false};
}
