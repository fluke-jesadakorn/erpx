import { platformOwnerProjectionSources as identity_accessPlatformSources } from "../../identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "../../people_payroll/public/platform-owner-projection-sources";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
const staffSelect = (datePredicate: string) => `SELECT p.id person_id,p.user_id,p.legal_name,e.id engagement_id,e.version engagement_version,
  e.company_id,e.branch_id,e.department_id,e.worker_type,e.start_date,e.end_date,e.manager_person_id,e.level_id,l.name level_name,l.rank level_rank,l.level_number,e.position_id,pos.is_ceo,pos.roster_required,e.self_service_top_level
  FROM (${people_payrollPlatformSources.hr_people}) p JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id
  LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id
  LEFT JOIN organization_positions pos ON pos.id=e.position_id AND pos.company_id=e.company_id AND pos.active=1
  WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL
  AND e.status='active' ${datePredicate}
  AND EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.assignments}) a WHERE a.user_id=p.user_id AND a.company_id=p.company_id
    AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=? AND a.valid_until>?)`;
export const activeStaffSelect = staffSelect("AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))");
export function StaffPolicyOrganizationFacts(database: SqlDatabase) {
    return { readRequestRouteHead(companyId: string, department_id: string) {
            return database.prepare("SELECT h.person_id,h.version FROM department_heads h JOIN organization_nodes n ON n.id=h.department_id WHERE h.company_id=? AND h.department_id=? AND n.company_id=h.company_id AND n.status='active' AND n.kind='department'")
                .bind(companyId, department_id)
                .first<{
                person_id: string;
                version: number;
            }>();
        } };
}
