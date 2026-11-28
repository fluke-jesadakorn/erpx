import "server-only";
import {nativeWorkflowReadFirst} from './native-workflow-read';
import {receiptRecordIdSql,exportReceiptResultSql} from './receipt-sql';
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
/** Provided-transaction receipt lookups preserve Finance's original read timing. */
export function financeCommandReceipts(db:SqlDatabase){return {
 record:(commandId:string)=>db.prepare(receiptRecordIdSql).bind(commandId).first<{record_id:string}>(),
 refundExport:(commandId:string|number|null)=>db.prepare(exportReceiptResultSql).bind(commandId,'finance.refund.export').first<{result_json:string}>(),
 /** Owner-native read: the sales-invoice completion receipt is planned by the
  * mounted Workflow wave3 command planner (WF_NATIVE_FINANCE_SALES_COMPLETION)
  * and executed on the caller's open company unit. This read carries no SQL. */
 salesCompletion:(companyId:string,invoiceId:string)=>nativeWorkflowReadFirst<{result_json:string}>('wf_native_finance_sales_completion',[invoiceId,companyId],companyId,db),
};}
