import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {financeDocumentCommandContext,financeDocumentFormUnchanged} from './document-post-control';
import {paymentReversalAcknowledged} from './service-payment-controls';

type Source={base:string;paymentId?:string};
const record=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const id=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
function salesReceipt(path:string,input:unknown,key:string,value:unknown):value is Record<string,unknown>{
 if(!record(value)||!id(value.id))return false;
 if(path==='invoices')return value.id===`invoice-${key}-invoice`&&value.status==='draft'&&value.version===1;
 if(path==='defaults')return value.id===`sales-defaults-${key}`&&value.status==='draft'&&value.version===1;
 const decision=/^defaults\/([^/]+)\/decide$/.exec(path);
 if(decision)return record(input)&&value.id===decision[1]&&value.version===Number(input.expectedVersion)+1&&value.status===(input.action==='approve'?'approved':input.action==='reject'?'rejected':null);
 // The original issue operation returns only the retained receipt ID.
 return /^receipts\/[^/]+\/issue$/.test(path);
}
type Attempt={source:Source;path:string;input:unknown;body:string;key:string;context:string;uncertain:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
/** Only these original owner commands share this UI completion protocol. Their
 * original response, live context and captured draft finalize together. */
function commands(current:()=>Source|null,correction:boolean,send:typeof workFetch){
 let attempt:Attempt|null=null,busy=false;
 const identity=(source:Source)=>JSON.stringify([source.base,source.paymentId??null]);
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original Finance command.');
  const source=current();if(!source||identity(source)!==identity(captured.source)||financeDocumentCommandContext()!==captured.context)throw Error('Return to the original Company, record and work role to confirm the original request.');
  const assertWork=captureWorkCommandContext(),wasUncertain=captured.uncertain;
  const assertCurrent=()=>{assertWork();const live=current();if(!live||identity(live)!==identity(captured.source)||financeDocumentCommandContext()!==captured.context)throw Object.assign(Error('Your Finance context changed. The original draft and request are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  busy=true;
  try{
   const url=`/api/v1${source.base}/finance/${correction?`payments/${encodeURIComponent(source.paymentId??'')}/reverse`:`sales/${captured.path}`}`;
   const response=await send(url,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   if(!response.ok){if(!wasUncertain&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED'){assertCurrent();attempt=null;}throw Object.assign(Error(body.error?.message??'Unable to confirm the original Finance command.'),{code:body.error?.code});}
   if(correction?!paymentReversalAcknowledged(body.data,source.paymentId??''):!salesReceipt(captured.path,captured.input,captured.key,body.data))throw Error('The original Finance receipt is incomplete. Retry its original confirmation.');
   assertCurrent();captured.uncertain=true;const data=body.data as Record<string,unknown>;
   return {data,completion:captured.completion(),unchanged:()=>{const completion=captured.completion();return captured.hadCompletion?!!completion&&financeDocumentFormUnchanged(completion):true;},acknowledge(){
    assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();
    if(typeof window!=='undefined'){if(completion){window.dispatchEvent(new CustomEvent('erpx:finance-command-acknowledged',{detail:completion}));window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,financeAcknowledged:true}}));}announceWorkChange();}return true;
   }};
  }catch(error){if(attempt===captured)captured.uncertain=true;throw error;}finally{busy=false;}
 }
 return {submit(path:string,input:unknown,completion?:WorkCommandCompletion){
  const source=current();if(!source)throw Error('Return to the original Finance editor.');
  if(!correction&&!(path==='invoices'||path==='defaults'||/^defaults\/[^/]+\/decide$/.test(path)||/^receipts\/[^/]+\/issue$/.test(path)))throw Error('This Sales command is not available.');
  const body=JSON.stringify(input),context=financeDocumentCommandContext();
  if(attempt&&(attempt.body!==body||attempt.path!==path||identity(source)!==identity(attempt.source)||context!==attempt.context))throw Error('Confirm the original request first. Your newer draft is retained.');
  attempt??={source:{...source},path,input,body,key:crypto.randomUUID(),context,uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);
 },retry(){if(!attempt)throw Error('There is no original Finance command to confirm.');return run(attempt);},pending:()=>attempt!==null};
}
export function createFinanceSalesCommands(current:()=>{base:string}|null,send:typeof workFetch=workFetch){return commands(current,false,send);}
export function createPaymentCorrectionCommands(current:()=>{base:string;paymentId:string}|null,send:typeof workFetch=workFetch){
 const owner=commands(current,true,send);return {submit:(input:unknown,completion?:WorkCommandCompletion)=>owner.submit('reverse',input,completion),retry:owner.retry,pending:owner.pending};
}
