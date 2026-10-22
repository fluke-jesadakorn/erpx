import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { CalendarNode } from "@erpx/feature-work_projection/lib/calendar";
export function calendarAccessOrganizationFacts(db: SqlDatabase) { return { organizationNodes: (companyId: string) => db.prepare('SELECT id,parent_id "parentId",display_name name,kind,branch_id "branchId" FROM organization_nodes WHERE company_id=? ORDER BY id LIMIT 5001').bind(companyId).all<CalendarNode>().then(result => result.results) }; }
