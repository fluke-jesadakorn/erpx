import { z } from "zod";

export const leaveSegmentSchema = z.enum(["full", "first_half", "second_half"]);
export type LeaveSegment = z.infer<typeof leaveSegmentSchema>;

export const leavePreviewSchema = z.object({
  leaveTypeId: z.string().min(1).max(180),
  startDate: z.iso.date(),
  endDate: z.iso.date(),
  startSegment: leaveSegmentSchema.default("full"),
  endSegment: leaveSegmentSchema.default("full"),
  handoverNote: z.string().trim().max(2048).default(""),
});

export const leaveCreateSchema = leavePreviewSchema.extend({
  saveOnly: z.boolean().default(false),
});

export type LeaveCalculationDay = {
  date: string;
  segment: LeaveSegment;
  units: number;
  source: "roster" | "workweek";
};

export function addLeaveDays(date: string, days: number) {
  return new Date(Date.parse(`${date}T12:00:00Z`) + days * 86_400_000)
    .toISOString()
    .slice(0, 10);
}

export function dateRange(start: string, end: string) {
  if (end < start) throw new Error("END_BEFORE_START");
  const dates: string[] = [];
  for (let date = start; date <= end; date = addLeaveDays(date, 1)) {
    dates.push(date);
    if (dates.length > 366) throw new Error("DATE_RANGE_TOO_LONG");
  }
  return dates;
}

export function isoWeekday(date: string) {
  const day = new Date(`${date}T12:00:00Z`).getUTCDay();
  return day === 0 ? 7 : day;
}

export function boundarySegment(
  date: string,
  start: string,
  end: string,
  startSegment: LeaveSegment,
  endSegment: LeaveSegment,
): LeaveSegment {
  if (start === end) {
    if (startSegment === "full" && endSegment === "full") return "full";
    if (startSegment !== "full" && endSegment !== "full" && startSegment !== endSegment)
      throw new Error("INVALID_SINGLE_DAY_SEGMENTS");
    return startSegment !== "full" ? startSegment : endSegment;
  }
  if (date === start) return startSegment;
  if (date === end) return endSegment;
  return "full";
}

export function calculateLeaveDays(input: {
  startDate: string;
  endDate: string;
  startSegment: LeaveSegment;
  endSegment: LeaveSegment;
  workweek: number[];
  holidays: Set<string>;
  rosterDates: Set<string>;
  rosterPublishedDates: Set<string>;
}): LeaveCalculationDay[] {
  const days: LeaveCalculationDay[] = [];
  for (const date of dateRange(input.startDate, input.endDate)) {
    const rosterControlsDate = input.rosterPublishedDates.has(date);
    if (!rosterControlsDate && input.holidays.has(date)) continue;
    if (rosterControlsDate && !input.rosterDates.has(date)) continue;
    if (!rosterControlsDate && !input.workweek.includes(isoWeekday(date))) continue;
    const segment = boundarySegment(
      date,
      input.startDate,
      input.endDate,
      input.startSegment,
      input.endSegment,
    );
    days.push({
      date,
      segment,
      units: segment === "full" ? 1 : 0.5,
      source: rosterControlsDate ? "roster" : "workweek",
    });
  }
  return days;
}

