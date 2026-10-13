import { beforeAll, expect, it } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { calculateSeatForecast, nextBillingMonth, parseForecastBaht, parseForecastGb } from "../../../src/lib/billing/billing-forecast";
import { readSeatForecastBaseline } from "../../../src/server/modules/billing/public/billing-forecast";
import type { TargetPrice } from "../../../src/lib/billing/billing-target-pricing";

const db = env.CONTROL_DB;
beforeAll(() => applyMigrations(db, env.CONTROL_TEST_MIGRATIONS));

const price: TargetPrice = {
  id: "holding-usage-2026-12-v3", effectiveMonth: "2026-12",
  baseMonthlySatang: 0, includedOriginalBytes: 2_000_000_000,
  storageSatangPerGbMonth: 500, aiMarkupPercent: 10,
  seatTermsStatus: "defined", includedSeats: 0, seatMonthlySatang: 49_900,
  storageAllowanceMode: "per_paid_seat_time", ratingEnabled: true,
};

it("forecasts a full month from real seat counts without inventing an AI total", () => {
  const result = calculateSeatForecast({ month: "2026-10", price, seats: 282,
    averageOriginalBytes: BigInt(2_636_112), assumedProviderAiCostSatang: null });
  expect(result).toEqual({ seatSatang: 14_071_800, includedOriginalGb: 564,
    storageSatang: 0, subtotalSatang: 14_071_800, aiSatang: null, totalSatang: null });
  const withNoAi = calculateSeatForecast({ month: "2026-10", price, seats: 282,
    averageOriginalBytes: BigInt(2_636_112), assumedProviderAiCostSatang: 0 });
  expect(withNoAi.totalSatang).toBe(14_071_800);
});

it("charges excess storage and assumed Platform-funded AI with exact satang rounding", () => {
  const result = calculateSeatForecast({ month: "2026-10", price, seats: 1,
    averageOriginalBytes: BigInt(2_500_000_000), assumedProviderAiCostSatang: 10_005 });
  expect(result.seatSatang).toBe(49_900);
  expect(result.storageSatang).toBe(250);
  expect(result.aiSatang).toBe(11_006);
  expect(result.totalSatang).toBe(61_156);
  expect(calculateSeatForecast({ month: "2026-10", price, seats: 0,
    averageOriginalBytes: BigInt(1_000_000_000), assumedProviderAiCostSatang: 0 }).storageSatang).toBe(500);
});

it("uses Bangkok next-month boundaries and full-month pricing in short and leap Februaries", () => {
  expect(nextBillingMonth(Date.parse("2026-09-30T16:59:59Z"))).toBe("2026-10");
  expect(nextBillingMonth(Date.parse("2026-09-30T17:00:00Z"))).toBe("2026-11");
  for (const month of ["2027-02", "2028-02"])
    expect(calculateSeatForecast({ month, price, seats: 1,
      averageOriginalBytes: BigInt(2_000_000_000), assumedProviderAiCostSatang: 0 }).totalSatang).toBe(49_900);
});

it("requires explicit, valid scenario inputs", () => {
  expect(parseForecastGb("")).toBeNull();
  expect(parseForecastGb("2.000001")).toBe(BigInt(2_000_001_000));
  expect(parseForecastGb("2.0000001")).toBeNull();
  expect(parseForecastBaht("")).toBeNull();
  expect(parseForecastBaht("0")).toBe(0);
  expect(parseForecastBaht("100.05")).toBe(10_005);
  expect(parseForecastBaht("-1")).toBeNull();
  expect(() => calculateSeatForecast({ month: "2026-10", price, seats: -1,
    averageOriginalBytes: BigInt(0), assumedProviderAiCostSatang: null })).toThrow("INVALID_FORECAST_SEATS");
});

it("reads the current projected users and original files without creating a bill", async () => {
  const holdingId = `forecast_${crypto.randomUUID()}`;
  const now = Date.parse("2026-09-28T04:00:00Z");
  await db.pool.query("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES($1,$1,'Forecast test',$2,$3,$4,'active')",
    [holdingId, `W${holdingId}`, `R${holdingId}`, `F${holdingId}`]);
  await db.pool.query(`INSERT INTO billing_usage_snapshots
    (holding_id,month,snapshot_id,refreshed_at,source_count,storage_coverage,active_seat_count,active_seat_observed_at)
    VALUES($1,'2026-09','forecast-snapshot',$2,1,'partial',282,$2)`, [holdingId, now]);
  await db.pool.query(`INSERT INTO billing_usage_meters
    (holding_id,month,source_key,company_id,company_name,meter_kind,funding_source,pricing_status,
     quantity,input_tokens,output_tokens,average_daily_bytes,active_at_period_end,snapshot_id)
    VALUES($1,'2026-09',$2,'company','Company','storage','unknown','unpriced',2636112,0,0,2636112,TRUE,'forecast-snapshot')`,
    [holdingId, "a".repeat(64)]);
  const first = await readSeatForecastBaseline(db, holdingId, now);
  expect(first.forecastMonth).toBe("2026-10");
  expect(first.price?.id).toBe("holding-usage-2026-12-v3");
  expect(first.priceEffectiveForMonth).toBe(false);
  expect(first.seats).toEqual({count:282,observedAt:now});
  expect(first.storage).toEqual({originalBytes:2_636_112,observedAt:now,coverage:"partial"});
  expect((await db.pool.query("SELECT COUNT(*)::int n FROM billing_target_assignments WHERE holding_id=$1", [holdingId])).rows[0].n).toBe(0);
  expect((await db.pool.query("SELECT COUNT(*)::int n FROM billing_target_rated_periods WHERE holding_id=$1", [holdingId])).rows[0].n).toBe(0);
  await db.pool.query("UPDATE billing_usage_snapshots SET source_count=2 WHERE holding_id=$1 AND month='2026-09'", [holdingId]);
  const mismatch = await readSeatForecastBaseline(db, holdingId, now);
  expect(mismatch.storage).toEqual({originalBytes:null,observedAt:null,coverage:"unavailable"});
  expect(mismatch.seats.count).toBe(282);
});
