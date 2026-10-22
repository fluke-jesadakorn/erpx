import { describe, expect, it } from "vitest";
import { decimalToMinor, minorToDecimal } from "@erpx/feature-finance/lib/finance-money";

describe("finance money boundaries", () => {
  it("converts decimal strings without floating point drift", () => {
    expect(decimalToMinor("1.10")).toBe(110);
    expect(decimalToMinor("-0.25")).toBe(-25);
    expect(minorToDecimal(123456)).toBe("1234.56");
  });

  it("rejects values with too much precision", () => {
    expect(() => decimalToMinor("1.001")).toThrow();
  });
});
