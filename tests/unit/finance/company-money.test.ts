import { describe, expect, it } from "vitest";
import { convertUnits, convertWithRates, currencyScale, decimalUnits, resolveRate, unitsDecimal, type ManualRate } from "../../../src/lib/finance/company-money";

const rate = (id: string, currency: string, effectiveDate: string, currencyPerUsd: string, confirmedAt = 1): ManualRate =>
  ({ id, currency, effectiveDate, currencyPerUsd, confirmedAt });

describe("company currency amounts", () => {
  it("parses USD/THB, JPY, KWD and a company-configured special scale exactly", () => {
    expect(currencyScale("USD")).toBe(2);
    expect(currencyScale("THB")).toBe(2);
    expect(currencyScale("JPY")).toBe(0);
    expect(currencyScale("KWD")).toBe(3);
    expect(() => currencyScale("XAU")).toThrow(/company currency scale/);
    expect(currencyScale("XAU", 6)).toBe(6);
    expect(decimalUnits("100.01", 2)).toBe(10001n);
    expect(decimalUnits("100", 0)).toBe(100n);
    expect(decimalUnits("1.234", 3)).toBe(1234n);
    expect(decimalUnits("0.000001", 6)).toBe(1n);
    expect(() => decimalUnits("1.001", 2)).toThrow(/2 decimal places/);
    expect(unitsDecimal(-1234n, 3)).toBe("-1.234");
  });

  it("converts with high precision and rounds once to the target minor unit", () => {
    expect(convertUnits(10000n, 2, 2, "1", "36")).toBe(360000n);
    expect(convertUnits(10000n, 2, 0, "1", "150.555555555555555555")).toBe(15056n);
    expect(convertUnits(10000n, 2, 3, "1", "0.3075")).toBe(30750n);
    expect(convertUnits(1n, 3, 0, "0.3", "150")).toBe(1n);
    expect(convertUnits(convertUnits(1n, 3, 2, "0.3", "1"), 2, 0, "1", "150")).toBe(0n);
    expect(convertUnits(1n, 2, 2, "1", "0.5")).toBe(1n);
    expect(convertUnits(-1n, 2, 2, "1", "0.5")).toBe(-1n);
    expect(() => convertUnits(100n, 2, 2, "0", "36")).toThrow();
  });

  it("uses the date-specific confirmed revision and marks latest-only fallback", () => {
    const rates = [rate("old", "THB", "2026-01-01", "35"), rate("revised", "THB", "2026-01-01", "36", 2), rate("future", "THB", "2026-10-01", "40")];
    expect(resolveRate(rates, "THB", "2026-09-29")?.rate.id).toBe("revised");
    expect(resolveRate(rates, "THB", "2025-12-31")).toMatchObject({ rate: { id: "revised" }, usedLatestFallback: true });
    expect(resolveRate([], "THB", "2026-09-29")).toBeNull();
    expect(convertWithRates({ amountUnits: 360000n, sourceCurrency: "THB", sourceScale: 2, targetCurrency: "USD", targetScale: 2, date: "2026-09-29", rates })?.amountUnits).toBe(10000n);
  });
});
