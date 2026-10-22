import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import type {WorkforceKind} from '@erpx/feature-people_payroll/lib/workforce-controls';
export type WorkforceCommand={kind:WorkforceKind;action:string;id?:string;input:Record<string,unknown>};
type Attempt={request:WorkforceCommand;body:string;key:string;uncertain:boolean};
export type WorkforceReceipt={id:string;version:number;state?:string;status?:string;preview?:unknown};
type Result={request:WorkforceCommand;receipt:WorkforceReceipt;completion?:WorkCommandCompletion};
function route(base:string,request:WorkforceCommand){
 if(request.action==="create"?Boolean(request.id):!request.id)throw Error("Use the original Workforce record identity.");
 const id=request.id?`/${encodeURIComponent(request.id)}`:'',action=request.action;
 if(request.kind==='time'&&['create','save','approve','reject'].includes(action))return {path:`/api/v1${base}/people/time-entries${id}${['approve','reject'].includes(action)?`/${action}`:''}`,method:'POST'};
 if(request.kind==='overtime'&&['create','save','submit','actuals','authorize','verify','approve','return','reject','withdraw'].includes(action))return {path:`/api/v1${base}/overtime-requests${id}`,method:'POST'};
 if(request.kind==='leave'&&['create','save','submit','approve','reject','cancel','approve_cancellation','reject_cancellation'].includes(action))return {path:`/api/v1${base}/leave-requests${id}${request.id&&!['save','submit'].includes(action)?action==='approve'?'/approve':action.endsWith('_cancellation')?'/cancellation-review':'/transition':''}`,method:request.id&&['save','submit'].includes(action)?'PATCH':'POST'};
 throw Error('Use the original registered Workforce action.');
}
export function validWorkforceReceipt(request:WorkforceCommand,key:string,value:unknown):value is WorkforceReceipt{
 const data=value&&typeof value==='object'?value as Record<string,unknown>:null,expected=request.id??`${request.kind==='overtime'?'ot':request.kind}-${key}`,version=request.id?Number(request.input.expectedVersion)+1:1;
 if(!data||data.id!==expected||data.version!==version)return false;
 if(request.kind==='time')return data.status===({create:'submitted',save:'submitted',approve:'approved',reject:'rejected'} as Record<string,string>)[request.action];
 const allowed=request.kind==='overtime'?({create:['draft'],save:['draft','rejected','authorized','returned'],submit:['pending_authorization','pending_verification'],actuals:['pending_verification'],authorize:['authorized','pending_authorization'],verify:['pending_verification','pending_hr','approved'],approve:['pending_hr','approved'],return:['rejected','returned'],reject:['rejected','returned'],withdraw:['withdrawn']} as Record<string,string[]>)[request.action]
 :['create','save','submit'].includes(request.action)?[request.input.saveOnly===false?'pending_approval':'draft']
 :request.action==='approve'?['pending_approval','approved']:request.action==='reject'?['rejected']:request.action==='cancel'?['approved','cancelled']:['approved','cancelled'];
 if(typeof data.state!=='string'||!allowed?.includes(data.state))return false;
 if(request.kind==='leave'&&request.action.endsWith('_cancellation'))return ['pending_review','approved','rejected'].includes(String(data.cancellationState));
 if(request.kind==='leave'&&request.action==='cancel'&&data.state==='approved')return data.cancellationState==='pending_review';
 return true;
}
const capture=():WorkCommandCompletion|undefined=>{const active=typeof document==='undefined'?null:document.activeElement,form=active?.closest<HTMLFormElement>('form')??(active as HTMLButtonElement|null)?.form;return form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined;};
export function workforceFormUnchanged(completion?:WorkCommandCompletion){return !completion||completion.form.isConnected&&completion.formEditVersion===completion.form.getAttribute('data-erpx-edit-version');}
/** One original owner action per opaque rendered account/role context. No key
 * is replaced merely because the user edits a draft after an uncertain result. */
export function createWorkforceCommandClient(base:string,context:string){
 const storageKey=`erpx:workforce:${context}`;let attempt:Attempt|null=null,inFlight=false,submittedForm:WorkCommandCompletion|undefined;
 function persist(){try{if(attempt)sessionStorage.setItem(storageKey,JSON.stringify(attempt));else sessionStorage.removeItem(storageKey);}catch{}}
 function restore(){if(attempt)return attempt;try{const value=JSON.parse(sessionStorage.getItem(storageKey)??'null') as Attempt|null;if(value&&typeof value.key==='string'&&typeof value.body==='string'&&value.request&&['time','leave','overtime'].includes(value.request.kind)&&value.body===JSON.stringify(value.request.input)){route(base,value.request);attempt=value;}}catch{}return attempt;}
 async function execute(current:Attempt):Promise<Result>{
  if(inFlight)throw Error('Wait for the current Workforce action to finish.');inFlight=true;attempt=current;persist();const endpoint=route(base,current.request),completion=submittedForm;
  try{const response=await workFetch(endpoint.path,{method:endpoint.method,headers:{'Content-Type':'application/json','Idempotency-Key':current.key},body:current.body},completion,'caller'),definitive=response.status>=400&&response.status<500&&![408,429].includes(response.status);
   const result=await response.json().catch(()=>{throw Error('The result could not be read. Check the original Workforce action.');});
   const contextChanged=response.status===409&&['WORK_CONTEXT_CHANGED','WORK_ROLE_REQUIRED','WORK_ROLE_UNAVAILABLE'].includes(result?.error?.code),retain=contextChanged||current.uncertain&&[401,403,404,409].includes(response.status);
   if(definitive&&!retain){attempt=null;submittedForm=undefined;persist();}
   if(!response.ok)throw Error(result?.error?.message??'The Workforce action was not acknowledged.');if(!validWorkforceReceipt(current.request,current.key,result?.data))throw Error('The response did not acknowledge the original Workforce record and version. Check the original action.');
   attempt=null;submittedForm=undefined;persist();if(typeof window!=='undefined'){if(completion)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,workforceAcknowledged:true}}));announceWorkChange();}return {request:current.request,receipt:result.data,completion};
  }catch(error){if(attempt){attempt.uncertain=true;persist();}throw error;}finally{inFlight=false;}
 }
 return {send(request:WorkforceCommand){const body=JSON.stringify(request.input),prior=restore();route(base,request);if(prior&&(JSON.stringify({...prior.request,input:null})!==JSON.stringify({...request,input:null})||prior.body!==body))return Promise.reject(Error('A previous Workforce action has an uncertain result. Check its original request; your newer draft is retained.'));if(!prior)submittedForm=capture();return execute(prior??{request:{...request,input:JSON.parse(body)},body,key:crypto.randomUUID(),uncertain:false});},pending(){const current=restore();return current?{kind:current.request.kind,action:current.request.action,id:current.request.id}:null;},async recover(){const current=restore();return current?execute(current):null;}};
}
