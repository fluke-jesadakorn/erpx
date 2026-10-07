import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

/** Finance's already-authorized invoice detail retains the provided Company-scoped match read. */
export function procurementInvoiceMatchForFinance(db: SqlDatabase, invoiceId: string, companyId: string) {
  return db.prepare("SELECT id,reference FROM procurement_bill_matches WHERE invoice_id=? AND company_id=?").bind(invoiceId, companyId).first<{ id: string; reference: string }>();
}
