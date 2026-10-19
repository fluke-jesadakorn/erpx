import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {isFirstPmsEditableRejection} from './workspace-command-rejections';

export type PmsWorkspaceCommandSource={base:string;propertyId?:string;section?:string;frontMode?:boolean};
export type PmsWorkspaceCommandAttempt={body:string;key:string;scope:string;sends:number;completion:ReturnType<typeof retainWorkCommandCompletion>;onAcknowledged?:(refreshed:boolean)=>void;onEditableRejected?:()=>void};
type Receipt={id:string;propertyId:string;version:number};
const changed=()=>Object.assign(new Error('Your hotel or work context changed. Return to the original context before retrying.'),{code:'WORK_CONTEXT_CHANGED'});
function scope(source:PmsWorkspaceCommandSource|null){
 const work=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
 const front=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-front-app]');
 return JSON.stringify([source,typeof window==='undefined'?null:[window.location.pathname,window.location.search],work&&[work.dataset.workUser,work.dataset.workRevision,work.dataset.workCompany,work.dataset.workAssignment],front&&[front.dataset.frontApp,front.dataset.frontLocation]]);
}
export function capturePmsWorkspaceCommandContext(current:()=>PmsWorkspaceCommandSource|null){
 if(!current())throw changed();
 const original=scope(current()),assertWork=captureWorkCommandContext();
 const front=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-front-app]');
 return ()=>{
  assertWork();
  if(!current()||scope(current())!==original||front&&(front.isConnected===false||document.querySelector('[data-front-app]')!==front))throw changed();
 };
}
function captureForm():WorkCommandCompletion|undefined{
 const form=typeof document==='undefined'?null:document.activeElement?.closest<HTMLFormElement>('form');
 return form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined;
}
async function receipt(value:unknown,source:PmsWorkspaceCommandSource,body:string,key:string):Promise<Receipt>{
 const data=value&&typeof value==='object'&&'data' in value?value.data:null;
 if(!data||typeof data!=='object'||!('id' in data)||typeof data.id!=='string'||!data.id||
  !('propertyId' in data)||typeof data.propertyId!=='string'||!data.propertyId||
  !('version' in data)||typeof data.version!=='number'||!Number.isSafeInteger(data.version)||data.version<1)
  throw Error('The hotel acknowledgement is incomplete. Retry the submitted action.');
 const command=JSON.parse(body) as {operation:string;expectedVersion?:number;input?:Record<string,unknown>};
 const input=command.input??{},row=data as Record<string,unknown>,op=command.operation;
 const expected=command.expectedVersion;
 if(op==='property.create'?data.id!==data.propertyId||data.version!==1:
  data.propertyId!==source.propertyId||!Number.isSafeInteger(expected)||Number(expected)<1||data.version!==Number(expected)+(['payment.request_transfer','payment.cancel_transfer'].includes(op)?0:1))
  throw Error('The hotel acknowledgement does not match the submitted hotel and version. Retry the submitted action.');
 let valid=true;
 // These original engine branches return an existing native target, unlike
 // payments, charges, stock moves and invoice-producing guest-account closure.
 const inputTargets=['rate.calendar','block.release','reservation.cancel','reservation.no_show','stay.cancel','stay.no_show','stay.assign','stay.move','stay.upgrade','stay.guest','stay.route_guest_account','stay.check_in','stay.change_dates','stay.replace','stay.check_out','stay.preview_checkout','refund.approve','refund.reject','credit.approve','credit.reject','guest_account.credit','shift.close','shift.review','task.assign','task.start','task.complete','task.inspect','room.dnd','room.observe','maintenance.close','lost.release','night.resume','night.step'];
 const existingSaves=['room_type.save','room.save','rate.save','guest.save','item.save','lost.save'];
 const propertyTargets=['booking.retry_email','booking.save','property.save','booking_fees.prepare','booking_fees.approve','online_accounting.prepare','online_accounting.approve','accounting.save','accounting.bootstrap','accounting.approve','property.enable'];
 if(inputTargets.includes(op)||existingSaves.includes(op)&&typeof input.id==='string'&&!!input.id)valid=typeof input.id==='string'&&data.id===input.id;
 if(propertyTargets.includes(op))valid=data.id===source.propertyId;
 if(op==='cash.move')valid=data.id===input.shiftId;
 if(op==='guest_account.merge')valid=data.id===input.toGuestAccountId;
 if(op==='guest.service_request'&&typeof input.requestId==='string'&&input.requestId)valid=data.id===input.requestId;
 if(op==='payment.refund_reserve')valid=data.id===input.guestAccountId;
 if(op==='payment.request_transfer')valid=row.state==='pending'&&row.transferVersion===1;
 if(op==='payment.cancel_transfer')valid=data.id===input.id&&row.state==='cancelled'&&row.transferVersion===Number(input.expectedTransferVersion)+1;
 if(op==='payment.verify_transfer')valid=data.id===input.id&&row.state==='verified'&&row.transferVersion===Number(input.expectedTransferVersion)+1&&typeof row.entryId==='string'&&!!row.entryId&&row.bankTransactionId===input.bankTransactionId;
 if(op==='property.create'||op==='charge.correction.request'){
  const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(key));
  const suffix=Array.from(new Uint8Array(digest),byte=>byte.toString(16).padStart(2,'0')).join('').slice(0,32);
  valid=op==='property.create'
   ? data.id==='property-'+suffix&&data.propertyId==='property-'+suffix
   : data.id==='pms-correction-'+suffix&&row.status==='pending'&&row.correctionVersion===1;
 }
 if(op==='charge.correction.review')valid=data.id===input.id&&row.status===(input.action==='approve'?'approved':'rejected')&&row.correctionVersion===Number(input.expectedVersion)+1&&(input.action==='approve'?typeof row.reversalEntryId==='string'&&!!row.reversalEntryId:row.reversalEntryId===null);
 if(!valid)throw Error('The hotel acknowledgement does not match the submitted action. Retry the submitted action.');
 return {id:data.id,propertyId:data.propertyId,version:data.version};
}

