import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import { hasPermission } from '@/lib/permissions';
import type { CalendarAccess } from "../../domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
export type CalendarRefundsUnit = {
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
        obligation_id: string;
        planned_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
        prepared_by: string;
        owner_name: string;
        number: string;
    }[]>;
};
export function createCalendarRefunds() {
    async function refundCalendarItems(db: CalendarRefundsUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'finance.read'))
            return [];
        const records = await db.read1({ value1: a.actor.companyId, value2: a.actor.branchId, value3: a.actor.departmentId, value4: idList(a.scopeIds), value5: from, value6: to, value7: q.mode ?? 'mine', value8: a.actor.userId, value9: q.person ?? null, value10: q.search, value11: limit });
        return records.map(row => ({ id: `refund:${row.id}`, source: 'refunds', sourceId: row.id, kind: 'task', title: `Refund · ${row.number}`, description: 'Review the instruction and bank evidence in Finance.', allDay: true, startDate: row.planned_date, endDate: addDays(row.planned_date, 1), startsAt: null, endsAt: null, timezone: q.timezone, branchId: row.branch_id, departmentId: row.department_id, participants: [], owner: row.owner_name, status: row.status, version: row.version, permittedActions: [], href: `/finance/refunds/${row.obligation_id}`, warnings: row.status === 'unknown' ? ['Bank result uncertain: keep the amount reserved until evidence is reviewed.'] : [], history: [] }));
    }
    return { refundCalendarItems };
}
