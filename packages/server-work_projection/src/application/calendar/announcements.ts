import type { CalendarItem, CalendarQuery } from "@erpx/server-work_projection/public/rules/calendar-domain";
import { addDays } from "@erpx/server-people_payroll/public/rules/people-roster-domain";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
import { calendarQuerySchema } from "@erpx/server-work_projection/public/rules/calendar-domain";
import type { AnnouncementRow } from "@erpx/server-work_projection/domain/calendar/announcements";
export type CalendarAnnouncementsUnit = {
    read1(facts: {
        value1: string;
        value2: string;
        value3: boolean;
        value4: string;
        value5: string;
        value6: string;
        value7: string;
        value8: number;
    }): Promise<(AnnouncementRow & {
        owner_name: string;
    })[]>;
    access(actor: WorkContext): Promise<CalendarAccess>;
};
export function createCalendarAnnouncements() {
    function announcementAudience(a: CalendarAccess) {
        const ids = new Set(a.scopeIds);
        // A scoped workspace also sees notices addressed to its ancestors.
        const roots = a.actor.departmentId
            ? [a.actor.departmentId]
            : a.actor.branchId
                ? [a.actor.branchId]
                : [];
        for (const root of roots) {
            let next: string | null = root;
            const seen = new Set<string>();
            while (next && !seen.has(next)) {
                seen.add(next);
                ids.add(next);
                next = a.nodes.find((n) => n.id === next)?.parentId ?? null;
            }
        }
        return [...ids];
    }
    async function announcementItems(db: CalendarAnnouncementsUnit, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number, publishedOnly = false): Promise<CalendarItem[]> {
        const manage = hasPermission(a.actor, "calendar.announcements.publish"), audience = announcementAudience(a);
        const records = await db.read1({ value1: a.actor.companyId, value2: idList(audience), value3: manage && !publishedOnly, value4: idList(a.scopeIds), value5: to, value6: from, value7: q.search, value8: limit });
        return records.map((r) => ({
            id: `announcement:${r.id}`,
            source: "announcements",
            sourceId: r.id,
            kind: "announcement",
            title: r.title,
            description: r.content,
            allDay: true,
            startDate: r.start_date,
            endDate: addDays(r.end_date, 1),
            startsAt: null,
            endsAt: null,
            timezone: "Asia/Bangkok",
            departmentId: r.department_id,
            branchId: r.branch_id,
            participants: [],
            owner: r.owner_name,
            status: r.status,
            version: r.version,
            permittedActions: manage &&
                a.scopeIds.includes(r.department_id ?? r.branch_id ?? a.actor.companyId)
                ? [
                    "announcement.save",
                    "announcement.reschedule",
                    ...(r.status !== "published"
                        ? ["announcement.published"]
                        : ["announcement.withdrawn"]),
                ]
                : [],
            warnings: [],
            history: [],
        }));
    }
    // Feed and calendar call the same adapter and audience predicate; there is no copied social post.
    async function publishedAnnouncements(db: CalendarAnnouncementsUnit, actor: WorkContext, day: string) {
        return announcementItems(db, await db.access(actor), calendarQuerySchema.parse({ date: day }), day, addDays(day, 1), 30, true);
    }
    return { announcementAudience, announcementItems, publishedAnnouncements };
}
