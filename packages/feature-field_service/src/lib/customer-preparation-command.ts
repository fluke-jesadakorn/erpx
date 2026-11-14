import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {contextGeneration} from '@erpx/feature-work_projection/lib/context-switch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import type {ServiceCustomerInput,ServiceCustomerResult,ServiceCustomerConfirmation} from '@erpx/feature-field_service/lib/customer-preparation-model';
import {decimalToMinor} from '@erpx/feature-finance/lib/finance-money';

export type ServiceCustomerSource={base:string;companyId:string;workspaceKey:string;contextKey:string|null};
const work=()=>{const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?JSON.stringify([node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workAssignmentVersion]):'server';};
const owner=(source:ServiceCustomerSource)=>JSON.stringify([source.base,source.companyId,source.workspaceKey]);
const route=()=>typeof window==='undefined'?'server':window.location.pathname+window.location.search;
const record=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
export function validServiceCustomerResult(value:unknown,input:ServiceCustomerInput['input']):value is ServiceCustomerResult{
 return record(value)&&typeof value.id==='string'&&/^party-.+/.test(value.id)&&value.kind==='customer'&&value.code===input.code&&value.name===input.name&&value.currency===input.currency&&value.taxRegistration===input.taxRegistration&&value.paymentTermsDays===input.paymentTermsDays&&value.creditLimit===input.creditLimit&&value.creditLimitMinor===decimalToMinor(input.creditLimit);
}
export function validServiceCustomerConfirmation(value:unknown,result:ServiceCustomerResult,key:string,companyId:string):value is ServiceCustomerConfirmation{
 return record(value)&&value.commandId===key&&value.companyId===companyId&&value.id===result.id&&value.kind==='customer'&&['active','blocked'].includes(String(value.status))&&['code','name','currency'].every(field=>typeof value[field]==='string'&&value[field].length>0);
}
type Attempt={busy:boolean;source:ServiceCustomerSource;work:string;route:string;body:string;request:ServiceCustomerInput;key:string;uncertain:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
// Retain only within this browser document and its rendered Work boundary.
// A component/RSC remount must recover the same original bytes and key.
const listeners=new Map<string,Set<()=>void>>();
const retained=new Map<string,Attempt>(),boundaries=new WeakMap<object,number>();let boundarySequence=0;
export function createServiceCustomerCommands(current:()=>ServiceCustomerSource|null,send:typeof workFetch=workFetch){
 let storeKey:string|null=null;
 const get=()=>{if(!storeKey){const source=current(),node=typeof document==='undefined'?null:document.querySelector('[data-work-revision]');if(!source||!node)return null;let id=boundaries.get(node);if(id===undefined){id=++boundarySequence;boundaries.set(node,id);}storeKey=JSON.stringify([id,owner(source),work(),route(),contextGeneration()]);}return retained.get(storeKey)??null;};
 const publish=()=>{if(storeKey)for(const listener of listeners.get(storeKey)??[])listener();};
 const set=(value:Attempt|null)=>{get();if(!storeKey)return;if(value)retained.set(storeKey,value);else{retained.delete(storeKey);publish();}};
 async function execute(captured:Attempt){
  if(captured.busy)throw Error('Wait for the original customer preparation request.');
  const source=current();if(!source||owner(source)!==owner(captured.source)||work()!==captured.work||route()!==captured.route)throw Error('Return to the original Service workspace and work role to confirm this customer.');
  const assertWork=captureWorkCommandContext(),invocation={...source},assertCurrent=()=>{assertWork();const live=current();if(!live||owner(live)!==owner(invocation)||live.contextKey!==invocation.contextKey||work()!==captured.work||route()!==captured.route)throw Object.assign(Error('The Service customer context changed. Your original request and draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  const priorUncertain=captured.uncertain;captured.busy=true;publish();let sent=false;
  try{
   assertCurrent();sent=true;
   const init={method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},response=await send(`/api/v1${source.base}/field-service/customer-preparation`,init,captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};assertCurrent();
   if(!response.ok){
    // Rust rejects these exact first-response paths before owner effects.
    // A previously uncertain request retains its original bytes and key.
    const editable=response.status===422&&['VALIDATION_FAILED','SERVICE_CUSTOMER_INPUT_INVALID','INVALID_JSON','INVALID_MONEY','MONEY_SCALE','MONEY_OVERFLOW','CURRENCY_SCALE_REQUIRED','FINANCE_CURRENCY_SCALE_UNSUPPORTED'].includes(body.error?.code??'')||response.status===409&&['SERVICE_CUSTOMER_CONTEXT_CHANGED','SERVICE_CUSTOMER_CODE_EXISTS'].includes(body.error?.code??'');
    if(!priorUncertain&&editable)set(null);
    throw Object.assign(Error(body.error?.message??'The original customer preparation result is uncertain. Retry the original request.'),{code:body.error?.code});
   }
   if(!validServiceCustomerResult(body.data,captured.request.input))throw Error('The original customer receipt is incomplete. Retry the same request.');const result=body.data;
   const confirmed=await send(`/api/v1${source.base}/field-service/customer-preparation/confirmations`,init,undefined,'caller'),confirmationBody=await confirmed.json() as {data?:unknown;error?:{message?:string}};assertCurrent();
   if(!confirmed.ok)throw Error(confirmationBody.error?.message??'Confirm the original customer from its saved receipt.');
   if(!validServiceCustomerConfirmation(confirmationBody.data,result,captured.key,source.companyId))throw Error('The saved customer does not match the original receipt. Retry the original request.');const confirmation=confirmationBody.data;captured.uncertain=true;
   return{result,confirmation,assertCurrent,unchanged:()=>{const first=captured.completion();return captured.hadCompletion?!!first&&first.form.isConnected&&first.form.getAttribute('data-erpx-edit-version')===first.formEditVersion:true;},acknowledge(){assertCurrent();if(get()!==captured)return false;set(null);const first=captured.completion();if(first&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...first,financeAcknowledged:true}}));return true;}};
  }catch(error){if(get()===captured){if(!sent&&!captured.uncertain)set(null);else captured.uncertain=true;}throw error;}finally{captured.busy=false;publish();}
 }
 return{submit(request:ServiceCustomerInput,completion?:WorkCommandCompletion){const source=current();if(!source?.contextKey||request.contextKey!==source.contextKey)throw Error('Review the current customer preparation before submitting.');const body=JSON.stringify(request);let attempt=get();if(attempt&&(attempt.body!==body||owner(attempt.source)!==owner(source)||attempt.work!==work()||attempt.route!==route()))throw Error('Confirm the original customer request first. Your newer draft is retained.');if(!attempt){attempt={busy:false,source:{...source},work:work(),route:route(),body,request:structuredClone(request),key:crypto.randomUUID(),uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};set(attempt);}return execute(attempt);},retry(){const attempt=get();if(!attempt)throw Error('There is no original customer preparation request to confirm.');return execute(attempt);},pending:()=>get()!==null,busy:()=>get()?.busy??false,subscribe(listener:()=>void){get();if(!storeKey)return()=>{};const key=storeKey;let set=listeners.get(key);if(!set){set=new Set();listeners.set(key,set);}set.add(listener);return()=>{set.delete(listener);if(!set.size)listeners.delete(key);};}};
}
