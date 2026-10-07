import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
export function StaffSettingsOrganizationFacts(database: SqlDatabase) {
    return { readListLevelsEffect(companyId: string) {
            return database.prepare("SELECT * FROM staff_levels WHERE company_id=? ORDER BY level_number ASC NULLS LAST,rank DESC,code")
                .bind(companyId)
                .all<{
                id: string;
                company_id: string;
                code: string;
                name: string;
                rank: number;
                level_number: number | null;
                active: number;
                version: number;
            }>();
        },
        readSaveLevelPrevious(id: string, companyId: string) { return database.prepare("SELECT level_number FROM staff_levels WHERE id=? AND company_id=?").bind(id, companyId).first<{
            level_number: number | null;
        }>(); },
        guard5(key: string, value: "level-version", id: string, companyId: string, expectedVersion: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM staff_levels WHERE id=? AND company_id=? AND version=?)", [id, companyId, expectedVersion]);
        },
        stageSaveLevelEffect(code: string, name: string, rank: number, value: 0 | 1, value2: number | null, now: number, id: string) {
            return database.prepare("UPDATE staff_levels SET code=?,name=?,rank=?,active=?,level_number=COALESCE(?,level_number),version=version+1,updated_at=? WHERE id=?")
                .bind(code, name, rank, value, value2, now, id);
        },
        stageSaveLevelEffect2(recordId: string, companyId: string, code: string, name: string, rank: number, value: 0 | 1, value2: number | null, now: number, now2: number) {
            return database.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,active,level_number,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(recordId, companyId, code, name, rank, value, value2, now, now2);
        },
        guard8(key: string, value: "level-active", levelId: string, companyId: string) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM staff_levels WHERE id=? AND company_id=? AND active=1)", [levelId, companyId]);
        },
        readSetDepartmentHeadDepartment(id: string, companyId: string) {
            return database.prepare("SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=? AND kind='department'")
                .bind(id, companyId)
                .first<{
                branch_id: string | null;
            }>();
        },
        guard14(key: string, value: "department", id: string, companyId: string) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active')", [id, companyId]);
        },
        guard15(key: string, value: "head-version", expectedVersion: number, id: string, id2: string, expectedVersion2: number) {
            return assertion(database, key, value, "(?=0 AND NOT EXISTS(SELECT 1 FROM department_heads WHERE department_id=?)) OR EXISTS(SELECT 1 FROM department_heads WHERE department_id=? AND version=?)", [expectedVersion, id, id2, expectedVersion2]);
        },
        stageSetDepartmentHeadEffect(id: string, companyId: string, personId: string, value: number, userId: string, now: number) {
            return database.prepare("INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(department_id) DO UPDATE SET person_id=excluded.person_id,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at")
                .bind(id, companyId, personId, value, userId, now);
        } };
}
