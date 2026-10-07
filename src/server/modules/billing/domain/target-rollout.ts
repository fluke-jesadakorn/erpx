
export type TargetMarginReport = {
  month: string;
  ratedRevenueSatang: number | null;
  verifiedProviderAiCostSatang: number | null;
  verifiedOperatingCostSatang: number | null;
  grossMarginBps: number | null;
  missingCosts: string[];
};
export type TargetRolloutReadiness = {
  holdingId: string;
  effectiveMonth: string;
  minimumMarginBps: number;
  shadowMonths: [string, string];
  margins: TargetMarginReport[];
  capacity: {
    reviewed: boolean;
    projectedOriginalBytes: number | null;
    primaryAvailableBytes: number | null;
    backupAvailableBytes: number | null;
    reservedBytes: number | null;
    companyLimitBytes: number | null;
  };
  ready: boolean;
  blockers: string[];
};
export type EffectivePriceRow={
    id: string; included_original_bytes: string; seat_terms_status: string; rating_enabled: boolean;
  };

export type RatedShadowPeriodRow={
      state: string; mode: string; total_satang: string | null; price_id: string;
      meter_snapshot_id: string; lines_json: string;
    };

export type UsageSnapshotRow={snapshot_id:string};

export type AvailableOperatingCostsRow={
      category: string; amount_satang: string; source_state: string;
    };

export type CapacityReviewRow={
    projected_original_bytes: string; primary_available_bytes: string;
    backup_available_bytes: string; reserved_bytes: string;
    company_limit_bytes: string; source_state: string;
  };

