import type { CalendarItem, CalendarQuery } from "@erpx/server-work_projection/public/rules/calendar-domain";
import { addDays } from "@erpx/server-people_payroll/public/rules/people-roster-domain";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
export type CalendarSupplierBillsUnit = {
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
        reference: string;
        due_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
    }[]>;
};
export function createCalendarSupplierBills() {
    async function supplierBillCalendarItems(db: CalendarSupplierBillsUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'finance.read'))
            return [];
        const records = await db.read1({ value1: a.actor.companyId, value2: a.actor.branchId, value3: a.actor.departmentId, value4: idList(a.scopeIds), value5: from, value6: to, value7: q.mode ?? 'mine', value8: a.actor.userId, value9: q.person ?? null, value10: q.search, value11: limit });
        return records.map(r => ({ id: `supplier_bill:${r.id}`, source: 'supplier_bills', sourceId: r.id, kind: 'task', title: `Supplier invoice · ${r.reference}`, description: r.status === 'returned' ? 'Obtain the corrected supplier invoice from the purchase order receipt.' : 'Invoice due date. Resolve matching exceptions before preparing AP.', allDay: true, startDate: r.due_date, endDate: addDays(r.due_date, 1), startsAt: null, endsAt: null, timezone: q.timezone, branchId: r.branch_id, departmentId: r.department_id, participants: [], owner: r.status === 'review' ? 'Independent Finance reviewer' : r.status === 'returned' ? 'Purchase order invoicing owner' : 'Accounts payable', status: r.status, version: r.version, permittedActions: [], href: `/supplier-bills/${r.id}`, warnings: [], history: [] }));
    }
    return { supplierBillCalendarItems };
}
