import {financeSupplierPaymentSourceFacts} from '@/server/modules/commerce/public/finance-supplier-payment-source-facts';
import {financeContractorPaymentClaimFacts} from '@/server/modules/people_payroll/public/finance-contractor-payment-claim-facts';
import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {hasPermission} from "@/lib/permissions";
import {minorToDecimal} from "@/lib/finance-money";
import {assertion,executeCommand} from "../../workflow/public/commands";
import {supplierExportReceipt} from "../../workflow/public/supplier-export-receipt";
import {prepareJournal} from "../public/finance";
import {assertControlledThaiTaxPaymentOwner} from "../public/finance-withholding";
import {prepareAttachmentBinding} from "../../files/public/attachments/service";
import {attachmentPurposeFacts} from "../../files/public/attachment-purpose-facts";
import {stageSupplierBatchNotifications} from "../../collaboration/public/supplier-batch-notifications";
import type {SupplierPaymentMutation,SupplierPaymentRow as Row,InstructionConfirmation} from "../../procurement/public/supplier-payment-contract";
export type SupplierPaymentEvidence=Awaited<ReturnType<typeof prepareAttachmentBinding>>;
export type SupplierPaymentJournal=Awaited<ReturnType<typeof prepareJournal>>;
const assertBatch=(db:SqlDatabase,key:string,actor:WorkContext,row:Row,expected:number)=>assertion(db,key,'batch',"EXISTS(SELECT 1 FROM finance_supplier_batches WHERE id=? AND company_id=? AND version=? AND status=?)",[row.id,actor.companyId,expected,row.status]);
export function supplierPaymentRepository(write:SqlDatabase,actor:WorkContext){return {
 batch:(id:string)=>write.prepare('SELECT * FROM finance_supplier_batches WHERE id=? AND company_id=?').bind(id,actor.companyId).first<Row>(),
 item:(id:string)=>write.prepare('SELECT i.* FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE i.id=? AND b.company_id=?').bind(id,actor.companyId).first<Row>(),
 async list(forWork:boolean){
 return (await write.prepare(`SELECT b.*,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('reserved','exported','unknown')) pending_count,(SELECT count(*) FROM finance_supplier_instructions i JOIN finance_invoices f ON f.id=i.invoice_id WHERE i.batch_id=b.id AND i.status='failed' AND f.outstanding_minor>0) failed_count,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown') unknown_count FROM finance_supplier_batches b WHERE b.company_id=? ORDER BY b.updated_at DESC${forWork?'':' LIMIT 200'}`).bind(actor.companyId).all<Row>()).results;
},async workspace(id:string|undefined,batch:Row|null){
 const [items,invoices,accounts,periods,bank]=await Promise.all([
  id?write.prepare('SELECT s.*,i.invoice_number,i.currency FROM finance_supplier_instructions s JOIN finance_invoices i ON i.id=s.invoice_id WHERE s.batch_id=? ORDER BY s.id').bind(id).all<Row>():Promise.resolve({results:[] as Row[]}),
  !id?write.prepare(`SELECT i.*,p.name party_name,i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) FROM finance_supplier_instructions s WHERE s.invoice_id=i.id AND s.status IN ('reserved','exported','unknown')),0) available_minor FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? AND i.kind='payable' AND i.source_type IN ('standalone','commerce_fulfillment') AND i.status IN ('posted','partially_paid') ORDER BY i.due_date,i.id LIMIT 200`).bind(actor.companyId).all<Row>():Promise.resolve({results:[] as Row[]}),
  write.prepare(`SELECT a.id,a.code,a.name,a.currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.category='asset' AND a.status='active' AND ${financeSupplierPaymentSourceFacts().configuredCashAccount} AND c.status='active' AND a.currency=c.base_currency ORDER BY a.code`).bind(actor.companyId).all<Row>(),
  write.prepare("SELECT id,name FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date").bind(actor.companyId).all<Row>(),
  batch&&hasPermission(actor,'finance.pay')?write.prepare("SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND s.bank_account=? AND t.match_status IN ('unmatched','proposed') ORDER BY t.transaction_date DESC LIMIT 200").bind(actor.companyId,batch.bank_account_id).all<Row>():Promise.resolve({results:[] as Row[]}),
 ]);
 return {items:items.results,invoices:invoices.results,accounts:accounts.results,periods:periods.results,bank:bank.results};
},controlledTaxOwner:(date:string)=>assertControlledThaiTaxPaymentOwner(write,actor,date),
 bankAccount:(id:string)=>write.prepare(`SELECT a.*,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.category='asset' AND a.status='active' AND ${financeSupplierPaymentSourceFacts().configuredCashAccount} AND c.status='active'`).bind(id,actor.companyId).first<Row>(),
 exportItems:async(id:string)=>(await write.prepare('SELECT s.*,i.invoice_number FROM finance_supplier_instructions s JOIN finance_invoices i ON i.id=s.invoice_id WHERE s.batch_id=? ORDER BY s.id').bind(id).all<Row>()).results,
 exportReceipt:(key:string)=>supplierExportReceipt(write,key),
 invoice:(id:string)=>write.prepare('SELECT i.*,j.chart_id FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=?').bind(id,actor.companyId).first<Row>(),
 bankTransaction:(id:string)=>write.prepare('SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?').bind(id,actor.companyId).first<Row>(),
 evidenceSets:(ids:string[])=>attachmentPurposeFacts(write,actor.companyId,ids),
 prepareEvidence:(id:string,batch:Row,input:InstructionConfirmation,key:string)=>prepareAttachmentBinding(write,actor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'finance_supplier_batch',bindingType:'finance_supplier_payment_confirmation',contexts:[{purposeKey:'payment_proof',componentKind:'action',componentRef:`supplier-instruction:${id}`}]},targetId:String(batch.id),expectedTargetVersion:Number(batch.version),commandId:key,bindingId:id}),
 async prepareJournal(invoice:Row,batch:Row,bank:Row,amount:number,input:InstructionConfirmation,id:string,key:string){const journal=await prepareJournal(write,actor,{chartId:String(invoice.chart_id),periodId:input.periodId,entryDate:String(bank.transaction_date),currency:String(batch.currency),memo:`Supplier payment ${invoice.invoice_number}: ${input.bankReference}`,sourceType:'supplier_batch',sourceId:id,isOpening:false,lines:[{accountId:String(invoice.credit_account_id),debit:minorToDecimal(amount),credit:'0'},{accountId:String(batch.bank_account_id),debit:'0',credit:minorToDecimal(amount)}]},key,'posted');return {journal,id:journal.result.id};},
 commit:<Result>(m:SupplierPaymentMutation<SupplierPaymentEvidence,SupplierPaymentJournal>,result:Result)=>commit(write,actor,m,result),
 };}
