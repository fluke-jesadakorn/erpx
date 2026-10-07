import { z } from 'zod';
import { cancelCloseSchema, closeProofSchema, type FinanceRow, type CloseReconciliation } from '../domain/finance';
import type { VatCloseEvidence } from '../domain/close';
import { createFinancePolicy } from '../domain/finance-policy';
import { hasPermission } from '@/lib/permissions';
import type { FinanceError } from '../domain/invoice-line-revision';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
export type CancelCloseInput = z.infer<typeof cancelCloseSchema>;
export type CloseInput = z.infer<typeof closeProofSchema>;
export type CloseReadiness = {
    draftJournals: number;
    draftInvoices: number;
    unmatchedBank: number;
    openManufacturing: number;
    pendingCalculations: number;
};
export type CloseEvidence = {
    rows: CloseReconciliation[];
    vat: VatCloseEvidence | null;
    ready: boolean;
    reconciliationHash: string;
};
export interface FinanceClosePorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    digest(value: unknown): Promise<string>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    vatEvidence(actor: WorkContext, date: string): Promise<VatCloseEvidence | null>;
    expansionPending(companyId: string, start: string, end: string): Promise<{
        kind: string;
        id: string;
    }[]>;
    commitClose(facts: {
        actor: WorkContext;
        periodId: string;
        input: CloseInput;
        id: string;
        period: FinanceRow;
        reconciliation: CloseEvidence;
        version: number;
        now: number;
    }): Promise<Record<string,unknown>>;
    controlAccounts(facts: {
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    primaryBookAccount(facts: {
        actor: WorkContext;
        accountId: string;
        period: FinanceRow;
    }): Promise<{
        primary_account_id: string;
    } | null>;
    ledgerSigned(facts: {
        actor: WorkContext;
        accountId: string;
        period: FinanceRow;
    }): Promise<number>;
    bankClosing(facts: {
        actor: WorkContext;
        sourceAccountId: string;
        period: FinanceRow;
    }): Promise<FinanceRow | null>;
    subledgerClosing(facts: {
        actor: WorkContext;
        period: FinanceRow;
        account: FinanceRow;
        kind: CloseReconciliation["kind"];
        sourceAccountId: string;
    }): Promise<FinanceRow | null>;
    beginPeriod(facts: {
        periodId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    draftJournals(facts: {
        actor: WorkContext;
        periodId: string;
        period: FinanceRow;
    }): Promise<{
        count: number;
    } | null>;
    draftInvoices(facts: {
        actor: WorkContext;
        period: FinanceRow;
    }): Promise<{
        count: number;
    } | null>;
    unmatchedBank(facts: {
        actor: WorkContext;
        period: FinanceRow;
    }): Promise<{
        count: number;
    } | null>;
    openManufacturing(facts: {
        actor: WorkContext;
    }): Promise<{
        count: number;
    } | null>;
    pendingCalculations(facts: {
        actor: WorkContext;
        periodId: string;
    }): Promise<{
        count: number;
    } | null>;
    pendingTax(facts: {
        period: FinanceRow;
        actor: WorkContext;
    }): Promise<number>;
    pendingSupplierBills(facts: {
        actor: WorkContext;
        period: FinanceRow;
    }): Promise<number>;
    pendingSupplierBank(facts: {
        actor: WorkContext;
        period: FinanceRow;
    }): Promise<{
        count: number;
    } | null>;
    pendingRefunds(facts: {
        actor: WorkContext;
        period: FinanceRow;
    }): Promise<{
        returns: number;
        instructions: number;
    } | null>;
    commitBegin(facts: {
        actor: WorkContext;
        id: string;
        periodId: string;
        nextVersion: number;
        reconciliation: CloseEvidence;
        period: FinanceRow;
        readiness: CloseReadiness;
        checkSource: string;
        checkedAt: number;
    }): Promise<Record<string,unknown>>;
    cancelledPeriod(facts: {
        periodId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    commitCancel(facts: {
        actor: WorkContext;
        id: string;
        input: CancelCloseInput;
        periodId: string;
    }): Promise<Record<string,unknown>>;
    closedPeriod(facts: {
        periodId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    reopenedPeriod(facts: {
        periodId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    commitReopen(facts: {
        actor: WorkContext;
        id: string;
        input: CloseInput;
        periodId: string;
        version: number;
    }): Promise<Record<string,unknown>>;
}
export function createFinanceClose(ports: FinanceClosePorts) {
    const { requireFinanceRole } = createFinancePolicy(ports.error);
    async function calculateCloseReconciliations(actor: WorkContext, period: FinanceRow) {
        const accounts = await ports.controlAccounts({ actor });
        const rows: CloseReconciliation[] = [];
        for (const account of accounts) {
            const kind = String(account.control_kind) as CloseReconciliation["kind"];
            const accountId = String(account.id);
            const bookSource = await ports.primaryBookAccount({ actor, accountId, period });
            const sourceAccountId = bookSource?.primary_account_id ?? accountId;
            const ledgerSigned = Number(await ports.ledgerSigned({ actor, accountId, period }) ?? 0);
            const ledgerMinor = ledgerSigned === 0 ? 0 : kind === "payable" ? -ledgerSigned : ledgerSigned;
            if (kind === "bank") {
                const statement = await ports.bankClosing({ actor, sourceAccountId, period });
                const sourceMinor = statement ? Number(Number(statement.base_closing_minor) === 0 && Number(statement.closing_minor) !== 0 && Number(statement.closing_fx_rate_micros ?? 1000000) === 1000000 ? statement.closing_minor : statement.base_closing_minor) : 0;
                rows.push({ kind, accountId, accountCode: String(account.code), accountName: String(account.name), chartName: String(account.chart_name), currency: String(account.currency), ledgerMinor, sourceMinor, differenceMinor: ledgerMinor - sourceMinor, source: { sourceAccountId, statementId: statement?.id ?? null, statementNumber: statement?.statement_number ?? null, statementDate: statement?.statement_date ?? null, transactionClosingMinor: statement?.closing_minor ?? null, closingFxRateMicros: statement?.closing_fx_rate_micros ?? null, periodEnd: period.end_date, evidenceComplete: statement?.statement_date === period.end_date } });
                continue;
            }
            const source = await ports.subledgerClosing({ actor, period, account, kind, sourceAccountId });
            const sourceMinor = Number(source?.value ?? 0);
            rows.push({ kind, accountId, accountCode: String(account.code), accountName: String(account.name), chartName: String(account.chart_name), currency: String(account.currency), ledgerMinor, sourceMinor, differenceMinor: ledgerMinor - sourceMinor, source: { sourceAccountId, invoiceCount: Number(source?.invoice_count ?? 0), foreignOpenCount: Number(source?.foreign_open_count ?? 0), legacyDateCount: Number(source?.legacy_date_count ?? 0), asOf: period.end_date } });
        }
        const vat = await ports.vatEvidence(actor, String(period.end_date));
        const ready = (!vat || vat.ready) && rows.every(row => row.differenceMinor === 0 && Number(row.source.foreignOpenCount ?? 0) === 0 && Number(row.source.legacyDateCount ?? 0) === 0 && (row.kind !== "bank" || row.source.evidenceComplete === true));
        const reconciliationHash = await ports.digest({ rows: rows.map(row => ({ kind: row.kind, accountId: row.accountId, currency: row.currency, ledgerMinor: row.ledgerMinor, sourceMinor: row.sourceMinor, differenceMinor: row.differenceMinor, source: row.source })), vat });
        return { rows, vat, ready, reconciliationHash };
    }
    async function beginClose(actor: WorkContext, periodId: string, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.replay(actor, id, "finance.period.begin-close", periodId, { periodId });
        if (previous)
            return previous;
        const period = await ports.beginPeriod({ periodId, actor });
        if (!period)
            throw ports.error(404, "PERIOD_NOT_FOUND", "Accounting period not found.");
        if (period.state !== "open")
            throw ports.error(409, "INVALID_PERIOD_TRANSITION", "Only an open period can begin close.");
        const draftJournals = await ports.draftJournals({ actor, periodId, period });
        const draftInvoices = await ports.draftInvoices({ actor, period });
        const unmatchedBank = await ports.unmatchedBank({ actor, period });
        const openManufacturing = await ports.openManufacturing({ actor });
        const calculationPending = await ports.pendingCalculations({ actor, periodId });
        const expansionPending = await ports.expansionPending(actor.companyId, String(period.start_date), String(period.end_date));
        if (expansionPending.length)
            throw ports.error(409, "CLOSE_NOT_READY", `Resolve ${expansionPending.length} Finance source tasks before close: ${[...new Set(expansionPending.map(r => r.kind))].join(', ')}. Open Finance Today for the source workflows.`);
        const pendingTax = Number(await ports.pendingTax({ period, actor }) ?? 0);
        if (pendingTax)
            throw ports.error(409, "CLOSE_NOT_READY", `Review ${pendingTax} source VAT documents in the statutory ledger before beginning close.`);
        const billsPending = await ports.pendingSupplierBills({ actor, period });
        if (Number(billsPending))
            throw ports.error(409, 'CLOSE_NOT_READY', 'Resolve supplier invoice matching and AP posting before beginning close.');
        const supplierPending = await ports.pendingSupplierBank({ actor, period });
        if (Number(supplierPending?.count))
            throw ports.error(409, "CLOSE_NOT_READY", "Resolve exported supplier bank instructions before beginning close.");
        const refunds = await ports.pendingRefunds({ actor, period });
        if (Number(refunds?.returns) || Number(refunds?.instructions))
            throw ports.error(409, "CLOSE_NOT_READY", `Complete ${refunds?.returns ?? 0} return adjustments and resolve ${refunds?.instructions ?? 0} exported refund instructions before beginning close.`);
        const readiness = { draftJournals: draftJournals?.count ?? 0, draftInvoices: draftInvoices?.count ?? 0, unmatchedBank: unmatchedBank?.count ?? 0, openManufacturing: openManufacturing?.count ?? 0, pendingCalculations: calculationPending?.count ?? 0 };
        if (Object.values(readiness).some((value) => value > 0))
            throw ports.error(409, "CLOSE_NOT_READY", `Resolve ${readiness.draftJournals} journals, ${readiness.draftInvoices} invoices, ${readiness.unmatchedBank} bank rows, ${readiness.openManufacturing} manufacturing orders, and ${readiness.pendingCalculations} calculation runs before beginning close.`);
        const reconciliation = await calculateCloseReconciliations(actor, period);
        if (reconciliation.vat && !reconciliation.vat.ready)
            throw ports.error(409, "CLOSE_VAT_RECONCILIATION_FAILED", "Reconcile the reviewed statutory VAT register with every originating VAT account before beginning close.");
        if (!reconciliation.ready) {
            const failed = reconciliation.rows.filter(row => row.differenceMinor !== 0 || Number(row.source.foreignOpenCount ?? 0) > 0 || Number(row.source.legacyDateCount ?? 0) > 0 || (row.kind === "bank" && row.source.evidenceComplete !== true));
            throw ports.error(409, "CLOSE_RECONCILIATION_FAILED", `Reconcile, retain dated source journals or revalue ${failed.map(row => `${row.accountCode} ${row.accountName}`).join(", ")} before beginning close.`);
        }
        const nextVersion = Number(period.close_version) + 1, checkSource = await ports.digest({ periodId, closeVersion: period.close_version, readiness, reconciliation: reconciliation.reconciliationHash }), checkedAt = ports.now();
        return ports.commitBegin({ actor, id, periodId, nextVersion, reconciliation, period, readiness, checkSource, checkedAt });
    }
    async function cancelClose(actor: WorkContext, periodId: string, input: z.infer<typeof cancelCloseSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.replay(actor, id, "finance.period.cancel-close", periodId, input);
        if (previous)
            return previous;
        const period = await ports.cancelledPeriod({ periodId, actor });
        if (!period)
            throw ports.error(404, "PERIOD_NOT_FOUND", "Accounting period not found.");
        if (period.state !== "closing")
            throw ports.error(409, "PERIOD_NOT_CLOSING", "Only a prepared close can be cancelled.");
        if (period.close_started_by !== actor.userId && !hasPermission(actor, "finance.approve"))
            throw ports.error(403, "CLOSE_CANCELLATION_FORBIDDEN", "The preparer or a Finance approver must cancel the preparation.");
        if (Number(period.close_version) !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "The close changed. Refresh and review again.");
        return ports.commitCancel({ actor, id, input, periodId });
    }
    async function closePeriod(actor: WorkContext, periodId: string, raw: z.input<typeof closeProofSchema>, id: string) {
        const input = { ...raw, attachmentSetIds: raw.attachmentSetIds ?? [] };
        actor = ports.authorize(actor, "finance.write");
        requireFinanceRole(actor, true);
        await ports.ready(actor);
        const previous = await ports.replay(actor, id, "finance.period.close", periodId, input);
        if (previous)
            return previous;
        const period = await ports.closedPeriod({ periodId, actor });
        if (!period)
            throw ports.error(404, "PERIOD_NOT_FOUND", "Accounting period not found.");
        if (period.state !== "closing")
            throw ports.error(409, "INVALID_PERIOD_TRANSITION", "Begin the close before approving it.");
        if (String(period.close_started_by) === actor.userId)
            throw ports.error(409, "CLOSE_APPROVAL_SEPARATION", "A different person must approve the period close.");
        if (input.expectedVersion !== undefined && input.expectedVersion !== Number(period.close_version))
            throw ports.error(409, "VERSION_CONFLICT", "The close changed. Refresh and review again.");
        const reconciliation = await calculateCloseReconciliations(actor, period);
        if (!reconciliation.ready || reconciliation.reconciliationHash !== period.reconciliation_hash)
            throw ports.error(409, "CLOSE_RECONCILIATION_STALE", "Ledger, subledger, or bank evidence changed after preparation. Reopen the period and prepare the close again.");
        const now = ports.now(), version = Number(period.close_version);
        return ports.commitClose({ actor, periodId, input, id, period, reconciliation, version, now });
    }
    async function reopenPeriod(actor: WorkContext, periodId: string, raw: z.input<typeof closeProofSchema>, id: string) {
        const input = { ...raw, attachmentSetIds: raw.attachmentSetIds ?? [] };
        actor = ports.authorize(actor, "finance.write");
        requireFinanceRole(actor, true);
        await ports.ready(actor);
        const previous = await ports.replay(actor, id, "finance.period.reopen", periodId, input);
        if (previous)
            return previous;
        if (input.attachmentSetIds.length)
            throw ports.error(422, "ATTACHMENT_ACTION_UNSUPPORTED", "Close workpapers bind only to the close approval, not to reopening a period.");
        const period = await ports.reopenedPeriod({ periodId, actor });
        if (!period)
            throw ports.error(404, "PERIOD_NOT_CLOSED", "Closed period not found.");
        if (period.closed_by === actor.userId)
            throw ports.error(409, "CLOSE_APPROVAL_SEPARATION", "A different approver must reopen this period.");
        const version = input.expectedVersion ?? Number(period.close_version);
        return ports.commitReopen({ actor, id, input, periodId, version });
    }
    return { calculateCloseReconciliations, beginClose, cancelClose, closePeriod, reopenPeriod };
}

