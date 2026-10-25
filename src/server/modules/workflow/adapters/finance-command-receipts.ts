import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Provided-transaction receipt lookups preserve Finance's original read timing. */
export function financeCommandReceipts(db:SqlDatabase){return {
 record:(commandId:string)=>db.prepare("SELECT record_id FROM command_receipts WHERE command_id=?").bind(commandId).first<{record_id:string}>(),
 refundExport:(commandId:string|number|null)=>db.prepare("SELECT result_json FROM command_receipts WHERE command_id=? AND operation='finance.refund.export'").bind(commandId).first<{result_json:string}>(),
 salesCompletion:(companyId:string,invoiceId:string)=>db.prepare("SELECT result_json FROM command_receipts WHERE record_id=? AND operation='finance.sales-invoice.prepare' AND authority_json::jsonb->>'companyId'=? ORDER BY created_at DESC LIMIT 1").bind(invoiceId,companyId).first<{result_json:string}>(),
};}
