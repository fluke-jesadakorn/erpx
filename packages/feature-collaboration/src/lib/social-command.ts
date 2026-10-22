import {aiResultSchema} from '@erpx/server-collaboration/public/rules/social-ai';
import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {procurementCommandContext,procurementFormUnchanged} from '@erpx/feature-procurement/lib/command';
export type SocialAttempt={url:string;body:string;key:string;context:string;completion?:WorkCommandCompletion};
export type SocialCommandIdentity={companyId:string;userId:string};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
export function socialCommandContext(rendered=''){return JSON.stringify([rendered,procurementCommandContext()]);}
export async function validSocialReceipt(attempt:SocialAttempt,value:unknown,identity?:SocialCommandIdentity){
 if(!object(value))return false;const path=new URL(attempt.url,'https://erpx.invalid').pathname,input:Record<string,unknown>=JSON.parse(attempt.body),parts=path.split('/'),id=decodeURIComponent(parts.at(-1)??''),version=(expected:number)=>Number.isSafeInteger(value.version)&&value.version===expected;
 if(path.endsWith('/uploads'))return typeof value.uploadIntentId==='string'&&!!value.uploadIntentId&&typeof value.attachmentLinkId==='string'&&typeof value.attachmentSetId==='string'&&value.uploadUrl===`${path}/${encodeURIComponent(value.uploadIntentId)}/content`;
 if(typeof value.id!=='string'||!value.id)return false;
 if(/\/beta\/drafts(?:\/[^/]+)?$/.test(path))return value.id===(id==='drafts'?`post-draft-${attempt.key}`:id)&&version(id==='drafts'?1:Number(input.expectedVersion)+1);
 if(/\/beta\/drafts\/[^/]+\/publish$/.test(path))return value.id===`post-${decodeURIComponent(parts.at(-2)!)}`&&version(1);
 if(path.endsWith('/beta/comment-uploads'))return value.id===`comment-upload-${attempt.key}`;
 if(path.endsWith('/beta/post-edit-uploads'))return value.id===`post-edit-upload-${attempt.key}`&&version(1);
 if(/\/beta\/(comment-uploads|post-edit-uploads|agent-replies)\/[^/]+$/.test(path))return value.id===id;
 if(/\/posts\/[^/]+\/task$/.test(path))return value.id===`task-post-${decodeURIComponent(parts.at(-2)!)}`&&version(1)&&value.state==='draft';
 if(/\/posts\/[^/]+\/ai-drafts$/.test(path)){
  if(!identity)return false;const bytes=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(`${identity.companyId}:${identity.userId}:${attempt.key}`)),expected=[...new Uint8Array(bytes)].map(byte=>byte.toString(16).padStart(2,'0')).join('');
  return value.id===expected&&Number.isSafeInteger(value.sourceVersion)&&Number(value.sourceVersion)>0&&typeof value.sourceHash==='string'&&!!value.sourceHash&&aiResultSchema.safeParse(value.result).success&&Array.isArray(value.commentIds)&&JSON.stringify([...value.commentIds].sort())===JSON.stringify([...(Array.isArray(input.commentIds)?input.commentIds:[])].sort());
 }
 if(/\/posts\/[^/]+$/.test(path)){
  if(input.action==='comment')return value.id===id;
  if(input.action==='edit_comment')return value.id===input.commentId&&Number.isSafeInteger(value.version)&&Number(value.version)>0;
  if(input.action==='delete_comment')return value.id===input.commentId;
  if(input.action==='edit')return value.id===id&&version(Number(input.expectedVersion)+1);
  if(input.action==='retract')return value.id===id&&Number.isSafeInteger(value.version)&&Number(value.version)>0;
  return ['appreciate','follow','report','hide'].includes(String(input.action))&&value.id===id;
 }
 return false;
}
/** A malformed/lost acknowledgement retains exactly the submitted endpoint,
 * body, key and rendered context. It cannot consume a later local edit. */
