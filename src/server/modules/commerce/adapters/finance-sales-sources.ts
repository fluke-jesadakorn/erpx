/** Closed projection for Finance's branch/department sales readers. */
export const commerceFinanceSalesSources = {
  invoiceScope: "SELECT f.invoice_id,o.company_id,o.branch_id,o.department_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE o.kind='sales'",
} as const;

import type { SqlDatabase } from "../../runtime/public/runtime/types";
export type CommerceSalesLine = { id: string; fulfillment_id: string; description: string; quantity_milli: number; price_minor: number; tax_bps: number; subtotal_minor: number; tax_minor: number; item_type: string; product_id: string; completion_date: string };
/** Finance must first authorize the Company invoice; this reader returns only its linked sales lines. */
export async function commerceInvoiceSalesLines(db: SqlDatabase, companyId: string, invoiceId: string): Promise<CommerceSalesLine[]> {
  return (await db.prepare("SELECT fl.id,fl.fulfillment_id,ol.description,fl.quantity_milli,ol.price_minor,ol.tax_bps,fl.subtotal_minor,fl.tax_minor,ol.item_type,ol.product_id,f.fulfillment_date completion_date FROM commerce_fulfillment_lines fl JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_orders o ON o.id=f.order_id WHERE f.invoice_id=? AND o.company_id=? AND o.kind='sales' ORDER BY f.created_at,f.id,ol.ordinal,fl.id").bind(invoiceId, companyId).all<CommerceSalesLine>()).results;
}
