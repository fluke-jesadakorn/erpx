import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
/** Unresolved bill close facts preserve the caller's transaction and date cutoff. */
export function financeCloseProcurementSource(db:SqlDatabase){return {
 guard:(key:string,companyId:string,endDate:string|number|null)=>assertion(db,key,"unresolved-supplier-bills","NOT EXISTS(SELECT 1 FROM procurement_bill_matches WHERE company_id=? AND status IN ('review','approved','invoiced') AND invoice_date<=?)",[companyId,endDate]),
 count:async(companyId:string,endDate:string|number|null)=>Number(await db.prepare("SELECT COUNT(*) n FROM procurement_bill_matches WHERE company_id=? AND status IN ('review','approved','invoiced') AND invoice_date<=?").bind(companyId,endDate).first('n')??0),
};}
