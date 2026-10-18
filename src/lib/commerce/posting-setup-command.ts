import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {commerceCommandContext,commerceFormUnchanged} from '@/lib/commerce/commerce-command-fetch';
import type {CommercePostingSetupInput} from '@/server/modules/commerce/public/posting-setup';

export type PostingSetupSource={base:string;companyId:string;contextKey:string|null};
export type SavedPostingSetup=Pick<PostingSetupSource,'base'|'companyId'>&{input:CommercePostingSetupInput};
/** Local receipts belong to their original Company. Only an untouched, unsubmitted
 * draft may follow a newly supplied server seed. */
export function postingSetupViewState(source:PostingSetupSource,state:{saved:SavedPostingSetup|null;notice:string;seed:string|null;draftScope:string;dirty:boolean;pending:boolean}){
 const currentScope=JSON.stringify([source.base,source.companyId,source.contextKey]),saved=state.saved?.base===source.base&&state.saved.companyId===source.companyId?state.saved.input:null,retain=state.dirty||state.pending;
 return {saved,notice:saved?state.notice:'',seed:retain?state.seed:source.contextKey,draftScope:retain?state.draftScope:currentScope,currentScope};
}
export type PostingSetupReceipt={id:string;status:'configured';version:1};
export function validPostingSetupReceipt(value:unknown,companyId:string):value is PostingSetupReceipt{
 return !!value&&typeof value==='object'&&!Array.isArray(value)&&'id' in value&&value.id===companyId&&'status' in value&&value.status==='configured'&&'version' in value&&value.version===1;
}
type Attempt={source:PostingSetupSource;workContext:string;body:string;key:string;uncertain:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
/** One initial mapping, with the originally submitted body/key retained until
 * an exact owner receipt is acknowledged in the same live work context. */
export function createPostingSetupCommands(current:()=>PostingSetupSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 const same=(a:PostingSetupSource,b:PostingSetupSource)=>a.base===b.base&&a.companyId===b.companyId;
 async function run(submitted:Attempt){
  if(busy)throw Error('Wait for the submitted posting setup.');
  const source=current();
  if(!source||!same(source,submitted.source)||commerceCommandContext()!==submitted.workContext)throw Error('Return to the original Company and work role before retrying posting setup.');
  const assertWork=captureWorkCommandContext(),before={...source},wasUncertain=submitted.uncertain;
  const assertCurrent=()=>{assertWork();const live=current();if(!live||!same(before,live)||live.contextKey!==before.contextKey||commerceCommandContext()!==submitted.workContext)throw Object.assign(Error('Your posting setup context changed. Return to the original Company before retrying.'),{code:'WORK_CONTEXT_CHANGED'});};
  busy=true;
  try{
   assertCurrent();
   const response=await send(`/api/v1${submitted.source.base}/commerce/posting-setup`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':submitted.key},body:submitted.body},submitted.completion(),'caller');
   const result=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   if(!response.ok){
    if(!wasUncertain&&response.status<500&&result.error?.code!=='WORK_CONTEXT_CHANGED'){assertCurrent();attempt=null;}
    throw Object.assign(Error(result.error?.message??'Posting setup was not saved.'),{code:result.error?.code});
   }
   if(!validPostingSetupReceipt(result.data,submitted.source.companyId))throw Error('Posting setup acknowledgement is incomplete. Retry the submitted setup.');
   assertCurrent();submitted.uncertain=true;const receipt=result.data;
   return {receipt,submitted:JSON.parse(submitted.body) as CommercePostingSetupInput,unchanged:()=>{const completion=submitted.completion();return submitted.hadCompletion?!!completion&&commerceFormUnchanged(completion):true;},acknowledge(){
    assertCurrent();if(attempt!==submitted)return false;attempt=null;
    const completion=submitted.completion();if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,commerceAcknowledged:true}}));
    announceWorkChange();return true;
   }};
  }catch(error){if(attempt===submitted)submitted.uncertain=true;throw error;}finally{busy=false;}
 }
 return {
  submit(input:CommercePostingSetupInput,completion?:WorkCommandCompletion){
   const source=current();if(!source||input.contextKey!==source.contextKey)throw Error('Reload posting setup before starting a new mapping. Your draft is retained.');
   const body=JSON.stringify(input);
   if(attempt&&(attempt.body!==body||!same(source,attempt.source)||attempt.workContext!==commerceCommandContext()))throw Error('Retry the submitted setup first. Your newer draft is retained.');
   attempt??={source:{...source},workContext:commerceCommandContext(),body,key:crypto.randomUUID(),uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};
   return run(attempt);
  },
  retry(){if(!attempt)throw Error('There is no submitted posting setup to retry.');return run(attempt);},
  pending:()=>attempt!==null,
 };
}
