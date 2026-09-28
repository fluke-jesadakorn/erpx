import { expect, it } from "vitest";
import {
  rateTargetAi,
  rateTargetBase,
  rateTargetStorage,
  targetServiceWindow,
  type TargetPrice,
} from "../src/lib/billing-target-pricing";

const price: TargetPrice = {
  id: "holding-usage-2026-10-v1",
  effectiveMonth: "2026-10",
  baseMonthlySatang: 149_000,
  includedOriginalBytes: 20_000_000_000,
  storageSatangPerGbMonth: 500,
  aiMarkupPercent: 10,
  seatTermsStatus: "undefined",
  includedSeats: null,
  seatMonthlySatang: null,
  storageAllowanceMode: "holding_flat",
  ratingEnabled: false,
};
const day = BigInt(86_400_000);
const gb = BigInt(1_000_000_000);

it("prorates the Holding fee by Bangkok calendar day, including leap February", () => {
  expect(rateTargetBase(price, "2026-10", Date.parse("2026-10-01T23:59:00+07:00"))).toBe(149_000);
  expect(rateTargetBase(price, "2026-10", Date.parse("2026-10-16T23:59:00+07:00"))).toBe(76_903);
  expect(targetServiceWindow("2024-02", Date.parse("2024-02-29T23:59:00+07:00")).monthDays).toBe(29);
  expect(rateTargetBase(price, "2024-02", Date.parse("2024-02-29T23:59:00+07:00"))).toBe(5_138);
});

it("charges exact original-file byte-time once, excludes mid-month deletion, and prorates first-month allowance", () => {
  const start = Date.parse("2026-10-01T00:00:00+07:00");
  expect(rateTargetStorage(price, "2026-10", start, BigInt(28) * gb * BigInt(31) * day)).toBe(4_000);
  expect(rateTargetStorage(price, "2026-10", start, BigInt(28) * gb * BigInt(15) * day)).toBe(0);
  const mid = Date.parse("2026-10-16T00:00:00+07:00");
  expect(rateTargetStorage(price, "2026-10", mid, BigInt(28) * gb * BigInt(16) * day)).toBe(2_065);
  expect(rateTargetStorage(price, "2026-10", mid, BigInt(20) * gb * BigInt(16) * day)).toBe(0);
});

it("marks up only verified satang cost and rounds per request deterministically", () => {
  expect(rateTargetAi(price, 7_500)).toBe(8_250);
  expect(rateTargetAi(price, 5)).toBe(6);
  expect(rateTargetAi(price, 4)).toBe(4);
  expect(() => rateTargetAi(price, -1)).toThrow();
});

it("charges 499 baht per full active named seat and unions overlapping branch access", async () => {
  const { rateTargetSeats } = await import("../src/lib/billing-target-pricing");
  const seatPrice: TargetPrice = { ...price, id: "holding-usage-2026-12-v3",
    effectiveMonth: "2026-12", baseMonthlySatang: 0,
    includedOriginalBytes: 2_000_000_000,
    seatTermsStatus: "defined", includedSeats: 0,
    seatMonthlySatang: 49_900, storageAllowanceMode: "per_paid_seat_time",
    ratingEnabled: true };
  const start = Date.parse("2026-12-01T00:00:00+07:00");
  const end = Date.parse("2027-01-01T00:00:00+07:00");
  const full = rateTargetSeats(seatPrice, "2026-12", start,
    [[[start, end], [start + 1000, end - 1000]]]);
  expect(full.amountSatang).toBe(49_900);
  expect(full.activeSeatMilliseconds).toBe(BigInt(end - start));
  expect(rateTargetStorage(seatPrice, "2026-12", start,
    BigInt(2) * gb * BigInt(end - start), full.activeSeatMilliseconds)).toBe(0);
  expect(rateTargetStorage(seatPrice, "2026-12", start,
    BigInt(3) * gb * BigInt(end - start), full.activeSeatMilliseconds)).toBe(500);
  expect(rateTargetStorage(seatPrice, "2026-12", start,
    BigInt(2) * gb * BigInt(end - start), BigInt(0))).toBe(1_000);
  const midday = start + 12 * 3_600_000;
  const partial = rateTargetSeats(seatPrice, "2026-12", midday, [[[start, end]]]);
  expect(partial.activeSeatMilliseconds).toBe(BigInt(end - midday));
  expect(partial.amountSatang).toBeLessThan(49_900);
});
