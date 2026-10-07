import { finalRootOwnerProjectionSources as projectsProjectionSources } from "../../projects/public/final-root-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { scheduledFeaturePredicate } from "@/server/modules/pms/public/hospitality-rollout";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { type StaffTaskAlertResult, type TaskAlertRow } from '../domain/staff-task-alerts';
import { createStaffTaskAlertsApplication, type StaffTaskAlertsRepository } from '../application/staff-task-alerts';
import { BusinessRuleError } from '../../shared';
import type { SqlStatement } from '../../runtime/public/runtime/types';
import { StaffTaskAlertsWorkflowFacts } from '../../workflow/public/operations-staff-task-alerts-facts';
import { StaffTaskAlertsCollaborationFacts } from '../../collaboration/public/operations-staff-task-alerts-facts';
export type { StaffTaskAlertResult } from '../domain/staff-task-alerts';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: StaffTaskAlertsRepository = { readEscalateStaffTasksRows(now: number, now2: number, value: number) {
            return database.prepare(`SELECT t.id,t.company_id,t.title,t.due_at,t.version,t.assignee_user_id,
          t.creator_user_id,p.owner_user_id project_owner_id,d.delegate_user_id
         FROM staff_tasks t
         LEFT JOIN (${projectsProjectionSources.staff_projects}) p ON p.id=t.project_id AND p.company_id=t.company_id
         LEFT JOIN LATERAL (SELECT sd.delegate_user_id FROM staff_task_delegations sd
           WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active'
             AND sd.starts_at<=? AND sd.expires_at>?
           ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) d ON TRUE
         WHERE t.state NOT IN ('completed','cancelled') AND t.due_at IS NOT NULL AND t.due_at<=?
           AND ${scheduledFeaturePredicate("t.company_id", "operations")}
         ORDER BY t.due_at,t.id
         LIMIT 500`)
                .bind(now, now2, value)
                .all<TaskAlertRow>();
        },
        readEscalateStaffTasksApprovers(...args) { return StaffTaskAlertsWorkflowFacts(database).readEscalateStaffTasksApprovers(...args); },
        stageEscalateStaffTasksResult(...args) { return StaffTaskAlertsCollaborationFacts(database).stageEscalateStaffTasksResult(...args); } };
    return createStaffTaskAlertsApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => { if (!provided)
            database = (await nativeHoldingResources(holding)).write; }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, });
}
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function escalateStaffTasks(db: SqlDatabase, now = Date.now(), warningWindowMs = 86400000): Promise<StaffTaskAlertResult> { return invoke(() => application(db).escalateStaffTasks(now, warningWindowMs)); }
