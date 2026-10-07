import type { SqlDatabase } from '../../runtime/public/runtime/types';
export function projectStructureSources(db: SqlDatabase) {
    return { prepareHandoffNotice(value: string, companyId: string, recipientId: string, handoffId: string, now: number) { return db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'project_handoff',?,1,'Review the completed project and frozen handoff evidence',?)").bind(value, companyId, recipientId, handoffId, now); },
        prepareHandoffDecisionNotice(value: string, companyId: string, prepared_by: string, id: string, value2: number, value3: string, now: number) { return db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'project_handoff',?,?,?,?)").bind(value, companyId, prepared_by, id, value2, value3, now); } };
}
