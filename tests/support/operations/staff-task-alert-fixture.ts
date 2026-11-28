/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover Staff Work alert worker.
 *
 * Production alerts are now owned by the native staff_task_alert_worker
 * (platform/composition), which calls the owner `escalate_staff_tasks` application.
 * This copy keeps the legacy TS worker for the isolated integration harness.
 *
 * Tracked for removal by `retire.operations.staff-task-alert-worker`: delete
 * once the harness drives the native worker result directly.
 */
import {projectTaskAlertReadScope} from "@erpx/projects-server/public/scheduled-tasks";
import {workTaskAlertReadScope,withWorkTaskAlert} from "@erpx/operations-server/adapters/work-task-alert-purchases";
import { finalRootOwnerProjectionSources as projectsProjectionSources } from "@erpx/projects-server/public/final-root-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
import { holdingResources as nativeHoldingResources } from '../../../platform/runtime/server/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/identity-access-server/public/iam-read-snapshot';

import { ApiError as NativeApiError } from '@erpx/platform-runtime-server/public/http';
import { type StaffTaskAlertResult, type TaskAlertRow } from '@erpx/operations-server/domain/staff-task-alerts';
import { createStaffTaskAlertsApplication, type StaffTaskAlertsRepository } from '@erpx/operations-server/application/staff-task-alerts';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import type { SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { StaffTaskAlertsWorkflowFacts } from '@erpx/workflow-server/public/operations-staff-task-alerts-facts';
const StaffTaskAlertsCollaborationFacts = (database: SqlDatabase) => ({ stageEscalateStaffTasksResult(value: string, company_id: string, userId: string, value2: string, id: string, version: number, message: string, now: number, value3: string[]) {
  return database.prepare(`INSERT INTO staff_notifications (id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,?,?,?,?,?,?,? WHERE ${scheduledFeaturePredicate("?", "operations")} ON CONFLICT DO NOTHING`)
    .bind(value, company_id, userId, value2, id, version, message, now, ...value3).run();
} });
export type { StaffTaskAlertResult } from '@erpx/operations-server/domain/staff-task-alerts';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: StaffTaskAlertsRepository = { async readEscalateStaffTasksRows(now: number, now2: number, value: number) {
            const projectRead=await projectTaskAlertReadScope(now),workRead=await workTaskAlertReadScope(now,value);
            return database.prepare(`${workRead.prefix}SELECT t.id,t.company_id,t.project_id,t.title,t.due_at,t.version,t.assignee_user_id,
          t.creator_user_id,p.owner_user_id project_owner_id,d.delegate_user_id
         FROM staff_tasks t
         LEFT JOIN (${projectsProjectionSources.staff_projects}) p ON p.id=t.project_id AND p.company_id=t.company_id
         LEFT JOIN LATERAL (SELECT sd.delegate_user_id FROM staff_task_delegations sd
           WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active'
             AND sd.starts_at<=? AND sd.expires_at>?
           ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) d ON TRUE
         WHERE t.state NOT IN ('completed','cancelled') AND t.due_at IS NOT NULL AND t.due_at<=?
           AND ${scheduledFeaturePredicate("t.company_id", "operations")} AND (${projectRead}) AND (${workRead.where})
         ORDER BY t.due_at,t.id
         LIMIT 500`)
                .bind(...workRead.args,now, now2, value)
                .all<TaskAlertRow>();
        },
        readEscalateStaffTasksApprovers(...args) { return StaffTaskAlertsWorkflowFacts(database).readEscalateStaffTasksApprovers(...args); },
        async stageEscalateStaffTasksResult(value,companyId,userId,recordType,id,version,message,now,releaseArgs,source) {
          return await withWorkTaskAlert(database,source,{id,companyId,userId,version},transaction=>StaffTaskAlertsCollaborationFacts(transaction).stageEscalateStaffTasksResult(value,companyId,userId,recordType,id,version,message,now,releaseArgs))??{success:true,meta:{changes:0,duration:0}};
        } };
    return createStaffTaskAlertsApplication<SqlStatement>({repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, prepare: async (holding) => { if (!provided)
            database = (await nativeHoldingResources(holding)).write; }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow});
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
