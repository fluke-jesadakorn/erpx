import type {DirectorySourceApproveInput,DirectorySourcePreviewInput,DirectorySourceReviewData} from './directory-source-admin.generated.ts';

/** Captured render inputs only. The native service rechecks every pin. */
export type DirectorySourceSelectionPins={companyId:string;revision:number;assignmentId:string;assignmentVersion:number};
export type DirectorySourceTarget={holdingSlug:string;connectionId:string};
export type DirectorySourceFetch=(path:string,init:RequestInit)=>Promise<Response>;
export class DirectorySourceClientError extends Error {
  constructor(readonly code:string, message:string, readonly status:number){super(message);this.name='DirectorySourceClientError';}
}
const LIMIT=16*1024;
const record=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const integer=(value:unknown,min:number):value is number=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=min;
const id=(value:unknown):value is string=>typeof value==='string'&&value.length>0&&!/[\u0000-\u001f\u007f-\u009f]/.test(value)&&new TextEncoder().encode(value).length<=200;
const reason=(value:unknown):value is string=>typeof value==='string'&&value===value.trim()&&Array.from(value).length>=3&&Array.from(value).length<=1000;
function invalid(status=0):never{throw new DirectorySourceClientError('DIRECTORY_SOURCE_CONTRACT_INVALID','The Directory source response did not match its contract.',status);}
function pinsHeaders(pins:DirectorySourceSelectionPins):Record<string,string>{
  if(!id(pins.companyId)||!id(pins.assignmentId)||!integer(pins.revision,0)||!integer(pins.assignmentVersion,1))invalid();
  return {'x-erpx-context-company':pins.companyId,'x-erpx-context-revision':String(pins.revision),'x-erpx-context-assignment':pins.assignmentId,'x-erpx-context-assignment-version':String(pins.assignmentVersion)};
}
function path(target:DirectorySourceTarget):string{
  if(!id(target.holdingSlug)||!id(target.connectionId))invalid();
  return `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/iam/connections/${encodeURIComponent(target.connectionId)}/source`;
}
function keyHeader(key:string):string{if(typeof key!=='string'||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))invalid();return key;}
function normalizedReason(value:string):string{if(typeof value!=='string')invalid();const normalized=value.trim();if(!reason(normalized))invalid();return normalized;}
function data(value:unknown):value is DirectorySourceReviewData{
  if(!record(value)||!id(value.id)||!integer(value.version,1)||!integer(value.sourceRevision,1)||(typeof value.status!=='string'||!['pending','approved'].includes(value.status)))return false;
  const names=['id','status','version','sourceRevision',...(value.status==='approved'?['decisionReason']:[])];
  return Object.keys(value).length===names.length&&names.every(name=>Object.hasOwn(value,name))&&(value.status==='pending'||reason(value.decisionReason));
}
async function boundedJson(response:Response):Promise<unknown>{
  const declared=response.headers.get('content-length');if(declared!==null&&(!/^\d+$/.test(declared)||Number(declared)>LIMIT)){await response.body?.cancel();invalid(response.status);}
  if(!response.body)invalid(response.status);
  const reader=response.body.getReader();const chunks:Uint8Array[]=[];let size=0;
  try{for(;;){const part=await reader.read();if(part.done)break;size+=part.value.byteLength;if(size>LIMIT){await reader.cancel();invalid(response.status);}chunks.push(part.value);}}
  finally{reader.releaseLock();}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
  if(response.status===404)throw new DirectorySourceClientError('NOT_FOUND','Not found.',404);
  try{return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{invalid(response.status);}
}
/** Explicit transport injection; no Next/backend import, ambient host discovery,
 * default backend, browser storage, authorization DTO or operator configuration. */
export function createDirectorySourceAdminClient(fetcher:DirectorySourceFetch){
  async function post(url:string,pins:DirectorySourceSelectionPins,key:string,body:object,signal?:AbortSignal):Promise<DirectorySourceReviewData>{
    const response=await fetcher(url,{method:'POST',credentials:'include',cache:'no-store',redirect:'error',signal,headers:{...pinsHeaders(pins),Accept:'application/json','Content-Type':'application/json','Idempotency-Key':keyHeader(key)},body:JSON.stringify(body)});
    const value=await boundedJson(response);
    if(!response.ok){
      if(!record(value)||Object.keys(value).length!==1||!record(value.error)||Object.keys(value.error).length!==2||typeof value.error.code!=='string'||!/^[A-Z0-9_]{1,100}$/.test(value.error.code)||typeof value.error.message!=='string'||value.error.message.length>4096)invalid(response.status);
      throw new DirectorySourceClientError(value.error.code,value.error.message,response.status);
    }
    if(response.status!==200||!record(value)||Object.keys(value).length!==1||!data(value.data))invalid(response.status);
    return value.data;
  }
  return {
    preview(target:DirectorySourceTarget,pins:DirectorySourceSelectionPins,key:string,input:DirectorySourcePreviewInput,signal?:AbortSignal){
      if(typeof input.keyId!=='string'||!/^[a-zA-Z0-9_-]{1,80}$/.test(input.keyId)||!integer(input.expectedConnectionVersion,1)||(input.expectedSourceRevision!=null&&!integer(input.expectedSourceRevision,1)))invalid();
      const body={keyId:input.keyId,expectedConnectionVersion:input.expectedConnectionVersion,...(input.expectedSourceRevision!==undefined?{expectedSourceRevision:input.expectedSourceRevision}:{}),reason:normalizedReason(input.reason)};
      return post(`${path(target)}/preview`,pins,key,body,signal);
    },
    approve(target:DirectorySourceTarget,reviewId:string,pins:DirectorySourceSelectionPins,key:string,input:DirectorySourceApproveInput,signal?:AbortSignal){
      if(!id(reviewId)||!integer(input.expectedVersion,1))invalid();
      return post(`${path(target)}/reviews/${encodeURIComponent(reviewId)}/approve`,pins,key,{expectedVersion:input.expectedVersion,reason:normalizedReason(input.reason)},signal);
    },
  };
}
