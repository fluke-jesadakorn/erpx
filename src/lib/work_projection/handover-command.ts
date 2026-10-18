import {workFetch} from './work-fetch';
import type {HandoverView} from '@/server/modules/operations/public/handover';
export type HandoverInput={date:string;body:string;sources:Array<{id:string;sourceVersion:number;kind:'staff_task'|'pms_record'}>;recipientIds:string[]};
export type HandoverPublishAttempt={endpoint:string;body:string;key:string;context:string;editRevision:number};
export type HandoverPublishReceipt={id:string;version:1};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const strings=(value:unknown):value is string[]=>Array.isArray(value)&&value.every(item=>typeof item==='string'&&!!item)&&new Set(value).size===value.length;
function validInput(value:unknown):value is HandoverInput{return object(value)&&typeof value.date==='string'&&/^\d{4}-\d{2}-\d{2}$/.test(value.date)&&typeof value.body==='string'&&value.body.trim().length>0&&value.body.length<=3000&&strings(value.recipientIds)&&value.recipientIds.length>0&&value.recipientIds.length<=20&&Array.isArray(value.sources)&&value.sources.length>0&&value.sources.length<=5&&value.sources.every(source=>object(source)&&typeof source.id==='string'&&!!source.id&&Number.isSafeInteger(source.sourceVersion)&&Number(source.sourceVersion)>0&&['staff_task','pms_record'].includes(String(source.kind)))&&new Set(value.sources.map(source=>source.id)).size===value.sources.length;}
export function handoverViewMatches(value:unknown):value is HandoverView{
 return object(value)&&typeof value.date==='string'&&/^\d{4}-\d{2}-\d{2}$/.test(value.date)&&typeof value.ownerKey==='string'&&!!value.ownerKey&&typeof value.unavailable==='boolean'&&(value.reason===null||typeof value.reason==='string')&&strings(value.defaultIds)&&Array.isArray(value.people)&&value.people.every(person=>object(person)&&typeof person.id==='string'&&typeof person.name==='string')&&Array.isArray(value.items)&&value.items.every(item=>object(item)&&typeof item.id==='string'&&typeof item.title==='string'&&typeof item.status==='string'&&typeof item.href==='string'&&Number.isSafeInteger(item.sourceVersion)&&Number(item.sourceVersion)>0&&typeof item.updatedAt==='number'&&typeof item.planned==='boolean'&&['completed','pending','blocked'].includes(String(item.kind))&&(item.sourceKind===undefined||['staff_task','pms_record'].includes(String(item.sourceKind))))&&value.defaultIds.every(id=>(value.items as Array<{id:string}>).some(item=>item.id===id));
}
export function handoverPublishReceiptMatches(attempt:HandoverPublishAttempt,value:unknown):value is HandoverPublishReceipt{return object(value)&&value.id===`post-post-draft-${attempt.key}`&&value.version===1;}
/** Only the original submitted body/key/context can resolve an unknown result.
 * The receipt cannot clear edits made after this exact submission. */
export function createHandoverPublishCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let pending:HandoverPublishAttempt|null=null;
 return {pending:()=>pending,prepare(endpoint:string,input:HandoverInput,context:string,editRevision:number){const body=JSON.stringify(input);if(pending&&(pending.endpoint!==endpoint||pending.body!==body||pending.context!==context))throw Error('Retry the submitted handover first. Your newer draft remains here.');return pending??{endpoint,body,key:newKey(),context,editRevision};},
 restore(value:unknown,endpoint:string,context:string){if(!object(value)||value.endpoint!==endpoint||value.context!==context||typeof value.key!=='string'||!/^[a-f0-9-]{36}$/i.test(value.key)||typeof value.body!=='string'||!Number.isSafeInteger(value.editRevision)||Number(value.editRevision)<0)return null;let input:unknown;try{input=JSON.parse(value.body);}catch{return null;}if(!validInput(input))return null;pending={endpoint,body:value.body,key:value.key,context,editRevision:Number(value.editRevision)};return {attempt:pending,input};},
 async execute(attempt:HandoverPublishAttempt,currentContext:()=>string,currentEditRevision:()=>number){
  if(attempt.context!==currentContext())throw Error('Return to the original account, Company and role before retrying this handover.');const previouslyUncertain=pending!==null;let uncertain=true;
  try{const response=await send(attempt.endpoint,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key,'x-erpx-handover-publication':'recoverable'},body:attempt.body},undefined,'caller'),envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(attempt.context!==currentContext())throw Error('Your account or role changed before the handover was acknowledged.');
   if(!response.ok){uncertain=previouslyUncertain||response.status>=500||['WORK_CONTEXT_CHANGED','WORK_ROLE_REQUIRED','WORK_ROLE_UNAVAILABLE'].includes(envelope.error?.code??'');throw Error(envelope.error?.message??'The handover was not acknowledged.');}
   if(!handoverPublishReceiptMatches(attempt,envelope.data))throw Error('The handover acknowledgement is incomplete. Retry the original submitted handover.');pending=null;return {receipt:envelope.data,unchanged:attempt.editRevision===currentEditRevision()};
  }catch(error){pending=uncertain?attempt:null;throw error;}
 }};
}
