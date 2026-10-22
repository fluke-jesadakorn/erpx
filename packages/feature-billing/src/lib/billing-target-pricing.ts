import { monthBounds } from "@erpx/feature-billing/lib/billing";

/** Pure, integer-only rating math for the proposed Holding usage offering. */
export type TargetPrice = Readonly<{
  id: string;
  effectiveMonth: string;
  baseMonthlySatang: number;
  includedOriginalBytes: number;
  storageSatangPerGbMonth: number;
  aiMarkupPercent: number;
  seatTermsStatus: "undefined" | "defined";
  includedSeats: number | null;
  seatMonthlySatang: number | null;
  storageAllowanceMode: "holding_flat" | "per_paid_seat_time";
  ratingEnabled: boolean;
}>;

const GB = BigInt(1_000_000_000);
const DAY = 86_400_000;

function integer(value: number, name: string): bigint {
  if (!Number.isSafeInteger(value) || value < 0)
    throw new RangeError(`INVALID_${name}`);
  return BigInt(value);
}

function safe(value: bigint): number {
  if (value < BigInt(0) || value > BigInt(Number.MAX_SAFE_INTEGER))
    throw new RangeError("TARGET_CHARGE_OUT_OF_RANGE");
  return Number(value);
}

/** Nonnegative rational, rounded half up exactly once to whole satang. */
export function roundSatang(numerator: bigint, denominator: bigint): number {
  if (numerator < BigInt(0) || denominator <= BigInt(0))
    throw new RangeError("INVALID_TARGET_RATE");
  return safe((numerator + denominator / BigInt(2)) / denominator);
}

export function targetServiceWindow(month: string, startedAt: number): {
  start: number;
  exactStart: number;
  end: number;
  serviceDays: number;
  monthDays: number;
} {
  const bounds = monthBounds(month);
  if (!Number.isSafeInteger(startedAt)) throw new RangeError("INVALID_SERVICE_START");
  if (startedAt >= bounds.end) throw new RangeError("SERVICE_NOT_STARTED");
  // Monthly service begins on its first Bangkok calendar day, like the legacy
  // Hosted offering. A sign-up at 23:59 still receives that day's service.
  const start = Math.max(
    bounds.start,
    Math.floor((startedAt + 7 * 3_600_000) / DAY) * DAY - 7 * 3_600_000,
  );
  return {
    start,
    exactStart: Math.max(bounds.start, startedAt),
    end: bounds.end,
    serviceDays: (bounds.end - start) / DAY,
    monthDays: (bounds.end - bounds.start) / DAY,
  };
}

export function rateTargetBase(price: TargetPrice, month: string, startedAt: number): number {
  const days = targetServiceWindow(month, startedAt);
  return roundSatang(
    integer(price.baseMonthlySatang, "BASE") * BigInt(days.serviceDays),
    BigInt(days.monthDays),
  );
}

/**
 * `originalByteMilliseconds` is the sum of original-file size × lifetime,
 * clipped to the actual service window. One original is counted once even
 * when linked to several jobs. A flat legacy allowance uses service time;
 * the seat-only offer earns 2 GB for each paid seat-month of time. Excess is
 * divided by the full calendar month to prorate the charge.
 */
export function rateTargetStorage(
  price: TargetPrice,
  month: string,
  startedAt: number,
  originalByteMilliseconds: bigint,
  paidSeatMilliseconds: bigint = BigInt(0),
): number {
  const window = targetServiceWindow(month, startedAt);
  if (originalByteMilliseconds < BigInt(0)) throw new RangeError("INVALID_BYTE_TIME");
  if (paidSeatMilliseconds < BigInt(0)) throw new RangeError("INVALID_SEAT_TIME");
  const includedByteMilliseconds =
    integer(price.includedOriginalBytes, "INCLUDED_STORAGE") *
    (price.storageAllowanceMode === "per_paid_seat_time"
      ? paidSeatMilliseconds : BigInt(window.end - window.exactStart));
  const excess = originalByteMilliseconds > includedByteMilliseconds
    ? originalByteMilliseconds - includedByteMilliseconds
    : BigInt(0);
  return roundSatang(
    excess * integer(price.storageSatangPerGbMonth, "STORAGE_RATE"),
    GB * BigInt(monthBounds(month).end - monthBounds(month).start),
  );
}

/** Provider cost must already be matched and verified in THB satang. */
export function rateTargetAi(price: TargetPrice, verifiedProviderCostSatang: number): number {
  const markup = integer(price.aiMarkupPercent, "AI_MARKUP");
  if (markup > BigInt(100)) throw new RangeError("INVALID_AI_MARKUP");
  return roundSatang(
    integer(verifiedProviderCostSatang, "PROVIDER_COST") * (BigInt(100) + markup),
    BigInt(100),
  );
}

export type SeatInterval = readonly [number, number];

/** Union overlapping access intervals for one human across all assignments. */
export function unionSeatIntervals(intervals: readonly SeatInterval[]): SeatInterval[] {
  const sorted = intervals.map(([start, end]) => {
    if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || end <= start)
      throw new RangeError("INVALID_SEAT_INTERVAL");
    return [start, end] as SeatInterval;
  }).sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const merged: SeatInterval[] = [];
  for (const [start, end] of sorted) {
    const last = merged.at(-1);
    if (last && start <= last[1]) merged[merged.length - 1] = [last[0], Math.max(last[1], end)];
    else merged.push([start, end]);
  }
  return merged;
}

/** Count one active human once per instant, even across many branches. */
export function rateTargetSeats(
  price: TargetPrice,
  month: string,
  startedAt: number,
  users: readonly (readonly SeatInterval[])[],
): { activeSeatMilliseconds: bigint; amountSatang: number } {
  if (price.seatTermsStatus !== "defined" || price.includedSeats !== 0 ||
      price.seatMonthlySatang === null)
    throw new RangeError("SEAT_PRICE_UNDEFINED");
  const window = targetServiceWindow(month, startedAt);
  let activeSeatMilliseconds = BigInt(0);
  for (const intervals of users) {
    for (const [start, end] of unionSeatIntervals(intervals)) {
      const duration = Math.max(0, Math.min(end, window.end) - Math.max(start, window.exactStart));
      activeSeatMilliseconds += BigInt(duration);
    }
  }
  return {
    activeSeatMilliseconds,
    amountSatang: roundSatang(
      activeSeatMilliseconds * integer(price.seatMonthlySatang, "SEAT_RATE"),
      BigInt(monthBounds(month).end - monthBounds(month).start),
    ),
  };
}
