import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {procurementCommandContext,procurementFormUnchanged} from '@erpx/feature-procurement/lib/command';
export type RoutineReceipt={id:string;version:number;state?:string};
export type RoutineAttempt={path:string;input:Readonly<Record<string,unknown>>;body:string;key:string;context:string;completion?:WorkCommandCompletion;existing:boolean};
export function routineCommandContext(rendered:string){return JSON.stringify([rendered,procurementCommandContext()]);}
export function validRoutineReceipt(attempt:RoutineAttempt,value:unknown):value is RoutineReceipt{
 if(!value||typeof value!=='object'||Array.isArray(value)||!('id'in value)||!('version'in value)||typeof value.id!=='string'||!Number.isSafeInteger(value.version))return false;
 const parts=attempt.path.split('/'),expected=attempt.input.expectedVersion,updated=Number.isSafeInteger(expected)&&Number(expected)>0?Number(expected)+1:null;
 if(parts[0]==='routine-schedules'&&parts.length===3){const state=({pause:'paused',resume:'active',end:'ended'} as Record<string,string>)[parts[2]];return !!state&&value.id===parts[1]&&value.version===updated&&'state'in value&&value.state===state;}
 if(parts[0]!=='routine-templates')return false;
 if(parts[2]==='run')return parts.length===3&&value.id===`task-${attempt.key}`&&value.version===1&&'state'in value&&['draft','start_approval','ready'].includes(String(value.state));
 if(parts[2]==='schedules')return parts.length===3?value.id===`schedule-${attempt.key}`&&value.version===1&&'state'in value&&value.state==='paused':parts.length===4&&value.id===parts[3]&&value.version===updated&&'state'in value&&value.state==='paused';
 if(parts.length===1||parts.length===3&&parts[2]==='clone')return value.id===`routine-${attempt.key}`&&value.version===1;
 return parts.length===2&&value.id===parts[1]&&value.version===updated;
}
export function createRoutineCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 const keys=new Map<string,string>();let pending:RoutineAttempt|null=null;const signature=(path:string,body:string,context:string)=>JSON.stringify([path,body,context]);
 return {pending:()=>pending,prepare(path:string,input:Record<string,unknown>,form:HTMLFormElement|undefined,context:string,existing=false){
  const body=JSON.stringify(input);if(pending&&(pending.path!==path||pending.body!==body||pending.context!==context))throw Error('Retry the submitted routine action first. Your newer draft remains here.');
  const stamp=signature(path,body,context),key=keys.get(stamp)??newKey();keys.set(stamp,key);return pending??{path,input:Object.freeze(JSON.parse(body) as Record<string,unknown>),body,key,context,existing,completion:form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined};
 },async execute(base:string,attempt:RoutineAttempt,currentContext:()=>string){
  if(attempt.context!==currentContext())throw Error('Your account, Company or role changed. Return to the original routine context before retrying.');const alreadyUncertain=pending!==null;let uncertain=true;
  try{const response=await send(`/api/v1${base}/${attempt.path}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body},attempt.completion,'caller'),envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(attempt.context!==currentContext())throw Error('Your account, Company or role changed. Review the submitted action in its original routine context.');
   if(!response.ok){uncertain=alreadyUncertain||response.status>=500||['WORK_CONTEXT_CHANGED','WORK_ROLE_REQUIRED','WORK_ROLE_UNAVAILABLE'].includes(envelope.error?.code??'');throw Error(envelope.error?.message??'Unable to save the routine action.');}
   if(!validRoutineReceipt(attempt,envelope.data))throw Error('Routine acknowledgement is incomplete. Retry the submitted action.');const {id,version,state}=envelope.data;pending=null;keys.delete(signature(attempt.path,attempt.body,attempt.context));return {result:{id,version,...(state===undefined?{}:{state})},unchanged:procurementFormUnchanged(attempt.completion)};
  }catch(error){pending=uncertain?attempt:null;throw error;}
 }};
}
