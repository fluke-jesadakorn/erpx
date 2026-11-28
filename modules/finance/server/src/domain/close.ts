import type { FinanceRow } from '@erpx/finance-server/domain/finance';
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
