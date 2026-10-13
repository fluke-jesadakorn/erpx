import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { CalendarAccess } from "../../domain/calendar/access";
export type CalendarSupplierBatchesUnit = {
    read1(facts: {
        value1: string;
        value2: string;
        value3: string;
        value4: "mine" | "team";
        value5: string;
        value6: string | null;
        value7: string;
        value8: number;
    }): Promise<{
        id: string;
        planned_date: string;
        status: string;
        version: number;
        unknown_count: number;
    }[]>;
};
export function createCalendarSupplierBatches() {
    async function supplierBatchCalendarItems(db: CalendarSupplierBatchesUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'finance.read') || a.actor.branchId || a.actor.departmentId || !a.scopeIds.includes(a.actor.companyId))
            return [];
        const records = await db.read1({ value1: a.actor.companyId, value2: from, value3: to, value4: q.mode ?? 'mine', value5: a.actor.userId, value6: q.person ?? null, value7: q.search, value8: limit });
        return records.map(row => ({ id: `supplier_batch:${row.id}`, source: 'supplier_batches', sourceId: row.id, kind: 'task', title: 'Supplier payment batch', description: 'Review invoice sources, approval and individual bank results in Finance.', allDay: true, startDate: row.planned_date, endDate: addDays(row.planned_date, 1), startsAt: null, endsAt: null, timezone: q.timezone, branchId: null, departmentId: null, participants: [], owner: 'Company Finance / Treasury', status: row.status, version: row.version, permittedActions: [], href: `/finance/supplier-batches/${row.id}`, warnings: row.unknown_count ? ['Bank result uncertain: reservations remain in place.'] : [], history: [] }));
    }
    return { supplierBatchCalendarItems };
}
