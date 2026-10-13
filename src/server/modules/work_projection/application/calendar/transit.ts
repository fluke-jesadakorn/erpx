import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { CalendarAccess } from "../../domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
export type CalendarTransitUnit = {
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
        number: string;
        due_date: string;
        status: string;
        version: number;
        target_branch_id: string;
    }[]>;
};
export function createCalendarTransit() {
    async function transitCalendarItems(db: CalendarTransitUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'inventory.read') || a.actor.departmentId)
            return [];
        const records = await db.read1({ value1: a.actor.companyId, value2: a.actor.branchId, value3: idList(a.scopeIds), value4: from, value5: to, value6: q.mode ?? 'mine', value7: a.actor.userId, value8: q.person ?? null, value9: q.search, value10: limit });
        return records.map(d => ({ id: `inventory_transit:${d.id}`, source: 'stock_transit', sourceId: d.id, kind: 'task', title: `Transport · ${d.number}`, description: 'Record physical dispatch, receipt and independently reviewed discrepancies.', allDay: true, startDate: d.due_date, endDate: addDays(d.due_date, 1), startsAt: null, endsAt: null, timezone: q.timezone, branchId: d.target_branch_id, departmentId: null, participants: [], owner: d.status === 'awaiting_dispatch' ? 'Source warehouse' : 'Receiving warehouse', status: d.status, version: d.version, permittedActions: [], href: `/stock-transit/${d.id}`, warnings: [], history: [] }));
    }
    return { transitCalendarItems };
}
