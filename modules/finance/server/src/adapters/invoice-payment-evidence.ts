import {rejectNativeServiceInvoicePaymentEvidence} from './native-service-invoice-payment-evidence';
import { INVOICE_EXISTS_PREFIX } from './finance-read-tail-r30-sql';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import {getInvoice} from '@erpx/finance-server/public/finance-invoice-read';
import {createNativeFlowReader}from '@erpx/workflow-server/public/flow-reader';
// getInvoice retains the invoice's source/history gate. The existing invoice
// workflow branch has no additional History or decoration step.
const invoiceFlow=createNativeFlowReader('invoice',{requireHistory:async()=>{},decorate:view=>view});
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import { expenseAuthoritySnapshot } from "@erpx/people-payroll-server/public/expense-routing";

export async function invoicePaymentOwner(db:SqlDatabase,actor:WorkContext,id:string,editable=false) {
  // This source marker only chooses the authoritative runtime. It grants no
  // permission and never becomes an HTTP success guard in this transaction.
  const marker=await db.prepare("SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND source_type='field_service') native").bind(id).first<{native:boolean}>();
  if(!marker||typeof marker.native!=='boolean')throw new ApiError(503,'INVOICE_OWNER_UNAVAILABLE','The invoice owner could not be verified.');
  if(marker.native)return rejectNativeServiceInvoicePaymentEvidence(actor,id);
  const {invoice}=await getInvoice(actor,id,db);
  if(invoice.kind!=="payable")throw new ApiError(422,"PAYABLE_REQUIRED","Payment slips belong to supplier bills.");
  if(editable){
    const flow=await invoiceFlow(db,actor,id);
    if(!["posted","partially_paid"].includes(String(invoice.status))||!flow.canAct)throw new ApiError(403,"TREASURY_REQUIRED",flow.blocker??"The independent assigned Finance payer must attach payment evidence.");
  }
  return invoice;
}
export async function invoiceEvidenceGuard(db:SqlDatabase,actor:WorkContext,id:string,key:string){
  const authority=await expenseAuthoritySnapshot(db,actor.companyId);
  const invoice=await invoicePaymentOwner(db,actor,id,true);
  return {invoice,guards:[authority.guard(key,"invoice-payment-authority"),assertion(db,key,"invoice-payment-version",INVOICE_EXISTS_PREFIX + " AND version=? AND status IN ('posted','partially_paid'))",[id,actor.companyId,invoice.version])]};
}
