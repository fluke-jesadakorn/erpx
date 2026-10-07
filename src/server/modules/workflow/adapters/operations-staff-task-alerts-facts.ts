import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function StaffTaskAlertsWorkflowFacts(database: SqlDatabase) {
    return { readEscalateStaffTasksApprovers(company_id: string, id: string) {
            return database.prepare(`SELECT s.approver_user_id
           FROM approval_instances i
           JOIN approval_steps s ON s.instance_id=i.id AND s.state='pending'
          WHERE i.company_id=? AND i.record_type='task' AND i.record_id=? AND i.state='pending'`)
                .bind(company_id, id)
                .all<{
                approver_user_id: string;
            }>();
        } };
}
