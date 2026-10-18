import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {procurementCommandContext,procurementFormUnchanged} from '@/lib/procurement/command';
export type HandbookReceipt={id:string;version:number};
export type HandbookAttempt={path:string;body:string;key:string;context:string;input:Readonly<Record<string,unknown>>;completion?:WorkCommandCompletion;settlement:boolean};
export const handbookCommandContext=(rendered:string)=>JSON.stringify([rendered,procurementCommandContext()]);
export function validHandbookReceipt(attempt:HandbookAttempt,value:unknown):value is HandbookReceipt{
 if(!value||typeof value!=='object'||Array.isArray(value)||!('id'in value)||typeof value.id!=='string'||!('version'in value)||!Number.isSafeInteger(value.version))return false;
 const parts=attempt.path.split('/'),expected=attempt.input.expectedVersion;
 if(parts[0]!=='handbook'||!Number.isSafeInteger(expected))return false;
 if(parts.length===1)return value.id===`article-${attempt.key}`&&value.version===1;
 if(parts.length===2&&parts[1]==='editors')return value.id===attempt.input.userId&&value.version===Number(expected)+1;
 return (parts.length===2||parts.length===3&&parts[2]==='actions')&&value.id===parts[1]&&value.version===Number(expected)+1;
}
/** Submitted body/key/context survive uncertainty. Changed drafts cannot
 * replace an unresolved command, and later edits are never acknowledged. */
export function createHandbookCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let pending:HandbookAttempt|null=null;
 return {pending:()=>pending,prepare(path:string,input:Record<string,unknown>,context:string,form?:HTMLFormElement,settlement=false){
  const body=JSON.stringify(input);if(pending&&(pending.path!==path||pending.body!==body||pending.context!==context))throw Error('Retry the submitted handbook action first. Your newer draft remains here.');
  return pending??{path,body,key:newKey(),context,input:Object.freeze(JSON.parse(body) as Record<string,unknown>),settlement,completion:form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined};
 },async execute(base:string,attempt:HandbookAttempt,currentContext:()=>string){
  if(attempt.context!==currentContext())throw Error('Your account, Company or role changed. Return to the original handbook context.');const previous=!!pending;let uncertain=true;
  try{const response=await send(`/api/v1${base}/${attempt.path}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body},attempt.completion,'caller'),envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(attempt.context!==currentContext())throw Error('Your handbook context changed. Review the submitted action in its original account.');
   if(!response.ok){uncertain=previous||response.status>=500||['WORK_CONTEXT_CHANGED','WORK_ROLE_REQUIRED','WORK_ROLE_UNAVAILABLE'].includes(envelope.error?.code??'');throw Error(envelope.error?.message??'Unable to save the handbook action.');}
   if(!validHandbookReceipt(attempt,envelope.data))throw Error('The handbook acknowledgement is incomplete. Retry the submitted action.');pending=null;return {result:{id:envelope.data.id,version:envelope.data.version},unchanged:procurementFormUnchanged(attempt.completion)};
  }catch(error){pending=uncertain?attempt:null;throw error;}
 }};
}
