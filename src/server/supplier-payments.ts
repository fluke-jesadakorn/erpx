import 'server-only';
import { z } from 'zod';
import { createHash } from 'node:crypto';
import type { WorkContext } from './authorization';
import type { SqlDatabase, SqlStatement } from './runtime/types';
import type { WorkItem } from '@/lib/work-item';
import { holdingResources } from './runtime';
import { requirePermission } from './iam-permissions';
import { hasPermission } from '@/lib/permissions';
import { samePerson } from '@/lib/iam';
import { ApiError } from './http';
import { assertion, executeCommand, replayCommand } from './commands';
import { prepareJournal } from './finance';
import { decimalToMinor, minorToDecimal } from '@/lib/finance-money';
import { prepareAttachmentBinding } from './attachments/service';
import { assertControlledThaiTaxPaymentOwner } from './finance-withholding';

type Row = Record<string, string | number | null>;
const version = z.number().int().positive();
const reason = z.string().trim().min(3).max(1000);
export const supplierBatchSchema = z.object({
 bankAccountId: z.string().min(1), plannedDate: z.iso.date(),
 items: z.array(z.object({invoiceId:z.string().min(1),expectedVersion:version,amount:z.string().regex(/^\d+(\.\d{1,2})?$/),beneficiaryName:z.string().trim().min(2).max(160),beneficiaryAccount:z.string().trim().min(3).max(160)})).min(1).max(50),
});
export const supplierDecisionSchema = z.object({expectedVersion:version,reason});
export const supplierExportSchema = z.object({expectedVersion:version});
export const supplierResultSchema = z.object({expectedVersion:version,outcome:z.enum(['unknown','failed']),evidence:reason});
export const supplierConfirmSchema = z.object({expectedVersion:version,bankTransactionId:z.string().min(1),bankVersion:version,periodId:z.string().min(1),bankReference:z.string().trim().min(2).max(160),evidence:reason,attachmentSetIds:z.array(z.string().min(1)).max(20).default([])});
function company(actor:WorkContext) {
 requirePermission(actor,'finance.read');
 if(actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_TREASURY_REQUIRED','Use a Company finance assignment for supplier bank batches.');
}
function payer(actor:WorkContext,batch:Row) {
 company(actor);requirePermission(actor,'finance.pay');
 if(samePerson(actor,batch.created_by)||samePerson(actor,batch.approved_by))throw new ApiError(403,'INDEPENDENT_PAYER_REQUIRED','A third person must export and confirm this approved batch.');
}
async function batchRow(db:SqlDatabase,actor:WorkContext,id:string) {
 company(actor);const row=await db.prepare('SELECT * FROM finance_supplier_batches WHERE id=? AND company_id=?').bind(id,actor.companyId).first<Row>();
 if(!row)throw new ApiError(404,'BATCH_NOT_FOUND','Supplier batch not found.');return row;
}
async function itemRow(db:SqlDatabase,actor:WorkContext,id:string) {
 company(actor);const item=await db.prepare('SELECT i.* FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE i.id=? AND b.company_id=?').bind(id,actor.companyId).first<Row>();
 if(!item)throw new ApiError(404,'INSTRUCTION_NOT_FOUND','Supplier instruction not found.');
 return {item,batch:await batchRow(db,actor,String(item.batch_id))};
}
const assertBatch=(db:SqlDatabase,key:string,actor:WorkContext,row:Row,expected:number)=>assertion(db,key,'batch',"EXISTS(SELECT 1 FROM finance_supplier_batches WHERE id=? AND company_id=? AND version=? AND status=?)",[row.id,actor.companyId,expected,row.status]);
function notifications(db:SqlDatabase,key:string,id:string,message:string,actor:string) {
 return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
 SELECT DISTINCT ?||'-'||a.user_id,b.company_id,a.user_id,'supplier_batch',b.id,b.version,?,floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM finance_supplier_batches b JOIN assignments a ON a.company_id=b.company_id WHERE b.id=? AND a.user_id<>? AND a.scope_kind='holding'
 AND a.role IN ('holding_owner','finance_approver','finance_preparer','treasury') AND a.revoked_at IS NULL AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ON CONFLICT DO NOTHING`).bind(key,message,id,actor);
}
export async function getSupplierBatch(actor:WorkContext,id:string) {const {write}=await holdingResources(actor.holdingId);return batchRow(write,actor,id);}
export async function listSupplierBatches(actor:WorkContext,forWork=false) {
 company(actor);const {write}=await holdingResources(actor.holdingId);
 return (await write.prepare(`SELECT b.*,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('reserved','exported','unknown')) pending_count,(SELECT count(*) FROM finance_supplier_instructions i JOIN finance_invoices f ON f.id=i.invoice_id WHERE i.batch_id=b.id AND i.status='failed' AND f.outstanding_minor>0) failed_count,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown') unknown_count FROM finance_supplier_batches b WHERE b.company_id=? ORDER BY b.updated_at DESC${forWork?'':' LIMIT 200'}`).bind(actor.companyId).all<Row>()).results;
}
export async function supplierBatchWorkspace(actor:WorkContext,id?:string) {
 company(actor);const {write}=await holdingResources(actor.holdingId),batch=id?await batchRow(write,actor,id):null;
 const [items,invoices,accounts,periods,bank]=await Promise.all([
  id?write.prepare('SELECT s.*,i.invoice_number,i.currency FROM finance_supplier_instructions s JOIN finance_invoices i ON i.id=s.invoice_id WHERE s.batch_id=? ORDER BY s.id').bind(id).all<Row>():Promise.resolve({results:[] as Row[]}),
  !id?write.prepare(`SELECT i.*,p.name party_name,i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) FROM finance_supplier_instructions s WHERE s.invoice_id=i.id AND s.status IN ('reserved','exported','unknown')),0) available_minor FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? AND i.kind='payable' AND i.source_type IN ('standalone','commerce_fulfillment') AND i.status IN ('posted','partially_paid') ORDER BY i.due_date,i.id LIMIT 200`).bind(actor.companyId).all<Row>():Promise.resolve({results:[] as Row[]}),
  write.prepare("SELECT a.id,a.code,a.name,a.currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.category='asset' AND a.status='active' AND a.id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=a.company_id) AND c.status='active' AND a.currency=c.base_currency ORDER BY a.code").bind(actor.companyId).all<Row>(),
  write.prepare("SELECT id,name FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date").bind(actor.companyId).all<Row>(),
  batch&&hasPermission(actor,'finance.pay')?write.prepare("SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND s.bank_account=? AND t.match_status IN ('unmatched','proposed') ORDER BY t.transaction_date DESC LIMIT 200").bind(actor.companyId,batch.bank_account_id).all<Row>():Promise.resolve({results:[] as Row[]}),
 ]);
 return {batch,items:items.results,invoices:invoices.results,accounts:accounts.results,periods:periods.results,bank:bank.results,userId:actor.userId,canCreate:hasPermission(actor,'finance.write'),canApprove:hasPermission(actor,'finance.approve'),canPay:hasPermission(actor,'finance.pay')};
}
export async function createSupplierBatch(actor:WorkContext,input:z.infer<typeof supplierBatchSchema>,key:string) {
 input=supplierBatchSchema.parse(input);company(actor);requirePermission(actor,'finance.write');const {write}=await holdingResources(actor.holdingId),id=`supplier-batch-${key}`;
 const prior=await replayCommand(write,actor,key,'finance.supplier_batch.create',id,input);if(prior)return prior;
 await assertControlledThaiTaxPaymentOwner(write,actor,input.plannedDate);
 if(new Set(input.items.map(i=>i.invoiceId)).size!==input.items.length)throw new ApiError(422,'DUPLICATE_INVOICE','Select each invoice once.');
 const bank=await write.prepare("SELECT a.*,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.category='asset' AND a.status='active' AND a.id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=a.company_id) AND c.status='active'").bind(input.bankAccountId,actor.companyId).first<Row>();
 if(!bank||bank.currency!==bank.base_currency)throw new ApiError(422,'BANK_ACCOUNT_INVALID','Select an active bank ledger in the functional currency.');
 const now=Date.now(),steps:SqlStatement[]=[assertion(write,key,'open-period',"EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[actor.companyId,input.plannedDate,input.plannedDate]),write.prepare('INSERT INTO finance_supplier_batches(id,company_id,currency,bank_account_id,planned_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)').bind(id,actor.companyId,bank.currency,bank.id,input.plannedDate,actor.userId,now,now)];
 for(const [index,item] of input.items.entries()) {
  const amount=decimalToMinor(item.amount);if(amount<=0)throw new ApiError(422,'INVALID_AMOUNT','Use a positive payment amount.');
  // Touch the invoice version to serialize all reservations and source mutations.
  steps.push(assertion(write,key,`invoice-${index}`,`EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id JOIN finance_parties p ON p.id=i.party_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.kind='payable' AND i.source_type IN ('standalone','commerce_fulfillment') AND i.status IN ('posted','partially_paid') AND j.status='posted' AND j.chart_id=? AND i.currency=? AND p.status='active' AND p.kind='supplier' AND i.invoice_date<=? AND i.credit_account_id<>? AND NOT EXISTS(SELECT 1 FROM commerce_returns r WHERE r.invoice_id=i.id AND r.status IN ('requested','approved')) AND NOT EXISTS(SELECT 1 FROM hr_payment_batches h WHERE h.company_id=i.company_id AND h.source_type='contractor' AND h.source_id=i.id AND h.status<>'cancelled') AND i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) FROM finance_supplier_instructions s WHERE s.invoice_id=i.id AND s.status IN ('reserved','exported','unknown')),0)>=?)`,[item.invoiceId,actor.companyId,item.expectedVersion,bank.chart_id,bank.currency,input.plannedDate,bank.id,amount]),
   write.prepare('INSERT INTO finance_supplier_instructions(id,batch_id,invoice_id,invoice_version,amount_minor,beneficiary_name,beneficiary_account,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)').bind(`${id}-${index}`,id,item.invoiceId,item.expectedVersion+1,amount,item.beneficiaryName,item.beneficiaryAccount,now,now),
   write.prepare('UPDATE finance_invoices SET version=version+1,updated_at=? WHERE id=?').bind(now,item.invoiceId));
 }
 steps.push(notifications(write,key,id,'Supplier batch prepared: independent review required',actor.userId));
 return executeCommand(write,actor,key,'finance.supplier_batch.create',input,id,1,{id,status:'draft',version:1},steps);
}
export async function decideSupplierBatch(actor:WorkContext,id:string,input:z.infer<typeof supplierDecisionSchema>,key:string,decision:'approve'|'cancel') {
 input=supplierDecisionSchema.parse(input);company(actor);requirePermission(actor,decision==='approve'?'finance.approve':'finance.write');const {write}=await holdingResources(actor.holdingId),operation=`finance.supplier_batch.${decision}`;
 const prior=await replayCommand(write,actor,key,operation,id,input);if(prior)return prior;
 const batch=await batchRow(write,actor,id);
 if(decision==='approve'&&samePerson(actor,batch.created_by))throw new ApiError(403,'INDEPENDENT_APPROVER_REQUIRED','A different person must review the source, amount and beneficiary.');
 if(decision==='cancel'&&!samePerson(actor,batch.created_by)&&!hasPermission(actor,'finance.approve'))throw new ApiError(403,'CANCEL_FORBIDDEN','The preparer or Finance approver must cancel this batch.');
 if(batch.status!=='draft'&&(decision==='approve'||batch.status!=='approved'))throw new ApiError(409,'BATCH_NOT_EDITABLE','Exported batches require individual bank-result review.');
 const now=Date.now(),steps=[assertBatch(write,key,actor,batch,input.expectedVersion)];
 if(decision==='approve')steps.push(write.prepare("UPDATE finance_supplier_batches SET status='approved',approved_by=?,approval_reason=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,input.reason,now,id));
 else steps.push(write.prepare("UPDATE finance_supplier_batches SET status='cancelled',version=version+1,updated_at=? WHERE id=?").bind(now,id),write.prepare("UPDATE finance_supplier_instructions SET status='cancelled',evidence=?,resolved_by=?,version=version+1,updated_at=? WHERE batch_id=? AND status='reserved'").bind(input.reason,actor.userId,now,id));
 steps.push(notifications(write,key,id,decision==='approve'?'Supplier batch approved: independent Treasury export required':'Supplier batch cancelled: review source invoices before preparing again',actor.userId));
 return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status:decision==='approve'?'approved':'cancelled',version:input.expectedVersion+1},steps);
}
const csvCell=(value:unknown)=>`"${String(value??'').replace(/^[=+@\-\t\r]/,"'$&").replaceAll('"','""')}"`;
export async function exportSupplierBatch(actor:WorkContext,id:string,input:z.infer<typeof supplierExportSchema>,key:string) {
 input=supplierExportSchema.parse(input);company(actor);requirePermission(actor,'finance.pay');const {write}=await holdingResources(actor.holdingId);
 const prior=await replayCommand(write,actor,key,'finance.supplier_batch.export',id,input);if(prior)return prior;
 const batch=await batchRow(write,actor,id);payer(actor,batch);
 if(batch.status!=='approved')throw new ApiError(409,'BATCH_NOT_APPROVED','Export an independently approved batch.');
 const items=(await write.prepare('SELECT s.*,i.invoice_number FROM finance_supplier_instructions s JOIN finance_invoices i ON i.id=s.invoice_id WHERE s.batch_id=? ORDER BY s.id').bind(id).all<Row>()).results;
 const content=[['instruction_id','invoice','date','currency','amount','company_bank_ledger','beneficiary','beneficiary_account'].map(csvCell).join(','),...items.map(i=>[i.id,i.invoice_number,batch.planned_date,batch.currency,minorToDecimal(Number(i.amount_minor)),batch.bank_account_id,i.beneficiary_name,i.beneficiary_account].map(csvCell).join(','))].join('\r\n');
 const sha256=createHash('sha256').update(content,'utf8').digest('hex'),now=Date.now();
 return executeCommand(write,actor,key,'finance.supplier_batch.export',input,id,input.expectedVersion+1,{id,content,sha256,rows:items.length,confirmationRequired:true},[
  assertBatch(write,key,actor,batch,input.expectedVersion),
  assertion(write,key,'bank-account',"EXISTS(SELECT 1 FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.status='active' AND a.category='asset' AND a.id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=a.company_id) AND c.status='active' AND a.currency=? AND c.base_currency=?)",[batch.bank_account_id,actor.companyId,batch.currency,batch.currency]),
  assertion(write,key,'open-period',"EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[actor.companyId,batch.planned_date,batch.planned_date]),
  assertion(write,key,'source-balances',`NOT EXISTS(SELECT 1 FROM finance_supplier_instructions s JOIN finance_invoices i ON i.id=s.invoice_id JOIN finance_parties p ON p.id=i.party_id JOIN finance_journals j ON j.id=i.journal_id WHERE s.batch_id=? AND (s.status<>'reserved' OR i.status NOT IN ('posted','partially_paid') OR p.status<>'active' OR j.status<>'posted' OR i.outstanding_minor<(SELECT SUM(r.amount_minor) FROM finance_supplier_instructions r WHERE r.invoice_id=i.id AND r.status IN ('reserved','exported','unknown')) OR EXISTS(SELECT 1 FROM commerce_returns cr WHERE cr.invoice_id=i.id AND cr.status IN ('requested','approved'))))`,[id]),
  write.prepare("UPDATE finance_supplier_batches SET status='exported',exported_by=?,export_command_id=?,export_hash=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,key,sha256,now,id),
  write.prepare("UPDATE finance_supplier_instructions SET status='exported',version=version+1,updated_at=? WHERE batch_id=? AND status='reserved'").bind(now,id),
  notifications(write,key,id,'Supplier batch exported: confirm each bank result before settlement',actor.userId),
 ]);
}
export async function recoverSupplierExport(actor:WorkContext,id:string) {
 const {write}=await holdingResources(actor.holdingId),batch=await batchRow(write,actor,id);payer(actor,batch);
 const receipt=await write.prepare("SELECT result_json FROM command_receipts WHERE command_id=? AND operation='finance.supplier_batch.export'").bind(batch.export_command_id).first<{result_json:string}>();
 if(!receipt)throw new ApiError(404,'EXPORT_NOT_FOUND','No exported file exists.');return JSON.parse(receipt.result_json) as {content:string;sha256:string};
}
export async function recordSupplierResult(actor:WorkContext,id:string,input:z.infer<typeof supplierResultSchema>,key:string) {
 input=supplierResultSchema.parse(input);company(actor);const {write}=await holdingResources(actor.holdingId);
 const prior=await replayCommand(write,actor,key,'finance.supplier_batch.result',id,input);if(prior)return prior;
 const {item,batch}=await itemRow(write,actor,id);
 if(input.outcome==='unknown')payer(actor,batch);
 else {requirePermission(actor,'finance.approve');if(samePerson(actor,batch.exported_by))throw new ApiError(403,'INDEPENDENT_REVIEW_REQUIRED','Another Finance approver must verify that the bank did not pay.');}
 if(!['exported','unknown'].includes(String(item.status)))throw new ApiError(409,'INSTRUCTION_NOT_PENDING','Only an unresolved exported instruction can receive a bank result.');
 const now=Date.now();return executeCommand(write,actor,key,'finance.supplier_batch.result',input,id,input.expectedVersion+1,{id,status:input.outcome,version:input.expectedVersion+1},[
  assertion(write,key,'item',"EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE id=? AND version=? AND status IN ('exported','unknown'))",[id,input.expectedVersion]),
  write.prepare('UPDATE finance_supplier_instructions SET status=?,evidence=?,resolved_by=?,version=version+1,updated_at=? WHERE id=?').bind(input.outcome,input.evidence,actor.userId,now,id),
  write.prepare('UPDATE finance_invoices SET version=version+1,updated_at=? WHERE id=?').bind(now,item.invoice_id),
  write.prepare('UPDATE finance_supplier_batches SET version=version+1,updated_at=? WHERE id=?').bind(now,batch.id),
  notifications(write,key,String(batch.id),'Supplier bank result updated: review remaining instructions',actor.userId),
 ]);
}
export async function confirmSupplierInstruction(actor:WorkContext,id:string,input:z.infer<typeof supplierConfirmSchema>,key:string) {
 input=supplierConfirmSchema.parse(input);company(actor);requirePermission(actor,'finance.pay');const {write}=await holdingResources(actor.holdingId);
 const prior=await replayCommand(write,actor,key,'finance.supplier_batch.confirm',id,input);if(prior)return prior;
 const {item,batch}=await itemRow(write,actor,id);payer(actor,batch);
 if(!['exported','unknown'].includes(String(item.status)))throw new ApiError(409,'INSTRUCTION_NOT_PENDING','Confirm an exported instruction.');
 const invoice=await write.prepare('SELECT i.*,j.chart_id FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=?').bind(item.invoice_id,actor.companyId).first<Row>();
 if(!invoice)throw new ApiError(404,'INVOICE_NOT_FOUND','Source invoice is unavailable.');
 const bank=await write.prepare('SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?').bind(input.bankTransactionId,actor.companyId).first<Row>(),amount=Number(item.amount_minor);
 if(!bank||bank.currency!==batch.currency||bank.bank_account!==batch.bank_account_id||Number(bank.amount_minor)!==-amount||String(bank.transaction_date)<String(batch.planned_date))throw new ApiError(422,'BANK_EVIDENCE_MISMATCH','Match the approved bank account, currency, date and exact outgoing amount.');
 await assertControlledThaiTaxPaymentOwner(write,actor,String(bank.transaction_date));
 let attachmentBinding:null|Awaited<ReturnType<typeof prepareAttachmentBinding>>=null;
 if(input.attachmentSetIds.length){const placeholders=input.attachmentSetIds.map(()=>'?').join(','),sets=(await write.prepare(`SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${placeholders})`).bind(actor.companyId,...input.attachmentSetIds).all<{id:string;purpose_key:string;component_kind:string;component_ref:string|null}>()).results;
  if(sets.length!==new Set(input.attachmentSetIds).size||sets.some(set=>set.purpose_key!=='payment_proof'||set.component_kind!=='action'||set.component_ref!==`supplier-instruction:${id}`))throw new ApiError(422,'SUPPLIER_ATTACHMENT_PURPOSE','Choose only payment proof prepared for this supplier instruction.');
  attachmentBinding=await prepareAttachmentBinding(write,actor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'finance_supplier_batch',bindingType:'finance_supplier_payment_confirmation',contexts:[{purposeKey:'payment_proof',componentKind:'action',componentRef:`supplier-instruction:${id}`}]},targetId:String(batch.id),expectedTargetVersion:Number(batch.version),commandId:key,bindingId:id});
 }
 const journal=await prepareJournal(write,actor,{chartId:String(invoice.chart_id),periodId:input.periodId,entryDate:String(bank.transaction_date),currency:String(batch.currency),memo:`Supplier payment ${invoice.invoice_number}: ${input.bankReference}`,sourceType:'supplier_batch',sourceId:id,isOpening:false,lines:[{accountId:String(invoice.credit_account_id),debit:minorToDecimal(amount),credit:'0'},{accountId:String(batch.bank_account_id),debit:'0',credit:minorToDecimal(amount)}]},key,'posted');
 const paymentId=`supplier-payment-${key}`,now=Date.now();
 return executeCommand(write,actor,key,'finance.supplier_batch.confirm',input,id,input.expectedVersion+1,{id,status:'confirmed',version:input.expectedVersion+1,paymentId,journalId:journal.result.id},[
  ...(attachmentBinding?.guards??[]),
  assertion(write,key,'item',"EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE id=? AND version=? AND status IN ('exported','unknown'))",[id,input.expectedVersion]),
  assertion(write,key,'invoice',"EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND status IN ('posted','partially_paid') AND outstanding_minor>=?)",[invoice.id,actor.companyId,invoice.version,amount]),
  assertion(write,key,'bank',"EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND version=? AND match_status IN ('unmatched','proposed'))",[input.bankTransactionId,actor.companyId,input.bankVersion]),
  ...journal.steps,
  write.prepare("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,released_by,created_at,updated_at) VALUES(?,?,?,'payment',?,?,?,?,0,?,'settled',?,?,?,?,?,?)").bind(paymentId,actor.companyId,invoice.party_id,`SB-${key}`,bank.transaction_date,batch.currency,amount,batch.bank_account_id,journal.result.id,batch.created_by,batch.approved_by,actor.userId,now,now),
  write.prepare('INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)').bind(`allocation-${key}`,actor.companyId,paymentId,invoice.id,amount,actor.userId,now),
  write.prepare("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?").bind(amount,amount,now,invoice.id),
  write.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?").bind(paymentId,bank.id),
  write.prepare("UPDATE finance_supplier_instructions SET status='confirmed',bank_reference=?,evidence=?,payment_id=?,bank_transaction_id=?,resolved_by=?,version=version+1,updated_at=? WHERE id=?").bind(input.bankReference,input.evidence,paymentId,bank.id,actor.userId,now,id),
  write.prepare('UPDATE finance_supplier_batches SET version=version+1,updated_at=? WHERE id=?').bind(now,batch.id),
  ...(attachmentBinding?.steps??[]),
  notifications(write,key,String(batch.id),'Supplier payment matched: source invoice and accounting updated',actor.userId),
 ]);
}
export async function supplierBatchWork(actor:WorkContext):Promise<WorkItem[]> {
 if(!hasPermission(actor,'finance.read')||actor.branchId||actor.departmentId)return [];
 return (await listSupplierBatches(actor,true)).map(b=>{
  const needsRetry=Number(b.failed_count)>0;
  const completed=b.status==='cancelled'||b.status==='exported'&&Number(b.pending_count)===0&&!needsRetry;
  const canAct=!completed&&(needsRetry&&hasPermission(actor,'finance.write')|| (b.status==='draft'?hasPermission(actor,'finance.approve')&&!samePerson(actor,b.created_by):hasPermission(actor,'finance.pay')&&!samePerson(actor,b.created_by)&&!samePerson(actor,b.approved_by)||b.status==='exported'&&hasPermission(actor,'finance.approve')&&!samePerson(actor,b.exported_by)));
  return {id:String(b.id),type:'supplier_batch',actionKind:b.status==='draft'&&!needsRetry&&canAct?'approval':'work',title:'Supplier payment batch',detail:`${b.planned_date} · ${b.currency} · ${b.pending_count} pending · ${b.failed_count} failed with an unpaid source${Number(b.unknown_count)?' · Bank result uncertain':''}`,href:`/finance/supplier-batches/${b.id}`,status:completed?'completed':String(b.status),updatedAt:Number(b.updated_at),queue:completed?'completed':canAct?'todo':'waiting',canAct,responsibleQueue:needsRetry?'Company Finance preparer':b.status==='draft'?'Independent Finance approver':'Company Treasury / Finance review',nextAction:needsRetry?'Review failed instructions and prepare payment for the remaining source balance':b.status==='draft'?'Review invoices, amounts and beneficiary details':'Review export and each bank result',relationships:[canAct?'approval':'team']};
 });
}
