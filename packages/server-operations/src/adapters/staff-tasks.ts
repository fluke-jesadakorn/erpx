import{taskWorkState,canAttachTask}from '@erpx/server-operations/public/staff-task-read';
export{taskWorkState,canAttachTask};
import{nativeTaskReadRepository,taskSelect,taskAccess}from '@erpx/server-operations/adapters/staff-task-read';

import { finalRootOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/final-root-owner-projection-sources";

import { getProject } from "@erpx/server-projects/public/staff-projects";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {readablePost} from '@erpx/server-collaboration/public/social-source';
import { taskSourceStatements, type TaskSource } from "@erpx/server-collaboration/public/social-task";
import { z } from "zod";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {approvalView,closeApprovals,taskRoute} from '@erpx/server-workflow/public/approval-planning';

import { activeStaff, staffGuard, notification, type StaffActor } from "@erpx/server-operations/public/staff-policy";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { taskFieldsSchema, taskSchema, type TaskRow, type TaskDetail, canConfigureTask as purecanConfigureTask, type TaskWorkState } from '@erpx/server-operations/domain/staff-tasks';
import { createStaffTasksApplication, type StaffTasksRepository } from '@erpx/server-operations/application/staff-tasks';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { StaffTasksIdentityAccessFacts } from '@erpx/server-identity_access/public/operations-staff-tasks-facts';
import { StaffTasksProjectsFacts } from '@erpx/server-projects/public/operations-staff-tasks-facts';
import { StaffTasksPeoplePayrollFacts } from '@erpx/server-people_payroll/public/operations-staff-tasks-facts';
export { taskFieldsSchema } from '@erpx/server-operations/domain/staff-tasks';
export { taskSchema } from '@erpx/server-operations/domain/staff-tasks';
export { projectSchema } from '@erpx/server-operations/domain/staff-tasks';
export { taskActionSchema } from '@erpx/server-operations/domain/staff-tasks';
export { taskDelegationSchema } from '@erpx/server-operations/domain/staff-tasks';
export { taskDelegationRevokeSchema } from '@erpx/server-operations/domain/staff-tasks';
export type { TaskRow } from '@erpx/server-operations/domain/staff-tasks';
export type { TaskDetail } from '@erpx/server-operations/domain/staff-tasks';
export type { ProjectRow } from '@erpx/server-operations/domain/staff-tasks';
export type { TaskWorkState } from '@erpx/server-operations/domain/staff-tasks';

import {projectTaskPurchaseAccess,projectTaskReadScope,projectTaskWorkState} from '@erpx/server-projects/public/task-purchases';
import {workTaskPurchaseAccess,workTaskReadScope,workTaskWorkState,withWorkTaskPurchasedCommand} from '@erpx/server-operations/adapters/work-task-purchases';
function application(provided?: SqlDatabase, projectReadWhere='TRUE',workReadArgs:(string|number|null)[]=[]) {
    let database = provided!;
    const repository: StaffTasksRepository<SqlStatement> = { readListTasksRows(companyId: string, userId: string, userId2: string, userId3: string, userId4: string, userId5: string, userId6: string, userId7: string, userId8: string, value: string | null, value2: string | null) {
            return database.prepare(`${taskSelect()} WHERE t.company_id=? AND ${taskAccess()} AND (${projectReadWhere}) AND (?::text IS NULL OR t.project_id=?) ORDER BY t.updated_at DESC,t.id LIMIT 250`)
                .bind(companyId, userId, userId2, userId3, userId4, userId5, userId6, userId7, userId8,...workReadArgs, value, value2)
                .all<TaskRow>();
        },
        readGetTaskRow(...args){return nativeTaskReadRepository(database,projectReadWhere,workReadArgs).readGetTaskRow(...args);},
        guard3(...args){return nativeTaskReadRepository(database,projectReadWhere,workReadArgs).guard3(...args);},
        guard11(...args) { return StaffTasksIdentityAccessFacts(database).guard11(...args); },
        guard12(key: string, value: "task-version", id: string, version: number, states: string[]) {
            return assertion(database, key, value, `EXISTS(SELECT 1 FROM staff_tasks WHERE id=? AND version=? AND state IN (${states.map(() => "?").join(",")}))`, [id, version, ...states]);
        },
        guard13(...args) { return StaffTasksProjectsFacts(database).guard13(...args); },
        stageSaveTaskEffect(title: string, description: string, assigneeUserId: string, dueAt: number | null, projectId: string | null, value: string, value2: string, nextState: "ready" | "draft", value3: 0 | 1, now: number, recordId: string) {
            return database.prepare("UPDATE staff_tasks SET title=?,description=?,assignee_user_id=?,due_at=?,project_id=?,start_approvers_json=?,completion_approvers_json=?,state=?,completion_note='',blocked_reason=NULL,revision=revision+?,version=version+1,updated_at=? WHERE id=?")
                .bind(title, description, assigneeUserId, dueAt, projectId, value, value2, nextState, value3, now, recordId);
        },
        stageSaveTaskEffect2(recordId: string, companyId: string, branch_id: string | null, department_id: string | null, title: string, description: string, userId: string, assigneeUserId: string, dueAt: number | null, projectId: string | null, value: string, value2: string, nextState: "ready" | "draft", now: number, now2: number) {
            return database.prepare("INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,due_at,project_id,start_approvers_json,completion_approvers_json,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(recordId, companyId, branch_id, department_id, title, description, userId, assigneeUserId, dueAt, projectId, value, value2, nextState, now, now2);
        },
        readTaskDetailAttachments(id: string) {
            return database.prepare(`SELECT id,original_name,bytes,media_type FROM (${filesProjectionSources.attachments}) attachments WHERE owner_type='staff_task' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at`)
                .bind(id)
                .all<TaskDetail["attachments"][number]>();
        },
        readTaskDetailDelegationHistory(...args) { return StaffTasksPeoplePayrollFacts(database).readTaskDetailDelegationHistory(...args); },
        };
    return createStaffTasksApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, getProject: (...args) => fromNative(() => getProject(database, ...args)), readablePost: (...args) => fromNative(() => readablePost(database, ...args)), taskSourceStatements: (...args) => fromNative(() => taskSourceStatements(database, ...args)), approvalView: (...args) => fromNative(() => approvalView(database, ...args)), closeApprovals: (...args) => closeApprovals(database, ...args), taskRoute: (...args) => fromNative(() => taskRoute(database, ...args)), activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), staffGuard: (...args) => staffGuard(database, ...args), notification: (...args) => notification(database, ...args) });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
