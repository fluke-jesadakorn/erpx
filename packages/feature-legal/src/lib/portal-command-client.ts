import {holdingFetch} from '@erpx/contracts/holding-fetch';
import {legalPortalUploadFingerprintInput} from '@erpx/contracts/legal-portal-upload';
export type PortalCommandSource={shareId:string;version:number;checksum:string};
type Command={kind:'command';path:string;key:string;source:PortalCommandSource;body:string;uncertain:boolean};
type Upload={kind:'upload';path:string;key:string;source:PortalCommandSource;name:string;mediaType:string;size:number;checksum:string;uncertain:boolean};
type Attempt=Command|Upload;
export type PortalAcknowledgement={id:string};
export type PortalCommandResult={attempt:Attempt;data:PortalAcknowledgement;file?:File};
const record=(value:unknown):Record<string,unknown>|null=>value!==null&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:null;
const actions=['accept','comment','view','billing.submit'];
const sameSource=(a:PortalCommandSource,b:PortalCommandSource)=>a.shareId===b.shareId&&a.version===b.version&&a.checksum===b.checksum;
/** Plain public-session transport. An uncertain action keeps its exact key and
 * payload under the server-supplied account/session scope. Upload bytes stay in
 * memory; after refresh only the same reselected bytes can recover their key. */
