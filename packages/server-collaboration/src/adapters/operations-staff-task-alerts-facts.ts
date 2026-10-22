import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/hospitality-rollout";
export function StaffTaskAlertsCollaborationFacts(database: SqlDatabase) {
    return { stageEscalateStaffTasksResult(value: string, company_id: string, userId: string, value2: "staff_task_alert", id: string, version: number, message: string, now: number, value3: string[]) {
            return database.prepare(`INSERT INTO staff_notifications
             (id,company_id,user_id,record_type,record_id,record_version,message,created_at)
           SELECT ?,?,?,?,?,?,?,? WHERE ${scheduledFeaturePredicate("?", "operations")}
           ON CONFLICT DO NOTHING`)
                .bind(value, company_id, userId, value2, id, version, message, now, ...value3)
                .run();
        } };
}