/** Only this owner delays acknowledgement until its original body and optional
 * refresh have finished in the captured hotel and rendered work context. */
export async function sendPmsWorkspaceCommand(attempts:Map<string,PmsWorkspaceCommandAttempt>,current:()=>PmsWorkspaceCommandSource|null,signature:string,body:string,
 refresh:(result:Receipt,assertCurrent:()=>void)=>Promise<boolean>,onAcknowledged?:(refreshed:boolean)=>void,onEditableRejected?:()=>void){
 const source=current();if(!source)throw changed();
 const assertCurrent=capturePmsWorkspaceCommandContext(current);assertCurrent();
 let attempt=attempts.get(signature);
 if(!attempt&&attempts.size)throw Object.assign(Error('Confirm the original hotel action first. Your newer entries are retained.'),{code:'PMS_ORIGINAL_COMMAND_PENDING'});
 if(!attempt){attempt={body,key:crypto.randomUUID(),scope:scope(source),sends:0,completion:retainWorkCommandCompletion(captureForm()),onAcknowledged,onEditableRejected};attempts.set(signature,attempt);}
 if(attempt.scope!==scope(source))throw changed();
 const submitted=attempt;
 ++submitted.sends;
 const response=await workFetch(`/api/v1${source.base}/pms`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':submitted.key},body:submitted.body},submitted.completion(),'caller');
 const parsed:unknown=await response.json();
 assertCurrent();
 if(!response.ok){
  const error=parsed&&typeof parsed==='object'&&'error' in parsed?parsed.error:null;
  const code=error&&typeof error==='object'&&'code' in error?error.code:undefined;
  if(submitted.sends===1&&isFirstPmsEditableRejection(source,submitted.body,response.status,code)&&attempts.get(signature)===submitted){
   submitted.onEditableRejected?.();
   assertCurrent();
   attempts.delete(signature);
  }
  throw Object.assign(new Error(error&&typeof error==='object'&&'message' in error&&typeof error.message==='string'?error.message:'Save failed.'),{status:response.status,code});
 }
 const result=await receipt(parsed,source,submitted.body,submitted.key);
 assertCurrent();
 let refreshed=false;
 try{refreshed=await refresh(result,assertCurrent);}catch{
  // A valid committed command stays saved when an optional read fails. A lost
  // context instead retains this exact attempt for its original-context replay.
  assertCurrent();
 }
 assertCurrent();
  return {receipt:result,refreshed,acknowledge(){
  assertCurrent();
  if(attempts.get(signature)!==submitted)return false;
  submitted.onAcknowledged?.(refreshed);
  assertCurrent();
  attempts.delete(signature);
  if(typeof window!=='undefined'){
   window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:submitted.completion()??{form:null,formEditVersion:null}}));
   announceWorkChange();
  }
  return true;
 }};
}


export function pendingPmsWorkspaceCommand(attempts:Map<string,PmsWorkspaceCommandAttempt>){
 const first=attempts.entries().next().value;
 return first?{signature:first[0],body:first[1].body,key:first[1].key}:null;
}
