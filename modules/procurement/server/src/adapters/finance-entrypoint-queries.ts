import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeProcurementReadFirst, PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE } from "./native-procurement-read";

/** Finance's already-authorized invoice detail retains the provided Company-scoped match read.
 * The statement is planned by the Procurement native read port
 * (`RD_INVOICE_MATCH_FOR_FINANCE`) and executed on the caller's open unit; this
 * module carries no SQL for this read. */
export function procurementInvoiceMatchForFinance(db: SqlDatabase, invoiceId: string, companyId: string) {
  return nativeProcurementReadFirst<{ id: string; reference: string }>(
    PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE, [invoiceId, companyId], db,
  );
}
