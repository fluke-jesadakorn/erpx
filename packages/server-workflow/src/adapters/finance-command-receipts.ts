import "server-only";
import {nativeWorkflowReadFirst} from './native-workflow-read';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Provided-transaction receipt lookups preserve Finance's original read timing. */
export function financeCommandReceipts(db:SqlDatabase){return {
 record:(commandId:string)=>db.prepare("SELECT record_id FROM command_receipts WHERE command_id=?").bind(commandId).first<{record_id:string}>(),
 refundExport:(commandId:string|number|null)=>db.prepare("SELECT result_json FROM command_receipts WHERE command_id=? AND operation='finance.refund.export'").bind(commandId).first<{result_json:string}>(),
 /** Owner-native read: the sales-invoice completion receipt is planned by the
  * mounted Workflow wave3 command planner (WF_NATIVE_FINANCE_SALES_COMPLETION)
  * and executed on the caller's open company unit. This read carries no SQL. */
 salesCompletion:(companyId:string,invoiceId:string)=>nativeWorkflowReadFirst<{result_json:string}>('wf_native_finance_sales_completion',[invoiceId,companyId],companyId,db),
};}
