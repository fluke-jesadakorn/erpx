import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import type { CalendarAccess } from "../../domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
export type CalendarLeaveUnit = {
    read1(facts: {
        value1: string;
        value2: string;
        value3: string;
        value4: string;
        value5: string;
        value6: string;
        value7: boolean;
        value8: string | null;
        value9: string;
        value10: number;
    }): Promise<{
        id: string;
        person_id: string;
        name: string;
        start_date: string;
        end_date: string | null;
        days: number;
        state: string;
        version: number;
        branch_id: string | null;
        department_id: string | null;
        full_detail: boolean;
        leave_date: string | null;
        segment: string | null;
        timezone: string;
    }[]>;
    read2(facts: {
        value1: string;
        value2: string;
        value3: string;
        value4: string;
        value5: string | null;
        value6: boolean;
        value7: string;
        value8: string;
        value9: number;
    }): Promise<{
        id: string;
        name: string;
        holiday_date: string;
        timezone: string;
        calendar: string;
        participants: string | {
            id: string;
            name: string;
        }[];
        department_id: string | null;
        branch_id: string | null;
    }[]>;
};
export function createCalendarLeave() {
    async function leaveItems(db: CalendarLeaveUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        const person = q.person ?? (q.mode === "mine" ? (a.staff?.person_id ?? "none") : null);
        // No getLeave/approvalView here: those initialize legacy routing. Calendar reads never write.
        const leave = await db.read1({ value1: a.actor.userId, value2: a.actor.assignmentId, value3: a.actor.companyId, value4: to, value5: from, value6: idList(a.scopeIds), value7: a.availability, value8: person, value9: q.search, value10: limit });
        const absences: CalendarItem[] = leave.map((l) => ({
            id: `leave:${l.id}${l.leave_date ? `:${l.leave_date}` : ''}`,
            source: "leave",
            sourceId: l.id,
            kind: "leave",
            title: `${l.name} · Unavailable`,
            allDay: true,
            startDate: l.leave_date ?? l.start_date,
            endDate: addDays(l.leave_date ?? l.end_date ?? addDays(l.start_date, l.days - 1), 1),
            startsAt: null,
            endsAt: null,
            timezone: l.timezone,
            departmentId: l.department_id,
            branchId: l.branch_id,
            participants: [{ id: l.person_id, name: l.name }],
            owner: l.name,
            status: l.state,
            version: l.full_detail ? l.version : 0,
            permittedActions: l.full_detail ? ["leave.open"] : [],
            ...(l.full_detail
                ? { href: `/tasks/people_payroll/${encodeURIComponent(l.id)}` }
                : { redacted: true }),
            warnings: [],
            history: [],
        }));
        const holidays = await db.read2({ value1: a.actor.companyId, value2: from, value3: to, value4: idList(a.scopeIds), value5: person, value6: a.availability || a.rosterReader || a.lifecycleReader, value7: a.staff?.person_id ?? "none", value8: q.search, value9: limit });
        const holidayItems: CalendarItem[] = holidays.map(holiday => {
            const key = `${holiday.id}:${holiday.department_id ?? holiday.branch_id ?? a.actor.companyId}`;
            const participants = (typeof holiday.participants === "string" ? JSON.parse(holiday.participants) : holiday.participants) as {
                id: string;
                name: string;
            }[];
            return { id: `holiday:${key}`, source: "leave", sourceId: holiday.id, kind: "holiday", title: holiday.name, description: holiday.calendar, allDay: true, startDate: holiday.holiday_date, endDate: addDays(holiday.holiday_date, 1), startsAt: null, endsAt: null, timezone: holiday.timezone, departmentId: holiday.department_id, branchId: holiday.branch_id, participants: participants.sort((a, b) => a.name.localeCompare(b.name)), owner: null, status: "holiday", version: 0, permittedActions: [], warnings: [], history: [] };
        });
        return [...absences, ...holidayItems];
    }
    return { leaveItems };
}
