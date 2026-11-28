import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { CalendarNode } from "@erpx/work-projection-ui/lib/calendar";
import { nativeOrgAll } from "./native-organization-owner-write";

/** Native-owned calendar-access organization-node list. The authoritative SQL
 * and Company scope live in the Organization owner crate; this adapter sends
 * only the typed kind and the caller's Company value and keeps the
 * `CalendarNode[]` row-list shape its work_projection consumer expects. */
export function calendarAccessOrganizationFacts(db: SqlDatabase) {
    return {
        organizationNodes: (companyId: string) =>
            nativeOrgAll<CalendarNode>(db, companyId, 'calendar_access_organization_nodes', [companyId]),
    };
}
