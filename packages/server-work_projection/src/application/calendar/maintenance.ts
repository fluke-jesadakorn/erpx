import type { CalendarItem, CalendarQuery } from "@erpx/server-work_projection/public/rules/calendar-domain";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
export type CalendarMaintenanceUnit = {
    read1(facts: {
        value1: string;
        value2: string | null;
        value3: string | null;
        value4: string;
        value5: number;
        value6: number;
        value7: "mine" | "team";
        value8: string;
        value9: string | null;
        value10: string;
        value11: number;
    }): Promise<{
        id: string;
        source_kind: string;
        title: string;
        branch_id: string;
        department_id: string | null;
        status: string;
        version: number;
        due_at: number;
        owner_name: string | null;
    }[]>;
};
export function createCalendarMaintenance() {
    async function maintenanceCalendarItems(db: CalendarMaintenanceUnit, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number): Promise<CalendarItem[]> {
        if (!hasPermission(a.actor, 'records.read') || !hasModuleAccess(a.actor.modules, 'maintenance'))
            return [];
        const records = await db.read1({ value1: a.actor.companyId, value2: a.actor.branchId, value3: a.actor.departmentId, value4: idList(a.scopeIds), value5: from, value6: to, value7: q.mode ?? 'mine', value8: a.actor.userId, value9: q.person ?? null, value10: q.search, value11: limit });
        return records.map(r => ({ id: `${r.source_kind}:${r.id}`, source: 'maintenance', sourceId: r.id, kind: 'task', title: `${r.source_kind === 'maintenance_plan' ? 'Preventive due' : 'Maintenance'} · ${r.title}`,
            description: r.source_kind === 'maintenance_plan' ? 'Generate the approved preventive work occurrence.' : 'Review the assigned maintenance work, spare costs and completion evidence.', allDay: false, startDate: null, endDate: null,
            startsAt: r.due_at, endsAt: r.due_at + 1, timezone: q.timezone, branchId: r.branch_id, departmentId: r.department_id, participants: [], owner: r.owner_name ?? 'Assigned maintenance colleague', status: r.status, version: r.version,
            permittedActions: [], href: r.source_kind === 'maintenance_plan' ? `/maintenance/plans/${r.id}` : `/maintenance/${r.id}`, warnings: [], history: [] }));
    }
    return { maintenanceCalendarItems };
}
