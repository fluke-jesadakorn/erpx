import { nativePpStatement } from '@erpx/people-payroll-server/adapters/native-people-payroll-command';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
export function peopleDirectoryEmployeeFacts(database: SqlDatabase) {
    return { stageSaveDirectoryPersonEffect3(legalName: string, branchId: string, departmentId: string | null, id: string) {
            return nativePpStatement('pp_operations_people_directory_facts_stmt_1')
                .bind(legalName, branchId, departmentId, id);
        },
        stageSaveDirectoryPersonEffect6(value: string, id: string, companyId: string, branchId: string, departmentId: string | null, legalName: string, now: number) {
            return nativePpStatement('pp_operations_people_directory_facts_stmt_2')
                .bind(value, id, companyId, branchId, departmentId, legalName, now);
        },
        stageSaveDirectoryPersonEffect9(linkedMemberUserId: string, id: string) {
            return nativePpStatement('pp_operations_people_directory_facts_stmt_3')
                .bind(linkedMemberUserId, id);
        },
        stageSetDirectoryStatusEffect3(value: "active" | "inactive", personId: string) {
            return nativePpStatement('pp_operations_people_directory_facts_stmt_4')
                .bind(value, personId);
        } };
}
