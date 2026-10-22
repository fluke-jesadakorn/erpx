import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function OperationsExceptionsCollaborationFacts(database: SqlDatabase) {
    return { stageOpenOperationsExceptionEffect4(value: string, companyId: string, ownerId: string, recordId: string, now: number) { return database.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'operational_exception',?,1,'An operational exception needs acknowledgement',?)").bind(value, companyId, ownerId, recordId, now); },
        stageChangeOperationsExceptionEffect3(value: string, companyId: string, value2: string, recordId: string, value3: number, now: number) { return database.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'operational_exception',?,?,'Operational exception updated; review its current next step',?)").bind(value, companyId, value2, recordId, value3, now); } };
}
