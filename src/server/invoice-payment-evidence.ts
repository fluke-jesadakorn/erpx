import type { SqlDatabase } from "./runtime/types";
import type { WorkContext } from "./authorization";
import { getInvoice } from "./finance";
import { resolveFlow } from "./flow-routing";
import { ApiError } from "./http";
import { assertion } from "./commands";
import { expenseAuthoritySnapshot } from "./expense-routing";

export async function invoicePaymentOwner(db:SqlDatabase,actor:WorkContext,id:string,editable=false) {
  const {invoice}=await getInvoice(actor,id);
  if(invoice.kind!=="payable")throw new ApiError(422,"PAYABLE_REQUIRED","Payment slips belong to supplier bills.");
  if(editable){
    const flow=await resolveFlow(db,actor,"invoice",id);
    if(!["posted","partially_paid"].includes(String(invoice.status))||!flow.canAct)throw new ApiError(403,"TREASURY_REQUIRED",flow.blocker??"The independent assigned Finance payer must attach payment evidence.");
  }
  return invoice;
}
export async function invoiceEvidenceGuard(db:SqlDatabase,actor:WorkContext,id:string,key:string){
  const authority=await expenseAuthoritySnapshot(db,actor.companyId);
  const invoice=await invoicePaymentOwner(db,actor,id,true);
  return {invoice,guards:[authority.guard(key,"invoice-payment-authority"),assertion(db,key,"invoice-payment-version","EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND status IN ('posted','partially_paid'))",[id,actor.companyId,invoice.version])]};
}
