import { finalRootOwnerProjectionSources as collaborationProjectionSources } from "@erpx/server-collaboration/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import { projectReadProjection } from "@erpx/server-projects/public/project-read-projections";
function taskSelect(){return `SELECT t.*,o.post_id source_post_id,p.title project_title,p.owner_user_id project_owner_id,COALESCE(a.legal_name,t.assignee_user_id) assignee_name,COALESCE(c.legal_name,t.creator_user_id) creator_name,d.id active_delegation_id,d.delegate_user_id active_delegate_user_id,COALESCE(da.legal_name,d.delegate_user_id) active_delegate_name,d.expires_at active_delegate_expires_at,d.reason active_delegate_reason,d.version active_delegate_version FROM staff_tasks t LEFT JOIN (${collaborationProjectionSources.social_task_origins}) o ON o.task_id=t.id ${projectReadProjection.taskProject} LEFT JOIN LATERAL (SELECT sd.id,sd.delegate_user_id,sd.expires_at,sd.reason,sd.version FROM staff_task_delegations sd WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active' AND sd.starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND sd.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) d ON TRUE LEFT JOIN (${people_payrollProjectionSources.hr_people}) a ON a.user_id=t.assignee_user_id AND a.company_id=t.company_id AND a.merged_into_id IS NULL LEFT JOIN (${people_payrollProjectionSources.hr_people}) c ON c.user_id=t.creator_user_id AND c.company_id=t.company_id AND c.merged_into_id IS NULL LEFT JOIN (${people_payrollProjectionSources.hr_people}) da ON da.user_id=d.delegate_user_id AND da.company_id=t.company_id AND da.merged_into_id IS NULL`;}
import { getProject } from "@erpx/server-projects/public/staff-projects";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { readablePost } from "@erpx/server-collaboration/public/social-source";
import { taskSourceStatements, type TaskSource } from "@erpx/server-collaboration/public/social-task";
import { z } from "zod";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { approvalView, closeApprovals, openApproval, participantSql, prepareDecision, taskRoute } from "@erpx/server-workflow/public/approvals";
import { activeStaff, staffGuard, notification, type StaffActor } from "@erpx/server-operations/public/staff-policy";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { taskFieldsSchema, taskSchema, taskActionSchema, taskDelegationSchema, taskDelegationRevokeSchema, type TaskRow, type TaskDetail, canConfigureTask as purecanConfigureTask, type TaskWorkState } from '@erpx/server-operations/domain/staff-tasks';
import { createStaffTasksApplication, type StaffTasksRepository } from '@erpx/server-operations/application/staff-tasks';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { StaffTasksCollaborationFacts } from '@erpx/server-collaboration/public/operations-staff-tasks-facts';
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
function taskAccess(){return `(iam_same_person(t.creator_user_id,?,t.company_id) OR iam_same_person(t.assignee_user_id,?,t.company_id) OR iam_same_person(d.delegate_user_id,?,t.company_id) OR iam_same_person(p.owner_user_id,?,t.company_id) OR ${projectReadProjection.taskMember} OR EXISTS(SELECT 1 FROM erpx_json_each(t.start_approvers_json) WHERE value=?) OR EXISTS(SELECT 1 FROM erpx_json_each(t.completion_approvers_json) WHERE value=?) OR ${participantSql("task", "t")})`;}
import {projectTaskPurchaseAccess,projectTaskReadScope,projectTaskWorkState} from '@erpx/server-projects/public/task-purchases';
import {workTaskPurchaseAccess,workTaskReadScope,workTaskWorkState,withWorkTaskPurchasedCommand} from '@erpx/server-operations/adapters/work-task-purchases';
function application(provided?: SqlDatabase, projectReadWhere='TRUE',workReadArgs:(string|number|null)[]=[]) {
    let database = provided!;
    const repository: StaffTasksRepository<SqlStatement> = { readListTasksRows(companyId: string, userId: string, userId2: string, userId3: string, userId4: string, userId5: string, userId6: string, userId7: string, userId8: string, value: string | null, value2: string | null) {
            return database.prepare(`${taskSelect()} WHERE t.company_id=? AND ${taskAccess()} AND (${projectReadWhere}) AND (?::text IS NULL OR t.project_id=?) ORDER BY t.updated_at DESC,t.id LIMIT 250`)
                .bind(companyId, userId, userId2, userId3, userId4, userId5, userId6, userId7, userId8,...workReadArgs, value, value2)
                .all<TaskRow>();
        },
        readGetTaskRow(id: string, companyId: string, userId: string, userId2: string, userId3: string, userId4: string, userId5: string, userId6: string, userId7: string, userId8: string) {
            return database.prepare(`${taskSelect()} WHERE t.id=? AND t.company_id=? AND ${taskAccess()} AND (${projectReadWhere})`)
                .bind(id, companyId, userId, userId2, userId3, userId4, userId5, userId6, userId7, userId8,...workReadArgs)
                .first<TaskRow>();
        },
        guard3(key: string, step: string, id: string, version: number, companyId: string, value: string[]) {
            return assertion(database, key, step, `EXISTS(${taskSelect()} WHERE t.id=? AND t.version=? AND t.company_id=? AND ${taskAccess()})`, [id, version, companyId, ...value]);
        },
        guard4(key: string, value: "task-delegation-unique", companyId: string, id: string, now: number, now2: number) { return assertion(database, key, value, `NOT EXISTS(SELECT 1 FROM staff_task_delegations WHERE company_id=? AND task_id=? AND status='active' AND starts_at<=? AND expires_at>?)`, [companyId, id, now, now2]); },
        stageDelegateTaskEffect(delegationId: string, companyId: string, id: string, userId: string, delegateUserId: string, now: number, expiresAt: number, reason: string, now2: number, now3: number) { return database.prepare("INSERT INTO staff_task_delegations(id,company_id,task_id,delegator_user_id,delegate_user_id,starts_at,expires_at,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(delegationId, companyId, id, userId, delegateUserId, now, expiresAt, reason, now2, now3); },
        stageDelegateTaskEffect2(now: number, id: string, companyId: string) { return database.prepare("UPDATE staff_tasks SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, id, companyId); },
        readRevokeTaskDelegationDelegation(delegationId: string, companyId: string, id: string, now: number, now2: number) {
            return database.prepare("SELECT * FROM staff_task_delegations WHERE id=? AND company_id=? AND task_id=? AND status='active' AND starts_at<=? AND expires_at>?").bind(delegationId, companyId, id, now, now2).first<{
                id: string;
                delegate_user_id: string;
                version: number;
            }>();
        },
        guard8(key: string, value: "task-delegation-active", id: string, companyId: string, id2: string, now: number, now2: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM staff_task_delegations WHERE id=? AND company_id=? AND task_id=? AND status='active' AND starts_at<=? AND expires_at>?)", [id, companyId, id2, now, now2]); },
        stageRevokeTaskDelegationEffect(userId: string, reason: string, now: number, now2: number, id: string, companyId: string) { return database.prepare("UPDATE staff_task_delegations SET status='revoked',revoked_by=?,revoke_reason=?,revoked_at=?,updated_at=?,version=version+1 WHERE id=? AND company_id=?").bind(userId, reason, now, now2, id, companyId); },
        stageRevokeTaskDelegationEffect2(now: number, id: string, companyId: string) { return database.prepare("UPDATE staff_tasks SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, id, companyId); },
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
        stageTransitionTaskEffect(state: string, action: "submit" | "start" | "save-progress" | "complete" | "approve" | "reject" | "cancel", note: string, now: number, id: string) {
            return database.prepare("UPDATE staff_tasks SET state=?,completion_note=CASE WHEN ? IN ('complete','save-progress') THEN ? ELSE completion_note END,blocked_reason=NULL,version=version+1,updated_at=? WHERE id=?")
                .bind(state, action, note, now, id);
        },
        readTransitionTaskFollowers(...args) { return StaffTasksCollaborationFacts(database).readTransitionTaskFollowers(...args); } };
    return createStaffTasksApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, getProject: (...args) => fromNative(() => getProject(database, ...args)), readablePost: (...args) => fromNative(() => readablePost(database, ...args)), taskSourceStatements: (...args) => fromNative(() => taskSourceStatements(database, ...args)), approvalView: (...args) => fromNative(() => approvalView(database, ...args)), closeApprovals: (...args) => closeApprovals(database, ...args), openApproval: (...args) => fromNative(() => openApproval(database, ...args)), prepareDecision: (...args) => fromNative(() => prepareDecision(database, ...args)), taskRoute: (...args) => fromNative(() => taskRoute(database, ...args)), activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), staffGuard: (...args) => staffGuard(database, ...args), notification: (...args) => notification(database, ...args), prepareAttachmentBinding: (...args) => fromNative(() => prepareAttachmentBinding(database, ...args)) });
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
export async function getTask(db: SqlDatabase, actor: StaffActor, id: string) { await projectTaskPurchaseAccess(db,actor,id);await workTaskPurchaseAccess(db,actor,id);const project=await projectTaskReadScope(db,actor),work=await workTaskReadScope(db,actor);return invoke(() => application(db,`(${project.where}) AND (${work.where})`,work.args).getTask(actor, id)); }
export function taskSourceReadGuard(db: SqlDatabase, actor: StaffActor, id: string, version: number, key: string, step: string) { return invokeSync(() => application(db).taskSourceReadGuard(actor, id, version, key, step)); }
export function canConfigureTask(actor: StaffActor, task: TaskRow) { return invokeSync(() => purecanConfigureTask(actor, task)); }
export async function delegateTask(db: SqlDatabase, actor: StaffActor, id: string, input: z.infer<typeof taskDelegationSchema>, key: string) { return withWorkTaskPurchasedCommand(db,actor,{key,operations:['task.delegate'],id,purpose:'settle-existing'},tx=>invoke(() => application(tx).delegateTask(actor, id, input, key))); }
export async function revokeTaskDelegation(db: SqlDatabase, actor: StaffActor, id: string, input: z.infer<typeof taskDelegationRevokeSchema>, key: string) { return withWorkTaskPurchasedCommand(db,actor,{key,operations:['task.delegate-revoke'],id,purpose:'settle-existing'},tx=>invoke(() => application(tx).revokeTaskDelegation(actor, id, input, key))); }
export function taskVersionGuard(db: SqlDatabase, key: string, id: string, version: number, states: string[]) { return invokeSync(() => application(db).taskVersionGuard(key, id, version, states)); }
export async function validateTaskConfiguration(db: SqlDatabase, actor: StaffActor, input: z.infer<typeof taskFieldsSchema>, creator = actor.userId, serviceTask = false) { return invoke(() => application(db).validateTaskConfiguration(actor, input, creator, serviceTask)); }
export function configGuards(db: SqlDatabase, key: string, actor: StaffActor, input: z.infer<typeof taskFieldsSchema>) { return invokeSync(() => application(db).configGuards(key, actor, input)); }
export async function saveTask(db: SqlDatabase, actor: StaffActor, id: string | null, input: z.infer<typeof taskSchema>, key: string, revision = false, source?: TaskSource) { return withWorkTaskPurchasedCommand(db,actor,{key,operations:[revision?'task.revise':id?'task.update':'task.create'],id:id??(source?`task-post-${source.postId}`:`task-${key}`),purpose:'new-work',configure:true,projectId:input.projectId,assigneeUserId:input.assigneeUserId,sourcePostId:source?.postId},tx=>invoke(() => application(tx).saveTask(actor, id, input, key, revision, source))); }
export async function taskWorkState(db: SqlDatabase, actor: StaffActor, task: TaskRow): Promise<TaskWorkState> { return workTaskWorkState(db,actor,task.id,await projectTaskWorkState(db,actor,task.id,await invoke(() => application(db).taskWorkState(actor, task)))); }
export async function taskDetail(db: SqlDatabase, actor: StaffActor, id: string): Promise<TaskDetail> { await projectTaskPurchaseAccess(db,actor,id);await workTaskPurchaseAccess(db,actor,id);const project=await projectTaskReadScope(db,actor),work=await workTaskReadScope(db,actor),detail=await invoke(() => application(db,`(${project.where}) AND (${work.where})`,work.args).taskDetail(actor, id));return {...detail,...await workTaskWorkState(db,actor,id,await projectTaskWorkState(db,actor,id,detail))}; }
export async function transitionTask(db: SqlDatabase, actor: WorkContext, id: string, input: z.infer<typeof taskActionSchema>, key: string) { return withWorkTaskPurchasedCommand(db,actor,{key,operations:[`task.${input.action}`],id,purpose:input.action==='submit'?'new-work':'settle-existing'},tx=>invoke(() => application(tx).transitionTask(actor, id, input, key))); }
export function canAttachTask(actor: StaffActor, task: TaskRow) { return invokeSync(() => application().canAttachTask(actor, task)); }
export { getProject, listProjects, saveProject } from "@erpx/server-projects/public/staff-projects";
