import { nativeLeaveLegacy } from "@erpx/server-collaboration/adapters/native-leave-legacy";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
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
            return nativeLeaveLegacy(database).employee(companyId, [employeeId, person_id, companyId, value, department_id, legal_name, now]);
        },
        stageCreateLeaveEffect2(id: string, value: string, person_id: string, companyId: string, person_id2: string, engagement_id: string, companyId2: string, value2: string, department_id: string | null, startDate: string, days: number, handoverNote: string, userId: string, now: number, now2: number) {
            return nativeLeaveLegacy(database).request(companyId, [id, value, person_id, companyId, person_id2, engagement_id, companyId2, value2, department_id, startDate, days, handoverNote, userId, now, now2]);
        } };
}
