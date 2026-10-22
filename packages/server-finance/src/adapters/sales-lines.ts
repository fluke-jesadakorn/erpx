import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { commerceInvoiceSalesLines } from "@erpx/server-commerce/public/finance-sales-sources";
import { currentInvoiceLines } from "@erpx/server-finance/public/finance-invoice-lines";
import type { PersistedInvoiceLine } from "@erpx/server-finance/domain/invoice-line-revision";
/** Native invoice revisions take precedence; Commerce retains delivered quantities and amounts. */
export async function salesInvoiceLines(db: SqlDatabase, companyId: string, invoiceId: string, revision: number): Promise<PersistedInvoiceLine[]> {
  const native = await currentInvoiceLines(db, companyId, invoiceId, revision);
  if (native.length && native.some(line => line.origin_kind === "native_line")) return native;
  const invoice = await db.prepare("SELECT currency,credit_account_id,tax_account_id FROM finance_invoices WHERE id=? AND company_id=? AND kind='receivable' AND source_type='commerce_fulfillment'").bind(invoiceId, companyId).first<{currency: string; credit_account_id: string; tax_account_id: string | null}>();
  if (!invoice) return native;
  const sources = await commerceInvoiceSalesLines(db, companyId, invoiceId);
  return sources.map((line, ordinal) => ({ id: line.id, invoice_id: invoiceId, source_version: revision, ordinal, description: line.description, quantity_millis: Number(line.quantity_milli), unit_price_minor: Number(line.price_minor), tax_bps: Number(line.tax_bps), subtotal_minor: Number(line.subtotal_minor), tax_minor: Number(line.tax_minor), total_minor: Number(line.subtotal_minor) + Number(line.tax_minor), account_id: invoice.credit_account_id, tax_account_id: invoice.tax_account_id, source_snapshot_json: JSON.stringify(line), currency: invoice.currency, origin_kind: "commerce_line" }));
}
