import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function StaffSettingsPeoplePayrollFacts(database: SqlDatabase) {
    return { readStaffSettingsLevelspeopledepartmentscanManage(companyId: string) {
            return database.prepare(`SELECT n.id,n.display_name,n.branch_id,h.person_id,h.version,COALESCE(p.legal_name,'Not configured') head_name ${organizationFinalSources.operations_staff_settings_facts_organization_nodes_n_from} LEFT ${organizationFinalSources.operations_staff_settings_facts_department_heads_h_join} ON h.department_id=n.id AND h.company_id=n.company_id LEFT JOIN hr_people p ON p.id=h.person_id WHERE n.company_id=? AND n.kind='department' AND n.status='active' ORDER BY n.display_name`)
                .bind(companyId)
                .all<{
                id: string;
                display_name: string;
                branch_id: string | null;
                person_id: string | null;
                version: number | null;
                head_name: string;
            }>();
        },
        readSaveLevelEffect(id: string, id2: string) { return database.prepare(`SELECT 1 FROM hr_engagements WHERE level_id=? AND status='active' UNION ALL SELECT 1 ${organizationFinalSources.operations_staff_settings_facts_organization_positions_from} WHERE level_id=? AND active=1 LIMIT 1`).bind(id, id2).first(); },
        readAssignLevelEngagement(id: string, companyId: string) {
            return database.prepare("SELECT * FROM hr_engagements WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<{
                person_id: string;
                position_id: string | null;
                manager_person_id: string | null;
                status: string;
                level_id: string | null;
                start_date: string;
                currency: string;
                branch_id: string | null;
                department_id: string | null;
            }>();
        },
        guard11(key: string, value: "engagement-version", id: string, companyId: string, expectedVersion: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=?)", [id, companyId, expectedVersion]);
        },
        stageAssignLevelEffect(levelId: string, now: number, id: string) {
            return database.prepare("UPDATE hr_engagements SET level_id=?,version=version+1,updated_at=? WHERE id=?")
                .bind(levelId, now, id);
        } };
}
