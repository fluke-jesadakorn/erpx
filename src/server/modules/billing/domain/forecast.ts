

import type { TargetPrice } from "@/lib/billing-target-pricing";
export type SeatForecastBaseline = {
  generatedAt: number;
  forecastMonth: string;
  sourceMonth: string;
  model: "seat_what_if";
  price: TargetPrice | null;
  priceEffectiveForMonth: boolean;
  seats: { count: number | null; observedAt: number | null };
  storage: { originalBytes: number | null; observedAt: number | null;
    coverage: "measured" | "partial" | "unavailable" };
};
export type PriceRow = {
  id: string; effective_month: string; base_monthly_satang: string;
  included_original_bytes: string; storage_satang_per_gb_month: string;
  ai_markup_percent: number; seat_terms_status: "defined";
  included_seats: number; seat_monthly_satang: string;
  storage_allowance_mode: "per_paid_seat_time"; rating_enabled: boolean;
};
export type CurrentPriceRow=PriceRow;

export type FuturePriceRow=PriceRow;

export type UsageSnapshotRow={snapshot_id:string;refreshed_at:string;source_count:number;
        storage_coverage:"measured"|"partial";active_seat_count:number|null;
        active_seat_observed_at:string|null};

export type RetainedStorageTotalsRow={row_count:string;active_original_bytes:string};

