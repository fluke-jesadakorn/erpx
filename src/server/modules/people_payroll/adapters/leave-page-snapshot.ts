import "server-only";
import type { WorkContext } from "../../identity_access/public/authorization";
import { holdingResources } from "../../runtime/public/runtime";
import { calendarData } from "../../work_projection/public/calendar/index";
import { today } from "@/lib/work_projection/calendar-date";
import { leavePageRow, leavePageTeam, type LeavePageSnapshot } from "@/lib/people_payroll/leave-page-snapshot";
import { leaveCalendarMonth, leaveListFilterSchema, leaveSummary, listOwnLeavePage } from "./leave-self-service";

/** Calls the original authorized readers, with no internal HTTP or private Work snapshot. */
export async function leavePageSnapshot(actor: WorkContext, query: { month?: string | string[]; filter?: string | string[] }): Promise<LeavePageSnapshot> {
  const summary = await leaveSummary(actor), currentDate = today(summary.policy?.timezone ?? "Asia/Bangkok");
  const month = typeof query.month === "string" && /^\d{4}-(0[1-9]|1[0-2])$/.test(query.month) ? query.month : currentDate.slice(0, 7);
  const parsedFilter = leaveListFilterSchema.safeParse(typeof query.filter === "string" ? query.filter : "all"), filter = parsedFilter.success ? parsedFilter.data : "all";
  const team = async () => {
    const { write } = await holdingResources(actor.holdingId), items: LeavePageSnapshot["team"] = [], visited = new Set<number>();
    let cursor: number | null = 0, failed = false;
    while (cursor !== null) {
      if (visited.has(cursor)) throw new Error("Calendar pagination did not advance.");
      visited.add(cursor);
      const page = await calendarData(write, actor, { date: `${month}-01`, view: "month", mode: "team", layers: "leave", cursor: String(cursor), limit: "200" });
      items.push(...leavePageTeam(page.items));
      failed ||= page.sources.some(source => source.source === "leave" && source.status === "error");
      cursor = page.nextCursor;
    }
    return { items, failed };
  };
  const [own, availability, list] = await Promise.allSettled([leaveCalendarMonth(actor, month), team(), listOwnLeavePage(actor, { filter, limit: 10 })]);
  return {
    summary: {
      setupRequired: summary.setupRequired, setupMessage: summary.setupMessage, purchase: summary.purchase,
      policy: summary.policy ? { name: summary.policy.name, timezone: summary.policy.timezone, version: summary.policy.version } : null,
      balances: summary.balances.map(balance => ({ type: { id: balance.type.id, name: balance.type.name, tracked: balance.type.tracked }, available: balance.available, reserved: balance.reserved })),
    },
    currentDate, month, filter,
    calendar: own.status === "fulfilled" ? { month: own.value.month, requests: own.value.requests.map(leavePageRow), holidays: own.value.holidays.map(holiday => ({ holiday_date: holiday.holiday_date, name: holiday.name })) } : null,
    team: availability.status === "fulfilled" ? availability.value.items : [],
    calendarError: own.status === "rejected" ? "Your leave calendar could not be loaded. Retry to read the current month." : "",
    teamError: availability.status === "rejected" || availability.value.failed ? "Some approved team absences could not be loaded. Your requests and holidays are still available." : "",
    requests: list.status === "fulfilled" ? list.value.items.map(leavePageRow) : [], nextCursor: list.status === "fulfilled" ? list.value.nextCursor : null,
    listError: list.status === "rejected" ? "Your requests could not be loaded. Retry to read the current list." : "",
  };
}
