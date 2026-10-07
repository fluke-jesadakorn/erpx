import { billingMonth } from "@/lib/billing";
import { nextBillingMonth } from "@/lib/billing-forecast";
import type { TargetPrice } from "@/lib/billing-target-pricing";
import type {SeatForecastBaseline,CurrentPriceRow,FuturePriceRow,UsageSnapshotRow,RetainedStorageTotalsRow} from '../domain/forecast';

export interface SeatForecastRepository{currentPrice(forecastMonth:string):Promise<{rows:CurrentPriceRow[];rowCount:number|null}>;
futurePrice(forecastMonth:string):Promise<{rows:FuturePriceRow[];rowCount:number|null}>;
usageSnapshot(holdingId:string,sourceMonth:string):Promise<{rows:UsageSnapshotRow[];rowCount:number|null}>;
retainedStorageTotals(holdingId:string,sourceMonth:string,snapshot_id:string):Promise<{rows:RetainedStorageTotalsRow[];rowCount:number|null}>;}
function safeNumber(value: string | number): number {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 0) throw new RangeError("INVALID_FORECAST_SOURCE");
  return number;
}

export async function readSeatForecastBaseline(
  c: SeatForecastRepository, holdingId: string, now: number,
): Promise<SeatForecastBaseline> {
  const sourceMonth = billingMonth(now), forecastMonth = nextBillingMonth(now);
  
    const currentPrice = await c.currentPrice(forecastMonth);
    const futurePrice = currentPrice.rows.length ? null : await c.futurePrice(forecastMonth);
    const snapshotResult = await c.usageSnapshot(holdingId,sourceMonth);
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
    const meters = snapshot ? (await c.retainedStorageTotals(holdingId,sourceMonth,snapshot.snapshot_id)).rows[0] : null;
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
}