async function commit<Result>(write:SqlDatabase,actor:WorkContext,m:SupplierPaymentMutation<SupplierPaymentEvidence,SupplierPaymentJournal>,result:Result):Promise<Result>{
 const {key,id,now,operation}=m;
 if(m.kind==='create'){const {input,bank}=m;
 const steps:SqlStatement[]=[assertion(write,key,'open-period',"EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[actor.companyId,input.plannedDate,input.plannedDate]),write.prepare('INSERT INTO finance_supplier_batches(id,company_id,currency,bank_account_id,planned_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)').bind(id,actor.companyId,bank.currency,bank.id,input.plannedDate,actor.userId,now,now)];
 for(const {index,item,amount} of m.items) {
  // Touch the invoice version to serialize all reservations and source mutations.
  steps.push(assertion(write,key,`invoice-${index}`,`EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id JOIN finance_parties p ON p.id=i.party_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.kind='payable' AND i.source_type IN ('standalone','commerce_fulfillment') AND i.status IN ('posted','partially_paid') AND j.status='posted' AND j.chart_id=? AND i.currency=? AND p.status='active' AND p.kind='supplier' AND i.invoice_date<=? AND i.credit_account_id<>? AND ${financeSupplierPaymentSourceFacts().noPendingReturn} AND ${financeContractorPaymentClaimFacts().noActiveContractorClaim} AND i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) FROM finance_supplier_instructions s WHERE s.invoice_id=i.id AND s.status IN ('reserved','exported','unknown')),0)>=?)`,[item.invoiceId,actor.companyId,item.expectedVersion,bank.chart_id,bank.currency,input.plannedDate,bank.id,amount]),
   write.prepare('INSERT INTO finance_supplier_instructions(id,batch_id,invoice_id,invoice_version,amount_minor,beneficiary_name,beneficiary_account,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)').bind(`${id}-${index}`,id,item.invoiceId,item.expectedVersion+1,amount,item.beneficiaryName,item.beneficiaryAccount,now,now),
   write.prepare('UPDATE finance_invoices SET version=version+1,updated_at=? WHERE id=?').bind(now,item.invoiceId));
 }
 steps.push(stageSupplierBatchNotifications(write,key,id,'Supplier batch prepared: independent review required',actor.userId));
 return executeCommand(write,actor,key,'finance.supplier_batch.create',input,id,1,result,steps);
 }
 if(m.kind==='decision'){const {input,batch,decision}=m;
 const steps=[assertBatch(write,key,actor,batch,input.expectedVersion)];
 if(decision==='approve')steps.push(write.prepare("UPDATE finance_supplier_batches SET status='approved',approved_by=?,approval_reason=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,input.reason,now,id));
 else steps.push(write.prepare("UPDATE finance_supplier_batches SET status='cancelled',version=version+1,updated_at=? WHERE id=?").bind(now,id),write.prepare("UPDATE finance_supplier_instructions SET status='cancelled',evidence=?,resolved_by=?,version=version+1,updated_at=? WHERE batch_id=? AND status='reserved'").bind(input.reason,actor.userId,now,id));
 steps.push(stageSupplierBatchNotifications(write,key,id,decision==='approve'?'Supplier batch approved: independent Treasury export required':'Supplier batch cancelled: review source invoices before preparing again',actor.userId));
 return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,result,steps);
 }
 if(m.kind==='export'){const {input,batch,sha256}=m;
 return executeCommand(write,actor,key,'finance.supplier_batch.export',input,id,input.expectedVersion+1,result,[
  assertBatch(write,key,actor,batch,input.expectedVersion),
  assertion(write,key,'bank-account',`EXISTS(SELECT 1 FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.status='active' AND a.category='asset' AND ${financeSupplierPaymentSourceFacts().configuredCashAccount} AND c.status='active' AND a.currency=? AND c.base_currency=?)`,[batch.bank_account_id,actor.companyId,batch.currency,batch.currency]),
  assertion(write,key,'open-period',"EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[actor.companyId,batch.planned_date,batch.planned_date]),
  assertion(write,key,'source-balances',`NOT EXISTS(SELECT 1 FROM finance_supplier_instructions s JOIN finance_invoices i ON i.id=s.invoice_id JOIN finance_parties p ON p.id=i.party_id JOIN finance_journals j ON j.id=i.journal_id WHERE s.batch_id=? AND (s.status<>'reserved' OR i.status NOT IN ('posted','partially_paid') OR p.status<>'active' OR j.status<>'posted' OR i.outstanding_minor<(SELECT SUM(r.amount_minor) FROM finance_supplier_instructions r WHERE r.invoice_id=i.id AND r.status IN ('reserved','exported','unknown')) OR ${financeSupplierPaymentSourceFacts().pendingReturn}))`,[id]),
  write.prepare("UPDATE finance_supplier_batches SET status='exported',exported_by=?,export_command_id=?,export_hash=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,key,sha256,now,id),
  write.prepare("UPDATE finance_supplier_instructions SET status='exported',version=version+1,updated_at=? WHERE batch_id=? AND status='reserved'").bind(now,id),
  stageSupplierBatchNotifications(write,key,id,'Supplier batch exported: confirm each bank result before settlement',actor.userId),
 ]);
 }
 if(m.kind==='result'){const {input,item,batch}=m;
 return executeCommand(write,actor,key,'finance.supplier_batch.result',input,id,input.expectedVersion+1,result,[
  assertion(write,key,'item',"EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE id=? AND version=? AND status IN ('exported','unknown'))",[id,input.expectedVersion]),
  write.prepare('UPDATE finance_supplier_instructions SET status=?,evidence=?,resolved_by=?,version=version+1,updated_at=? WHERE id=?').bind(input.outcome,input.evidence,actor.userId,now,id),
  write.prepare('UPDATE finance_invoices SET version=version+1,updated_at=? WHERE id=?').bind(now,item.invoice_id),
  write.prepare('UPDATE finance_supplier_batches SET version=version+1,updated_at=? WHERE id=?').bind(now,batch.id),
  stageSupplierBatchNotifications(write,key,String(batch.id),'Supplier bank result updated: review remaining instructions',actor.userId),
 ]);
 }
 const {input,batch,invoice,bank,amount,evidence:attachmentBinding,journal,paymentId}=m;
 return executeCommand(write,actor,key,'finance.supplier_batch.confirm',input,id,input.expectedVersion+1,result,[
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
 ...(attachmentBinding?.steps??[]),stageSupplierBatchNotifications(write,key,String(batch.id),'Supplier payment matched: source invoice and accounting updated',actor.userId),
 ]);
}
