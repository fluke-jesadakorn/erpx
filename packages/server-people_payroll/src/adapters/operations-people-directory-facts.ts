import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleDirectoryEmployeeFacts(database: SqlDatabase) {
    return { stageSaveDirectoryPersonEffect3(legalName: string, branchId: string, departmentId: string | null, id: string) {
            return database.prepare("UPDATE employees SET display_name=?,branch_id=?,department_id=? WHERE person_id=?")
                .bind(legalName, branchId, departmentId, id);
        },
        stageSaveDirectoryPersonEffect6(value: string, id: string, companyId: string, branchId: string, departmentId: string | null, legalName: string, now: number) {
            return database.prepare("INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'active',?)")
                .bind(value, id, companyId, branchId, departmentId, legalName, now);
        },
        stageSaveDirectoryPersonEffect9(linkedMemberUserId: string, id: string) {
            return database.prepare("UPDATE employees SET user_id=? WHERE person_id=?")
                .bind(linkedMemberUserId, id);
        },
        stageSetDirectoryStatusEffect3(value: "active" | "inactive", personId: string) {
            return database.prepare("UPDATE employees SET status=? WHERE person_id=?")
                .bind(value, personId);
        } };
}