export function createLegalPortalCommandClient(base:string,scope:string){
 const storageKey=`erpx:legal-portal:${scope}`;let attempt:Attempt|null=null,inFlight=false,fileCache:File|undefined;
 const persist=()=>{try{if(attempt)sessionStorage.setItem(storageKey,JSON.stringify(attempt));else sessionStorage.removeItem(storageKey);}catch{}};
 const restore=()=>{if(attempt)return attempt;try{const saved=record(JSON.parse(sessionStorage.getItem(storageKey)??'null')),source=record(saved?.source);if(!saved||!source||typeof source.shareId!=='string'||!Number.isSafeInteger(source.version)||typeof source.checksum!=='string'||typeof saved.key!=='string'||typeof saved.uncertain!=='boolean')return null;
  const bound={shareId:source.shareId,version:Number(source.version),checksum:source.checksum};
  if(saved.kind==='command'&&saved.path===base&&typeof saved.body==='string'){const body=record(JSON.parse(saved.body));if(body?.shareId===bound.shareId&&actions.includes(String(body.action)))attempt={kind:'command',path:base,key:saved.key,body:saved.body,source:bound,uncertain:saved.uncertain};}
  else if(saved.kind==='upload'&&saved.path===`${base}/${encodeURIComponent(bound.shareId)}/upload`&&typeof saved.name==='string'&&typeof saved.mediaType==='string'&&Number.isSafeInteger(saved.size)&&typeof saved.checksum==='string')attempt={kind:'upload',path:saved.path,key:saved.key,source:bound,name:saved.name,mediaType:saved.mediaType,size:Number(saved.size),checksum:saved.checksum,uncertain:saved.uncertain};
 }catch{}return attempt;};
 async function transmit(current:Attempt,file?:File):Promise<PortalCommandResult>{
  attempt=current;persist();
  try{
   const response=await holdingFetch(current.path,{method:'POST',headers:current.kind==='command'?{'Content-Type':'application/json','Idempotency-Key':current.key}:{'Content-Type':current.mediaType,'X-File-Name':encodeURIComponent(current.name),'Idempotency-Key':current.key,'X-Legal-Upload-Recovery':'status-v1'},body:current.kind==='command'?current.body:file});
   const definitive=response.status>=400&&response.status<500&&![408,429].includes(response.status),keepUnknown=current.uncertain&&[401,403,404,409].includes(response.status);
   if(definitive&&!keepUnknown){attempt=null;persist();}
   const payload=await response.json().catch(()=>{throw Error(definitive&&!keepUnknown?'The request was rejected. Review the retained draft.':'The result could not be read. Check the previous result before sending another action.');});
   if(!response.ok)throw Error(record(record(payload)?.error)?.message as string||'The request was not acknowledged. Check the previous result.');
   const data=record(record(payload)?.data),expected=current.kind==='command'?current.source.shareId:current.key;
   if(data&&Object.prototype.hasOwnProperty.call(data,'uploadOutcome')){
    const outcome=record(data.uploadOutcome),keys=outcome?Object.keys(outcome).sort():[];
    if(current.kind!=='upload'||response.status!==200||!outcome||keys.join(',')!=='id,outcome,requestFingerprint'||outcome.id!==current.key||outcome.outcome!=='not-applied'||outcome.requestFingerprint!==await textFingerprint(legalPortalUploadFingerprintInput({shareId:current.source.shareId,name:current.name,mediaType:current.mediaType,bytes:current.size,checksum:current.checksum})))throw Error('The previous upload outcome could not be confirmed. Check its original request.');
    attempt=null;persist();
    throw Error('The previous upload was not applied because its source changed. Your selected file was retained. Review it and submit again with a new key.');
   }
   if(data?.id!==expected)throw Error('The response did not acknowledge this exact shared-document action. Check the previous result.');
   attempt=null;fileCache=undefined;persist();return {attempt:current,data:{id:expected},...(file?{file}: {})};
  }catch(error){if(attempt){attempt.uncertain=true;persist();}throw error;}
 }
 async function run<Result>(task:()=>Promise<Result>){if(inFlight)throw Error('Wait for the current shared-document action to finish.');inFlight=true;try{return await task();}finally{inFlight=false;}}
 async function fingerprint(file:File){const digest=await crypto.subtle.digest('SHA-256',await file.arrayBuffer());return Array.from(new Uint8Array(digest),byte=>byte.toString(16).padStart(2,'0')).join('');}
 async function textFingerprint(text:string){const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(text));return Array.from(new Uint8Array(digest),byte=>byte.toString(16).padStart(2,'0')).join('');}
 async function upload(source:PortalCommandSource,file:File){return run(async()=>{
  if(!file.size||file.size>10*1024*1024||!['application/pdf','application/vnd.openxmlformats-officedocument.wordprocessingml.document'].includes(file.type))throw Error('Select a PDF or DOCX between 1 byte and 10 MB.');
  const checksum=await fingerprint(file),path=`${base}/${encodeURIComponent(source.shareId)}/upload`,prior=restore();
  if(prior&&(prior.kind!=='upload'||prior.path!==path||!sameSource(prior.source,source)||prior.name!==file.name||prior.mediaType!==file.type||prior.size!==file.size||prior.checksum!==checksum))throw Error('A previous action has an uncertain result. Check its original details before sending a different file or action.');
  const current:Upload=prior?.kind==='upload'?prior:{kind:'upload',path,key:crypto.randomUUID(),source:{...source},name:file.name,mediaType:file.type,size:file.size,checksum,uncertain:false};fileCache=file;return transmit(current,file);
 });}
 return {
  command(source:PortalCommandSource,input:Record<string,unknown>){return run(async()=>{if(input.shareId!==source.shareId||!actions.includes(String(input.action)))throw Error('Use the original shared-document action.');const body=JSON.stringify(input),prior=restore();if(prior&&(prior.kind!=='command'||prior.body!==body||!sameSource(prior.source,source)))throw Error('A previous action has an uncertain result. Check its original details before sending a different action.');return transmit(prior??{kind:'command',path:base,key:crypto.randomUUID(),source:{...source},body,uncertain:false});});},
  upload,
  pending(){const current=restore();return current?{kind:current.kind,shareId:current.source.shareId,needsFile:current.kind==='upload'&&!fileCache,...(current.kind==='upload'?{name:current.name}: {})}:null;},
  async recover(file?:File){const current=restore();if(!current)return null;if(current.kind==='command')return run(()=>transmit(current));const original=file??fileCache;if(!original)throw Error('Reselect the same original file to check the previous upload.');return upload(current.source,original);},
 };
}
