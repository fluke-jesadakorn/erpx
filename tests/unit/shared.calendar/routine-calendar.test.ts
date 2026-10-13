import { describe, expect, it } from "vitest";
import {
  nextOccurrence,
  zonedTime,
  type RoutineRule,
} from "../../../src/lib/work_projection/routine-calendar";
const rule: RoutineRule = {
  frequency: "monthly",
  timezone: "Asia/Bangkok",
  occurrence_time: "09:00",
  start_date: "2026-01-31",
  end_date: null,
  weekday: null,
  month_day: 31,
};
describe("Routine calendar", () => {
  it("uses the last day for monthly dates beyond month-end", () => {
    const first = nextOccurrence(rule, Date.parse("2026-01-31T02:00:00Z"));
    expect(new Date(first!).toISOString()).toBe("2026-02-28T02:00:00.000Z");
    expect(new Date(nextOccurrence(rule, first!)!).toISOString()).toBe(
      "2026-03-31T02:00:00.000Z",
    );
  });
  it("honors leap days, weekday and inclusive end dates", () => {
    expect(
      new Date(
        nextOccurrence(
          { ...rule, start_date: "2028-01-31" },
          Date.parse("2028-02-01T00:00:00Z"),
        )!,
      ).toISOString(),
    ).toBe("2028-02-29T02:00:00.000Z");
    expect(
      nextOccurrence(
        { ...rule, end_date: "2026-02-28" },
        Date.parse("2026-02-28T02:00:00Z"),
      ),
    ).toBeNull();
    expect(
      new Date(
        nextOccurrence(
          { ...rule, frequency: "weekly", weekday: 1 },
          Date.parse("2026-02-01T00:00:00Z"),
        )!,
      ).getUTCDay(),
    ).toBe(1);
  });
  it("schedules one occurrence through DST gaps and overlaps", () => {
    expect(
      new Date(
        zonedTime("2026-03-08", "02:30", "America/New_York"),
      ).toISOString(),
    ).toBe("2026-03-08T07:30:00.000Z");
    expect(
      new Date(
        zonedTime("2026-11-01", "01:30", "America/New_York"),
      ).toISOString(),
    ).toBe("2026-11-01T05:30:00.000Z");
  });
});
