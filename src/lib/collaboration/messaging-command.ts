import {workFetch} from '@/lib/work_projection/work-fetch';
export type MessagingAttempt={path:string;body:string;key:string;context:string};
export function messagingCommandContext(token=''){const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return JSON.stringify([token,node?[node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment]:null]);}
export function validMessagingReceipt(attempt:MessagingAttempt,value:unknown){
 if(!value||typeof value!=='object'||!('id'in value)||typeof value.id!=='string')return false;
 const {path,key}=attempt,input:Record<string,unknown>=JSON.parse(attempt.body);
 if(/^rooms\/[^/]+\/messages$/.test(path))return value.id===`message-${key}`;
 if(/^files\/[^/]+\/discard$/.test(path))return value.id===decodeURIComponent(path.split('/')[1])&&'state'in value&&value.state==='withdrawn';
 if(path==='rooms')return input.kind==='direct'?/^dm-[a-f0-9]{64}$/.test(value.id):value.id===`room-${key}`;
 if(path==='teams')return value.id===`team-${key}`&&'roomId'in value&&value.roomId===`general-${key}`;
 if(/^teams\/[^/]+\/channels$/.test(path))return value.id===`channel-${key}`;
 if(/^(rooms|teams)\/[^/]+$/.test(path))return value.id===decodeURIComponent(path.split('/')[1]);
 return false;
}
export function validMessagingUpload(value:unknown,id:string):value is {id:string;attachmentLinkId:string;sourceId:string;state:string}{return !!value&&typeof value==='object'&&'id'in value&&value.id===id&&'attachmentLinkId'in value&&typeof value.attachmentLinkId==='string'&&value.attachmentLinkId.startsWith('attachment-link-')&&'sourceId'in value&&typeof value.sourceId==='string'&&value.sourceId.startsWith('attachment-')&&'state'in value&&value.state==='available';}
/** A lost response retains its exact endpoint, body, key and account context.
 * A newer local draft cannot silently replace that unresolved operation. */
export function createMessagingCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let pending:MessagingAttempt|null=null;
 return {pending:()=>pending,retain:(attempt:MessagingAttempt)=>{if(pending&&pending!==attempt)throw Error('Resolve the original submitted Messages change first.');pending=attempt;},prepare(path:string,input:unknown,context:string){const body=JSON.stringify(input);if(pending&&(pending.path!==path||pending.body!==body||pending.context!==context))throw Error('Retry the submitted Messages change first. Your newer draft remains here.');return pending??{path,body,key:newKey(),context};},async execute(root:string,attempt:MessagingAttempt,current:string|(()=>string)){
  const context=()=>typeof current==='function'?current():current;
  if(attempt.context!==context())throw Error('Return to the original account, Company and role before retrying this Messages change.');const known=pending!==null;let uncertain=true;
  try{const response=await send(`${root}/${attempt.path}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body},undefined,'caller'),envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(attempt.context!==context())throw Error('The Messages account, Company or role changed before acknowledgement. Return to the original context before retrying.');
   if(!response.ok){uncertain=known||response.status>=500||envelope.error?.code==='WORK_CONTEXT_CHANGED';throw Error(envelope.error?.message??'Messages could not save this change.');}if(!validMessagingReceipt(attempt,envelope.data))throw Error('Messages acknowledgement is incomplete. Retry the submitted change.');pending=null;return envelope.data as {id:string;roomId?:string;state?:string};
  }catch(error){pending=uncertain?attempt:null;throw error;}
 }};
}
