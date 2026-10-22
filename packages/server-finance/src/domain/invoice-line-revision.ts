import type { InvoiceLineInput } from "@erpx/server-finance/domain/finance-invoice-lines";
export type FinanceError = (status: number, code: string, message: string) => Error;
export type InvoiceHeader = {
    partyId: string;
    kind: "payable" | "receivable";
    invoiceNumber: string;
    invoiceDate: string;
    currency: string;
    debitAccountId: string;
    creditAccountId: string;
    taxAccountId?: string;
    taxCode?: string;
    lines?: InvoiceLineInput[];
};
export type InvoiceAccount = {
    id: string;
    chart_id: string;
    code: string;
    name: string;
    category: string;
    currency: string;
    version: number;
    status: string;
};
export type PersistedInvoiceLine = {
    id: string;
    invoice_id: string;
    source_version: number;
    ordinal: number;
    description: string;
    quantity_millis: number | null;
    unit_price_minor: number | null;
    tax_bps: number | null;
    subtotal_minor: number;
    tax_minor: number;
    total_minor: number;
    account_id: string;
    tax_account_id: string | null;
    source_snapshot_json: string;
    currency: string;
    origin_kind: string;
};
export type PlannedInvoiceLine = {
    description: string;
    quantityMillis: number | null;
    unitPriceMinor: number | null;
    taxBps: number | null;
    subtotalMinor: number;
    taxMinor: number;
    totalMinor: number;
    accountId: string;
    taxAccountId: string | null;
};
export type InvoiceRevisionPlan = {
    accounts: InvoiceAccount[];
    party: Record<string, unknown>;
    rows: PlannedInvoiceLine[];
    controlAccount: string;
    mode: "native_lines" | "whole_document";
    revision: number;
    count: number;
    now: number;
};
export type InvoicePostingGuard = {
    kind: "revision";
    label: string;
    invoiceId: string | number | null;
    revision: string | number | null;
} | {
    kind: "account" | "party";
    label: string;
    id: string;
    version: number;
};
export type InvoicePostingLine = {
    accountId: string;
    description: string;
    debit: string;
    credit: string;
    dimensions?: Record<string, string>;
};
