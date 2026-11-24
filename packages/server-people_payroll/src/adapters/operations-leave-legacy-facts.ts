import { nativeLeaveLegacy } from "@erpx/server-collaboration/adapters/native-leave-legacy";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_LEAVE_LEGACY_EMPLOYEE,
} from "./native-people-payroll-read";
import "server-only";
export function leaveLegacyEmployeeFacts(database: SqlDatabase) {
    return { async readCreateLeaveEmployee(person_id: string, companyId: string) {
            return (await nativePeoplePayrollReadAll<{id:string}>(PEOPLE_PAYROLL_RD_LEAVE_LEGACY_EMPLOYEE,[person_id,companyId],database)).results[0]??null;
        },
        stageCreateLeaveEffect(employeeId: string, person_id: string, companyId: string, value: string, department_id: string | null, legal_name: string, now: number) {
            return nativeLeaveLegacy(database).employee(companyId, [employeeId, person_id, companyId, value, department_id, legal_name, now]);
        },
        stageCreateLeaveEffect2(id: string, value: string, person_id: string, companyId: string, person_id2: string, engagement_id: string, companyId2: string, value2: string, department_id: string | null, startDate: string, days: number, handoverNote: string, userId: string, now: number, now2: number) {
            return nativeLeaveLegacy(database).request(companyId, [id, value, person_id, companyId, person_id2, engagement_id, companyId2, value2, department_id, startDate, days, handoverNote, userId, now, now2]);
        } };
}
