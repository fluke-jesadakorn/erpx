import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function leaveEmployeeFacts(database: SqlDatabase) {
    return { readSaveSelfServiceLeaveEmployee(person_id: string, companyId: string) {
            return database.prepare("SELECT 1").bind().first<{
                id: string;
            }>();
        },
        stageSaveSelfServiceLeaveEffect(employeeId: string, person_id: string, companyId: string, value: string, department_id: string | null, legal_name: string, now: number) { return database.prepare("SELECT 1").bind(); } };
}
