import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function leaveEmployeeFacts(database: SqlDatabase) {
    return { readSaveSelfServiceLeaveEmployee(person_id: string, companyId: string) {
            return database.prepare('SELECT id FROM employees WHERE person_id=? AND company_id=?').bind(person_id, companyId).first<{
                id: string;
            }>();
        },
        stageSaveSelfServiceLeaveEffect(employeeId: string, person_id: string, companyId: string, value: string, department_id: string | null, legal_name: string, now: number) { return database.prepare("INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'active',?)").bind(employeeId, person_id, companyId, value, department_id, legal_name, now); } };
}
