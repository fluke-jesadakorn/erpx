import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/identity-access-server/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/people-payroll-server/public/platform-owner-projection-sources";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeOrgOne } from './native-organization-owner-write';
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
            return nativeOrgOne<{
                person_id: string;
                version: number;
            }>(database, companyId, 'staff_policy_request_route_head', [companyId, department_id]);
        } };
}