export function createSocialCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let pending:SocialAttempt|null=null;let waiting:{attempt:SocialAttempt;resolve:(reply:{result:unknown;unchanged:boolean})=>void}|null=null;
 return {pending:()=>pending,wait:(attempt:SocialAttempt)=>new Promise<{result:unknown;unchanged:boolean}>(resolve=>{if(pending!==attempt||waiting)throw Error('The original submitted conversation action is not awaiting this receipt.');waiting={attempt,resolve};}),prepare(url:string,input:unknown,context:string,form?:HTMLFormElement){const body=JSON.stringify(input);if(pending&&(pending.url!==url||pending.body!==body||pending.context!==context))throw Error('Retry the submitted conversation action first. Your newer draft remains here.');return pending??{url,body,key:newKey(),context,completion:form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined};},async execute(attempt:SocialAttempt,current:()=>string,identity?:SocialCommandIdentity){
  if(attempt.context!==current())throw Error('Return to the original account, Company and role before retrying this conversation action.');let uncertain=true;const known=pending!==null;
  try{const response=await send(attempt.url,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body},attempt.completion,'caller'),envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(attempt.context!==current())throw Error('The conversation account or role changed before acknowledgement. Review the submitted action in its original context.');
   if(!response.ok){uncertain=known||response.status>=500||['WORK_CONTEXT_CHANGED','WORK_ROLE_REQUIRED','WORK_ROLE_UNAVAILABLE'].includes(envelope.error?.code??'');throw Error(envelope.error?.message??'Unable to save this conversation action.');}
   if(!await validSocialReceipt(attempt,envelope.data,identity))throw Error('Conversation acknowledgement is incomplete. Retry the submitted action.');if(attempt.context!==current())throw Error('The original account or role changed before final acknowledgement.');pending=null;const reply={result:envelope.data,unchanged:procurementFormUnchanged(attempt.completion)};if(waiting?.attempt===attempt){const resume=waiting;waiting=null;resume.resolve(reply);}return reply;
  }catch(error){pending=uncertain?attempt:null;throw error;}
 }};
}

export function validSocialImageReceipt(url:string,value:unknown):value is {id:string;ownerVersion:number}{const parsed=new URL(url,'https://erpx.invalid'),match=/\/uploads\/([^/]+)\/content$/.exec(parsed.pathname);return !!match&&object(value)&&value.id===`attachment-${decodeURIComponent(match[1])}`&&typeof value.attachmentLinkId==='string'&&typeof value.attachmentSetId==='string'&&value.state==='available'&&Number.isSafeInteger(value.ownerVersion)&&Number(value.ownerVersion)>0;}
export type SocialImageAttempt={url:string;file:File;context:string};
/** Binary recovery keeps the exact original File and native reservation URL.
 * It resumes the waiting editor stage rather than saving a replacement draft. */
export function createSocialImageCommands(send:typeof workFetch=workFetch){
 let pending:SocialImageAttempt|null=null,waiting:((value:{id:string;ownerVersion:number})=>void)|null=null;
 return {pending:()=>pending,prepare(url:string,file:File,context:string){if(pending)throw Error('Retry the original submitted image before choosing another file.');return {url,file,context};},wait:()=>new Promise<{id:string;ownerVersion:number}>(resolve=>{if(!pending||waiting)throw Error('No original image upload is awaiting acknowledgement.');waiting=resolve;}),async execute(attempt:SocialImageAttempt,current:()=>string,root:string){
  const parsed=new URL(attempt.url,'https://erpx.invalid');if(!attempt.url.startsWith(`${root}/uploads/`)||parsed.search||parsed.hash||!/^\/.*\/uploads\/[^/]+\/content$/.test(parsed.pathname))throw Error('The original conversation image upload target is invalid.');
  if(attempt.context!==current())throw Error('Return to the original account and role before retrying the image.');const known=pending!==null;let uncertain=true;
  try{const response=await send(attempt.url,{method:'PUT',body:attempt.file},undefined,'caller'),envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(attempt.context!==current())throw Error('The original conversation account changed during image acknowledgement.');
   if(!response.ok){uncertain=known||response.status>=500||envelope.error?.code==='WORK_CONTEXT_CHANGED';throw Error(envelope.error?.message??'Image upload failed. Your original draft is retained.');}
   if(!validSocialImageReceipt(attempt.url,envelope.data))throw Error('The original clean image was not acknowledged. Retry the submitted image.');pending=null;const value=envelope.data;if(waiting){const resume=waiting;waiting=null;resume(value);}return value;
  }catch(error){pending=uncertain?attempt:null;throw error;}
 }};
}
