import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export const supplierExportReceipt=(db:SqlDatabase,key:string)=>(db.prepare("SELECT result_json FROM command_receipts WHERE command_id=? AND operation='finance.supplier_batch.export'").bind(key).first<{result_json:string}>());
