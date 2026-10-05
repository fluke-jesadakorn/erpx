import 'server-only';
import {z} from 'zod';
import {samePerson} from '@/lib/iam';
import {businessContractSchema,businessBillingSchema,contractBillingCalculation} from '@/lib/finance-contract-policy';
import {decimalToMinor,minorToDecimal} from '@/lib/finance-money';
import type {WorkContext} from './authorization';
import type {SqlDatabase,SqlStatement} from './runtime/types';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {requireFinanceTool} from './finance-company-tools';
import {assertion,executeCommand,replayCommand} from './commands';
import {financeTransaction} from './finance-transactions';
import {ApiError} from './http';
import {sha256} from './crypto';
import {billingRevisionSchema,businessContractConfiguration,prepareBusinessBilling} from './finance-business-contracts';
import {prepareJournal,postInvoice} from './finance';
import {prepareOwnedJournalReversal} from './finance-owned-reversal';
import {snapshotMoneyReversal} from './company-money';
import {refundNotifications} from './refund-notifications';

type Row=Record<string,string|number|null>;
export const billingCorrectionSchema=billingRevisionSchema.extend({
 invoiceExpectedVersion:z.number().int().positive(),number:z.string().trim().min(1).max(100),
 periodId:z.string().min(1),entryDate:z.string().date(),
}).strict();
export const billingCorrectionDecisionSchema=z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(10).max(2000)}).strict();
type Input=z.infer<typeof billingCorrectionSchema>;
function companyActor(actor:WorkContext,permission:'finance.write'|'finance.approve'){
 actor=requirePermission(actor,permission);
 if(actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','A posted billing correction requires Company Finance authority.');return actor;
}
async function source(db:SqlDatabase,actor:WorkContext,billId:string,input:Input){
 const bill=await db.prepare('SELECT * FROM finance_business_billing_runs WHERE id=? AND company_id=?').bind(billId,actor.companyId).first<Row>();
 if(!bill)throw new ApiError(404,'BILLING_RUN_NOT_FOUND','Billing run not found in this Company.');
 if(bill.version!==input.expectedVersion||!bill.invoice_id||await db.prepare('SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?').bind(bill.id).first())throw new ApiError(409,'BILLING_REVISION_SOURCE_CHANGED','Correct the current posted billing edition.');
 const contract=await db.prepare('SELECT * FROM finance_business_contracts WHERE id=? AND company_id=?').bind(bill.contract_id,actor.companyId).first<Row>();
 if(!contract||!['active','cancelled','superseded'].includes(String(contract.status)))throw new ApiError(409,'CONTRACT_CHANGED','Retained approved contract evidence is required.');
 const content=businessContractSchema.parse({...JSON.parse(String(bill.contract_content_json??contract.content_json)),...(input.pricing?{pricing:input.pricing}:{}),...(input.obligations?{obligations:input.obligations}:{})});
 const config=await businessContractConfiguration(db,actor,content);
 const invoice=await db.prepare("SELECT i.*,j.chart_id,j.entry_date posted_date,j.status journal_status,j.version journal_version FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id AND j.company_id=i.company_id WHERE i.id=? AND i.company_id=?").bind(bill.invoice_id,actor.companyId).first<Row>();
 const oldResult=JSON.parse(String(bill.result_json));
 if(!invoice||!['posted','partially_paid','paid'].includes(String(invoice.status))||invoice.journal_status!=='posted'||invoice.version!==input.invoiceExpectedVersion||invoice.kind!=='receivable'||invoice.currency!==content.currency||invoice.chart_id!==content.chartId||invoice.debit_account_id!==content.receivableAccountId||invoice.credit_account_id!==content.deferredAccountId||Number(invoice.line_revision)!==1||Number(invoice.subtotal_minor)!==decimalToMinor(oldResult.subtotal)||Number(invoice.tax_minor)!==decimalToMinor(oldResult.tax)||Number(invoice.base_total_minor)!==Number(invoice.total_minor)||Number(invoice.base_outstanding_minor)!==Number(invoice.outstanding_minor)||Number(invoice.fx_rate_micros)!==1000000)throw new ApiError(409,'BILLING_POSTED_SOURCE_CHANGED','Refresh the original posted functional-currency invoice and its reconciled control balance.');
 if(input.entryDate<String(invoice.posted_date)||input.billing.invoiceDate!==input.entryDate||input.billing.dueDate<input.entryDate)throw new ApiError(422,'BILLING_CORRECTION_DATE_INVALID','Date the credit and replacement together, on or after the original accounting date.');
 if(!await db.prepare("SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?").bind(input.periodId,actor.companyId,input.entryDate,input.entryDate).first())throw new ApiError(409,'PERIOD_NOT_OPEN','Choose an open period for the correction date.');
 if(await db.prepare("SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status IN ('requested','approved','posted')").bind(invoice.id).first())throw new ApiError(409,'BILLING_EXTERNAL_CREDIT_EXISTS','Resolve other source credits before correcting business billing.');
 const runs=(await db.prepare('SELECT * FROM finance_recognition_runs WHERE billing_run_id=? ORDER BY id').bind(bill.id).all<Row>()).results;
 if(runs.some(r=>r.status==='draft'||r.status==='posted'&&String(r.entry_date)>input.entryDate))throw new ApiError(409,'BILLING_RECOGNITION_REVIEW_REQUIRED','Resolve draft recognition and date the correction after all posted recognition.');
 const journals=(await db.prepare('SELECT j.*,l.id line_id,l.account_id,l.description,l.debit_minor,l.credit_minor,l.base_debit_minor,l.base_credit_minor,l.dimension_json,l.ordinal FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.company_id=? AND (j.id=? OR j.id IN (SELECT journal_id FROM finance_recognition_runs WHERE billing_run_id=? AND status=\'posted\')) ORDER BY j.id,l.ordinal').bind(actor.companyId,invoice.journal_id,bill.id).all<Row>()).results;
 const allocations=(await db.prepare('SELECT x.*,p.status payment_status,p.version payment_version FROM finance_allocations x JOIN finance_payments p ON p.id=x.payment_id WHERE x.invoice_id=? ORDER BY x.id').bind(invoice.id).all<Row>()).results;
 let result:ReturnType<typeof contractBillingCalculation>;
 try{result=contractBillingCalculation(content,businessBillingSchema.parse({...input.billing,expectedVersion:contract.version}));}catch(e){throw new ApiError(422,'BILLING_INPUT_INVALID',e instanceof Error?e.message:'Invalid corrected billing.');}
 if(result.billingKey!==bill.billing_key||result.serviceFrom!==bill.service_from||result.serviceTo!==bill.service_to)throw new ApiError(422,'BILLING_REVISION_WINDOW_CHANGED','Retain the exact original service window.');
 const evidence={bill,contract,invoice,runs,journals,allocations,config,content,result};
 return {...evidence,sourceHash:await sha256(JSON.stringify(evidence))};
}
export async function prepareBillingCorrection(actor:WorkContext,billId:string,raw:z.input<typeof billingCorrectionSchema>,key:string){
 actor=companyActor(actor,'finance.write');const input=billingCorrectionSchema.parse(raw),{write}=await holdingResources(actor.holdingId),id=`billing-correction-${key}`,op='finance.billing-correction.prepare',request={billId,...input};
 return financeTransaction(write,async db=>{
  const prior=await replayCommand(db,actor,key,op,id,request);if(prior)return prior;await requireFinanceTool(db,actor.companyId,'business_contracts');
  const s=await source(db,actor,billId,input);
  if(await db.prepare("SELECT 1 FROM finance_billing_corrections WHERE billing_run_id=? AND status IN ('draft','posted')").bind(billId).first())throw new ApiError(409,'BILLING_CORRECTION_EXISTS','Review the retained correction for this bill.');
  const now=Date.now();return executeCommand(db,actor,key,op,request,id,1,{id,status:'draft',version:1,result:s.result,creditTotal:minorToDecimal(Number(s.invoice.total_minor)),sourceHash:s.sourceHash},[
   db.prepare('INSERT INTO finance_billing_corrections(id,company_id,billing_run_id,invoice_id,number,entry_date,period_id,input_json,result_json,source_hash,source_json,subtotal_minor,tax_minor,total_minor,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(id,actor.companyId,billId,s.invoice.id,input.number,input.entryDate,input.periodId,JSON.stringify(input),JSON.stringify(s.result),s.sourceHash,JSON.stringify(s),s.invoice.subtotal_minor,s.invoice.tax_minor,s.invoice.total_minor,actor.userId,input.reason,now,now),
  ]);
 });
}
export async function decideBillingCorrection(actor:WorkContext,id:string,raw:z.input<typeof billingCorrectionDecisionSchema>,action:'post'|'reject',key:string){
 actor=companyActor(actor,'finance.approve');const input=billingCorrectionDecisionSchema.parse(raw),{write}=await holdingResources(actor.holdingId),op=`finance.billing-correction.${action}`;
 return financeTransaction(write,async db=>{
  const prior=await replayCommand(db,actor,key,op,id,input);if(prior)return prior;
  const row=await db.prepare('SELECT * FROM finance_billing_corrections WHERE id=? AND company_id=?').bind(id,actor.companyId).first<Row>();
  if(!row)throw new ApiError(404,'BILLING_CORRECTION_NOT_FOUND','Billing correction not found in this Company.');
  if(row.status!=='draft'||row.version!==input.expectedVersion)throw new ApiError(409,'BILLING_CORRECTION_CHANGED','Refresh the prepared correction before deciding.');
  if(samePerson(actor,row.prepared_by))throw new ApiError(403,'INDEPENDENT_APPROVER_REQUIRED','Another Finance person must review the credit, allocation and refund evidence.');
  const steps:SqlStatement[]=[assertion(db,key,'billing-correction-version',"EXISTS(SELECT 1 FROM finance_billing_corrections WHERE id=? AND company_id=? AND status='draft' AND version=?)",[id,actor.companyId,input.expectedVersion])],now=Date.now(),version=input.expectedVersion+1;
  if(action==='reject')return executeCommand(db,actor,key,op,input,id,version,{id,status:'rejected',version},[...steps,db.prepare("UPDATE finance_billing_corrections SET status='rejected',version=?,reviewed_by=?,review_reason=?,updated_at=? WHERE id=?").bind(version,actor.userId,input.reason,now,id)]);
  await requireFinanceTool(db,actor.companyId,'business_contracts');
  const request=billingCorrectionSchema.parse(JSON.parse(String(row.input_json))),s=await source(db,actor,String(row.billing_run_id),request);
  if(s.sourceHash!==row.source_hash)throw new ApiError(409,'BILLING_CORRECTION_SOURCE_CHANGED','Payments, recognition, terms or account evidence changed. Reject this correction and prepare current evidence.');
  const credit=await prepareJournal(db,actor,{chartId:s.content.chartId,periodId:request.periodId,entryDate:request.entryDate,currency:s.content.currency,memo:`Credit ${row.number} for retained invoice ${s.invoice.invoice_number}`,sourceType:'business_billing_credit',sourceId:id,isOpening:false,lines:s.journals.filter(l=>l.id===s.invoice.journal_id).map(l=>({accountId:String(l.account_id),description:String(l.description),debit:minorToDecimal(Number(l.credit_minor)),credit:minorToDecimal(Number(l.debit_minor)),dimensions:JSON.parse(String(l.dimension_json))}))},`${key}:credit`,'posted');
  steps.push(...credit.steps);
  for(const run of s.runs.filter(r=>r.status==='posted')){
   const reversed=await prepareOwnedJournalReversal(db,actor,String(run.journal_id),'business_recognition',String(run.id),request,`${key}:recognition:${run.id}`);
   steps.push(...reversed.steps,db.prepare("UPDATE finance_recognition_runs SET status='reversed',version=version+1,reversal_journal_id=?,reason=?,updated_at=? WHERE id=? AND version=? AND status='posted'").bind(reversed.id,input.reason,now,run.id,run.version));
  }
  const credited=Number(s.invoice.outstanding_minor),refund=Number(s.invoice.total_minor)-credited,refundId=refund?`refund-${id}`:null,replacementKey=`${key}:replacement`,replacementBillId=`business-bill-${replacementKey}`;
  const money=await snapshotMoneyReversal(db,actor,{sourceType:'finance_billing_correction',sourceId:id,amountKind:'credit_total',transactionDate:request.entryDate,amountUnits:String(s.invoice.total_minor),original:{sourceType:'finance_invoice',sourceId:String(s.invoice.id),amountKind:'total'}});
  steps.push(money.statement,assertion(db,key,'billing-invoice-version',"EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND status IN ('posted','partially_paid','paid'))",[s.invoice.id,actor.companyId,s.invoice.version]),db.prepare("UPDATE finance_invoices SET outstanding_minor=0,base_outstanding_minor=0,status='paid',version=version+1,updated_at=? WHERE id=?").bind(now,s.invoice.id),db.prepare("UPDATE finance_billing_corrections SET status='posted',version=?,reviewed_by=?,review_reason=?,credit_journal_id=?,replacement_bill_id=?,credited_minor=?,refund_minor=?,updated_at=? WHERE id=?").bind(version,actor.userId,input.reason,credit.result.id,replacementBillId,credited,refund,now,id));
  if(refundId)steps.push(db.prepare("INSERT INTO finance_refund_obligations(id,company_id,billing_correction_id,invoice_id,party_id,direction,currency,chart_id,balance_account_id,amount_minor,outstanding_minor,base_amount_minor,base_outstanding_minor,source_credit_journal_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'outgoing',?,?,?,?,?,?,?,?,?,?,?)").bind(refundId,actor.companyId,id,s.invoice.id,s.invoice.party_id,s.content.currency,s.content.chartId,s.content.receivableAccountId,refund,refund,refund,refund,credit.result.id,row.prepared_by,now,now),refundNotifications(db,key,refundId,'approval','Reviewed billing credit requires independent refund beneficiary approval',actor.userId));
  steps.push(db.prepare('INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)').bind(crypto.randomUUID(),key,id,op,actor.userId,actor.assignmentId,input.expectedVersion,await sha256(JSON.stringify(input)),now));
  const response={id,status:'posted',version,creditJournalId:credit.result.id,replacementBillId,creditedMinor:credited,refundMinor:refund,refundId};
  await executeCommand(db,actor,key,op,input,id,version,response,steps);
  const replacement=await prepareBusinessBilling(actor,String(s.bill.contract_id),{...request.billing,expectedVersion:Number(s.contract.version)},replacementKey,db,{expectedVersion:request.expectedVersion,invoiceExpectedVersion:Number(s.invoice.version)+1,billing:request.billing,pricing:request.pricing,obligations:request.obligations,reason:request.reason,parentId:String(s.bill.id)},id);
  if(replacement.invoiceId){
   await db.prepare("UPDATE finance_invoices SET created_by=? WHERE id=? AND company_id=? AND status='draft' AND version=1").bind(row.prepared_by,replacement.invoiceId,actor.companyId).run();
   await postInvoice(actor,String(replacement.invoiceId),{periodId:request.periodId,entryDate:request.entryDate,expectedVersion:1},`${key}:replacement-post`,db);
  }else{
   await db.prepare("UPDATE finance_business_billing_runs SET no_charge_status='approved',reviewed_by=?,review_reason=?,version=version+1 WHERE id=? AND company_id=? AND no_charge_status='draft'").bind(actor.userId,input.reason,replacementBillId,actor.companyId).run();
  }
  return response;
 });
}
