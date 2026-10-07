import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
export function leaveLegacyEmployeeFacts(database: SqlDatabase) {
    return { readCreateLeaveEmployee(person_id: string, companyId: string) {
            return database.prepare("SELECT id FROM employees WHERE person_id=? AND company_id=?")
                .bind(person_id, companyId)
                .first<{
                id: string;
            }>();
        },
        stageCreateLeaveEffect(employeeId: string, person_id: string, companyId: string, value: string, department_id: string | null, legal_name: string, now: number) {
            return database.prepare("INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'active',?) ON CONFLICT DO NOTHING")
                .bind(employeeId, person_id, companyId, value, department_id, legal_name, now);
        },
        stageCreateLeaveEffect2(id: string, value: string, person_id: string, companyId: string, person_id2: string, engagement_id: string, companyId2: string, value2: string, department_id: string | null, startDate: string, days: number, handoverNote: string, userId: string, now: number, now2: number) {
            return database.prepare("INSERT INTO leave_requests(id,number,employee_id,person_id,engagement_id,company_id,branch_id,department_id,start_date,days,handover_note,created_by,created_at,updated_at) VALUES(?,?,(SELECT id FROM employees WHERE person_id=? AND company_id=?),?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, value, person_id, companyId, person_id2, engagement_id, companyId2, value2, department_id, startDate, days, handoverNote, userId, now, now2);
        } };
}
