import { describe, expect, it } from "vitest";
import { formatUiDateTime, formatUiNumber } from "../src/lib/ui-format";

describe("English Gregorian UI formatting", () => {
  it("uses a Gregorian year and English month regardless of runtime locale", () => {
    expect(formatUiDateTime("2026-09-26T00:30:00Z", "Asia/Bangkok", { dateStyle: "long" }))
      .toBe("26 September 2026");
  });

  it("renders the business timezone when a timestamp crosses midnight", () => {
    expect(formatUiDateTime("2026-09-25T18:30:00Z", "Asia/Bangkok", {
      day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit", hourCycle: "h23",
    })).toBe("26 Sept 2026, 01:30");
  });

  it("uses stable English separators for UI numbers", () => {
    expect(formatUiNumber(12345.5, { maximumFractionDigits: 1 })).toBe("12,345.5");
  });
});
