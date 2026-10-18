import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import type {ExpensePaymentCancellationInput,ExpensePaymentCancellationResult} from '@/server/modules/people_payroll/public/expense-payments';
export type ExpensePaymentCancellationSource={base:string;companyId:string;expenseId:string;draftId:string;expenseVersion:number;draftVersion:number;canCancel:boolean;history:boolean;contextKey:string};
export const expensePaymentCancellationOwner=(source:ExpensePaymentCancellationSource)=>JSON.stringify([source.base,source.companyId,source.expenseId,source.draftId,source.contextKey]);
const work=()=>{const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?[node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment].join(':'):'server';};
const witness=(source:ExpensePaymentCancellationSource)=>JSON.stringify([expensePaymentCancellationOwner(source),source.expenseVersion,source.draftVersion,source.canCancel,source.history]);
export function validExpensePaymentCancellationReceipt(value:unknown,id:string,expenseId:string,input:ExpensePaymentCancellationInput):value is ExpensePaymentCancellationResult{
 if(!value||typeof value!=='object'||Array.isArray(value))return false;const row=value as Record<string,unknown>;
 return row.id===id&&row.expenseId===expenseId&&row.status==='cancelled'&&row.version===input.expectedVersion+1&&row.expenseVersion===input.expenseVersion&&row.reason===input.reason.trim()&&row.paymentNotExecuted===true;
}
type Attempt={body:string;key:string;input:ExpensePaymentCancellationInput;source:ExpensePaymentCancellationSource;work:string;uncertain:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
export function createExpensePaymentCancellationCommands(current:()=>ExpensePaymentCancellationSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original cancellation request.');const source=current();
  if(!source||expensePaymentCancellationOwner(source)!==expensePaymentCancellationOwner(captured.source)||work()!==captured.work||!source.history)throw Error('Return to the original Company, claim and Treasury role before retrying.');
  const stamp=witness(source),assertWork=captureWorkCommandContext(),assertCurrent=()=>{assertWork();const live=current();if(!live||witness(live)!==stamp||work()!==captured.work)throw Object.assign(Error('The payment preparation or work context changed. Your original request and newer draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  const prior=captured.uncertain;let sent=false;busy=true;
  try{
   assertCurrent();sent=true;const response=await send(`/api/v1${source.base}/expenses/${encodeURIComponent(source.expenseId)}/payments/${encodeURIComponent(source.draftId)}/cancel`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   assertCurrent();
   if(!response.ok){if(!prior&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED')attempt=null;throw Object.assign(Error(body.error?.message??'Cancellation was not confirmed. Retry its original request.'),{code:body.error?.code});}
   if(!validExpensePaymentCancellationReceipt(body.data,source.draftId,source.expenseId,captured.input))throw Error('The original cancellation receipt is incomplete. Confirm the same request.');
   const receipt=body.data;captured.uncertain=true;
   return {receipt,unchanged(){const completion=captured.completion();return captured.hadCompletion?!!completion&&completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion:true;},acknowledge(){assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,financeAcknowledged:true}}));announceWorkChange();return true;}};
  }catch(error){if(attempt===captured){if(!sent&&!prior)attempt=null;else captured.uncertain=true;}throw error;}finally{busy=false;}
 }
 return {submit(input:ExpensePaymentCancellationInput,completion?:WorkCommandCompletion){const source=current();if(!source?.canCancel||!source.history||input.expectedVersion!==source.draftVersion||input.expenseVersion!==source.expenseVersion||!Number.isSafeInteger(input.expectedVersion)||input.expectedVersion<1||!Number.isSafeInteger(input.expenseVersion)||input.expenseVersion<1||input.reason.trim().length<3||input.paymentNotExecuted!==true)throw Error('Review the original payment preparation, claim version and reason before cancelling.');const body=JSON.stringify(input);if(attempt&&(attempt.body!==body||expensePaymentCancellationOwner(attempt.source)!==expensePaymentCancellationOwner(source)||attempt.work!==work()))throw Error('Confirm the original cancellation first. Your newer edits remain unsent.');attempt??={source:{...source},work:work(),input:{...input},body,key:crypto.randomUUID(),uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);},retry(){if(!attempt)throw Error('There is no original cancellation request to retry.');return run(attempt);},pending:()=>attempt!==null};
}
