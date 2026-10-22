import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {calendarFirstRollback} from '@erpx/feature-work_projection/lib/calendar-command-rejections';
export type CalendarCommandReceipt={id:string;version:number;status?:string;warnings:string[]};
export type CalendarCommandAttempt={readonly body:string;readonly key:string;readonly context:string;readonly completion?:WorkCommandCompletion};
export function calendarCommandContext(){const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?[node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment].join(':'):'server';}
export function calendarFormUnchanged(completion?:WorkCommandCompletion){return !completion||completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion;}
export function validCalendarReceipt(attempt:CalendarCommandAttempt,value:unknown):value is CalendarCommandReceipt{
 if(!value||typeof value!=='object'||!('id'in value)||typeof value.id!=='string'||!('version'in value)||!Number.isSafeInteger(value.version)||Number(value.version)<1||!('warnings'in value)||!Array.isArray(value.warnings)||!value.warnings.every(message=>typeof message==='string'))return false;
 const input:Record<string,unknown>=JSON.parse(attempt.body),created=!input.id;
 if(value.id!==(created?`calendar-${attempt.key}`:input.id))return false;
 // Native invitation responses/reminders version the freshly read appointment;
 // those two owners intentionally do not enforce the optional viewed version.
 if(input.action==='appointment.respond'||input.action==='appointment.reminder.set')return 'status'in value&&value.status==='scheduled'&&(!Number.isSafeInteger(input.expectedVersion)||Number(value.version)>Number(input.expectedVersion));
 if(created&&value.version!==1)return false;
 if(Number.isSafeInteger(input.expectedVersion)&&value.version!==Number(input.expectedVersion)+1)return false;
 return typeof input.action==='string'&&['appointment.save','appointment.reschedule','appointment.status','announcement.save','announcement.reschedule','announcement.status','task.plan','task.reschedule','task.complete','shift.reschedule'].includes(input.action);
}
/** One original attempt/completion owns transport and acknowledgement. This is
 * mounted recovery only; no request, user content or authority is persisted. */
export function createCalendarCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 const keys=new Map<string,string>(),routes=new WeakMap<CalendarCommandAttempt,string>();
 let pending:CalendarCommandAttempt|null=null,inflight:CalendarCommandAttempt|null=null;
 const signature=(body:string,context:string)=>JSON.stringify([body,context]);
 return {
  pending:()=>pending,inflight:()=>inflight,
  prepare(input:unknown,completion?:WorkCommandCompletion,context=calendarCommandContext()){
   const body=JSON.stringify(input),held=pending??inflight;
   if(held){if(held.body!==body||held.context!==context)throw Error('Retry the submitted Calendar change first. Your newer draft is retained.');return held;}
   const stamp=signature(body,context),key=keys.get(stamp)??newKey();keys.set(stamp,key);
   return Object.freeze({body,key,context,completion:completion?Object.freeze({form:completion.form,formEditVersion:completion.formEditVersion}):undefined});
  },
  async execute(api:string,attempt:CalendarCommandAttempt,current:string|(()=>string)=calendarCommandContext,acknowledgement:'automatic'|'caller'='automatic'){
   const assertCurrent=()=>{if(attempt.context!==(typeof current==='function'?current():current))throw Error('Your account, Company or role changed. Return to the original Calendar role before retrying.');};
   assertCurrent();
   if(inflight)throw Error('The original Calendar action is still waiting for its response.');
   if(pending&&pending!==attempt)throw Error('Retry the original submitted Calendar action first.');
   const route=routes.get(attempt);if(route!==undefined&&route!==api)throw Error('Return to the original Calendar source before retrying.');
   const assertWorkCurrent=captureWorkCommandContext();assertWorkCurrent();
   const guard=()=>{assertWorkCurrent();assertCurrent();if(routes.get(attempt)!==api)throw Error('The original Calendar source changed.');};
   const alreadyUncertain=pending!==null;let uncertain=true;inflight=attempt;routes.set(attempt,api);
   try{
    const response=await send(api,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body},attempt.completion,'caller');
    const envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
    // ORIGINAL JSON and all current owner/Work fences precede both success and
    // the narrowly proven first-attempt rollback classification.
    guard();
    if(!response.ok){uncertain=alreadyUncertain||!calendarFirstRollback(api,JSON.parse(attempt.body),response.status,envelope.error?.code,envelope.error?.message);throw Error(envelope.error?.message??'Calendar could not save this change.');}
    if(!validCalendarReceipt(attempt,envelope.data))throw Error('Calendar acknowledgement is incomplete. Retry the submitted change.');
    const result=envelope.data;pending=attempt;
    const acknowledge=()=>{guard();if(pending!==attempt)throw Error('The original Calendar acknowledgement changed.');keys.delete(signature(attempt.body,attempt.context));pending=null;};
    if(acknowledgement==='automatic')acknowledge();
    return {result,get unchanged(){return calendarFormUnchanged(attempt.completion);},assertCurrent:guard,acknowledge};
   }catch(error){pending=uncertain?attempt:null;throw error;}finally{inflight=null;}
  },
 };
}
