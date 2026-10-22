import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function peopleDirectoryOrganizationFacts(database: SqlDatabase) {
    return { readDirectoryOptionsNodes(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return database.prepare("SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN ('branch','department') AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR kind='branch' OR id=?) ORDER BY display_name")
                .bind(companyId, branchId, branchId2, departmentId, departmentId2)
                .all();
        },
        readDirectoryPersonPerson(value: string | null, value2: string | null, personId: string, companyId: string, value3: string | null, value4: string | null, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return database.prepare(`SELECT p.*,e.id AS engagement_id,e.version AS engagement_version,e.branch_id,e.department_id,e.job_title,e.worker_type,e.employment_type,e.start_date,e.end_date,e.currency,e.status AS engagement_status,e.manager_person_id,e.pay_group_id,e.position_id,e.level_id,(SELECT name FROM staff_levels WHERE id=e.level_id) level_name,(SELECT level_number FROM staff_levels WHERE id=e.level_id) level_number
    FROM (${people_payrollPlatformSources.hr_people}) p LEFT JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.id=(SELECT x.id FROM (${people_payrollPlatformSources.hr_engagements}) x WHERE x.person_id=p.id AND x.company_id=p.company_id AND x.worker_type='employee' AND (?::text IS NULL OR x.id=?) ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1)
    WHERE p.id=? AND p.company_id=? AND p.merged_into_id IS NULL AND e.worker_type='employee' AND (?::text IS NULL OR e.id=?) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)`)
                .bind(value, value2, personId, companyId, value3, value4, branchId, branchId2, departmentId, departmentId2)
                .first<Record<string, string | number | null>>();
        },
        guard3(id: string, value: "branch", branchId: string, companyId: string) {
            return assertion(database, id, value, "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')", [branchId, companyId]);
        },
        guard4(id: string, value: "department", departmentId: string | null, departmentId2: string | null, companyId: string, branchId: string) {
            return assertion(database, id, value, "?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND kind='department' AND status='active')", [departmentId, departmentId2, companyId, branchId]);
        },
        readSaveDirectoryPersonPrevious(companyId: string, position_id: string | number) { return database.prepare("SELECT roster_required FROM organization_positions WHERE company_id=? AND id=?").bind(companyId, position_id).first<{
            roster_required: number;
        }>(); },
        readSaveDirectoryPersonNext(companyId: string, positionId: string) { return database.prepare("SELECT roster_required FROM organization_positions WHERE company_id=? AND id=?").bind(companyId, positionId).first<{
            roster_required: number;
        }>(); },
        guard27(commandId: string, value: "active-level", personId: string) {
            return assertion(database, commandId, value, `NOT EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_engagements}) e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.person_id=? AND e.status!='ended' AND (e.level_id IS NULL OR l.active IS DISTINCT FROM 1))`, [personId]);
        } };
}
