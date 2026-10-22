import type { LedgerRow, LedgerQuery, LedgerPage, LedgerStatements, LedgerReportDates } from "@erpx/server-finance/domain/ledger";
import type { FinanceError } from "@erpx/server-finance/domain/invoice-line-revision";
export interface LedgerPorts {
    error: FinanceError;
    charts(): Promise<LedgerRow[]>;
    profiles(): Promise<LedgerRow[]>;
    accounts(chartId: string | number | null): Promise<LedgerRow[]>;
    periods(): Promise<LedgerRow[]>;
    journals(chartId: string | number | null, query: LedgerQuery | undefined, paginated: boolean): Promise<{
        results: LedgerRow[];
        page: LedgerPage | null;
    }>;
    statements(periodId: string | undefined, chartId: string, dates: LedgerReportDates | undefined): Promise<LedgerStatements>;
    dimensions(): Promise<LedgerRow[]>;
    dimensionValues(): Promise<LedgerRow[]>;
    workJournals(): Promise<LedgerRow[]>;
    journal(id: string): Promise<LedgerRow | null>;
    journalLines(id: string): Promise<LedgerRow[]>;
    draftHistory(id: string): Promise<LedgerRow[]>;
}
export function createLedgerService(ports: LedgerPorts) {
    async function ledgerWorkspace(chartId?: string, query?: LedgerQuery) {
        const charts = await ports.charts();
        if (chartId && !charts.some(c => c.id === chartId))
            throw ports.error(404, "CHART_NOT_FOUND", "Ledger not found in this Company.");
        const profiles = chartId ? [] : await ports.profiles(), statutory = profiles.find(p => p.status === "approved" && (!query?.reportDates?.endDate || String(p.effective_date) <= query.reportDates.endDate));
        const chart = charts.find(c => c.id === (chartId ?? statutory?.statutory_chart_id)) ?? charts.find(c => c.name === "Operations ledger") ?? charts[0];
        if (!chart)
            throw ports.error(409, "CHART_REQUIRED", "Create a chart of accounts first.");
        const [accounts, periods, journals, report] = await Promise.all([ports.accounts(chart.id), ports.periods(), ports.journals(chart.id, query, !!query && query.print !== "1"), ports.statements(query?.periodId, String(chart.id), query?.reportDates)]);
        const dimensions = await ports.dimensions(), dimensionValues = await ports.dimensionValues();
        return { charts, chart, accounts, periods, journals: journals.results, journalPage: journals.page, report, dimensions, dimensionValues };
    }
    async function workJournalRows() { await ports.charts(); return ports.workJournals(); }
    async function journalDetail(id: string) { await ports.charts(); const journal = await ports.journal(id); if (!journal)
        throw ports.error(404, "JOURNAL_NOT_FOUND", "Journal not found."); const lines = await ports.journalLines(id), history = await ports.draftHistory(id); return { journal, lines, history }; }
    return { ledgerWorkspace, workJournalRows, journalDetail };
}
