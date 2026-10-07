import type { FinanceRow } from './finance';
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
