import type { FinanceRow } from '@erpx/server-finance/domain/finance';
export type VatCloseEvidence = {
    profileId: string;
    chartId: string;
    effectiveDate: string;
    endDate: string;
    rows: FinanceRow[];
    missing: number;
    pendingCorrections: number;
    ready: boolean;
};
