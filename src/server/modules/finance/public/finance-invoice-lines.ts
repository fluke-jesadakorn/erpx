/** Legacy public compatibility boundary; migration status is recorded in ownership.json. */
export { prepareInvoiceLineRevision, currentInvoiceLines, nativeInvoicePosting } from "@/server/modules/finance/adapters/finance-invoice-lines";
export type { PersistedInvoiceLine } from "@/server/modules/finance/adapters/finance-invoice-lines";
