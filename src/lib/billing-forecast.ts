import { billingMonth, monthBounds } from "./billing";
import { rateTargetAi, rateTargetSeats, rateTargetStorage, type TargetPrice } from "./billing-target-pricing";

export function nextBillingMonth(now: number): string {
  return billingMonth(monthBounds(billingMonth(now)).end);
}

/** Decimal GB, to six places, without floating-point currency or byte arithmetic. */
export function parseForecastGb(value: string): bigint | null {
  const trimmed = value.trim();
  if (!/^\d{1,6}(?:\.\d{1,6})?$/.test(trimmed)) return null;
  const [whole, fraction = ""] = trimmed.split(".");
  return BigInt(whole) * BigInt(1_000_000_000) + BigInt(fraction.padEnd(6, "0") || "0") * BigInt(1_000);
}

export function parseForecastBaht(value: string): number | null {
  const trimmed = value.trim();
  if (!/^\d{1,9}(?:\.\d{1,2})?$/.test(trimmed)) return null;
  const [whole, fraction = ""] = trimmed.split(".");
  const satang = BigInt(whole) * BigInt(100) + BigInt(fraction.padEnd(2, "0") || "0");
  return satang <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(satang) : null;
}

export type ForecastAmounts = {
  seatSatang: number;
  includedOriginalGb: number;
  storageSatang: number;
  subtotalSatang: number;
  aiSatang: number | null;
  totalSatang: number | null;
};

/** Full next calendar month at the selected constant seat and original-file levels. */
export function calculateSeatForecast(input: {
  month: string;
  price: TargetPrice;
  seats: number;
  averageOriginalBytes: bigint;
  assumedProviderAiCostSatang: number | null;
}): ForecastAmounts {
  const { month, price, seats, averageOriginalBytes, assumedProviderAiCostSatang } = input;
  if (!Number.isSafeInteger(seats) || seats < 0 || seats > 10_000)
    throw new RangeError("INVALID_FORECAST_SEATS");
  if (averageOriginalBytes < BigInt(0)) throw new RangeError("INVALID_FORECAST_STORAGE");
  if (price.baseMonthlySatang !== 0 || price.seatTermsStatus !== "defined" ||
      price.includedSeats !== 0 || price.seatMonthlySatang === null ||
      price.storageAllowanceMode !== "per_paid_seat_time" || !price.ratingEnabled)
    throw new RangeError("FORECAST_PRICE_NOT_SEAT_ONLY");
  const { start, end } = monthBounds(month);
  const duration = BigInt(end - start);
  const intervals = Array.from({ length: seats }, () => [[start, end]] as [number, number][]);
  const seat = rateTargetSeats(price, month, start, intervals);
  const storageSatang = rateTargetStorage(price, month, start,
    averageOriginalBytes * duration, seat.activeSeatMilliseconds);
  const aiSatang = assumedProviderAiCostSatang === null ? null : rateTargetAi(price, assumedProviderAiCostSatang);
  const subtotalSatang = seat.amountSatang + storageSatang;
  if (!Number.isSafeInteger(subtotalSatang) ||
      (aiSatang !== null && !Number.isSafeInteger(subtotalSatang + aiSatang)))
    throw new RangeError("FORECAST_AMOUNT_OUT_OF_RANGE");
  return {
    seatSatang: seat.amountSatang,
    includedOriginalGb: seats * price.includedOriginalBytes / 1_000_000_000,
    storageSatang,
    subtotalSatang,
    aiSatang,
    totalSatang: aiSatang === null ? null : subtotalSatang + aiSatang,
  };
}
