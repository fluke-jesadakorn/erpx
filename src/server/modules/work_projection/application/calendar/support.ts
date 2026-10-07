import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import { hasPermission } from '@/lib/permissions';
import type { WorkContext } from "../../../identity_access/public/authorization";
import type { CalendarAccess } from "../../domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
export type CalendarSupportUnit = {
    read1(facts: {
        value1: string;
        value2: string;
        value3: string | null;
        value4: string | null;
        value5: string;
        value6: string;
        value7: string;
        value8: "mine" | "team";
        value9: string;
        value10: string | null;
        value11: string;
        value12: number;
        actor: WorkContext;
    }): Promise<{
        id: string;
        title: string;
        due_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
        owner_name: string | null;
        sla_due_at: number | null;
        source_record_json: string | null;
    }[]>;
    read2(facts: {
        value1: string;
        value2: string | null;
        value3: string | null;
        value4: string;
        value5: string;
        value6: string;
        value7: "mine" | "team";
        value8: string;
        value9: string | null;
        value10: string;
        value11: number;
        actor: WorkContext;
    }): Promise<{
        id: string;
        title: string;
        due_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
    }[]>;
    sourceVisible(actor: WorkContext, source: string): Promise<boolean>;
};
export function createCalendarSupport() {
    async function ticketCalendarItems(db: CalendarSupportUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'support.read') && !hasPermission(a.actor, 'records.read'))
            return [];
        const records = await db.read1({ value1: q.timezone, value2: a.actor.companyId, value3: a.actor.branchId, value4: a.actor.departmentId, value5: idList(a.scopeIds), value6: from, value7: to, value8: q.mode ?? 'mine', value9: a.actor.userId, value10: q.person ?? null, value11: q.search, value12: limit, actor: a.actor });
        const visible: typeof records = [];
        for (const record of records) {
            if (record.source_record_json) {
                if (!await db.sourceVisible(a.actor, record.source_record_json))
                    continue;
            }
            visible.push(record);
        }
        return visible.map(r => ({ id: `support_ticket:${r.id}`, source: 'support', sourceId: r.id, kind: 'task', title: r.sla_due_at !== null ? `${r.title} · Resolution target` : r.title, description: 'Review the customer issue, next action and confirmation evidence.', allDay: r.sla_due_at === null, startDate: r.sla_due_at === null ? r.due_date : null, endDate: r.sla_due_at === null ? addDays(r.due_date, 1) : null, startsAt: r.sla_due_at, endsAt: r.sla_due_at === null ? null : r.sla_due_at + 1, timezone: q.timezone, branchId: r.branch_id, departmentId: r.department_id, participants: [], owner: r.owner_name ?? 'Assigned support colleague', status: r.status, version: r.version, permittedActions: [], href: `/support/${r.id}`, warnings: [], history: [] }));
    }
    async function incidentCalendarItems(db: CalendarSupportUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'support.read'))
            return [];
        const records = await db.read2({ value1: a.actor.companyId, value2: a.actor.branchId, value3: a.actor.departmentId, value4: idList(a.scopeIds), value5: from, value6: to, value7: q.mode ?? 'mine', value8: a.actor.userId, value9: q.person ?? null, value10: q.search, value11: limit, actor: a.actor });
        return records.map(r => ({ id: `support_incident:${r.id}`, source: 'support', sourceId: r.id, kind: 'task', title: `Incident · ${r.title}`, description: 'Investigate the shared cause and update linked customer case owners.', allDay: true, startDate: r.due_date, endDate: addDays(r.due_date, 1), startsAt: null, endsAt: null, timezone: q.timezone, branchId: r.branch_id, departmentId: r.department_id, participants: [], owner: 'Assigned incident owner', status: r.status, version: r.version, permittedActions: [], href: `/support/incidents/${r.id}`, warnings: [], history: [] }));
    }
    async function supportCalendarItems(db: CalendarSupportUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        const results = await Promise.all([ticketCalendarItems(db, a, q, from, to, limit), incidentCalendarItems(db, a, q, from, to, limit)]);
        return results.flat();
    }
    return { supportCalendarItems };
}
