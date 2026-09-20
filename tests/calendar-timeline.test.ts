import { describe, expect, it } from "vitest";
import type { CalendarItem } from "../src/lib/calendar";
import { calendarRange } from "../src/lib/calendar";
import {
  clipToDay,
  localDateTimeCandidates,
  moveInclusiveRange,
  placeTimelineItems,
  snapInstant,
  timelineDay,
} from "../src/lib/calendar-timeline";

const item = (id: string, startsAt: number, endsAt: number): CalendarItem => ({
  id,
  sourceId: id,
  source: "appointments",
  kind: "appointment",
  title: id,
  allDay: false,
  startDate: null,
  endDate: null,
  startsAt,
  endsAt,
  timezone: "UTC",
  departmentId: null,
  branchId: null,
  participants: [],
  owner: null,
  status: "scheduled",
  version: 1,
  permittedActions: [],
  warnings: [],
  history: [],
});

describe("calendar timeline geometry", () => {
  it("queries exactly one local date for day view", () => {
    expect(calendarRange({ date: "2027-01-12", view: "day" })).toEqual({ from: "2027-01-12", to: "2027-01-13" });
  });

  it("uses 23 and 25 actual hours across DST transitions and disambiguates the repeated hour", () => {
    const spring = timelineDay("2027-03-14", "America/New_York");
    const fall = timelineDay("2027-11-07", "America/New_York");
    expect(spring.duration).toBe(23 * 3_600_000);
    expect(spring.slots).toHaveLength(92);
    expect(fall.duration).toBe(25 * 3_600_000);
    expect(fall.slots).toHaveLength(100);
    expect(fall.hours.filter((hour) => hour.time === "01:00").map((hour) => hour.label)).toEqual([
      "01:00 GMT-4",
      "01:00 GMT-5",
    ]);
    expect(localDateTimeCandidates("2027-11-07T01:30", "America/New_York")).toHaveLength(2);
  });

  it("snaps to 15 minutes and clips overnight items without mutating duration", () => {
    const origin = Date.parse("2027-01-12T00:00:00Z");
    expect(snapInstant(origin + 8 * 60_000, origin)).toBe(origin + 15 * 60_000);
    expect(clipToDay(origin - 60_000, origin + 60_000, origin, origin + 86_400_000)).toMatchObject({
      start: origin,
      clippedStart: true,
      clippedEnd: false,
    });
  });

  it("packs overlapping events into lanes and reuses a lane after an event ends", () => {
    const start = Date.parse("2027-01-12T00:00:00Z");
    const placements = placeTimelineItems([
      item("a", start, start + 60_000),
      item("b", start + 30_000, start + 90_000),
      item("c", start + 90_000, start + 120_000),
    ], start, start + 1000_000);
    expect(placements.map((placement) => [placement.item.id, placement.lane, placement.laneCount])).toEqual([
      ["a", 0, 2],
      ["b", 1, 2],
      ["c", 0, 1],
    ]);
  });

  it("moves inclusive announcement ranges while preserving their length", () => {
    expect(moveInclusiveRange("2027-01-10", "2027-01-12", "2027-02-01")).toEqual({
      startDate: "2027-02-01",
      endDate: "2027-02-03",
    });
  });
});
