import type { CloseReconciliation, FinanceRow } from '@erpx/server-finance/domain/finance';
import type { VatCloseEvidence } from '@erpx/server-finance/domain/close';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
/** Read-only close reconciliation. Native period-close owns begin/close/cancel/reopen and their writes. */
export type CloseEvidence = {
    rows: CloseReconciliation[];
    vat: VatCloseEvidence | null;
    ready: boolean;
    reconciliationHash: string;
};
export interface FinanceCloseReadPorts {
    digest(value: unknown): Promise<string>;
    vatEvidence(actor: WorkContext, date: string): Promise<VatCloseEvidence | null>;
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
}
export function createFinanceCloseRead(ports: FinanceCloseReadPorts) {
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
    return { calculateCloseReconciliations };
}