function invokeSync<Result>(run: () => Result) {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function listTasks(db: SqlDatabase, actor: StaffActor, projectId?: string) { const project=await projectTaskReadScope(db,actor),work=await workTaskReadScope(db,actor);return invoke(() => application(db,`(${project.where}) AND (${work.where})`,work.args).listTasks(actor, projectId)); }

export function canConfigureTask(actor: StaffActor, task: TaskRow) { return invokeSync(() => purecanConfigureTask(actor, task)); }
export function taskVersionGuard(db: SqlDatabase, key: string, id: string, version: number, states: string[]) { return invokeSync(() => application(db).taskVersionGuard(key, id, version, states)); }
export async function validateTaskConfiguration(db: SqlDatabase, actor: StaffActor, input: z.infer<typeof taskFieldsSchema>, creator = actor.userId, serviceTask = false) { return invoke(() => application(db).validateTaskConfiguration(actor, input, creator, serviceTask)); }
export function configGuards(db: SqlDatabase, key: string, actor: StaffActor, input: z.infer<typeof taskFieldsSchema>) { return invokeSync(() => application(db).configGuards(key, actor, input)); }
export async function saveTask(db: SqlDatabase, actor: StaffActor, id: string | null, input: z.infer<typeof taskSchema>, key: string, revision = false, source?: TaskSource) { return withWorkTaskPurchasedCommand(db,actor,{key,operations:[revision?'task.revise':id?'task.update':'task.create'],id:id??(source?`task-post-${source.postId}`:`task-${key}`),purpose:'new-work',configure:true,projectId:input.projectId,assigneeUserId:input.assigneeUserId,sourcePostId:source?.postId},tx=>invoke(() => application(tx).saveTask(actor, id, input, key, revision, source))); }

export async function taskDetail(db: SqlDatabase, actor: StaffActor, id: string): Promise<TaskDetail> { await projectTaskPurchaseAccess(db,actor,id);await workTaskPurchaseAccess(db,actor,id);const project=await projectTaskReadScope(db,actor),work=await workTaskReadScope(db,actor),detail=await invoke(() => application(db,`(${project.where}) AND (${work.where})`,work.args).taskDetail(actor, id));return {...detail,...await workTaskWorkState(db,actor,id,await projectTaskWorkState(db,actor,id,detail))}; }

export { getProject, listProjects } from "@erpx/server-projects/public/staff-projects";
