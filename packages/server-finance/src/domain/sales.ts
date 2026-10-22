import { z } from "zod";

export const salesDefaultsSchema = z.object({
  chartId: z.string().min(1), currency: z.string().length(3).transform(value => value.toUpperCase()),
  receivableAccountId: z.string().min(1), revenueAccountId: z.string().min(1),
  taxAccountId: z.string().min(1).optional(), reason: z.string().trim().min(10).max(2000),
});
export const salesDefaultsDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(), action: z.enum(["approve", "reject"]),
  reason: z.string().trim().min(10).max(2000),
});
export const salesQuerySchema = z.object({
  tab: z.enum(["quotations", "orders", "invoices", "receipts"]).default("invoices"),
  q: z.string().max(200).default(""), status: z.string().max(40).default(""),
  startDate: z.preprocess(value => value === "" ? undefined : value, z.string().date().optional()), endDate: z.preprocess(value => value === "" ? undefined : value, z.string().date().optional()),
  cursor: z.string().max(2000).optional(), currency: z.string().length(3).optional(),
  queue: z.enum(["awaiting", "overdue", "received"]).optional(),
}).refine(value => !value.startDate || !value.endDate || value.startDate <= value.endDate, { message: "End date must follow start date." });
export type SalesDefaults = {
  id: string; company_id: string; chart_id: string; currency: string;
  receivable_account_id: string; revenue_account_id: string; tax_account_id: string | null;
  status: string; reason: string; created_by: string; reviewed_by: string | null;
  version: number; created_at: number; updated_at: number;
};
export type ReceiptSnapshot = {
  schema: "finance.customer-receipt.v1"; documentNumber: string; companyName: string;
  customerName: string; customerCode: string; date: string; currency: string; amountMinor: string;
  paymentId: string; paymentVersion: number; bankReference: string; journalId: string;
  allocations: { invoiceId: string; invoiceNumber: string; amountMinor: string }[];
};
const escape = (value: string) => value.replace(/[&<>"']/g, character => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character]!);
const money = (value: string) => { const units = BigInt(value), scale = BigInt(100); return `${units / scale}.${String(units % scale).padStart(2, "0")}`; };
export function renderCustomerReceipt(snapshot: ReceiptSnapshot) {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Receipt ${escape(snapshot.documentNumber)}</title><style>body{font-family:Arial,sans-serif;color:#172033;margin:40px}header{display:flex;justify-content:space-between}h1{font-size:30px}p{line-height:1.6}table{width:100%;border-collapse:collapse;margin:30px 0}td,th{padding:12px;border-bottom:1px solid #ddd;text-align:left}td:last-child,th:last-child{text-align:right}.total{text-align:right;font-size:22px}footer{font-size:12px;color:#555}@page{size:A4;margin:15mm}@media print{body{margin:0}}</style></head><body><header><div><h2>${escape(snapshot.companyName)}</h2><p>Received from<br><strong>${escape(snapshot.customerName)}</strong><br>${escape(snapshot.customerCode)}</p></div><div><h1>Receipt</h1><p>${escape(snapshot.documentNumber)}<br>${escape(snapshot.date)}</p></div></header><table><thead><tr><th>Invoice</th><th>Amount (${escape(snapshot.currency)})</th></tr></thead><tbody>${snapshot.allocations.map(item => `<tr><td>${escape(item.invoiceNumber)}</td><td>${money(item.amountMinor)}</td></tr>`).join("")}${BigInt(snapshot.amountMinor) > snapshot.allocations.reduce((sum, item) => sum + BigInt(item.amountMinor), BigInt(0)) ? `<tr><td>Unallocated customer receipt</td><td>${money(String(BigInt(snapshot.amountMinor) - snapshot.allocations.reduce((sum, item) => sum + BigInt(item.amountMinor), BigInt(0))))}</td></tr>` : ""}</tbody></table><p class="total">Received ${escape(snapshot.currency)} ${money(snapshot.amountMinor)}</p><p>Payment reference: ${escape(snapshot.bankReference)}</p><footer>This receipt records posted customer money. VAT documents are issued separately. Issued copies retain their original contents; check the receipt record for subsequent cancellation.</footer></body></html>`;
}

export const salesInvoiceDraftSchema = z.object({ partyId: z.string().min(1), invoiceDate: z.string().date(), dueDate: z.string().date(), currency: z.string().length(3).transform(value => value.toUpperCase()), completionConfirmed: z.literal(true), completionNote: z.string().trim().min(3).max(2000), lines: z.array(z.object({ description: z.string().trim().min(1).max(1000), quantity: z.string(), unitPrice: z.string(), taxBps: z.number().int().min(0).max(10000) })).min(1).max(100) });
