import {workFetch} from './work-fetch';
import {procurementCommandContext} from '@/lib/procurement/command';
import type {HandoverAcknowledgement} from '@/server/modules/operations/public/handover';
export type HandoverAttempt={endpoint:string;postId:string;version:number;key:string;context:string};
export function handoverReceiptContext(token=''){return JSON.stringify([token,procurementCommandContext()]);}
const record=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
export function handoverReceiptMatches(value:unknown,postId:string,version:number):value is HandoverAcknowledgement{return record(value)&&value.postId===postId&&value.postVersion===version&&typeof value.canAcknowledge==='boolean'&&typeof value.acknowledged==='boolean'&&Number.isSafeInteger(value.recipientCount)&&Number(value.recipientCount)>=0&&Number.isSafeInteger(value.acknowledgedCount)&&Number(value.acknowledgedCount)>=0&&Number(value.acknowledgedCount)<=Number(value.recipientCount)&&(!value.acknowledged||value.canAcknowledge&&Number(value.acknowledgedCount)>0);}
export function createHandoverReceiptCommands(send:typeof workFetch=workFetch,key:()=>string=()=>crypto.randomUUID()){
 let pending:HandoverAttempt|null=null;
 return {pending:()=>pending,prepare(endpoint:string,postId:string,version:number,context:string){if(pending&&(pending.endpoint!==endpoint||pending.postId!==postId||pending.version!==version||pending.context!==context))throw Error('Retry the submitted handover receipt before confirming another version.');return pending??{endpoint,postId,version,context,key:key()};},async execute(attempt:HandoverAttempt,current:()=>string){
  if(attempt.context!==current())throw Error('Return to the original account, Company and role before retrying this handover receipt.');let uncertain=true;const known=pending!==null;
  try{const response=await send(attempt.endpoint,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key,'x-erpx-handover-version':String(attempt.version)},body:'{}'},undefined,'caller'),envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(attempt.context!==current())throw Error('Your account or role changed before the handover receipt was acknowledged.');
   if(!response.ok){uncertain=known||response.status>=500||['WORK_CONTEXT_CHANGED','WORK_ROLE_REQUIRED','WORK_ROLE_UNAVAILABLE'].includes(envelope.error?.code??'');throw Error(envelope.error?.message??'The handover receipt was not acknowledged.');}
   const value=envelope.data;if(!record(value)||value.postId!==attempt.postId||value.postVersion!==attempt.version||value.acknowledged!==true)throw Error('The handover acknowledgement is incomplete. Retry the original submitted receipt.');
   pending=null;return {postId:attempt.postId,postVersion:attempt.version,acknowledged:true as const,snapshot:handoverReceiptMatches(value,attempt.postId,attempt.version)?value:null};
  }catch(error){pending=uncertain?attempt:null;throw error;}
 }};
}
