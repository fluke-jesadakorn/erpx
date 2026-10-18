import {workFetch,type WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
import {procurementCommandContext,procurementFormUnchanged} from "@/lib/procurement/command";
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
export type LearningReceipt={id:string;version:number;status:string};
export type LearningAttempt={input:Record<string,unknown>;body:string;key:string;context:string;completion?:WorkCommandCompletion};
export function validLearningReceipt(attempt:LearningAttempt,value:unknown):value is LearningReceipt{
 if(!value||typeof value!=='object'||!('id'in value)||!('version'in value)||!('status'in value)||typeof value.id!=='string'||!value.id||typeof value.status!=='string'||!Number.isSafeInteger(value.version)||Number(value.version)<1)return false;
 const input=attempt.input,course=input.entity==='course';
 if(input.entity==='certificate')return value.id===input.id&&Number.isSafeInteger(input.expectedAssignmentVersion)&&Number(input.expectedAssignmentVersion)>0&&value.version===input.expectedAssignmentVersion&&value.status==='revoked';
 if(typeof input.id!=='string')return value.id===`learning-${course?'course':'assignment'}-${attempt.key}`&&value.version===1&&value.status===(course?'draft':'assigned');
 const statuses:Record<string,string>=course?{publish:'published',reject:'rejected',retire:'retired'}:{start:'in_progress',complete:'completed',waive:'waived',cancel:'cancelled'};
 return value.id===input.id&&Number.isSafeInteger(input.expectedVersion)&&value.version===Number(input.expectedVersion)+1&&value.status===statuses[String(input.action)];
}
export function createLearningCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 const keys=new Map<string,string>();let pending:LearningAttempt|null=null;
 const signature=(body:string,context:string)=>JSON.stringify([body,context]);
 return {pending:()=>pending,prepare(input:Record<string,unknown>,form?:HTMLFormElement,context=procurementCommandContext()){
  const body=JSON.stringify(input);if(pending&&(pending.body!==body||pending.context!==context))throw Error('Retry the submitted Learning action first. Your newer draft remains here.');
  const stamp=signature(body,context),key=keys.get(stamp)??newKey();keys.set(stamp,key);return pending??{input,body,key,context,completion:form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined};
 },async execute(base:string,attempt:LearningAttempt,context=procurementCommandContext(),acknowledgement:'automatic'|'caller'='automatic',assertOwnerCurrent:()=>void=()=>{}){
  if(attempt.context!==context)throw Error('Your Company or role changed. Return to the original Learning role before retrying.');const assertWork=captureWorkCommandContext(),location=()=>typeof window==='undefined'?'server':[window.location.pathname,window.location.search,window.location.hash].join(''),source=location(),wasUncertain=pending===attempt;let uncertain=true;
  const assertCurrent=()=>{assertWork();assertOwnerCurrent();if(source!==location())throw Object.assign(Error('The Learning source changed. Return to the original page before retrying.'),{code:'WORK_CONTEXT_CHANGED'});};
  assertCurrent();pending=attempt;
  try{const response=await send(`/api/v1${base}/learning`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body},attempt.completion,'caller'),envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   assertCurrent();if(!response.ok){uncertain=wasUncertain||response.status>=500||envelope.error?.code==='WORK_CONTEXT_CHANGED';throw Error(envelope.error?.message??'Unable to save the Learning action.');}
   if(!validLearningReceipt(attempt,envelope.data))throw Error('Learning acknowledgement is incomplete. Retry the submitted action.');let acknowledged=false;
   const acknowledge=()=>{assertCurrent();if(acknowledged||pending!==attempt)return false;acknowledged=true;keys.delete(signature(attempt.body,attempt.context));pending=null;return true;};
   if(acknowledgement==='automatic')acknowledge();return {result:envelope.data,unchanged:procurementFormUnchanged(attempt.completion),acknowledge};
  }catch(error){pending=uncertain?attempt:null;throw error;}
 }};
}
