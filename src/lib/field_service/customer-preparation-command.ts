import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import type {ServiceCustomerInput,ServiceCustomerResult,ServiceCustomerConfirmation} from '@/server/modules/finance/public/service-customer-preparation';
import {decimalToMinor} from '@/lib/finance/finance-money';

export type ServiceCustomerSource={base:string;companyId:string;workspaceKey:string;contextKey:string|null};
const work=()=>{const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?[node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment].join(':'):'server';};
const owner=(source:ServiceCustomerSource)=>JSON.stringify([source.base,source.companyId,source.workspaceKey]);
const route=()=>typeof window==='undefined'?'server':window.location.pathname+window.location.search;
const record=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
export function validServiceCustomerResult(value:unknown,input:ServiceCustomerInput['input']):value is ServiceCustomerResult{
 return record(value)&&typeof value.id==='string'&&/^party-.+/.test(value.id)&&value.kind==='customer'&&value.code===input.code&&value.name===input.name&&value.currency===input.currency&&value.taxRegistration===input.taxRegistration&&value.paymentTermsDays===input.paymentTermsDays&&value.creditLimit===input.creditLimit&&value.creditLimitMinor===decimalToMinor(input.creditLimit);
}
export function validServiceCustomerConfirmation(value:unknown,result:ServiceCustomerResult,key:string,companyId:string):value is ServiceCustomerConfirmation{
 return record(value)&&value.commandId===key&&value.companyId===companyId&&value.id===result.id&&value.kind==='customer'&&['active','blocked'].includes(String(value.status))&&['code','name','currency'].every(field=>typeof value[field]==='string'&&value[field].length>0);
}
type Attempt={source:ServiceCustomerSource;work:string;route:string;body:string;request:ServiceCustomerInput;key:string;uncertain:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
/** A single mounted owner retains its original request. No reload persistence
 * or Work-revision rebasing is promised by the native Finance receipt. */
export function createServiceCustomerCommands(current:()=>ServiceCustomerSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function execute(captured:Attempt){
  if(busy)throw Error('Wait for the original customer preparation request.');
  const source=current();if(!source||owner(source)!==owner(captured.source)||work()!==captured.work||route()!==captured.route)throw Error('Return to the original Service workspace and work role to confirm this customer.');
  const assertWork=captureWorkCommandContext(),invocation={...source},assertCurrent=()=>{assertWork();const live=current();if(!live||owner(live)!==owner(invocation)||live.contextKey!==invocation.contextKey||work()!==captured.work||route()!==captured.route)throw Object.assign(Error('The Service customer context changed. Your original request and draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  const priorUncertain=captured.uncertain;busy=true;let sent=false;
  try{
   assertCurrent();sent=true;
   const init={method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},response=await send(`/api/v1${source.base}/field-service/customer-preparation`,init,captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};assertCurrent();
   if(!response.ok){
    // Exact first-response rollback paths: route schema rejects before invoking
    // Finance; context rejects before createParty; original money validation is
    // before commitCreateParty. No authority/status-wide inference is made.
    const editable=response.status===422&&['VALIDATION_FAILED','INVALID_JSON','INVALID_MONEY','MONEY_SCALE','MONEY_OVERFLOW','CURRENCY_SCALE_REQUIRED','FINANCE_CURRENCY_SCALE_UNSUPPORTED'].includes(body.error?.code??'')||response.status===409&&['SERVICE_CUSTOMER_CONTEXT_CHANGED','SERVICE_CUSTOMER_CODE_EXISTS'].includes(body.error?.code??'');
    if(!priorUncertain&&editable)attempt=null;
    throw Object.assign(Error(body.error?.message??'The original customer preparation result is uncertain. Retry the original request.'),{code:body.error?.code});
   }
   if(!validServiceCustomerResult(body.data,captured.request.input))throw Error('The original customer receipt is incomplete. Retry the same request.');const result=body.data;
   const confirmed=await send(`/api/v1${source.base}/field-service/customer-preparation/confirmations`,init,undefined,'caller'),confirmationBody=await confirmed.json() as {data?:unknown;error?:{message?:string}};assertCurrent();
   if(!confirmed.ok)throw Error(confirmationBody.error?.message??'Confirm the original customer from its saved receipt.');
   if(!validServiceCustomerConfirmation(confirmationBody.data,result,captured.key,source.companyId))throw Error('The saved customer does not match the original receipt. Retry the original request.');const confirmation=confirmationBody.data;captured.uncertain=true;
   return{result,confirmation,assertCurrent,unchanged:()=>{const first=captured.completion();return captured.hadCompletion?!!first&&first.form.isConnected&&first.form.getAttribute('data-erpx-edit-version')===first.formEditVersion:true;},acknowledge(){assertCurrent();if(attempt!==captured)return false;attempt=null;const first=captured.completion();if(first&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...first,financeAcknowledged:true}}));return true;}};
  }catch(error){if(attempt===captured){if(!sent&&!captured.uncertain)attempt=null;else captured.uncertain=true;}throw error;}finally{busy=false;}
 }
 return{submit(request:ServiceCustomerInput,completion?:WorkCommandCompletion){const source=current();if(!source?.contextKey||request.contextKey!==source.contextKey)throw Error('Review the current customer preparation before submitting.');const body=JSON.stringify(request);if(attempt&&(attempt.body!==body||owner(attempt.source)!==owner(source)||attempt.work!==work()||attempt.route!==route()))throw Error('Confirm the original customer request first. Your newer draft is retained.');attempt??={source:{...source},work:work(),route:route(),body,request:structuredClone(request),key:crypto.randomUUID(),uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return execute(attempt);},retry(){if(!attempt)throw Error('There is no original customer preparation request to confirm.');return execute(attempt);},pending:()=>attempt!==null};
}
