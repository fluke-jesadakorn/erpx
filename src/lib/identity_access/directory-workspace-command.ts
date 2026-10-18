import type {DirectoryPreview} from '@/lib/identity_access/directory-operations';
import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
export type DirectoryWorkspaceSource={base:string;connectionId:string;source:string;draft:number};
export type DirectoryWorkspacePath='preview'|'fields/preview'|'apply'|'submit'|'refresh'|'configure'|'fields/sync'|'retry'|'cancel';
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const strings=(value:unknown)=>Array.isArray(value)&&value.every(item=>typeof item==='string');
const preview=(path:string)=>path==='preview'||path==='fields/preview';
const paths:readonly string[]=['preview','fields/preview','apply','submit','refresh','configure','fields/sync','retry','cancel'];
const validPath=(value:string):value is DirectoryWorkspacePath=>paths.includes(value);
const workOwner=()=>{const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?JSON.stringify([node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment]):'server';};
const same=(a:DirectoryWorkspaceSource,b:DirectoryWorkspaceSource)=>a.base===b.base&&a.connectionId===b.connectionId;
export function validDirectoryPreview(value:unknown):value is DirectoryPreview{return object(value)&&text(value.token)&&['objects','people','computers','positions','warnings'].every(key=>strings(value[key]))&&Array.isArray(value.companies)&&value.companies.every(item=>object(item)&&text(item.id)&&typeof item.name==='string');}
export function validDirectoryWorkspaceResult(path:DirectoryWorkspacePath,input:unknown,value:unknown):value is Record<string,unknown>{
 if(!object(input)||!object(value))return false;
 if(path==='preview')return validDirectoryPreview(value);
 if(path==='fields/preview')return text(value.previewToken)&&typeof value.personName==='string'&&Array.isArray(value.changes)&&value.changes.every(item=>object(item)&&typeof item.label==='string'&&Object.hasOwn(item,'from')&&Object.hasOwn(item,'to'));
 if(path==='refresh')return Number.isSafeInteger(value.count)&&Number(value.count)>=0&&typeof value.providerVersion==='string';
 if(path==='configure')return typeof input.writeEnabled==='boolean'&&value.writeEnabled===input.writeEnabled;
 if(path==='retry'||path==='cancel')return value.id===input.id&&value.status===(path==='retry'?'queued':'cancelled');
 if(path==='fields/sync'&&input.direction==='read')return value.id===input.personId&&text(value.previewToken)&&object(value.impact)&&['membersAdded','membersRemoved','accessRemoved','accessPending','accessBlocked','matchingPeople'].every(key=>strings(value.impact&&object(value.impact)?value.impact[key]:null));
 return text(value.id)&&value.status===(path==='apply'?'queued':'pending_review')&&(path==='apply'?value.reviewId===null:text(value.reviewId));
}
type Attempt={source:DirectoryWorkspaceSource;work:string;path:DirectoryWorkspacePath;input:unknown;body:string;key:string;uncertain:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>;hadCompletion:boolean};
/** Standard-only controller: original operation/body/key, no automatic event or new status/version model. */
export function createDirectoryWorkspaceCommands(current:()=>DirectoryWorkspaceSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original directory request.');const now=current();if(!now||!same(now,captured.source)||workOwner()!==captured.work)throw Error('Return to the original directory connection and work role to recover this request.');
  const before={...now},work=captureWorkCommandContext(),assertCurrent=()=>{work();const live=current();if(!live||!same(before,live)||live.source!==before.source||workOwner()!==captured.work)throw Object.assign(Error('The directory context changed. Your original request and newer draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};busy=true;
  try{
   const response=await send(`/api/v1${before.base}/people/directory-operations/${captured.path}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),decoded:unknown=await response.json();assertCurrent();
   if(!response.ok){const error=object(decoded)&&object(decoded.error)?decoded.error:{};throw Object.assign(Error(typeof error.message==='string'?error.message:'The directory request was not confirmed.'),{code:error.code});}
   const data=object(decoded)?decoded.data:null;if(!validDirectoryWorkspaceResult(captured.path,captured.input,data))throw Error('The directory acknowledgement is incomplete. Recover the original request.');captured.uncertain=true;
   return {data,path:captured.path,input:captured.input,unchanged:()=>{const completion=captured.completion();return current()?.draft===captured.source.draft&&(!captured.hadCompletion||!!completion)&&(!completion||completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion);},acknowledge(){assertCurrent();if(attempt!==captured)return false;attempt=null;if(!preview(captured.path)){const completion=captured.completion();if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:completion}));announceWorkChange();}return true;}};
  // Readonly previews are ephemeral. Mutation errors, including a first typed
  // 4xx after send, cannot prove rollback after a lost COMMIT acknowledgement.
  }catch(error){if(attempt===captured){if(preview(captured.path))attempt=null;else captured.uncertain=true;}throw error;}finally{busy=false;}
 }
 return {submit(path:string,input:unknown,completion?:WorkCommandCompletion){if(!validPath(path))throw Error('Choose an original directory workflow.');const source=current();if(!source)throw Error('Return to the directory workspace.');if(object(input)&&typeof input.connectionId==='string'&&input.connectionId!==source.connectionId)throw Error('Return to the original directory connection.');const body=JSON.stringify(input);if(typeof body!=='string')throw Error('Choose an original directory request.');if(attempt&&(attempt.path!==path||attempt.body!==body||!same(source,attempt.source)||workOwner()!==attempt.work))throw Error('Recover the original directory request first. Your newer draft is retained.');attempt??={source:{...source},work:workOwner(),path,input:JSON.parse(body),body,key:crypto.randomUUID(),uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);},retry(){if(!attempt)throw Error('There is no directory request to recover.');return run(attempt);},pending:()=>attempt!==null};
}
