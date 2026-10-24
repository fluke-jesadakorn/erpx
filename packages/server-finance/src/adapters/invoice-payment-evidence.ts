import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {getInvoice} from '@erpx/server-finance/public/finance-invoice-read';
import {createNativeFlowReader}from '@erpx/server-workflow/public/flow-reader';
// getInvoice retains the invoice's source/history gate. The existing invoice
// workflow branch has no additional History or decoration step.
const invoiceFlow=createNativeFlowReader('invoice',{requireHistory:async()=>{},decorate:view=>view});
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { expenseAuthoritySnapshot } from "@erpx/server-people_payroll/public/expense-routing";

export async function invoicePaymentOwner(db:SqlDatabase,actor:WorkContext,id:string,editable=false) {
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
  return {invoice,guards:[authority.guard(key,"invoice-payment-authority"),assertion(db,key,"invoice-payment-version","EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND status IN ('posted','partially_paid'))",[id,actor.companyId,invoice.version])]};
}
