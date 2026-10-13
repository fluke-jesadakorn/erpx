import { describe, expect, it } from "vitest";
import { boundarySegment, calculateLeaveDays, dateRange } from "@/lib/people_payroll/leave-self-service";

describe("leave working-day calculation", () => {
  it("uses the policy workweek and excludes holidays across a year boundary", () => {
    const days = calculateLeaveDays({
      startDate: "2026-12-30", endDate: "2027-01-04", startSegment: "full", endSegment: "full",
      workweek: [1, 2, 3, 4, 5], holidays: new Set(["2027-01-01"]), rosterDates: new Set(), rosterPublishedDates: new Set(),
    });
    expect(days.map((item) => item.date)).toEqual(["2026-12-30", "2026-12-31", "2027-01-04"]);
    expect(days.reduce((sum, item) => sum + item.units, 0)).toBe(3);
  });

  it("lets a published roster override the fallback workweek", () => {
    const days = calculateLeaveDays({
      startDate: "2026-09-19", endDate: "2026-09-21", startSegment: "full", endSegment: "full",
      workweek: [1, 2, 3, 4, 5], holidays: new Set(),
      rosterPublishedDates: new Set(["2026-09-19", "2026-09-20", "2026-09-21"]),
      rosterDates: new Set(["2026-09-20"]),
    });
    expect(days).toEqual([{ date: "2026-09-20", segment: "full", units: 1, source: "roster" }]);
  });

  it("calculates half-day boundaries without changing intermediate days", () => {
    const days = calculateLeaveDays({
      startDate: "2026-09-14", endDate: "2026-09-16", startSegment: "second_half", endSegment: "first_half",
      workweek: [1, 2, 3, 4, 5], holidays: new Set(), rosterDates: new Set(), rosterPublishedDates: new Set(),
    });
    expect(days.map((item) => item.units)).toEqual([0.5, 1, 0.5]);
    expect(days.reduce((sum, item) => sum + item.units, 0)).toBe(2);
  });

  it("rejects contradictory halves for a single date", () => {
    expect(() => boundarySegment("2026-09-17", "2026-09-17", "2026-09-17", "first_half", "second_half")).toThrow("INVALID_SINGLE_DAY_SEGMENTS");
  });

  it("caps an accidental oversized range", () => {
    expect(() => dateRange("2026-01-01", "2028-01-01")).toThrow("DATE_RANGE_TOO_LONG");
  });
});
