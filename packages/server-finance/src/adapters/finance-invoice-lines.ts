import { idCompanyStatusVersionGuard } from './finance-read-tail-r20-sql';
import { INVOICE_EXISTS_PREFIX } from './finance-read-tail-r30-sql';
import { versionStatusGuard } from './finance-read-tail-r45-sql';
import{currentInvoiceLines}from '@erpx/server-finance/public/invoice-line-read';
import "server-only";
import { financeInvoiceLines } from '@erpx/server-collaboration/adapters/native-finance-invoice-lines';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import { createInvoiceLineRevision } from '@erpx/server-finance/application/invoice-line-revision';
import type { InvoiceHeader as Header, InvoiceAccount as Account } from '@erpx/server-finance/domain/invoice-line-revision';
export type { PersistedInvoiceLine } from '@erpx/server-finance/domain/invoice-line-revision';
export async function prepareInvoiceLineRevision(db: SqlDatabase, actor: WorkContext, input: Header, invoiceId: string, revision: number, amounts: {
    subtotal: number;
    tax: number;
}, commandId: string) {
    const plan = await invoiceLineService(db, actor).prepare(actor, input, revision, amounts), { accounts, party, rows, controlAccount, now } = plan, byId = new Map(accounts.map(account => [account.id, account]));
    const guards: SqlStatement[] = [
        ...accounts.map((account, index) => assertion(db, commandId, `line-account-${index}`, idCompanyStatusVersionGuard("finance_accounts", "active"), [account.id, actor.companyId, account.version])),
        assertion(db, commandId, "line-party", versionStatusGuard("finance_parties"), [input.partyId, actor.companyId, Number(party.version)]),
    ];
    const steps: SqlStatement[] = await Promise.all(rows.map(async (line, ordinal) => {
      const id = `${invoiceId}:v${revision}:${ordinal}`;
      const snapshot = { schema: "finance.invoice.line.v1", companyId: actor.companyId, invoiceDate: input.invoiceDate, invoiceNumber: input.invoiceNumber, kind: input.kind, taxCode: input.taxCode ?? null, party, netAccount: byId.get(line.accountId), taxAccount: line.taxAccountId ? byId.get(line.taxAccountId) : null, controlAccount: byId.get(controlAccount) };
      const values = [id, actor.companyId, invoiceId, revision, ordinal, line.description, line.quantityMillis, line.unitPriceMinor, line.taxBps, line.subtotalMinor, line.taxMinor, line.totalMinor, input.currency, line.accountId, line.taxAccountId, JSON.stringify(snapshot), plan.mode === "native_lines" ? "native_line" : "whole_document", actor.userId, now];
      return await financeInvoiceLines(db, actor.companyId).version(id, values);
    }));
    const snapshot = await snapshotMoney(db, actor, { sourceType: "finance_invoice", sourceId: invoiceId, amountKind: `draft:v${revision}`, transactionDate: input.invoiceDate, currency: input.currency, currencyScale: 2, amountUnits: String(amounts.subtotal + amounts.tax) });
    steps.push(snapshot.statement);
    return { guards, steps, mode: plan.mode, revision, count: rows.length };
}

export async function nativeInvoicePosting(db: SqlDatabase, actor: WorkContext, invoice: Record<string, string | number | null>, commandId: string) {
    const plan = await invoiceLineService(db, actor).posting(actor, invoice);
    if (!plan)
        return null;
    return { lines: plan.lines, guards: plan.guards.map(guard => guard.kind === 'revision' ? assertion(db, commandId, guard.label, INVOICE_EXISTS_PREFIX + " AND line_revision=?)", [guard.invoiceId, actor.companyId, guard.revision]) : guard.kind === 'account' ? assertion(db, commandId, guard.label, versionStatusGuard("finance_accounts"), [guard.id, actor.companyId, guard.version]) : assertion(db, commandId, guard.label, versionStatusGuard("finance_parties"), [guard.id, actor.companyId, guard.version])) };
}
function invoiceLineService(db: SqlDatabase, actor: WorkContext) {
    return createInvoiceLineRevision({ error: (...args) => new ApiError(...args), now: Date.now, repository: { accounts: async (companyId, ids) => (await db.prepare(`SELECT id,chart_id,code,name,category,currency,version,status FROM finance_accounts WHERE company_id=? AND id IN (${ids.map(() => "?").join(",")})`).bind(companyId, ...ids).all<Account>()).results, party: async (companyId, partyId) => db.prepare("SELECT id,code,name,kind,currency,tax_registration,version,status FROM finance_parties WHERE id=? AND company_id=? AND status='active'").bind(partyId, companyId).first<Record<string, unknown>>(), current: (_companyId, invoiceId, revision) => currentInvoiceLines(db, actor, invoiceId, revision), billingContent: async (companyId, invoiceId) => (await db.prepare("SELECT COALESCE(b.contract_content_json,c.content_json) content_json FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id WHERE b.invoice_id=? AND b.company_id=?").bind(invoiceId, companyId).first<{
                content_json: string;
            }>())?.content_json ?? null } });
}
