import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { CalendarAccess } from "../../domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
export type CalendarQualityUnit = {
    read1(facts: {
        value1: string;
        value2: string | null;
        value3: string;
        value4: string;
        value5: string;
        value6: "mine" | "team";
        value7: string;
        value8: string | null;
        value9: string;
        value10: number;
    }): Promise<{
        id: string;
        due_date: string;
        status: string;
        version: number;
        branch_id: string;
        product_name: string;
    }[]>;
};
export function createCalendarQuality() {
    async function qualityCalendarItems(db: CalendarQualityUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'quality.read') || a.actor.departmentId)
            return [];
        const records = await db.read1({ value1: a.actor.companyId, value2: a.actor.branchId, value3: idList(a.scopeIds), value4: from, value5: to, value6: q.mode ?? 'mine', value7: a.actor.userId, value8: q.person ?? null, value9: q.search, value10: limit });
        return records.map(h => ({ id: `quality_hold:${h.id}`, source: 'quality_holds', sourceId: h.id, kind: 'task', title: `Quality hold · ${h.product_name}`, description: h.status === 'cancel_pending' ? 'Independently review cancellation of an uninspected hold. Stock remains quarantined.' : 'Review source inspection evidence before independently releasing quarantined stock.', allDay: true, startDate: h.due_date, endDate: addDays(h.due_date, 1), startsAt: null, endsAt: null, timezone: q.timezone, branchId: h.branch_id, departmentId: null, participants: [], owner: h.status === 'held' ? 'Quality inspection team' : 'Independent Quality manager', status: h.status, version: h.version, permittedActions: [], href: `/quality/${h.id}`, warnings: [], history: [] }));
    }
    return { qualityCalendarItems };
}
