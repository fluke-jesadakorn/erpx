import { describe, expect, it } from "vitest";
import { defaultTimePreference, formatPreferredDateTime, formatUiDateTime, formatUiNumber, utcOffset } from "../../../src/lib/shared/ui-format";

describe("English Gregorian UI formatting", () => {
  it("uses a Gregorian year and English month regardless of runtime locale", () => {
    expect(formatUiDateTime("2026-09-26T00:30:00Z", "Asia/Bangkok", { dateStyle: "long" }))
      .toBe("26 September 2026");
  });

  it("renders the business timezone when a timestamp crosses midnight", () => {
    expect(formatUiDateTime("2026-09-25T18:30:00Z", "Asia/Bangkok", {
      day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit", hourCycle: "h23",
    })).toBe("26 Sept 2026, 01:30 UTC+07:00");
  });

  it("uses the selected timezone and hour cycle without changing the instant", () => {
    const instant = "2026-09-25T18:30:00Z";
    expect(formatPreferredDateTime(instant, defaultTimePreference, "Asia/Bangkok"))
      .toBe("26 Sept 2026, 01:30 UTC+07:00");
    expect(formatPreferredDateTime(instant, {mode:"manual",timeZone:"America/New_York",hourCycle:"h12",version:1}, "Asia/Bangkok"))
      .toContain("UTC-04:00");
    expect(new Date(instant).toISOString()).toBe(instant.replace("Z",".000Z"));
  });

  it("uses the offset at each event instant across daylight saving time", () => {
    expect(utcOffset("2026-03-08T06:30:00Z","America/New_York")).toBe("UTC-05:00");
    expect(utcOffset("2026-03-08T07:30:00Z","America/New_York")).toBe("UTC-04:00");
    expect(utcOffset("2026-09-25T00:00:00Z","UTC")).toBe("UTC+00:00");
  });

  it("uses stable English separators for UI numbers", () => {
    expect(formatUiNumber(12345.5, { maximumFractionDigits: 1 })).toBe("12,345.5");
  });
});
