import {workFetch} from '@erpx/feature-work_projection/lib/work-fetch';
type Attempt={id:string;body:string;digest:string;key:string;intent?:string;link?:string;set?:string;url?:string};
/** Legacy Leave evidence still uses the normal registered Files reservation,
 * clean scan and domain binding. Recovery stores metadata; never private bytes. */
export function createLeaveEvidenceClient(base:string,context:string){
 const storage=`erpx:workforce-evidence:${context}`;let attempt:Attempt|null=null,running=false;
 const persist=()=>{try{if(attempt)sessionStorage.setItem(storage,JSON.stringify(attempt));else sessionStorage.removeItem(storage);}catch{}};
 const restore=()=>{if(attempt)return attempt;try{const saved=JSON.parse(sessionStorage.getItem(storage)??'null') as Attempt|null;if(saved&&typeof saved.id==='string'&&typeof saved.body==='string'&&typeof saved.digest==='string'&&typeof saved.key==='string')attempt=saved;}catch{}return attempt;};
 return {pending:()=>Boolean(restore()),target:()=>restore()?.id??null,async upload(id:string,file:File,purpose:string){
  if(running)throw Error('Wait for the current evidence upload.');running=true;
  try{
   const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',await file.arrayBuffer())),byte=>byte.toString(16).padStart(2,'0')).join(''),body=JSON.stringify({ownerType:'leave_request',ownerId:id,purpose,filename:file.name,mediaType:file.type,bytes:file.size}),prior=restore();
   if(prior&&(prior.id!==id||prior.body!==body||prior.digest!==digest))throw Error('Check the original evidence upload with the same file before choosing another file.');
   const current=attempt=prior??{id,body,digest,key:crypto.randomUUID()};persist();
   if(!current.url){
    const response=await workFetch(`/api/v1${base}/uploads`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':current.key},body:current.body},undefined,'caller'),result=await response.json();if(!response.ok)throw Error(result.error?.message??'The evidence reservation was not acknowledged.');
    const value=result.data,expected=typeof value?.uploadIntentId==='string'?`/api/v1${base}/uploads/${encodeURIComponent(value.uploadIntentId)}/content`:'';
    if(!expected||value.uploadUrl!==expected||typeof value.attachmentLinkId!=='string'||typeof value.attachmentSetId!=='string')throw Error('The original evidence reservation was not acknowledged. Retry the same file.');
    current.intent=value.uploadIntentId;current.link=value.attachmentLinkId;current.set=value.attachmentSetId;current.url=value.uploadUrl;persist();
   }
   // A restored URL is validated again before any private bytes leave the page.
   if(current.url!==`/api/v1${base}/uploads/${encodeURIComponent(current.intent??'')}/content`)throw Error('The original evidence upload target is invalid.');
   const response=await workFetch(current.url,{method:'PUT',headers:{'Content-Type':file.type},body:file},undefined,'caller'),result=await response.json();if(!response.ok)throw Error(result.error?.message??'The draft is saved. Retry the original evidence upload.');
   const value=result.data;if(value?.uploadIntentId!==current.intent||value.attachmentLinkId!==current.link||value.attachmentSetId!==current.set||value.id!==`attachment-${current.intent}`||value.sourceId!==value.id||value.state!=='available')throw Error('Clean evidence was not acknowledged for the original upload. Retry the same file.');
   attempt=null;persist();return {sourceId:String(value.sourceId)};
  }finally{running=false;}
 }};
}
