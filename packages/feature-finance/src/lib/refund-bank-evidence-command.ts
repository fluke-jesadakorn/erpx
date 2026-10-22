import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {financeDocumentCommandContext,financeDocumentFormUnchanged} from '@erpx/feature-finance/lib/document-post-control';
import {minorToDecimal} from '@erpx/feature-finance/lib/finance-money';

export type RefundBankEvidenceSource={base:string;companyId:string;obligationId:string;instructionId:string;instructionVersion:number;bankAccount:string;currency:string;amountMinor:number;direction:'outgoing'|'incoming';plannedDate:string;functionalCurrency?:string};
export type RefundBankEvidenceFields={statementNumber:string;statementDate:string;opening:string;closing:string;sourceHash:string;externalId:string;transactionDate:string;closingFxRate?:string};
export type RefundBankEvidenceReceipt={id:string;imported:1;status:'imported';refundInstructionId:string;refundObligationId:string;bankTransactionId:string};
export function refundBankEvidenceInput(source:RefundBankEvidenceSource,fields:RefundBankEvidenceFields){
 if(!Number.isSafeInteger(source.instructionVersion)||source.instructionVersion<1||!Number.isSafeInteger(source.amountMinor)||source.amountMinor<=0)throw Error('Review the current original refund instruction.');
 return {expectedVersion:source.instructionVersion,bankAccount:source.bankAccount,statementNumber:fields.statementNumber,statementDate:fields.statementDate,opening:fields.opening,closing:fields.closing,...(fields.closingFxRate?.trim()?{closingFxRate:fields.closingFxRate.trim()}:{}),sourceHash:fields.sourceHash,rows:[{externalId:fields.externalId,transactionDate:fields.transactionDate,description:`Refund instruction ${source.instructionId}`,amount:minorToDecimal(source.direction==='outgoing'?-source.amountMinor:source.amountMinor),currency:source.currency}]};
}
export function refundBankEvidenceAcknowledged(value:unknown,key:string,source:RefundBankEvidenceSource):value is RefundBankEvidenceReceipt{
 return !!value&&typeof value==='object'&&!Array.isArray(value)&&'id' in value&&value.id===`statement-${key}`&&'imported' in value&&value.imported===1&&'status' in value&&value.status==='imported'&&'refundInstructionId' in value&&value.refundInstructionId===source.instructionId&&'refundObligationId' in value&&value.refundObligationId===source.obligationId&&'bankTransactionId' in value&&value.bankTransactionId===`bankrow-${key}-0`;
}
const owner=(source:RefundBankEvidenceSource)=>JSON.stringify([source.base,source.companyId,source.obligationId,source.instructionId]);
const witness=(source:RefundBankEvidenceSource)=>JSON.stringify([owner(source),source.instructionVersion,source.bankAccount,source.currency,source.amountMinor,source.direction,source.plannedDate]);
type Attempt={source:RefundBankEvidenceSource;body:string;key:string;context:string;uncertain:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
/** The original single-row owner receipt must finish decoding under the live
 * Work and instruction context before any local saved acknowledgement. */
export function createRefundBankEvidenceCommands(current:()=>RefundBankEvidenceSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original bank-evidence request.');
  const source=current();if(!source||owner(source)!==owner(captured.source)||financeDocumentCommandContext()!==captured.context)throw Error('Return to the original Company, refund instruction and work role to confirm this evidence.');
  const before={...source},wasUncertain=captured.uncertain;let sent=false;busy=true;
  try{
   const assertWork=captureWorkCommandContext(),assertCurrent=()=>{assertWork();const live=current();if(!live||witness(live)!==witness(before)||financeDocumentCommandContext()!==captured.context)throw Object.assign(Error('Your refund context changed. The original evidence, body and key are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
   sent=true;const response=await send(`/api/v1${captured.source.base}/finance/refund-instructions/${encodeURIComponent(captured.source.instructionId)}/bank-evidence`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   if(!response.ok){if(!wasUncertain&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED'){assertCurrent();attempt=null;}throw Object.assign(Error(body.error?.message??'Bank evidence was not confirmed. Retry the original request.'),{code:body.error?.code});}
   if(!refundBankEvidenceAcknowledged(body.data,captured.key,captured.source))throw Error('Bank evidence was not acknowledged for this exact refund instruction. Confirm its original request.');
   assertCurrent();captured.uncertain=true;const receipt=body.data;
   return {receipt,unchanged:()=>{const completion=captured.completion();return captured.hadCompletion?!!completion&&financeDocumentFormUnchanged(completion):true;},acknowledge(){assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();if(typeof window!=='undefined'){if(completion){window.dispatchEvent(new CustomEvent('erpx:finance-command-acknowledged',{detail:completion}));window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,financeAcknowledged:true}}));}announceWorkChange();}return true;}};
  }catch(error){if(attempt===captured){if(!sent&&!wasUncertain)attempt=null;else captured.uncertain=true;}throw error;}finally{busy=false;}
 }
 return {submit(fields:RefundBankEvidenceFields,completion?:WorkCommandCompletion){const source=current();if(!source)throw Error('Return to the original refund instruction.');const body=JSON.stringify(refundBankEvidenceInput(source,fields)),context=financeDocumentCommandContext();if(attempt&&(body!==attempt.body||owner(source)!==owner(attempt.source)||context!==attempt.context))throw Error('Confirm the original bank evidence first. Your newer entries are retained.');attempt??={source:{...source},body,key:crypto.randomUUID(),context,uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);},retry(){if(!attempt)throw Error('There is no original bank evidence to confirm.');return run(attempt);},pending:()=>attempt!==null};
}
