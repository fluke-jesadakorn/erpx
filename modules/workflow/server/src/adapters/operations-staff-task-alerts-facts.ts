import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeWorkflowReadAll } from "./native-workflow-read";

/** Owner-native read: the staff task alert approver statement is planned by the
 * mounted Workflow wave3 command planner (WF_NATIVE_STAFF_TASK_ALERT_APPROVERS)
 * and executed on the caller's open unit. This module carries no SQL. */
export function StaffTaskAlertsWorkflowFacts(database: SqlDatabase) {
    return {
        async readEscalateStaffTasksApprovers(company_id: string, id: string) {
            return {
                results: await nativeWorkflowReadAll<{ approver_user_id: string }>(
                    "wf_native_staff_task_alert_approvers",
                    [company_id, id],
                    company_id,
                    database,
                ),
            };
        },
    };
}
