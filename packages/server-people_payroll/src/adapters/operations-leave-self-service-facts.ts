import {ppRd32TwoColProjection} from './pp-read-tail-r32-sources';
import "server-only";
import { nativePeopleBatchA } from '@erpx/server-collaboration/adapters/native-people-batch-a';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function leaveEmployeeFacts(database: SqlDatabase) {
    return { readSaveSelfServiceLeaveEmployee(person_id: string, companyId: string) {
            return database.prepare(ppRd32TwoColProjection("id","employees","person_id","company_id")).bind(person_id, companyId).first<{
                id: string;
            }>();
        },
        stageSaveSelfServiceLeaveEffect(employeeId: string, person_id: string, companyId: string, value: string, department_id: string | null, legal_name: string, now: number) { return nativePeopleBatchA(database).employee({employeeId,personId:person_id,companyId,branch:value,department:department_id,legalName:legal_name,now}); } };
}
