import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL, commerceReturnsPointSql} from "./guarded-source-dedupe-sql";
/** Return guards and refund count projection preserve Finance's owned transaction. */
export function financeCloseCommerceSource(db:SqlDatabase){return {
 guard:(key:string,companyId:string,endDate:string|number|null)=>assertion(db,key,"unfinished-returns",`NOT EXISTS(${commerceReturnsPointSql('1',"company_id=? AND status='approved' AND return_date<=?")})`,[companyId,endDate]),
 reversalGuard:(key:string,label:string,invoiceId:string|number|null)=>assertion(db,key,label,COMMERCE_RETURN_INVOICE_UNREJECTED_GUARD_SQL,[invoiceId]),
 pendingRefundCount:commerceReturnsPointSql('COUNT(*)',"company_id=? AND status='approved' AND return_date<=?"),
};}
