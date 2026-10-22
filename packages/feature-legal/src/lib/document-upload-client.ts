import {workFetch} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureLegalCommandCompletion,acknowledgeLegalCommand} from '@erpx/feature-shared/lib/legal/command-completion';
export type LegalDocumentUploadSource={id:string;version:number};
type Attempt={id:string;version:number;key:string;name:string;mediaType:string;size:number;checksum:string;uncertain:boolean};
export type LegalDocumentUploadReceipt={id:string;version:number;documentId:string};
export function legalDocumentUploadAcknowledged(value:unknown,source:LegalDocumentUploadSource,executed=false):value is LegalDocumentUploadReceipt&{executedCopyId?:string}{const data=value&&typeof value==='object'?value as Record<string,unknown>:null;return Boolean(data&&data.id===source.id&&data.version===source.version+1&&typeof data.documentId==='string'&&data.documentId&&(!executed||typeof data.executedCopyId==='string'&&data.executedCopyId));}
/** Original document upload, under its original Work context. Only a fingerprint
 * is persisted; retry never substitutes a newer record version or file. */
export function createLegalDocumentUploadClient(base:string,context:string){
 const storageKey=`${context}:upload`;let attempt:Attempt|null=null,fileCache:File|undefined,inFlight=false;
 const persist=()=>{try{if(attempt)sessionStorage.setItem(storageKey,JSON.stringify(attempt));else sessionStorage.removeItem(storageKey);}catch{}};
 function restore(){if(attempt)return attempt;try{const value=JSON.parse(sessionStorage.getItem(storageKey)??'null') as Partial<Attempt>|null;if(value&&typeof value.id==='string'&&Number.isSafeInteger(value.version)&&Number(value.version)>0&&typeof value.key==='string'&&typeof value.name==='string'&&typeof value.mediaType==='string'&&Number.isSafeInteger(value.size)&&typeof value.checksum==='string')attempt={...value,uncertain:value.uncertain===true} as Attempt;}catch{}return attempt;}
 async function fingerprint(file:File){return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',await file.arrayBuffer())),byte=>byte.toString(16).padStart(2,'0')).join('');}
 async function upload(source:LegalDocumentUploadSource,file:File):Promise<LegalDocumentUploadReceipt>{
  if(inFlight)throw Error('Wait for the original Legal document upload to finish.');inFlight=true;
  try{if(!file.size||file.size>10*1024*1024)throw Error('Choose an original document between 1 byte and 10 MB.');const checksum=await fingerprint(file),prior=restore();
   if(prior&&(prior.id!==source.id||prior.version!==source.version||prior.name!==file.name||prior.mediaType!==file.type||prior.size!==file.size||prior.checksum!==checksum))throw Error('Retry the original file and Legal record version before uploading different work.');
   const current=prior??{...source,key:crypto.randomUUID(),name:file.name,mediaType:file.type,size:file.size,checksum,uncertain:false};attempt=current;fileCache=file;persist();
   const completion=captureLegalCommandCompletion();try{const response=await workFetch(`${base}/upload/${encodeURIComponent(current.id)}`,{method:'POST',headers:{'Content-Type':current.mediaType,'X-Record-Version':String(current.version),'X-File-Name':encodeURIComponent(current.name),'Idempotency-Key':current.key},body:file},completion,'caller');
    const definitive=response.status>=400&&response.status<500&&![408,429].includes(response.status),keepUnknown=current.uncertain&&[401,403,404,409].includes(response.status);if(definitive&&!keepUnknown){attempt=null;persist();}
    const result=await response.json().catch(()=>{throw Error(definitive&&!keepUnknown?'The upload was rejected. Review the original file.':'The upload result could not be read. Check the original upload before submitting another.');});if(!response.ok)throw Error(result?.error?.message??'The original upload has not been acknowledged.');const data=result?.data;
    if(!legalDocumentUploadAcknowledged(data,current))throw Error('The response did not acknowledge this exact Legal document upload. Check the original upload.');
    attempt=null;fileCache=undefined;persist();acknowledgeLegalCommand(completion);return {id:data.id,version:data.version,documentId:data.documentId};
   }catch(error){if(attempt){attempt.uncertain=true;persist();}throw error;}
  }finally{inFlight=false;}
 }
 return {upload,pending(){const current=restore();return current?{id:current.id,version:current.version,name:current.name,needsFile:!fileCache}:null;},async recover(file?:File){const current=restore();if(!current)return null;const original=file??fileCache;if(!original)throw Error(`Reselect the same original file (${current.name}) to check this upload.`);return upload({id:current.id,version:current.version},original);}};
}
