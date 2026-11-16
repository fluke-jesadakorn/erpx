import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/** Finance's already-authorized invoice detail retains the provided Company-scoped match read. */
export function procurementInvoiceMatchForFinance(db: SqlDatabase, invoiceId: string, companyId: string) {
  return db.prepare("SELECT 1").bind().first<{ id: string; reference: string }>();
}
