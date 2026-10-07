export type LedgerRow = Record<string, string | number | null>;
export type LedgerReportDates = {
    startDate?: string;
    endDate?: string;
};
export type LedgerQuery = {
    status?: string;
    cursor?: string;
    print?: string;
    reportDates?: LedgerReportDates;
    periodId?: string;
};
export type LedgerPage = {
    items: (LedgerRow & {
        id: string;
        page_key: string;
    })[];
    nextCursor: string | null;
    previousCursor: string | null;
    hasMore: boolean;
};
export type LedgerStatements = {
    dates: LedgerReportDates;
    balanceSheet: {
        assets: string;
        liabilities: string;
        equity: string;
        recordedEquity: string;
        unclosedProfit: string;
    };
    profitAndLoss: {
        revenue: string;
        expenses: string;
        otherIncome: string;
        otherExpenses: string;
        netProfit: string;
    };
    trialBalance: (LedgerRow & {
        debit: string;
        credit: string;
        balance: string;
    })[];
};
