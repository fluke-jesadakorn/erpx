import "server-only";
import {exportReceiptResultSql} from './receipt-sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export const supplierExportReceipt=(db:SqlDatabase,key:string)=>(db.prepare(exportReceiptResultSql).bind(key,'finance.supplier_batch.export').first<{result_json:string}>());
