import "server-only";

import { billingMonth } from "@/lib/billing";
import { nextBillingMonth } from "@/lib/billing-forecast";
import type { TargetPrice } from "@/lib/billing-target-pricing";
import type { PostgresDatabase } from "./runtime/postgres";

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

type PriceRow = {
  id: string; effective_month: string; base_monthly_satang: string;
  included_original_bytes: string; storage_satang_per_gb_month: string;
  ai_markup_percent: number; seat_terms_status: "defined";
  included_seats: number; seat_monthly_satang: string;
  storage_allowance_mode: "per_paid_seat_time"; rating_enabled: boolean;
};

function safeNumber(value: string | number): number {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 0) throw new RangeError("INVALID_FORECAST_SOURCE");
  return number;
}

export async function readSeatForecastBaseline(
  db: PostgresDatabase, holdingId: string, now = Date.now(),
): Promise<SeatForecastBaseline> {
  const sourceMonth = billingMonth(now), forecastMonth = nextBillingMonth(now);
  const client = await db.pool.connect();
  try {
    const currentPrice = await client.query<PriceRow>(`SELECT * FROM billing_target_prices
        WHERE rating_enabled=TRUE AND seat_terms_status='defined' AND base_monthly_satang=0
          AND included_seats=0 AND seat_monthly_satang IS NOT NULL
          AND storage_allowance_mode='per_paid_seat_time' AND effective_month<=$1
        ORDER BY effective_month DESC,id DESC LIMIT 1`, [forecastMonth]);
    const futurePrice = currentPrice.rows.length ? null : await client.query<PriceRow>(`SELECT * FROM billing_target_prices
        WHERE rating_enabled=TRUE AND seat_terms_status='defined' AND base_monthly_satang=0
          AND included_seats=0 AND seat_monthly_satang IS NOT NULL
          AND storage_allowance_mode='per_paid_seat_time' AND effective_month>$1
        ORDER BY effective_month ASC,id ASC LIMIT 1`, [forecastMonth]);
    const snapshotResult = await client.query<{snapshot_id:string;refreshed_at:string;source_count:number;
        storage_coverage:"measured"|"partial";active_seat_count:number|null;
        active_seat_observed_at:string|null}>(`SELECT snapshot_id,refreshed_at,source_count,storage_coverage,
          active_seat_count,active_seat_observed_at FROM billing_usage_snapshots
          WHERE holding_id=$1 AND month=$2`, [holdingId,sourceMonth]);
    const row = currentPrice.rows[0] ?? futurePrice?.rows[0];
    const price: TargetPrice | null = row ? {
      id: row.id, effectiveMonth: row.effective_month,
      baseMonthlySatang: safeNumber(row.base_monthly_satang),
      includedOriginalBytes: safeNumber(row.included_original_bytes),
      storageSatangPerGbMonth: safeNumber(row.storage_satang_per_gb_month),
      aiMarkupPercent: safeNumber(row.ai_markup_percent),
      seatTermsStatus: row.seat_terms_status, includedSeats: row.included_seats,
      seatMonthlySatang: safeNumber(row.seat_monthly_satang),
      storageAllowanceMode: row.storage_allowance_mode, ratingEnabled: row.rating_enabled,
    } : null;
    const snapshot = snapshotResult.rows[0];
    const meters = snapshot ? (await client.query<{row_count:string;active_original_bytes:string}>(
      `SELECT COUNT(*)::text row_count,
        COALESCE(SUM(quantity) FILTER (WHERE meter_kind='storage' AND active_at_period_end),0)::text active_original_bytes
       FROM billing_usage_meters WHERE holding_id=$1 AND month=$2 AND snapshot_id=$3`,
      [holdingId,sourceMonth,snapshot.snapshot_id],
    )).rows[0] : null;
    const storageValid = Boolean(snapshot && meters && safeNumber(meters.row_count) === snapshot.source_count);
    return {
      generatedAt: now, forecastMonth, sourceMonth, model: "seat_what_if", price,
      priceEffectiveForMonth: Boolean(price && price.effectiveMonth <= forecastMonth),
      seats: snapshot?.active_seat_count !== null && snapshot?.active_seat_count !== undefined &&
        snapshot.active_seat_observed_at !== null
        ? { count: safeNumber(snapshot.active_seat_count), observedAt: safeNumber(snapshot.active_seat_observed_at) }
        : { count: null, observedAt: null },
      storage: storageValid && snapshot && meters
        ? { originalBytes: safeNumber(meters.active_original_bytes),
            observedAt: safeNumber(snapshot.refreshed_at), coverage: snapshot.storage_coverage }
        : { originalBytes: null, observedAt: null, coverage: "unavailable" },
    };
  } finally {
    client.release();
  }
}
