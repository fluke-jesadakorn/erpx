import "server-only";
import {
  calendarQuerySchema,
  calendarRange,
  calendarLayers,
  type CalendarData,
  type CalendarItem,
  type CalendarLayer,
} from "@/lib/calendar";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import {
  calendarAccess,
  descendants,
  rows,
  one,
  fail,
  idList,
  inList,
} from "./access";
import { rosterItems } from "./roster";
import { leaveItems } from "./leave";
import { appointmentItems, appointmentWarningMessages } from "./appointments";
import { announcementItems } from "./announcements";
import { lifecycleItems } from "./lifecycle";
export { calendarHierarchy } from "./access";
export { calendarCommand } from "./commands";
export async function calendarData(
  db: SqlDatabase,
  actor: WorkContext,
  raw: unknown,
): Promise<CalendarData> {
  const q = calendarQuerySchema.parse(raw),
    a = await calendarAccess(db, actor);
  if(q.preset) q.mode = q.preset === "mine" ? "mine" : "team";
  q.mode ??= a.teamDefault ? "team" : "mine";
  if (q.node) {
    if (!a.scopeIds.includes(q.node)) fail("Choose a unit in this workspace.");
    const selected = descendants(a.nodes, q.node);
    a.scopeIds = a.scopeIds.filter((id) => selected.includes(id));
    a.rosterIds = a.rosterIds.filter((id) => selected.includes(id));
  }
  if (
    q.person &&
    !(await one(
      db,
      `SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.id=? AND p.company_id=? AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")} LIMIT 1`,
      q.person,
      actor.companyId,
      idList(a.peopleIds),
    ))
  )
    fail("This person is outside your calendar people scope.");
  const { from, to } = calendarRange(q);
  const bounds = await one<{ from_ms: number; to_ms: number }>(
    db,
    "SELECT (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint from_ms,(extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint to_ms",
    from,
    q.timezone,
    to,
    q.timezone,
  );
  const take = q.cursor + q.limit + 1,
    sources: CalendarData["sources"] = [];
  const adapters: Record<CalendarLayer, () => Promise<CalendarItem[]>> = {
    support: async () => (await import("./support")).supportCalendarItems(db,a,q,from,to,take),
    crm: async () => (await import("./crm")).crmCalendarItems(db,a,q,from,to,take),
    customer_followups: async () => (await import("./customer-followups")).customerFollowupCalendarItems(db,a,q,from,to,take),
    supplier_bills: async () => (await import("./supplier-bills")).supplierBillCalendarItems(db,a,q,from,to,take),
    procurement_rfqs: async () => (await import("./procurement-rfqs")).rfqCalendarItems(db,a,q,from,to,take),
    supplier_promises: async () => (await import("./supplier-promises")).supplierPromiseCalendarItems(db,a,q,from,to,take),
    stock_transit: async () => (await import("./transit")).transitCalendarItems(db,a,q,from,to,take),
    quality_holds: async () => (await import("./quality")).qualityCalendarItems(db,a,q,from,to,take),
    supplier_batches: async () => (await import("./supplier-batches")).supplierBatchCalendarItems(db,a,q,from,to,take),
    refunds: async () => (await import("./refunds")).refundCalendarItems(db,a,q,from,to,take),
    legal: async () => (await import("../legal/integrations")).legalCalendar(db,actor,q,from,to,take),
    roster: () => rosterItems(db, a, q, bounds!.from_ms, bounds!.to_ms, take),
    leave: () => leaveItems(db, a, q, from, to, take),
    appointments: () =>
      appointmentItems(db, a, q, bounds!.from_ms, bounds!.to_ms, take),
    announcements: () => announcementItems(db, a, q, from, to, take),
    lifecycle: () =>
      lifecycleItems(db, a, q, from, to, bounds!.from_ms, bounds!.to_ms, take),
  };
  const selected = calendarLayers.filter((layer) =>
    q.layers.split(",").includes(layer),
  );
  const responses = await Promise.allSettled(
    selected.map((layer) => adapters[layer]()),
  );
  const all: CalendarItem[] = [];
  for (let i = 0; i < responses.length; i++) {
    const response = responses[i],
      source = selected[i];
    if (response.status === "fulfilled") {
      sources.push({ source, status: "ready" });
      all.push(...response.value);
    } else {
      sources.push({
        source,
        status: "error",
        message: `${source[0].toUpperCase() + source.slice(1)} could not be loaded. Refresh to retry.`,
      });
      console.error(
        "Calendar source failed",
        source,
        response.reason instanceof Error
          ? response.reason.message
          : "Unknown error",
      );
    }
  }
  // Sorting by stable source identity makes pagination independent of the presentation/hierarchy.
  const unique = [...new Map(all.map((item) => [item.id, item])).values()].sort(
    (x, y) => x.id.localeCompare(y.id),
  );
  const items = unique.slice(q.cursor, q.cursor + q.limit),
    nextCursor = unique.length > q.cursor + q.limit ? q.cursor + q.limit : null;
  for (const item of items) {
    if (
      item.source === "appointments" &&
      !item.redacted &&
      item.status === "scheduled"
    )
      item.warnings.push(
        ...(await appointmentWarningMessages(
          db,
          actor.companyId,
          item.participants.map((p) => p.id),
          item.startsAt!,
          item.endsAt!,
          item.timezone,
          item.sourceId,
          item.details?.attendees?.map(attendee=>attendee.userId)??[],
        )),
      );
    if (item.source === "roster") {
      const pp = item.participants.filter(
        (p) => a.availability || p.id === a.staff?.person_id,
      );
      if (
        pp.length &&
        (await one(
          db,
          `SELECT 1 FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id WHERE l.company_id=? AND l.state='approved' AND ${inList("coalesce(l.person_id,e.person_id)")} AND EXISTS(SELECT 1 FROM (
 SELECT s.leave_date::date absence_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id
 UNION ALL SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id)
 ) absence WHERE absence_date<=(to_timestamp((?::bigint-1)/1000.0) AT TIME ZONE ?)::date AND absence_date>=(to_timestamp(?::bigint/1000.0) AT TIME ZONE ?)::date) LIMIT 1`,
          actor.companyId,
          idList(pp.map((p) => p.id)),
          item.endsAt!,
          item.timezone,
          item.startsAt!,
          item.timezone,
        ))
      )
        item.warnings.push(
          "Assigned staff have approved absence during this shift.",
        );
    }
  }
  const legacyIds = items
    .filter(
      (x) => !x.redacted && ["roster", "leave", "lifecycle"].includes(x.source),
    )
    .map((x) => x.sourceId);
  if (legacyIds.length) {
    const history = await rows<{
      record_id: string;
      action: string;
      actor: string;
      at: number;
    }>(
      db,
      `SELECT record_id,action,actor,at FROM (SELECT e.record_id,e.action,coalesce(p.legal_name,'Authorized colleague') actor,e.created_at at,row_number() OVER(PARTITION BY e.record_id ORDER BY e.created_at DESC,e.id DESC) ordinal FROM audit_events e JOIN command_receipts r ON r.command_id=e.command_id LEFT JOIN hr_people p ON p.user_id=e.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE r.authority_json::jsonb->>'companyId'=? AND ${inList("e.record_id")}) h WHERE ordinal<=20 ORDER BY at DESC`,
      actor.companyId,
      actor.companyId,
      idList(legacyIds),
    );
    for (const item of items)
      if (!item.redacted)
        item.history.push(
          ...history
            .filter((h) => h.record_id === item.sourceId)
            .map((h) => ({ action: h.action, actor: h.actor, at: h.at })),
        );
  }
  const historyIds = items
    .filter(
      (x) =>
        !x.redacted && ["appointment", "announcement", "task"].includes(x.kind),
    )
    .map((x) => x.sourceId);
  if (historyIds.length) {
    const history = await rows<{
      record_id: string;
      action: string;
      actor: string;
      at: number;
    }>(
      db,
      `SELECT h.record_id,h.action,coalesce(p.legal_name,'Authorized colleague') actor,h.created_at at FROM (SELECT *,row_number() OVER(PARTITION BY record_id ORDER BY created_at DESC,id DESC) ordinal FROM calendar_history WHERE company_id=? AND ${inList("record_id")}) h LEFT JOIN hr_people p ON p.user_id=h.actor_id AND p.company_id=h.company_id AND p.merged_into_id IS NULL WHERE ordinal<=20 ORDER BY h.created_at DESC`,
      actor.companyId,
      idList(historyIds),
    );
    for (const item of items)
      if (!item.redacted)
        item.history.push(
          ...history
            .filter((h) => h.record_id === item.sourceId)
            .map((h) => ({ action: h.action, actor: h.actor, at: h.at })),
        );
  }
  const shifts = items.filter((x) => x.source === "roster");
  return {
    query: q,
    from,
    to,
    items,
    nextCursor,
    sources,
    coverage: {
      shifts: shifts.length,
      assigned: shifts.reduce((n, s) => n + (s.assigned ?? 0), 0),
      open: shifts.reduce(
        (n, s) => n + Math.max(0, (s.capacity ?? 0) - (s.assigned ?? 0)),
        0,
      ),
      warnings: items.filter((s) => s.warnings.length).length,
      partial:
        nextCursor !== null ||
        q.cursor > 0 ||
        sources.some((s) => s.status === "error"),
    },
    selfPersonId: a.staff?.person_id ?? null,
    canManageRoster: hasPermission(actor, "roster.manage"),
    canSchedule: true,
    canAnnounce: hasPermission(actor, "calendar.announcements.publish"),
    teamDefault: a.teamDefault,
  };
}
