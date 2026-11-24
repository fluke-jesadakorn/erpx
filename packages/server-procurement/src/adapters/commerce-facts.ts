import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {Row} from "@erpx/server-commerce/public/commerce-contracts";
import {nativeProcurementReadFirst,PROCUREMENT_RD_BILL_MATCH_LINES_FOR_INVOICE,PROCUREMENT_RD_POSTED_BILL_MATCH_FOR_INVOICE} from "./native-procurement-read";
/** Owner-local read plans for the procurement-owned bill-match tables. Both
 * statements are planned by the Procurement command-plan and executed on the
 * caller's open unit; this adapter emits no SQL for these reads. */
export function CommerceProcurementFacts(db:SqlDatabase){return {
readProcurementBillMatchLines1:(facts:{invoice_id:string | number | null;id:string | number | null;})=>nativeProcurementReadFirst<Row>(PROCUREMENT_RD_BILL_MATCH_LINES_FOR_INVOICE,[facts.invoice_id,facts.id],db),
readProcurementBillMatches1:(facts:{invoice_id:string | number | null;companyId:string;})=>nativeProcurementReadFirst<Row>(PROCUREMENT_RD_POSTED_BILL_MATCH_FOR_INVOICE,[facts.invoice_id,facts.companyId],db)
};}
