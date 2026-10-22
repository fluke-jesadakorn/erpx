import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
/** Manufacturing close blockers use the existing Finance connection and batch. */
export function financeCloseManufacturingSource(db:SqlDatabase){return {
 guard:(key:string,label:string,companyId:string)=>assertion(db,key,label,"NOT EXISTS(SELECT 1 FROM manufacturing_orders WHERE company_id=? AND status IN ('reserved','in_progress','awaiting_qc'))",[companyId]),
 count:(companyId:string)=>db.prepare("SELECT COUNT(*) count FROM manufacturing_orders WHERE company_id=? AND status IN ('reserved','in_progress','awaiting_qc')").bind(companyId).first<{count:number}>(),
 engineQuery:"SELECT count(*) n FROM manufacturing_orders WHERE company_id=? AND status IN ('reserved','in_progress','awaiting_qc')",
};}
