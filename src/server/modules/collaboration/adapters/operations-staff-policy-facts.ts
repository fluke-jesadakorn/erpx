import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function StaffPolicyCollaborationFacts(database: SqlDatabase) {
    return { stageNotificationEffect(value: string, company_id: string, userId: string, type: string, id: string, version: number, message: string, now: number) {
            return database.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
                .bind(value, company_id, userId, type, id, version, message, now);
        } };
}
