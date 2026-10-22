import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function payrollOrganizationFacts(database: SqlDatabase) {
    return { readListPeopleRows(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, value: string | null, value2: string | null, value3: string | null, value4: string | null, value5: string | null) {
            return database.prepare(`SELECT p.id,p.user_id,p.legal_name,p.preferred_name,p.email,p.country_code,p.status,p.version,
      e.id AS engagement_id,e.job_title,e.worker_type,e.employment_type,e.status AS engagement_status,e.start_date,e.currency,
      e.level_id,e.department_id,e.position_id,e.manager_person_id,(SELECT name FROM staff_levels WHERE id=e.level_id) level_name,n.display_name AS branch_name
    FROM (${people_payrollPlatformSources.hr_people}) p LEFT JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.id=(SELECT x.id FROM (${people_payrollPlatformSources.hr_engagements}) x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1)
    LEFT JOIN organization_nodes n ON n.id=e.branch_id
    WHERE p.company_id=? AND p.merged_into_id IS NULL AND e.worker_type='employee' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)
    AND (?::text IS NULL OR (?='unset' AND e.level_id IS NULL) OR e.level_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY p.legal_name,p.id`).bind(companyId, branchId, branchId2, departmentId, departmentId2, value, value2, value3, value4, value5).all();
        },
        readAssignPayGroupMissing(engagementId: string, companyId: string) { return database.prepare(`SELECT 1 FROM (${people_payrollPlatformSources.hr_engagements}) e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.id=? AND e.company_id=? AND e.status='active' AND (l.id IS NULL OR l.active!=1)`).bind(engagementId, companyId).first(); },
        guard36(key: string, value: "active-level", engagementId: string) { return assertion(database, key, value, `NOT EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_engagements}) e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.id=? AND e.status='active' AND (l.id IS NULL OR l.active!=1))`, [engagementId]); },
        guard47(key: string, value: "active-level", engagementId: string) { return assertion(database, key, value, `EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_engagements}) e JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.id=? AND l.active=1)`, [engagementId]); } };
}
