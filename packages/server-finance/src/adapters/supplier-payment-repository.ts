import {supplierPaymentSourceOptions,supplierBatchOwnedSql} from "@erpx/server-finance/adapters/supplier-payment-purchases";
import {commerceFinanceRecordSources} from "@erpx/server-commerce/public/finance-record-sources";
import {financeSupplierPaymentSourceFacts} from '@erpx/server-commerce/public/finance-supplier-payment-source-facts';
import {platformOwnerProjectionSources} from "@erpx/server-finance/adapters/platform-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {prepareJournal} from '@erpx/server-finance/public/journal-planning';
import type {prepareAttachmentBinding} from "@erpx/server-files/public/attachments/service";
import type {SupplierPaymentRow as Row} from "@erpx/server-procurement/public/supplier-payment-contract";
export type SupplierPaymentEvidence=Awaited<ReturnType<typeof prepareAttachmentBinding>>;
export type SupplierPaymentJournal=Awaited<ReturnType<typeof prepareJournal>>;
/** Bounded SELECT-only read port consumed by the procurement supplier-payment application.
 *  The write half of this repository was retired by retire.finance.supplier-batches; this
 *  port is the remaining native read surface and is the only shape the consumer may use. */
export interface SupplierPaymentReadPort{
  batch(id:string):Promise<Row|null>;
  item(id:string):Promise<Row|null>;
  list(forWork:boolean):Promise<Row[]>;
  workspace(id:string|undefined,batch:Row|null):Promise<{items:Row[];invoices:Row[];accounts:Row[];periods:Row[];bank:Row[]}>;
}
export function supplierPaymentRepository(write:SqlDatabase,actor:WorkContext,options:{invoiceId?:string}={}):SupplierPaymentReadPort{return {
  batch:(id:string)=>write.prepare(`${platformOwnerProjectionSources.finance_supplier_batches} WHERE id=? AND company_id=?`).bind(id,actor.companyId).first<Row>(),
  item:(id:string)=>write.prepare('SELECT i.* FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE i.id=? AND b.company_id=?').bind(id,actor.companyId).first<Row>(),
  async list(forWork:boolean){
  const source=await supplierPaymentSourceOptions(write,actor);return (await write.prepare(`SELECT b.*,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('reserved','exported','unknown')) pending_count,(SELECT count(*) FROM finance_supplier_instructions i JOIN finance_invoices f ON f.id=i.invoice_id WHERE i.batch_id=b.id AND i.status='failed' AND f.outstanding_minor>0) failed_count,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown') unknown_count FROM finance_supplier_batches b WHERE b.company_id=? AND ${source.sourceOnly?supplierBatchOwnedSql("b"):"TRUE"} ORDER BY b.updated_at DESC${forWork?'':' LIMIT 200'}`).bind(actor.companyId).all<Row>()).results;
 },async workspace(id:string|undefined,batch:Row|null){
  const source=await supplierPaymentSourceOptions(write,actor),ownedInvoice=`EXISTS(SELECT 1 FROM (${commerceFinanceRecordSources.purchase.invoice}) owner_invoice WHERE owner_invoice.id=i.id AND owner_invoice.company_id=i.company_id)`;
  const [items,invoices,accounts,periods,bank]=await Promise.all([
   id?write.prepare('SELECT s.*,i.invoice_number,i.currency FROM finance_supplier_instructions s JOIN finance_invoices i ON i.id=s.invoice_id WHERE s.batch_id=? ORDER BY s.id').bind(id).all<Row>():Promise.resolve({results:[] as Row[]}),
   !id?write.prepare(`SELECT i.*,p.name party_name,i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) FROM finance_supplier_instructions s WHERE s.invoice_id=i.id AND s.status IN ('reserved','exported','unknown')),0) available_minor FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? AND i.kind='payable' AND i.source_type IN ('standalone','commerce_fulfillment') AND i.status IN ('posted','partially_paid') AND ${source.sourceOnly?ownedInvoice:"TRUE"} AND ${options.invoiceId?"i.id=?":"TRUE"} ORDER BY i.due_date,i.id LIMIT 200`).bind(actor.companyId,...(options.invoiceId?[options.invoiceId]:[])).all<Row>():Promise.resolve({results:[] as Row[]}),
   write.prepare(`SELECT a.id,a.code,a.name,a.currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.category='asset' AND a.status='active' AND ${financeSupplierPaymentSourceFacts().configuredCashAccount} AND c.status='active' AND a.currency=c.base_currency ORDER BY a.code`).bind(actor.companyId).all<Row>(),
   write.prepare("SELECT id,name FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date").bind(actor.companyId).all<Row>(),
   batch&&hasPermission(actor,'finance.pay')?write.prepare(`SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND s.bank_account=? AND t.match_status IN ('unmatched','proposed') AND ${source.sourceOnly?"t.currency=? AND t.transaction_date>=? AND EXISTS(SELECT 1 FROM finance_supplier_instructions instruction WHERE instruction.batch_id=? AND instruction.status IN ('exported','unknown') AND t.amount_minor=-instruction.amount_minor)":"TRUE"} ORDER BY t.transaction_date DESC LIMIT 200`).bind(actor.companyId,batch.bank_account_id,...(source.sourceOnly?[batch.currency,batch.planned_date,batch.id]:[])).all<Row>():Promise.resolve({results:[] as Row[]}),
  ]);
  return {items:items.results,invoices:invoices.results,accounts:accounts.results,periods:periods.results,bank:bank.results};
 }
  };}
