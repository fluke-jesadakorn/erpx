import { calculateInvoiceLines } from "../domain/finance-invoice-lines";
import type { FinanceError, InvoiceHeader, InvoiceAccount, PersistedInvoiceLine, InvoiceRevisionPlan, InvoicePostingGuard, InvoicePostingLine } from "../domain/invoice-line-revision";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { minorToDecimal } from "@/lib/finance-money";
export interface InvoiceLineRepository {
    accounts(companyId: string, ids: string[]): Promise<InvoiceAccount[]>;
    party(companyId: string, partyId: string): Promise<Record<string, unknown> | null>;
    current(companyId: string, invoiceId: string, revision: number): Promise<PersistedInvoiceLine[]>;
    billingContent(companyId: string, invoiceId: string | number | null): Promise<string | null>;
}
export function createInvoiceLineRevision(ports: {
    repository: InvoiceLineRepository;
    error: FinanceError;
    now(): number;
}) {
    async function prepare(actor: WorkContext, input: InvoiceHeader, revision: number, amounts: {
        subtotal: number;
        tax: number;
    }): Promise<InvoiceRevisionPlan> {
        let calculated: ReturnType<typeof calculateInvoiceLines> | null = null;
        try {
            calculated = input.lines ? calculateInvoiceLines(input.lines) : null;
        }
        catch (e) {
            throw ports.error(422, "INVALID_INVOICE_LINE", e instanceof Error ? e.message : "Invalid invoice lines.");
        }
        if (calculated && (calculated.subtotalMinor !== amounts.subtotal || calculated.taxMinor !== amounts.tax))
            throw ports.error(422, "INVOICE_LINE_TOTAL_MISMATCH", "The header subtotal and tax must equal the sum of the calculated lines.");
        const netAccount = input.kind === "payable" ? input.debitAccountId : input.creditAccountId, controlAccount = input.kind === "payable" ? input.creditAccountId : input.debitAccountId;
        const rows = calculated?.lines.map(line => ({ description: line.description, quantityMillis: line.quantityMillis, unitPriceMinor: line.unitPriceMinor, taxBps: line.taxBps, subtotalMinor: line.subtotalMinor, taxMinor: line.taxMinor, totalMinor: line.totalMinor, accountId: line.accountId, taxAccountId: line.taxAccountId ?? input.taxAccountId ?? null })) ?? [{ description: `Whole document ${input.invoiceNumber}`, quantityMillis: null, unitPriceMinor: null, taxBps: null, subtotalMinor: amounts.subtotal, taxMinor: amounts.tax, totalMinor: amounts.subtotal + amounts.tax, accountId: netAccount, taxAccountId: input.taxAccountId ?? null }];
        const ids = [...new Set([input.debitAccountId, input.creditAccountId, ...rows.flatMap(line => [line.accountId, ...(line.taxMinor && line.taxAccountId ? [line.taxAccountId] : [])])])];
        const accounts = await ports.repository.accounts(actor.companyId, ids);
        if (accounts.length !== ids.length || accounts.some(account => account.status !== "active" || account.currency !== input.currency) || new Set(accounts.map(account => account.chart_id)).size !== 1)
            throw ports.error(422, "ACCOUNT_INVALID", "Every invoice line must use the same active Company chart and currency.");
        for (const line of rows)
            if (line.accountId === controlAccount || (line.taxMinor && (!line.taxAccountId || [line.accountId, controlAccount].includes(line.taxAccountId))))
                throw ports.error(422, "ACCOUNT_INVALID", "Invoice net, control and tax accounts must be distinct.");
        const party = await ports.repository.party(actor.companyId, input.partyId);
        if (!party)
            throw ports.error(422, "PARTY_INVALID", "The invoice party is unavailable.");
        return { accounts, party, rows, controlAccount, now: ports.now(), mode: calculated ? "native_lines" : "whole_document", revision, count: rows.length };
    }
    async function posting(actor: WorkContext, invoice: Record<string, string | number | null>): Promise<{
        lines: InvoicePostingLine[];
        guards: InvoicePostingGuard[];
    } | null> {
        if (invoice.source_type !== "standalone")
            return null;
        const rows = await ports.repository.current(actor.companyId, String(invoice.id), Number(invoice.line_revision));
        if (!rows.length || rows.reduce((sum, row) => sum + Number(row.subtotal_minor), 0) !== Number(invoice.subtotal_minor) || rows.reduce((sum, row) => sum + Number(row.tax_minor), 0) !== Number(invoice.tax_minor))
            throw ports.error(409, "INVOICE_LINES_CHANGED", "Review the invoice line revision before posting.");
        const payable = invoice.kind === "payable", controlId = String(payable ? invoice.credit_account_id : invoice.debit_account_id);
        const lines: InvoicePostingLine[] = rows.flatMap(row => [{ accountId: row.account_id, description: row.description, debit: payable ? minorToDecimal(Number(row.subtotal_minor)) : "0", credit: payable ? "0" : minorToDecimal(Number(row.subtotal_minor)) }, ...(Number(row.tax_minor) ? [{ accountId: String(row.tax_account_id), description: `VAT: ${row.description}`, debit: payable ? minorToDecimal(Number(row.tax_minor)) : "0", credit: payable ? "0" : minorToDecimal(Number(row.tax_minor)) }] : [])]);
        lines.push({ accountId: controlId, description: `Control: ${invoice.invoice_number}`, debit: payable ? "0" : minorToDecimal(Number(invoice.total_minor)), credit: payable ? minorToDecimal(Number(invoice.total_minor)) : "0" });
        const guards: InvoicePostingGuard[] = [{ kind: "revision", label: "invoice-line-revision", invoiceId: invoice.id, revision: invoice.line_revision }];
        for (const [index, row] of rows.entries()) {
            const snapshot = JSON.parse(row.source_snapshot_json) as {
                party?: {
                    id: string;
                    version: number;
                };
                netAccount?: InvoiceAccount;
                taxAccount?: InvoiceAccount | null;
                controlAccount?: InvoiceAccount;
            };
            if (row.origin_kind === "historical_header")
                continue;
            for (const [key, account] of Object.entries({ net: snapshot.netAccount, tax: snapshot.taxAccount, control: snapshot.controlAccount }))
                if (account)
                    guards.push({ kind: "account", label: `invoice-line-${index}-${key}`, id: account.id, version: account.version });
            if (snapshot.party)
                guards.push({ kind: "party", label: `invoice-line-${index}-party`, id: snapshot.party.id, version: Number(snapshot.party.version) });
        }
        const billing = await ports.repository.billingContent(actor.companyId, invoice.id), dimensions = billing ? (JSON.parse(billing) as {
            dimensions: Record<string, string>;
        }).dimensions : undefined;
        return { lines: dimensions ? lines.map(line => ({ ...line, dimensions })) : lines, guards };
    }
    return { prepare, posting, current: ports.repository.current };
}
