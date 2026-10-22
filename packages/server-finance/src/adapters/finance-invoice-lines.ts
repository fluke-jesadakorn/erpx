import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { snapshotMoney } from "@erpx/server-finance/public/company-money";
import { createInvoiceLineRevision } from '@erpx/server-finance/application/invoice-line-revision';
import type { InvoiceHeader as Header, InvoiceAccount as Account, PersistedInvoiceLine } from '@erpx/server-finance/domain/invoice-line-revision';
export type { PersistedInvoiceLine } from '@erpx/server-finance/domain/invoice-line-revision';
export async function prepareInvoiceLineRevision(db: SqlDatabase, actor: WorkContext, input: Header, invoiceId: string, revision: number, amounts: {
    subtotal: number;
    tax: number;
}, commandId: string) {
    const plan = await invoiceLineService(db).prepare(actor, input, revision, amounts), { accounts, party, rows, controlAccount, now } = plan, byId = new Map(accounts.map(account => [account.id, account]));
    const guards: SqlStatement[] = [
        ...accounts.map((account, index) => assertion(db, commandId, `line-account-${index}`, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND status='active' AND version=?)", [account.id, actor.companyId, account.version])),
        assertion(db, commandId, "line-party", "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND version=? AND status='active')", [input.partyId, actor.companyId, Number(party.version)]),
    ];
    const steps: SqlStatement[] = rows.map((line, ordinal) => db.prepare(`INSERT INTO finance_invoice_line_versions(id,company_id,invoice_id,source_version,ordinal,description,quantity_millis,unit_price_minor,tax_bps,subtotal_minor,tax_minor,total_minor,currency,account_id,tax_account_id,source_snapshot_json,origin_kind,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(`${invoiceId}:v${revision}:${ordinal}`, actor.companyId, invoiceId, revision, ordinal, line.description, line.quantityMillis, line.unitPriceMinor, line.taxBps, line.subtotalMinor, line.taxMinor, line.totalMinor, input.currency, line.accountId, line.taxAccountId, JSON.stringify({ schema: "finance.invoice.line.v1", companyId: actor.companyId, invoiceDate: input.invoiceDate, invoiceNumber: input.invoiceNumber, kind: input.kind, taxCode: input.taxCode ?? null, party, netAccount: byId.get(line.accountId), taxAccount: line.taxAccountId ? byId.get(line.taxAccountId) : null, controlAccount: byId.get(controlAccount) }), plan.mode === "native_lines" ? "native_line" : "whole_document", actor.userId, now));
    const snapshot = await snapshotMoney(db, actor, { sourceType: "finance_invoice", sourceId: invoiceId, amountKind: `draft:v${revision}`, transactionDate: input.invoiceDate, currency: input.currency, currencyScale: 2, amountUnits: String(amounts.subtotal + amounts.tax) });
    steps.push(snapshot.statement);
    return { guards, steps, mode: plan.mode, revision, count: rows.length };
}
export async function currentInvoiceLines(db: SqlDatabase, companyId: string, invoiceId: string, revision: number) {
    return (await db.prepare("SELECT * FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? AND source_version=? ORDER BY ordinal").bind(invoiceId, companyId, revision).all<PersistedInvoiceLine>()).results;
}
export async function nativeInvoicePosting(db: SqlDatabase, actor: WorkContext, invoice: Record<string, string | number | null>, commandId: string) {
    const plan = await invoiceLineService(db).posting(actor, invoice);
    if (!plan)
        return null;
    return { lines: plan.lines, guards: plan.guards.map(guard => guard.kind === 'revision' ? assertion(db, commandId, guard.label, 'EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND line_revision=?)', [guard.invoiceId, actor.companyId, guard.revision]) : guard.kind === 'account' ? assertion(db, commandId, guard.label, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND version=? AND status='active')", [guard.id, actor.companyId, guard.version]) : assertion(db, commandId, guard.label, "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND version=? AND status='active')", [guard.id, actor.companyId, guard.version])) };
}
function invoiceLineService(db: SqlDatabase) {
    return createInvoiceLineRevision({ error: (...args) => new ApiError(...args), now: Date.now, repository: { accounts: async (companyId, ids) => (await db.prepare(`SELECT id,chart_id,code,name,category,currency,version,status FROM finance_accounts WHERE company_id=? AND id IN (${ids.map(() => "?").join(",")})`).bind(companyId, ...ids).all<Account>()).results, party: async (companyId, partyId) => db.prepare("SELECT id,code,name,kind,currency,tax_registration,version,status FROM finance_parties WHERE id=? AND company_id=? AND status='active'").bind(partyId, companyId).first<Record<string, unknown>>(), current: (companyId, invoiceId, revision) => currentInvoiceLines(db, companyId, invoiceId, revision), billingContent: async (companyId, invoiceId) => (await db.prepare("SELECT COALESCE(b.contract_content_json,c.content_json) content_json FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id WHERE b.invoice_id=? AND b.company_id=?").bind(invoiceId, companyId).first<{
                content_json: string;
            }>())?.content_json ?? null } });
}
