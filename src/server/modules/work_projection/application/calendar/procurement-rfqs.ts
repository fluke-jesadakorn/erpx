import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import { hasPermission } from '@/lib/permissions';
import type { CalendarAccess } from "../../domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
export type CalendarProcurementRfqsUnit = {
    read1(facts: {
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
    }): Promise<{
        id: string;
        title: string;
        needed_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
    }[]>;
};
export function createCalendarProcurementRfqs() {
    async function rfqCalendarItems(db: CalendarProcurementRfqsUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'commerce.read'))
            return [];
        const records = await db.read1({ value1: a.actor.companyId, value2: a.actor.branchId, value3: a.actor.departmentId, value4: idList(a.scopeIds), value5: from, value6: to, value7: q.mode ?? 'mine', value8: a.actor.userId, value9: q.person ?? null, value10: q.search, value11: limit });
        return records.map(r => ({ id: `procurement_rfq:${r.id}`, source: 'procurement_rfqs', sourceId: r.id, kind: 'task', title: `Supplier quotations · ${r.title}`, description: 'Demand required date. Review supplier delivery exceptions before awarding.', allDay: true, startDate: r.needed_date, endDate: addDays(r.needed_date, 1), startsAt: null, endsAt: null, timezone: q.timezone, branchId: r.branch_id, departmentId: r.department_id, participants: [], owner: r.status === 'review' ? 'Independent purchasing manager' : 'Purchasing team', status: r.status, version: r.version, permittedActions: [], href: `/procurement-rfqs/${r.id}`, warnings: [], history: [] }));
    }
    return { rfqCalendarItems };
}
