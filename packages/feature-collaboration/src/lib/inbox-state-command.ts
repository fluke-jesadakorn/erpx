import {workFetch} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {contextGeneration} from '@erpx/feature-work_projection/lib/context-switch';
export type InboxStatePatch={read?:boolean;archived?:boolean};
type Attempt={id:string;patch:InboxStatePatch;body:string;key:string;busy:boolean;uncertain:boolean;assertCurrent:()=>void};
// Document-local retention survives an RSC/component remount. Different Work
// identities never inherit the payload or key of the original acknowledgement.
const retained=new WeakMap<object,Map<string,Attempt>>();
const observers=new WeakMap<object,Map<string,Set<()=>void>>>();
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const time=(v:unknown)=>v===null||typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
export function createInboxStateCommands(base:string,send:typeof workFetch=workFetch){
 let store:Map<string,Attempt>|undefined,slot:string|undefined,listeners:Set<()=>void>|undefined;
 const bind=()=>{
  if(store&&slot)return;
  const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
  if(!node)throw Error('Review your current work context before updating the Inbox.');
  slot=JSON.stringify([base,contextGeneration(),node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workAssignmentVersion]);
  store=retained.get(node);if(!store){store=new Map();retained.set(node,store);}
  let scopes=observers.get(node);if(!scopes){scopes=new Map();observers.set(node,scopes);}listeners=scopes.get(slot);if(!listeners){listeners=new Set();scopes.set(slot,listeners);}
 };
 const pending=()=>{bind();return store!.get(slot!)??null;};
 async function execute(attempt:Attempt){
  if(attempt.busy)throw Error('The original update is still being confirmed.');
  attempt.busy=true;let sent=false,rejected=false;
  const clear=()=>{if(store!.get(slot!)===attempt){store!.delete(slot!);for(const notify of listeners!)notify();}};
  try{
   attempt.assertCurrent();
   sent=true;const response=await send(`/api/v1${base}/inbox/notifications/${encodeURIComponent(attempt.id)}/state`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body});
   const body:unknown=await response.json();attempt.assertCurrent();
   if(!response.ok){
    const error=object(body)&&object(body.error)?body.error:null;
    // A first, explicit pre-write rejection may release the intent. A later
    // rejection cannot prove that an earlier lost response did not commit.
    if(!attempt.uncertain&&error&&((response.status===422&&['INVALID_INBOX_STATE','INVALID_JSON','COMMAND_KEY_REQUIRED','INVALID_COMMAND_KEY'].includes(String(error.code)))||(response.status===404&&error.code==='INBOX_ITEM_NOT_FOUND'))){rejected=true;clear();}
    throw Error(error&&typeof error.message==='string'?error.message:'Confirm the original update before another change.');
   }
   const result=object(body)?body.data:undefined;
   if(!object(result)||result.id!==attempt.id||!time(result.readAt)||!time(result.archivedAt)||attempt.patch.read!==undefined&&(attempt.patch.read?result.readAt===null:result.readAt!==null)||attempt.patch.archived!==undefined&&(attempt.patch.archived?result.archivedAt===null:result.archivedAt!==null))throw Error('Confirm the original update from its saved result.');
   if(store!.get(slot!)!==attempt)throw Error('The pending Inbox update changed.');
   clear();return result;
  }catch(error){if(sent&&!rejected)attempt.uncertain=true;else if(!sent&&!attempt.uncertain)clear();throw error;}finally{attempt.busy=false;}
 }
 return{
  pending,
  hasPending(){try{return pending()!==null;}catch{return false;}},
  subscribe(listener:()=>void){bind();listeners!.add(listener);return()=>{listeners!.delete(listener);};},
  submit(id:string,patch:InboxStatePatch){
   if(!id||!Object.keys(patch).length||Object.entries(patch).some(([k,v])=>!['read','archived'].includes(k)||typeof v!=='boolean'))throw Error('Choose a read or archive state.');
   const body=JSON.stringify(patch);let attempt=pending();
   if(attempt&&(attempt.id!==id||attempt.body!==body))throw Error('Confirm the original Inbox update before another change.');
   if(!attempt){const assertCurrent=captureWorkCommandContext();assertCurrent();attempt={id,patch:{...patch},body,key:crypto.randomUUID(),busy:false,uncertain:false,assertCurrent};store!.set(slot!,attempt);for(const notify of listeners!)notify();}
   return execute(attempt);
  },
  retry(){const attempt=pending();if(!attempt)throw Error('There is no pending Inbox update.');return execute(attempt);},
 };
}
