import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {payrollCommandContext,payrollFormUnchanged} from '@/lib/people_payroll/payroll-command-fetch';
import {decimalToMinor} from '@/lib/finance/finance-money';
import type {PayrollOtherLiabilitiesView,PayrollOtherRemittanceInput,PayrollOtherBankEvidenceInput,PayrollOtherRemittanceResult,PayrollOtherBankEvidenceResult,PayrollOtherBankEvidenceIgnoreInput,PayrollOtherBankEvidenceIgnoreResult} from '@/server/modules/people_payroll/public/payroll-other-remittance';

export type PayrollOtherSource={base:string;companyId:string;runId:string;runVersion:number;currency:string;contextKey:string|null;purchaseRevision:string;canSettle:boolean;history?:boolean};
export type PayrollOtherRequest={action:'remit';input:PayrollOtherRemittanceInput}|{action:'bank-evidence';input:PayrollOtherBankEvidenceInput}|{action:'ignore-bank-evidence';input:PayrollOtherBankEvidenceIgnoreInput};
const row=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const amount=(v:unknown)=>Number.isSafeInteger(v)&&Number(v)>=0;
const positive=(v:unknown)=>Number.isSafeInteger(v)&&Number(v)>0;
const kind=(v:unknown)=>['benefit','deduction_other','employer_other'].includes(String(v));
export function validPayrollOtherLiabilities(value:unknown,source:Pick<PayrollOtherSource,'companyId'|'runId'|'runVersion'|'currency'>):value is PayrollOtherLiabilitiesView{
 if(!row(value)||value.companyId!==source.companyId||value.runId!==source.runId||value.runVersion!==source.runVersion||value.currency!==source.currency||!positive(value.runVersion)||!/^[A-Z]{3}$/.test(String(value.currency)))return false;
 if(typeof value.schemaAvailable!=='boolean'||typeof value.canRemit!=='boolean'||typeof value.canImportBankEvidence!=='boolean'||![null,'schema-required'].includes(value.unavailableReason as string|null)||value.contextKey!==null&&(typeof value.contextKey!=='string'||!/^[a-f0-9]{64}$/.test(value.contextKey)))return false;
 if(!Array.isArray(value.obligations)||!value.obligations.every(o=>row(o)&&kind(o.kind)&&['accountId','accountCode','accountName'].every(f=>text(o[f]))&&['accruedMinor','remittedMinor','outstandingMinor'].every(f=>amount(o[f]))))return false;
 if(new Set(value.obligations.map(o=>`${o.kind}:${o.accountId}`)).size!==value.obligations.length)return false;
 if(!Array.isArray(value.bankAccounts)||!value.bankAccounts.every(a=>row(a)&&['id','code','name'].every(f=>text(a[f]))&&a.currency===value.currency)||new Set(value.bankAccounts.map(a=>a.id)).size!==value.bankAccounts.length)return false;
 if(!Array.isArray(value.bankEvidence)||!value.bankEvidence.every(b=>row(b)&&['id','liabilityAccountId','bankAccountId','reference','matchStatus'].every(f=>text(b[f]))&&positive(b.version)&&kind(b.kind)&&typeof b.transactionDate==='string'&&/^\d{4}-\d{2}-\d{2}$/.test(b.transactionDate)&&Number.isSafeInteger(b.amountMinor)&&Number(b.amountMinor)<0&&b.currency===value.currency&&typeof b.canRemit==='boolean'&&(b.canIgnore===undefined||typeof b.canIgnore==='boolean')))return false;
 return !(value.canRemit||value.canImportBankEvidence)||(value.schemaAvailable&&value.unavailableReason===null&&text(value.contextKey));
}
const same=(a:PayrollOtherSource,b:PayrollOtherSource)=>Object.keys(a).every(k=>a[k as keyof PayrollOtherSource]===b[k as keyof PayrollOtherSource]);
const sameOwner=(a:PayrollOtherSource,b:PayrollOtherSource)=>a.base===b.base&&a.companyId===b.companyId&&a.runId===b.runId&&a.currency===b.currency;
const endpoint=(s:PayrollOtherSource)=>`/api/v1${s.base}/people/payroll/runs/${encodeURIComponent(s.runId)}`;
const changed=()=>Object.assign(Error('The Payroll source or work role changed. The original request and newer draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});
/** User refresh only: a matching server snapshot never causes a mount request. */
export async function readPayrollOtherLiabilities(current:()=>PayrollOtherSource|null,send:typeof workFetch=workFetch){
 const source=current();if(!source)throw changed();const captured={...source},guard=captureWorkCommandContext();guard();
 const response=await send(`${endpoint(captured)}/other-liabilities`,{cache:'no-store'}),body=await response.json() as {data?:unknown;error?:{message?:string}};
 guard();const live=current();if(!live||!same(captured,live))throw changed();
 if(!response.ok||!validPayrollOtherLiabilities(body.data,captured))throw Error(body.error?.message??'Current Payroll liabilities could not be loaded.');return body.data;
}
function receipt(value:unknown,request:PayrollOtherRequest,source:PayrollOtherSource,key:string):value is PayrollOtherRemittanceResult|PayrollOtherBankEvidenceResult|PayrollOtherBankEvidenceIgnoreResult{
 if(!row(value)||value.kind!==request.input.kind||value.liabilityAccountId!==request.input.liabilityAccountId)return false;
 if(request.action==='remit')return value.id===`payroll-remit-${key}`&&value.runId===source.runId&&value.amountMinor===request.input.amountMinor&&text(value.journalId)&&amount(value.remainingMinor);
 if(request.action==='ignore-bank-evidence')return value.id===request.input.bankTransactionId&&value.status==='ignored'&&value.version===request.input.bankVersion+1&&value.payrollRunId===source.runId;
 return value.id===`statement-${key}`&&value.imported===1&&value.status==='imported'&&value.payrollRunId===source.runId&&value.bankTransactionId===`bankrow-${key}-0`;
}
type Attempt={source:PayrollOtherSource;work:string;request:PayrollOtherRequest;body:string;key:string;uncertain:boolean;applied:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
export function createPayrollOtherRemittanceCommands(current:()=>PayrollOtherSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original Payroll request.');
  const wasUncertain=captured.uncertain;let sent=false;busy=true;
  try{
   const currentSource=current();if(!currentSource||!sameOwner(currentSource,captured.source)||currentSource.history===false||payrollCommandContext()!==captured.work)throw changed();
   // The body retains its original snapshot. A new invocation may recover the
   // same committed receipt after legitimate native revisions or withdrawal.
   // Its own full live snapshot must remain stable through JSON and ACK.
   const source={...currentSource},guard=captureWorkCommandContext(),assertCurrent=()=>{guard();const live=current();if(!live||!same(source,live)||payrollCommandContext()!==captured.work)throw changed();};
   assertCurrent();
   const suffix=captured.request.action==='remit'?'other-remittances':captured.request.action==='ignore-bank-evidence'?'other-remittances/bank-evidence/ignore':'other-remittances/bank-evidence';
   sent=true;const response=await send(`${endpoint(source)}/${suffix}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   assertCurrent();
   if(!response.ok){if(!wasUncertain&&response.status>=400&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED')attempt=null;throw Object.assign(Error(body.error?.message??'Payroll remittance could not be confirmed.'),{code:body.error?.code});}
   if(!receipt(body.data,captured.request,source,captured.key))throw Error('The Payroll receipt is incomplete. Retry the original request.');
   captured.applied=true;
   const refreshed=await send(`${endpoint(source)}/other-liabilities`,{cache:'no-store'}),decoded=await refreshed.json() as {data?:unknown;error?:{message?:string}};
   assertCurrent();
   if(!refreshed.ok||!validPayrollOtherLiabilities(decoded.data,source)||!decoded.data.schemaAvailable||decoded.data.unavailableReason!==null)throw Error(decoded.error?.message??'The request was saved. Confirm its current Payroll status using the original request.');
   const view=decoded.data;
   if(captured.request.action==='bank-evidence'){
    const input=captured.request.input,entry=input.rows[0];
    if(!view.bankEvidence.some(e=>e.id===`bankrow-${captured.key}-0`&&e.kind===input.kind&&e.liabilityAccountId===input.liabilityAccountId&&e.bankAccountId===input.bankAccount&&e.currency===entry.currency&&e.transactionDate===entry.transactionDate&&e.reference===entry.externalId&&e.amountMinor===decimalToMinor(entry.amount)))throw Error('The saved bank evidence is not available in the current Payroll view. Retry the original request.');
   }
   if(captured.request.action==='ignore-bank-evidence'){
    const input=captured.request.input;
    if(!view.bankEvidence.some(e=>e.id===input.bankTransactionId&&e.kind===input.kind&&e.liabilityAccountId===input.liabilityAccountId&&e.currency===source.currency&&e.version===input.bankVersion+1&&e.matchStatus==='ignored'&&e.canRemit===false&&e.canIgnore===false))throw Error('The ignored bank evidence is not available in the current Payroll view. Retry the original request.');
   }
   captured.uncertain=true;
   return {view,submitted:captured.request,unchanged:()=>{const c=captured.completion();return captured.hadCompletion?!!c&&payrollFormUnchanged(c):true;},acknowledge(){assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();if(typeof window!=='undefined'){if(completion)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,payrollAcknowledged:true}}));announceWorkChange();}return true;}};
  }catch(error){if(attempt===captured){if(!sent&&!wasUncertain&&!captured.applied)attempt=null;else captured.uncertain=true;}throw error;}finally{busy=false;}
 }
 return {submit(request:PayrollOtherRequest,completion?:WorkCommandCompletion){
  const source=current();if(!source||!source.canSettle||!source.contextKey||request.input.contextKey!==source.contextKey||request.input.expectedRunVersion!==source.runVersion)throw Error('Review the current Payroll liability before submitting.');
  const body=JSON.stringify(request.input);if(attempt&&(attempt.body!==body||attempt.request.action!==request.action||!same(source,attempt.source)||payrollCommandContext()!==attempt.work))throw Error('Confirm the original Payroll request first. Your newer draft is retained.');
  attempt??={source:{...source},work:payrollCommandContext(),request:JSON.parse(JSON.stringify(request)) as PayrollOtherRequest,body,key:crypto.randomUUID(),uncertain:false,applied:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);
 },retry(){if(!attempt)throw Error('There is no original Payroll request to retry.');return run(attempt);},pending:()=>attempt!==null,applied:()=>attempt?.applied??false};
}
