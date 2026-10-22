import type { leaveCalendarMonth, leaveSummary } from "@erpx/server-people_payroll/public/leave-self-service";
import type { CalendarItem } from "@erpx/feature-work_projection/lib/calendar";

type Summary = Awaited<ReturnType<typeof leaveSummary>>;
type Month = Awaited<ReturnType<typeof leaveCalendarMonth>>;
export type LeavePageRow = Pick<Month["requests"][number], "id" | "start_date" | "end_date" | "calculated_units" | "days" | "state" | "cancellation_state" | "leave_type_name">;
export type LeavePageMonth = { month: string; requests: LeavePageRow[]; holidays: Month["holidays"] };
export type LeavePageSummary = Pick<Summary, "setupRequired" | "setupMessage" | "purchase"> & {
  policy: Pick<NonNullable<Summary["policy"]>, "name" | "timezone" | "version"> | null;
  balances: { type: { id: string; name: string; tracked: boolean }; available: number; reserved: number }[];
};
export type LeavePageSnapshot = {
  summary: LeavePageSummary;
  currentDate: string;
  month: string;
  filter: "all" | "drafts" | "review" | "approved" | "closed";
  calendar: LeavePageMonth | null;
  team: CalendarItem[];
  calendarError: string;
  teamError: string;
  requests: LeavePageRow[];
  nextCursor: string | null;
  listError: string;
};

/** The page needs dates and statuses, never decision notes or private evidence. */
export function leavePageRow(row: Month["requests"][number]): LeavePageRow {
  return { id: row.id, start_date: row.start_date, end_date: row.end_date, calculated_units: row.calculated_units, days: row.days, state: row.state, cancellation_state: row.cancellation_state, leave_type_name: row.leave_type_name };
}

/** Only the original Calendar's redacted approved busy facts may reach this view. */
export function leavePageTeam(items: readonly CalendarItem[]): CalendarItem[] {
  return items.filter(item => item.source === "leave" && item.status === "approved" && item.redacted).map(item => ({
    id: item.id, source: "leave", sourceId: item.sourceId, kind: "leave", title: item.title,
    allDay: item.allDay, startDate: item.startDate, endDate: item.endDate, startsAt: item.startsAt, endsAt: item.endsAt, timezone: item.timezone,
    departmentId: null, branchId: null, participants: [], owner: null, status: "approved", version: 0, permittedActions: [], warnings: [], history: [], redacted: true,
  }));
}
