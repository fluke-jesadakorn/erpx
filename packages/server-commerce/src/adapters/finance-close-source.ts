import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
/** Return guards and refund count projection preserve Finance's owned transaction. */
export function financeCloseCommerceSource(db:SqlDatabase){return {
 guard:(key:string,companyId:string,endDate:string|number|null)=>assertion(db,key,"unfinished-returns","NOT EXISTS(SELECT 1 FROM commerce_returns WHERE company_id=? AND status='approved' AND return_date<=?)",[companyId,endDate]),
 reversalGuard:(key:string,label:string,invoiceId:string|number|null)=>assertion(db,key,label,"NOT EXISTS(SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status!='rejected')",[invoiceId]),
 pendingRefundCount:"SELECT COUNT(*) FROM commerce_returns WHERE company_id=? AND status='approved' AND return_date<=?",
};}
