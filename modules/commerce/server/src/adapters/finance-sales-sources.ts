/** Closed projection for Finance's branch/department sales readers. */
export const commerceFinanceSalesSources = {
  invoiceScope: "SELECT f.invoice_id,o.company_id,o.branch_id,o.department_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE o.kind='sales'",
} as const;

import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeCommerceInvoiceSalesLines, type CommerceInvoiceSalesLine } from './native-lane-13/client';
/** Finance must first authorize the Company invoice; the native owner returns only its linked sales lines. */
export type CommerceSalesLine = CommerceInvoiceSalesLine;
export async function commerceInvoiceSalesLines(_db: SqlDatabase, companyId: string, invoiceId: string): Promise<CommerceSalesLine[]> {
  return nativeCommerceInvoiceSalesLines(invoiceId, { companyId });
}
