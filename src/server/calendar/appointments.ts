import "server-only";
import type { CalendarItem, CalendarQuery } from "@/lib/calendar";
import type { SqlDatabase } from "../runtime/types";
import { hasPermission } from "@/lib/permissions";
import {
  rows,
  one,
  idList,
  inList,
  withinScope,
  type CalendarAccess,
} from "./access";
export type AppointmentRow = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  organizer_person_id: string;
  title: string;
  description: string;
  starts_at: number;
  ends_at: number;
  timezone: string;
  location: string;
  meeting_url: string;
  private: boolean;
  guests: NonNullable<CalendarItem["details"]>["guests"];
  status: string;
  version: number;
  created_by: string;
  created_at: number;
  updated_at: number;
};
export async function appointmentRows(
  db: SqlDatabase,
  a: CalendarAccess,
  q: CalendarQuery,
  from: number,
  to: number,
  limit: number,
) {
  const manager = hasPermission(a.actor, "calendar.appointments.manage");
  // Participant visibility is checked against a placement in the selected workspace.
  return rows<
    AppointmentRow & { full_detail: boolean; organizer_name: string }
  >(
    db,
    `WITH candidates AS (
 SELECT x.*,p.legal_name organizer_name,
 ((? AND ${inList("coalesce(x.department_id,x.branch_id,x.company_id)")}) OR x.organizer_person_id=? OR EXISTS(SELECT 1 FROM calendar_appointment_participants ap WHERE ap.appointment_id=x.id AND ap.person_id=?)) full_detail
 FROM calendar_appointments x JOIN hr_people p ON p.id=x.organizer_person_id AND p.company_id=x.company_id
 WHERE x.company_id=? AND x.starts_at<? AND x.ends_at>?
 AND (${inList("coalesce(x.department_id,x.branch_id,x.company_id)")} OR EXISTS(SELECT 1 FROM calendar_appointment_participants ap JOIN hr_engagements e ON e.person_id=ap.person_id AND e.company_id=ap.company_id WHERE ap.appointment_id=x.id AND e.status='active' AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")})))
 SELECT x.* FROM candidates x WHERE (full_detail OR (? AND x.status='scheduled'))
 AND (?::text IS NULL OR EXISTS(SELECT 1 FROM calendar_appointment_participants ap WHERE ap.appointment_id=x.id AND ap.person_id=?))
 AND (?='' OR (CASE WHEN full_detail THEN x.title ELSE 'Busy' END) ILIKE '%'||?||'%') ORDER BY x.id LIMIT ?`,
    manager,
    idList(a.scopeIds),
    a.staff?.person_id ?? null,
    a.staff?.person_id ?? null,
    a.actor.companyId,
    to,
    from,
    idList(a.scopeIds),
    idList(a.scopeIds),
    a.availability,
    q.person ?? (q.mode === "mine" ? (a.staff?.person_id ?? "none") : null),
    q.person ?? (q.mode === "mine" ? (a.staff?.person_id ?? "none") : null),
    q.search,
    q.search,
    limit,
  );
}
export async function appointmentItems(
  db: SqlDatabase,
  a: CalendarAccess,
  q: CalendarQuery,
  from: number,
  to: number,
  limit: number,
): Promise<CalendarItem[]> {
  const result = await appointmentRows(db, a, q, from, to, limit),
    ids = result.map((x) => x.id);
  const participants = ids.length
    ? await rows<{
        appointment_id: string;
        id: string;
        name: string;
        in_scope: boolean;
      }>(
        db,
        `SELECT ap.appointment_id,p.id,p.legal_name name,EXISTS(SELECT 1 FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}) in_scope FROM calendar_appointment_participants ap JOIN hr_people p ON p.id=ap.person_id AND p.company_id=ap.company_id WHERE ${inList("ap.appointment_id")} ORDER BY p.legal_name`,
        idList(a.scopeIds),
        idList(ids),
      )
    : [];
  return result
    .map((r): CalendarItem => {
      // Nonparticipants receive only availability, even for a nonprivate meeting; guest contacts never leak.
      const full = r.full_detail,
        manage =
          withinScope(a, r.branch_id, r.department_id) &&
          (hasPermission(a.actor, "calendar.appointments.manage") ||
            r.organizer_person_id === a.staff?.person_id);
      return {
        id: `appointment:${r.id}`,
        source: "appointments",
        sourceId: r.id,
        kind: "appointment",
        title: full ? r.title : "Busy",
        ...(full ? { description: r.description } : {}),
        allDay: false,
        startDate: null,
        endDate: null,
        startsAt: r.starts_at,
        endsAt: r.ends_at,
        timezone: r.timezone,
        departmentId: full ? r.department_id : null,
        branchId: full ? r.branch_id : null,
        participants: participants
          .filter((p) => p.appointment_id === r.id && (full || p.in_scope))
          .map((p) => ({ id: p.id, name: p.name })),
        owner: full ? r.organizer_name : null,
        status: full
          ? r.status
          : r.status === "scheduled"
            ? "busy"
            : "unavailable",
        version: full ? r.version : 0,
        permittedActions:
          full && manage && r.status === "scheduled"
            ? [
              "appointment.save",
                "appointment.reschedule",
                "appointment.cancelled",
                ...(r.starts_at <= Date.now()
                  ? ["appointment.completed", "appointment.no_show"]
                  : []),
              ]
            : [],
        warnings: [],
        history: [],
        redacted: !full,
        ...(full
          ? {
              details: {
                location: r.location,
                meetingUrl: r.meeting_url,
                guests:
                  typeof r.guests === "string"
                    ? JSON.parse(r.guests)
                    : r.guests,
                private: r.private,
                organizerPersonId: r.organizer_person_id,
              },
            }
          : {}),
      };
    })
    .filter((r) => !r.redacted || r.status === "busy");
}
export async function appointmentWarningMessages(
  db: SqlDatabase,
  companyId: string,
  ids: string[],
  start: number,
  end: number,
  zone: string,
  except: string,
) {
  const overlap = await one(
    db,
    `SELECT 1 FROM calendar_appointments a JOIN calendar_appointment_participants p ON p.appointment_id=a.id WHERE a.company_id=? AND a.status='scheduled' AND a.id!=? AND a.starts_at<? AND a.ends_at>? AND ${inList("p.person_id")} LIMIT 1`,
    companyId,
    except,
    end,
    start,
    idList(ids),
  );
  const leave = await one(
    db,
    `SELECT 1 FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id WHERE l.company_id=? AND l.state='approved' AND ${inList("coalesce(l.person_id,e.person_id)")} AND EXISTS(SELECT 1 FROM (
 SELECT s.leave_date::date absence_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id
 UNION ALL SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id)
 ) absence WHERE absence_date<=(to_timestamp((?::bigint-1)/1000.0) AT TIME ZONE ?)::date AND absence_date>=(to_timestamp(?::bigint/1000.0) AT TIME ZONE ?)::date) LIMIT 1`,
    companyId,
    idList(ids),
    end,
    zone,
    start,
    zone,
  );
  return [
    ...(overlap ? ["An attendee is unavailable during this time."] : []),
    ...(leave ? ["An attendee has approved absence during this time."] : []),
  ];
}
