import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { CalendarAccess } from "../../domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
export type CalendarSupplierPromisesUnit = {
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
        order_id: string;
        number: string;
        promised_date: string;
        remaining_milli: number;
        description: string;
        version: number;
        branch_id: string;
        department_id: string | null;
    }[]>;
};
export function createCalendarSupplierPromises() {
    async function supplierPromiseCalendarItems(db: CalendarSupplierPromisesUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'commerce.read'))
            return [];
        const records = await db.read1({ value1: a.actor.companyId, value2: a.actor.branchId, value3: a.actor.departmentId, value4: idList(a.scopeIds), value5: from, value6: to, value7: q.mode ?? 'mine', value8: a.actor.userId, value9: q.person ?? null, value10: q.search, value11: limit });
        return records.map(d => ({ id: `supplier_promise:${d.id}`, source: 'supplier_promises', sourceId: d.order_id, kind: 'task', title: `Supplier delivery · ${d.number}`, description: `${d.description} · ${d.remaining_milli / 1000} outstanding confirmed units`, allDay: true, startDate: d.promised_date, endDate: addDays(d.promised_date, 1), startsAt: null, endsAt: null, timezone: q.timezone, branchId: d.branch_id, departmentId: d.department_id, participants: [], owner: 'Purchasing / receiving warehouse', status: 'awaiting_receipt', version: d.version, permittedActions: [], href: `/commerce/orders/${d.order_id}#supplier-promises`, warnings: [], history: [] }));
    }
    return { supplierPromiseCalendarItems };
}
