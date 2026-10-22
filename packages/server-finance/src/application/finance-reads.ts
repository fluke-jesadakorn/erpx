import { z } from 'zod';
import { reportDateSchema, type ReportDates, type FinanceRow, type HistoricalAgingRow } from '@erpx/server-finance/domain/finance';
import type { LedgerPage } from '@erpx/server-finance/domain/ledger';
import { createFinancePolicy } from '@erpx/server-finance/domain/finance-policy';
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import type { PersistedInvoiceLine, FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
export type BankWorkspace = {
    accounts: FinanceRow[];
    statements: FinanceRow[];
    statementPage: LedgerPage | null;
    transactions: FinanceRow[];
    transactionPage: LedgerPage | null;
    payments: FinanceRow[];
    selectedTransaction: FinanceRow | null;
};
export interface FinanceReadPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    currentLines(companyId: string, id: string, revision: number): Promise<PersistedInvoiceLine[]>;
    reportCharts(actor: WorkContext): Promise<{
        id: string;
    }[]>;
    reportChart(actor: WorkContext, id: string): Promise<boolean>;
    reportPeriod(actor: WorkContext, id: string): Promise<boolean>;
    reportPeriodDates(actor: WorkContext, id: string): Promise<{
        start_date: string;
        end_date: string;
    } | null>;
    trialRows(facts: {
        actor: WorkContext;
        periodId?: string;
        chartId?: string;
        dates: ReportDates;
        options: {
            excludeYearClose?: boolean;
        };
    }): Promise<FinanceRow[]>;
    charts(facts: {
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    accounts(facts: {
        actor: WorkContext;
        chartId: string;
    }): Promise<FinanceRow[]>;
    invoices(facts: {
        actor: WorkContext;
        limit: number;
    }): Promise<FinanceRow[]>;
    workInvoices(facts: {
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    invoice(facts: {
        invoiceId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    invoiceAllocations(facts: {
        invoiceId: string;
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    lineHistory(facts: {
        invoiceId: string;
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    thaiDocuments(facts: {
        invoiceId: string;
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    payments(facts: {
        actor: WorkContext;
        limit: number;
    }): Promise<FinanceRow[]>;
    payment(facts: {
        paymentId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    paymentAllocations(facts: {
        paymentId: string;
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    paymentReversal(facts: {
        paymentId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    openPeriods(facts: {
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    confirmInvoices(facts: {
        actor: WorkContext;
        payment: FinanceRow;
    }): Promise<FinanceRow[]>;
    matchingBankRows(facts: {
        actor: WorkContext;
        payment: FinanceRow;
    }): Promise<FinanceRow[]>;
    fxAccounts(facts: {
        payment: FinanceRow;
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    workspaceAccounts(facts: {
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    workspaceParties(facts: {
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    workspacePeriods(facts: {
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    statement(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    statementRows(facts: {
        id: string;
        actor: WorkContext;
        page: number;
    }): Promise<FinanceRow[]>;
    agingBalances(facts: {
        actor: WorkContext;
        asOf: string;
        kind: "payable" | "receivable";
        chartId: string | undefined;
    }): Promise<HistoricalAgingRow[]>;
    listJournals(actor: WorkContext, query: {
        periodId?: string;
        status?: string;
    }): Promise<FinanceRow[]>;
    financeOverviewCounts(actor: WorkContext): Promise<{
        journals: number;
        posting: number;
        invoices: number;
        payments: number;
        receivable: number;
        payable: number;
        reconcile: number;
    }>;
    invoicesPage(actor: WorkContext, query: {
        q?: string;
        kind?: string;
        status?: string;
        queue?: string;
        cursor?: string;
        chartId?: string;
    }): Promise<LedgerPage>;
    bankWorkspace(actor: WorkContext, query?: {
        print?: string;
        transactionCursor?: string;
        statementCursor?: string;
        match?: string;
        new?: string;
        chart?: string;
        view?: string;
        q?: string;
    }): Promise<BankWorkspace>;
}
export function createFinanceReads(ports: FinanceReadPorts) {
    const { decimalToMinor } = createFinancePolicy(ports.error);
    async function listCharts(actor: WorkContext) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        return (await ports.charts({ actor }));
    }
    async function listAccounts(actor: WorkContext, chartId: string) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        return (await ports.accounts({ actor, chartId }));
    }
    async function listInvoices(actor: WorkContext, limit = 100) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        return (await ports.invoices({ actor, limit }));
    }
    async function workInvoiceRows(actor: WorkContext) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        return (await ports.workInvoices({ actor }));
    }
    async function getInvoice(actor: WorkContext, invoiceId: string) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        const invoice = await ports.invoice({ invoiceId, actor });
        if (!invoice)
            throw ports.error(404, "INVOICE_NOT_FOUND", "Invoice not found in this company.");
        const allocations = (await ports.invoiceAllocations({ invoiceId, actor }));
        const [lines, lineHistory, thaiDocuments] = await Promise.all([
            ports.currentLines(actor.companyId, invoiceId, Number(invoice.line_revision)),
            (await ports.lineHistory({ invoiceId, actor })),
            (await ports.thaiDocuments({ invoiceId, actor })),
        ]);
        return { invoice, allocations: allocations, lines, lineHistory: lineHistory, thaiDocuments: thaiDocuments };
    }
    async function listPayments(actor: WorkContext, limit = 100) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        return (await ports.payments({ actor, limit }));
    }
    async function getPayment(actor: WorkContext, paymentId: string) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        const payment = await ports.payment({ paymentId, actor });
        if (!payment)
            throw ports.error(404, "PAYMENT_NOT_FOUND", "Payment not found in this company.");
        const allocations = (await ports.paymentAllocations({ paymentId, actor }));
        const canConfirm = hasPermission(actor, "finance.pay") && !actor.branchId && !actor.departmentId;
        const [reversal, periods, invoices, bankTransactions, fxAccounts] = await Promise.all([
            ports.paymentReversal({ paymentId, actor }),
            (await ports.openPeriods({ actor })),
            canConfirm && payment.party_id && ["payment", "receipt"].includes(String(payment.kind)) ? (await ports.confirmInvoices({ actor, payment })) : Promise.resolve([] as FinanceRow[]),
            canConfirm && payment.bank_account ? (await ports.matchingBankRows({ actor, payment })) : Promise.resolve([] as FinanceRow[]),
            canConfirm && payment.bank_account ? (await ports.fxAccounts({ payment, actor })) : Promise.resolve([] as FinanceRow[]),
        ]);
        return { payment, allocations: allocations, reversal, periods: periods, invoices: invoices, bankTransactions: bankTransactions, fxAccounts: fxAccounts };
    }
    async function invoiceWorkspace(actor: WorkContext) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        const [charts, accounts, parties, invoices, periods] = await Promise.all([
            listCharts(actor), (await ports.workspaceAccounts({ actor })),
            (await ports.workspaceParties({ actor })), listInvoices(actor),
            (await ports.workspacePeriods({ actor })),
        ]);
        return { charts: charts as FinanceRow[], accounts: accounts, parties: parties, invoices, periods: periods };
    }
    async function bankStatementDetail(actor: WorkContext, id: string, page = 1) {
        actor = ports.authorize(actor, 'finance.read');
        await ports.ready(actor);
        const row = await ports.statement({ id, actor });
        if (!row)
            throw ports.error(404, 'BANK_STATEMENT_NOT_FOUND', 'Statement not found in this Company.');
        const rows = (await ports.statementRows({ id, actor, page }));
        return { row, rows, page, hasNext: rows.length > 30 };
    }
    async function aging(actor: WorkContext, kind: "payable" | "receivable", asOf: string, chartId?: string) {
        actor = ports.authorize(actor, "finance.read");
        if (!z.string().date().safeParse(asOf).success)
            throw ports.error(422, "INVALID_AS_OF", "Use a valid accounting cutoff date.");
        await ports.ready(actor);
        const rows = (await ports.agingBalances({ actor, asOf, kind, chartId }));
        return rows.map((row):HistoricalAgingRow & {outstanding_minor:number;base_outstanding_minor:number;outstanding:string;baseOutstanding:string;bucket:string;asOf:string} => {
            const days = Math.max(0, Math.floor((Date.parse(asOf) - Date.parse(String(row.due_date))) / 86400000));
            const bucket = days === 0 ? "current" : days <= 30 ? "1-30" : days <= 60 ? "31-60" : days <= 90 ? "61-90" : "90+";
            return { ...row, outstanding_minor: Number(row.historical_outstanding_minor), base_outstanding_minor: Number(row.historical_base_outstanding_minor),
                outstanding: minorToDecimal(Number(row.historical_outstanding_minor)), baseOutstanding: minorToDecimal(Number(row.historical_base_outstanding_minor)), bucket, asOf };
        });
    }
    async function trialBalance(actor: WorkContext, periodId?: string, chartId?: string, dates: ReportDates = {}, options: {
        excludeYearClose?: boolean;
    } = {}) {
        actor = ports.authorize(actor, "finance.read");
        dates = reportDateSchema.parse(dates);
        await ports.ready(actor);
        if (!chartId) {
            const charts = await ports.reportCharts(actor);
            if (charts.length > 1)
                throw ports.error(422, "CHART_REQUIRED", "Select a ledger explicitly when the Company has multiple ledgers, including archived ledgers.");
            chartId = charts[0]?.id;
        }
        if (chartId && !await ports.reportChart(actor, chartId))
            throw ports.error(404, "CHART_NOT_FOUND", "Ledger not found in this Company.");
        if (periodId && !await ports.reportPeriod(actor, periodId))
            throw ports.error(404, "PERIOD_NOT_FOUND", "Report period not found in this Company.");
        const rows = await ports.trialRows({ actor, periodId, chartId, dates, options });
        return rows.map((row): FinanceRow & {
            debit: string;
            credit: string;
            balance: string;
        } => ({ ...row, debit: minorToDecimal(Number(row.debit)), credit: minorToDecimal(Number(row.credit)), balance: minorToDecimal(Number(row.debit) - Number(row.credit)) }));
    }
    async function statements(actor: WorkContext, periodId?: string, chartId?: string, dates: ReportDates = {}) {
        actor = ports.authorize(actor, "finance.read");
        dates = reportDateSchema.parse(dates);
        if (periodId) {
            await ports.ready(actor);
            const period = await ports.reportPeriodDates(actor, periodId);
            if (!period)
                throw ports.error(404, "PERIOD_NOT_FOUND", "Report period not found in this Company.");
            dates = { startDate: period.start_date, endDate: period.end_date };
        }
        const [tb, balance, income] = await Promise.all([trialBalance(actor, undefined, chartId, dates), trialBalance(actor, undefined, chartId, { endDate: dates.endDate }), trialBalance(actor, undefined, chartId, dates, { excludeYearClose: true })]);
        const category = (rows: typeof tb, name: string) => rows.filter(row => row.category === name).reduce((total, row) => total + decimalToMinor(String(row.balance)), 0);
        const recordedEquity = -category(balance, "equity"), unclosedProfit = -category(balance, "revenue") - category(balance, "other_income") - category(balance, "expense") - category(balance, "other_expense");
        return { dates, balanceSheet: { assets: minorToDecimal(category(balance, "asset")), liabilities: minorToDecimal(-category(balance, "liability")), equity: minorToDecimal(recordedEquity + unclosedProfit), recordedEquity: minorToDecimal(recordedEquity), unclosedProfit: minorToDecimal(unclosedProfit) },
            profitAndLoss: { revenue: minorToDecimal(-category(income, "revenue")), expenses: minorToDecimal(category(income, "expense")), otherIncome: minorToDecimal(-category(income, "other_income")), otherExpenses: minorToDecimal(category(income, "other_expense")), netProfit: minorToDecimal(-category(income, "revenue") - category(income, "expense") - category(income, "other_income") - category(income, "other_expense")) }, trialBalance: tb };
    }
    async function listJournals(actor: WorkContext, query: {
        periodId?: string;
        status?: string;
    }) { actor = ports.authorize(actor, 'finance.read'); await ports.ready(actor); return ports.listJournals(actor, query); }
    async function financeOverviewCounts(actor: WorkContext) { actor = ports.authorize(actor, 'finance.read'); await ports.ready(actor); return ports.financeOverviewCounts(actor); }
    async function invoicesPage(actor: WorkContext, query: {
        q?: string;
        kind?: string;
        status?: string;
        queue?: string;
        cursor?: string;
        chartId?: string;
    }) { actor = ports.authorize(actor, 'finance.read'); await ports.ready(actor); return ports.invoicesPage(actor, query); }
    async function bankWorkspace(actor: WorkContext, query?: {
        print?: string;
        transactionCursor?: string;
        statementCursor?: string;
        match?: string;
        new?: string;
        chart?: string;
        view?: string;
        q?: string;
    }) { actor = ports.authorize(actor, 'finance.read'); await ports.ready(actor); const result = await ports.bankWorkspace(actor, query); if (query?.match && !result.selectedTransaction)
        throw ports.error(404, 'BANK_TRANSACTION_NOT_FOUND', 'Bank movement not found in this Company.'); return result; }
    return { listCharts, listAccounts, listInvoices, workInvoiceRows, getInvoice, listPayments, getPayment, invoiceWorkspace, bankStatementDetail, aging, trialBalance, statements, listJournals, financeOverviewCounts, invoicesPage, bankWorkspace };
}
