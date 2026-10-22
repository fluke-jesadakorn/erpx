import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { z } from "zod";
import type { TaskSource } from "@erpx/server-collaboration/public/social-task";
import type { RecordScope, StaffActor } from "@erpx/server-operations/public/staff-policy";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { getProject } from "@erpx/server-projects/public/staff-projects";
import type { readablePost } from "@erpx/server-collaboration/public/social-source";
import type { taskSourceStatements } from "@erpx/server-collaboration/public/social-task";
import type { approvalView } from "@erpx/server-workflow/public/approvals";
import type { closeApprovals } from "@erpx/server-workflow/public/approvals";
import type { openApproval } from "@erpx/server-workflow/public/approvals";
import type { prepareDecision } from "@erpx/server-workflow/public/approvals";
import type { taskRoute } from "@erpx/server-workflow/public/approvals";
import type { activeStaff } from "@erpx/server-operations/public/staff-policy";
import type { staffGuard } from "@erpx/server-operations/public/staff-policy";
import type { notification } from "@erpx/server-operations/public/staff-policy";
import type { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { taskFieldsSchema, taskSchema, taskActionSchema, taskDelegationSchema, taskDelegationRevokeSchema, type TaskRow, isOrdinaryTask, type TaskDetail, canConfigureTask, isActiveDelegate, type TaskWorkState } from '@erpx/server-operations/domain/staff-tasks';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { OperationsCommands } from '@erpx/server-operations/domain/commands';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: never[]) => unknown ? Value : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface StaffTasksRepository<Effect> {
    readListTasksRows(companyId: string, userId: string, userId2: string, userId3: string, userId4: string, userId5: string, userId6: string, userId7: string, userId8: string, value: string | null, value2: string | null): Promise<{
        results: (TaskRow)[];
    }>;
    readGetTaskRow(id: string, companyId: string, userId: string, userId2: string, userId3: string, userId4: string, userId5: string, userId6: string, userId7: string, userId8: string): Promise<TaskRow | null>;
    guard3(key: string, step: string, id: string, version: number, companyId: string, value: string[]): Effect;
    guard4(key: string, value: "task-delegation-unique", companyId: string, id: string, now: number, now2: number): Effect;
    stageDelegateTaskEffect(delegationId: string, companyId: string, id: string, userId: string, delegateUserId: string, now: number, expiresAt: number, reason: string, now2: number, now3: number): Effect;
    stageDelegateTaskEffect2(now: number, id: string, companyId: string): Effect;
    readRevokeTaskDelegationDelegation(delegationId: string, companyId: string, id: string, now: number, now2: number): Promise<{
        id: string;
        delegate_user_id: string;
        version: number;
    } | null>;
    guard8(key: string, value: "task-delegation-active", id: string, companyId: string, id2: string, now: number, now2: number): Effect;
    stageRevokeTaskDelegationEffect(userId: string, reason: string, now: number, now2: number, id: string, companyId: string): Effect;
    stageRevokeTaskDelegationEffect2(now: number, id: string, companyId: string): Effect;
    guard11(key: string, value: "task-configure", id: string, companyId: string, userId: string, userId2: string): Effect;
    guard12(key: string, value: "task-version", id: string, version: number, states: string[]): Effect;
    guard13(key: string, value: "task-project", projectId: string, companyId: string, userId: string, userId2: string): Effect;
    stageSaveTaskEffect(title: string, description: string, assigneeUserId: string, dueAt: number | null, projectId: string | null, value: string, value2: string, nextState: "ready" | "draft", value3: 0 | 1, now: number, recordId: string): Effect;
    stageSaveTaskEffect2(recordId: string, companyId: string, branch_id: string | null, department_id: string | null, title: string, description: string, userId: string, assigneeUserId: string, dueAt: number | null, projectId: string | null, value: string, value2: string, nextState: "ready" | "draft", now: number, now2: number): Effect;
    readTaskDetailAttachments(id: string): Promise<{
        results: (TaskDetail["attachments"][number])[];
    }>;
    readTaskDetailDelegationHistory(companyId: string, id: string): Promise<{
        results: (TaskDetail["delegationHistory"][number])[];
    }>;
    stageTransitionTaskEffect(state: string, action: "submit" | "start" | "save-progress" | "complete" | "approve" | "reject" | "cancel", note: string, now: number, id: string): Effect;
    readTransitionTaskFollowers(id: string): Promise<{
        results: ({
            principal_id: string;
        })[];
    }>;
}
export interface StaffTasksPorts<Effect> {
    repository: StaffTasksRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OperationsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    getProject(...args: Tail<Parameters<typeof getProject>>): PortResult<ReturnType<typeof getProject>, Effect>;
    readablePost(...args: Tail<Parameters<typeof readablePost>>): PortResult<ReturnType<typeof readablePost>, Effect>;
    taskSourceStatements(...args: Tail<Parameters<typeof taskSourceStatements>>): PortResult<ReturnType<typeof taskSourceStatements>, Effect>;
    approvalView(...args: Tail<Parameters<typeof approvalView>>): PortResult<ReturnType<typeof approvalView>, Effect>;
    closeApprovals(...args: Tail<Parameters<typeof closeApprovals>>): PortResult<ReturnType<typeof closeApprovals>, Effect>;
    openApproval(...args: Tail<Parameters<typeof openApproval>>): PortResult<ReturnType<typeof openApproval>, Effect>;
    prepareDecision(...args: Tail<Parameters<typeof prepareDecision>>): PortResult<ReturnType<typeof prepareDecision>, Effect>;
    taskRoute(...args: Tail<Parameters<typeof taskRoute>>): PortResult<ReturnType<typeof taskRoute>, Effect>;
    activeStaff(...args: Tail<Parameters<typeof activeStaff>>): PortResult<ReturnType<typeof activeStaff>, Effect>;
    staffGuard(...args: Tail<Parameters<typeof staffGuard>>): PortResult<ReturnType<typeof staffGuard>, Effect>;
    notification(...args: Tail<Parameters<typeof notification>>): PortResult<ReturnType<typeof notification>, Effect>;
    prepareAttachmentBinding(...args: Tail<Parameters<typeof prepareAttachmentBinding>>): PortResult<ReturnType<typeof prepareAttachmentBinding>, Effect>;
}
export function createStaffTasksApplication<Effect>(ports: StaffTasksPorts<Effect>) {
    const r = ports.repository;
    const scope = (record: RecordScope) => ({ companyId: record.company_id, branchId: record.branch_id, departmentId: record.department_id });
    async function listTasks(actor: StaffActor, projectId?: string) {
        const rows = await r.readListTasksRows(actor.companyId, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId, projectId ?? null, projectId ?? null);
        const visible: TaskRow[] = [];
        for (const row of rows.results) {
            try {
                if (row.source_post_id)
                    await ports.readablePost(actor, row.source_post_id);
                visible.push(row);
            }
            catch (e) {
                if (!(e instanceof BusinessRuleError) || e.status !== 404)
                    throw e;
            }
        }
        return visible;
    }
    async function getTask(actor: StaffActor, id: string) {
        const row = await r.readGetTaskRow(id, actor.companyId, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId);
        if (!row)
            throw new BusinessRuleError(404, "TASK_NOT_FOUND", "Task not found.");
        if (row.source_post_id)
            await ports.readablePost(actor, row.source_post_id);
        return row;
    }
    function taskSourceReadGuard(actor: StaffActor, id: string, version: number, key: string, step: string) {
        return r.guard3(key, step, id, version, actor.companyId, Array.from({ length: 8 }, () => actor.userId));
    }
    async function delegateTask(actor: StaffActor, id: string, input: z.infer<typeof taskDelegationSchema>, key: string) {
        const prior = await ports.commands.replay<{
            id: string;
            delegationId: string;
            version: number;
            expiresAt: number;
        }>(actor, key, "task.delegate", id, input);
        if (prior)
            return prior;
        const task = await getTask(actor, id);
        if (!canConfigureTask(actor, task))
            throw new BusinessRuleError(403, "TASK_DELEGATION_REQUIRED", "Only the task creator or project owner can delegate this task.");
        if (["completed", "cancelled"].includes(task.state))
            throw new BusinessRuleError(409, "TASK_TERMINAL", "Completed or cancelled tasks cannot be delegated.");
        if (samePerson(actor, input.delegateUserId) || samePerson({ ...actor, userId: input.delegateUserId }, task.assignee_user_id))
            throw new BusinessRuleError(422, "TASK_DELEGATE_INVALID", "Choose a different active staff member as the temporary delegate.");
        const approvers = [
            ...JSON.parse(task.start_approvers_json) as string[],
            ...JSON.parse(task.completion_approvers_json) as string[],
        ];
        if (approvers.some((userId) => userId === input.delegateUserId))
            throw new BusinessRuleError(422, "TASK_DELEGATE_INDEPENDENCE", "A configured approver cannot also receive temporary task ownership.");
        const expiresAt = Date.parse(input.expiresAt), now = ports.clock.now();
        if (!Number.isFinite(expiresAt) || expiresAt <= now)
            throw new BusinessRuleError(422, "TASK_DELEGATE_EXPIRY", "Delegation expiry must be in the future.");
        if (expiresAt > now + 90 * 86400000)
            throw new BusinessRuleError(422, "TASK_DELEGATE_EXPIRY", "Delegation expiry cannot be more than 90 days away.");
        await ports.activeStaff(actor.companyId, input.delegateUserId);
        const delegationId = `task-delegation-${key}`;
        const result = { id, delegationId, version: input.expectedVersion + 1, expiresAt };
        return ports.commands.commit(actor, key, "task.delegate", input, id, result.version, result, [
            ports.staffGuard(key, actor.companyId, actor.userId),
            ports.staffGuard(key, actor.companyId, input.delegateUserId, "delegate-staff"),
            configurationGuard(key, actor, task),
            taskVersionGuard(key, id, input.expectedVersion, [task.state]),
            r.guard4(key, "task-delegation-unique", actor.companyId, id, now, now),
            r.stageDelegateTaskEffect(delegationId, actor.companyId, id, actor.userId, input.delegateUserId, now, expiresAt, input.reason, now, now),
            r.stageDelegateTaskEffect2(now, id, actor.companyId),
            ports.notification(task, input.delegateUserId, "task", id, result.version, `Task delegated to you until ${new Date(expiresAt).toISOString()}`),
        ], scope(task));
    }
    async function revokeTaskDelegation(actor: StaffActor, id: string, input: z.infer<typeof taskDelegationRevokeSchema>, key: string) {
        const recordId = `${id}:${input.delegationId}`;
        const prior = await ports.commands.replay<{
            id: string;
            delegationId: string;
            version: number;
            status: string;
        }>(actor, key, "task.delegate-revoke", recordId, input);
        if (prior)
            return prior;
        const task = await getTask(actor, id);
        if (!canConfigureTask(actor, task))
            throw new BusinessRuleError(403, "TASK_DELEGATION_REQUIRED", "Only the task creator or project owner can revoke this delegation.");
        const delegation = await r.readRevokeTaskDelegationDelegation(input.delegationId, actor.companyId, id, ports.clock.now(), ports.clock.now());
        if (!delegation)
            throw new BusinessRuleError(404, "TASK_DELEGATION_NOT_FOUND", "Active task delegation not found.");
        const result = { id, delegationId: delegation.id, version: input.expectedVersion + 1, status: "revoked" };
        return ports.commands.commit(actor, key, "task.delegate-revoke", input, recordId, result.version, result, [
            ports.staffGuard(key, actor.companyId, actor.userId),
            configurationGuard(key, actor, task),
            taskVersionGuard(key, id, input.expectedVersion, [task.state]),
            r.guard8(key, "task-delegation-active", delegation.id, actor.companyId, id, ports.clock.now(), ports.clock.now()),
            r.stageRevokeTaskDelegationEffect(actor.userId, input.reason, ports.clock.now(), ports.clock.now(), delegation.id, actor.companyId),
            r.stageRevokeTaskDelegationEffect2(ports.clock.now(), id, actor.companyId),
            ports.notification(task, delegation.delegate_user_id, "task", id, result.version, "Task delegation revoked; the accountable assignee resumes ownership."),
        ], scope(task));
    }
    function configurationGuard(key: string, actor: StaffActor, task: TaskRow) {
        return r.guard11(key, "task-configure", task.id, actor.companyId, actor.userId, actor.userId);
    }
    function taskVersionGuard(key: string, id: string, version: number, states: string[]) {
        return r.guard12(key, "task-version", id, version, states);
    }
    async function validateTaskConfiguration(actor: StaffActor, input: z.infer<typeof taskFieldsSchema>, creator = actor.userId, serviceTask = false) {
        const staff = await ports.activeStaff(actor.companyId, input.assigneeUserId);
        if (input.projectId)
            await ports.getProject(actor, input.projectId);
        const recordScope: RecordScope = {
            company_id: actor.companyId,
            branch_id: staff.branch_id,
            department_id: staff.department_id,
        };
        for (const [phase, list] of [
            ["start", input.startApprovers],
            ["completion", input.completionApprovers],
        ] as const) {
            const route = await ports.taskRoute(recordScope, creator, input.assigneeUserId, phase, list, serviceTask);
            if (route.blockedReason)
                throw new BusinessRuleError(422, "TASK_ROUTE_INVALID", route.blockedReason);
        }
    }
    function configGuards(key: string, actor: StaffActor, input: z.infer<typeof taskFieldsSchema>) {
        return [
            ...[
                ...new Set([
                    input.assigneeUserId,
                    ...input.startApprovers,
                    ...input.completionApprovers,
                ]),
            ].map((user, i) => ports.staffGuard(key, actor.companyId, user, `config-user-${i}`)),
            ...(input.projectId
                ? [
                    r.guard13(key, "task-project", input.projectId, actor.companyId, actor.userId, actor.userId),
                ]
                : []),
        ];
    }
    async function saveTask(actor: StaffActor, id: string | null, input: z.infer<typeof taskSchema>, key: string, revision = false, source?: TaskSource) {
        const recordId = id ?? (source ? `task-post-${source.postId}` : `task-${key}`), operation = revision ? "task.revise" : id ? "task.update" : "task.create";
        const commandInput = source ? { ...input, source } : input;
        const prior = await ports.commands.replay<{
            id: string;
            state: string;
            version: number;
        }>(actor, key, operation, recordId, commandInput);
        if (prior)
            return prior;
        const sourceSteps = await ports.taskSourceStatements(actor, key, recordId, [input.assigneeUserId, ...input.startApprovers, ...input.completionApprovers], source, input.projectId);
        const staff = await ports.activeStaff(actor.companyId, actor.userId), current = id ? await getTask(actor, id) : null;
        if (current && !canConfigureTask(actor, current))
            throw new BusinessRuleError(403, "TASK_CREATOR_REQUIRED", "The creator or project owner manages task configuration.");
        const ordinary = input.startApprovers.length + input.completionApprovers.length === 0;
        const wasReviewed = current &&
            (JSON.parse(current.start_approvers_json) as string[]).length +
                (JSON.parse(current.completion_approvers_json) as string[]).length > 0;
        if (wasReviewed && ordinary)
            throw new BusinessRuleError(422, "TASK_REVIEW_REQUIRED", "A reviewed task must keep at least one approval step.");
        if (revision && !ordinary && !input.reason.trim())
            throw new BusinessRuleError(422, "REVISION_REASON_REQUIRED", "Explain the material revision; approvals will restart.");
        await validateTaskConfiguration(actor, input, current?.creator_user_id ?? actor.userId, !!(source || current?.source_post_id));
        const nextState = ordinary ? "ready" : "draft";
        const dueAt = input.dueAt ? Date.parse(input.dueAt) : null;
        const now = ports.clock.now(), version = current ? input.expectedVersion + 1 : 1;
        return ports.commands.commit(actor, key, operation, commandInput, recordId, version, { id: recordId, state: nextState, version }, [
            ports.staffGuard(key, actor.companyId, actor.userId),
            ...configGuards(key, actor, input),
            ...(current
                ? [
                    configurationGuard(key, actor, current),
                    taskVersionGuard(key, recordId, input.expectedVersion, revision
                        ? [
                            "draft",
                            "start_approval",
                            "ready",
                            "in_progress",
                            "completion_approval",
                            "rework",
                        ]
                        : ["draft"]),
                ]
                : []),
            ...(revision
                ? ports.closeApprovals(key, "task", recordId, actor.userId, "revise", input.reason, input.expectedVersion)
                : []),
            current
                ? r.stageSaveTaskEffect(input.title, input.description, input.assigneeUserId, dueAt, input.projectId, JSON.stringify(input.startApprovers), JSON.stringify(input.completionApprovers), nextState, revision ? 1 : 0, now, recordId)
                : r.stageSaveTaskEffect2(recordId, actor.companyId, staff.branch_id, staff.department_id, input.title, input.description, actor.userId, input.assigneeUserId, dueAt, input.projectId, JSON.stringify(input.startApprovers), JSON.stringify(input.completionApprovers), nextState, now, now),
            ...sourceSteps,
            ports.notification(current ?? staff, input.assigneeUserId, "task", recordId, version, revision ? "Task revised; approvals restart" : "Task assigned to you"),
        ], scope(current ?? staff));
    }
    async function taskWorkState(actor: StaffActor, task: TaskRow): Promise<TaskWorkState> {
        const approval = await ports.approvalView("task", task.id, actor), manage = canConfigureTask(actor, task), assigned = samePerson(actor, task.assignee_user_id) || isActiveDelegate(actor, task);
        let blockedReason = approval.blockedReason ?? task.blocked_reason;
        if (!["completed", "cancelled"].includes(task.state)) {
            const phases: ("start" | "completion")[] = task.state === "draft"
                ? ["start", "completion"]
                : ["ready", "in_progress", "rework", "start_approval"].includes(task.state)
                    ? ["completion"]
                    : [];
            for (const phase of phases) {
                const route = await ports.taskRoute(task, task.creator_user_id, task.assignee_user_id, phase, JSON.parse(phase === "start"
                    ? task.start_approvers_json
                    : task.completion_approvers_json), !!task.source_post_id);
                blockedReason ??= route.blockedReason;
            }
        }
        const permittedActions = [
            ...(assigned && task.state === "ready" && isOrdinaryTask(task) && !blockedReason
                ? ["complete"] : []),
            ...(manage && task.state === "draft"
                ? ["edit", ...(!blockedReason ? ["submit"] : [])]
                : []),
            ...(manage && !["completed", "cancelled"].includes(task.state)
                ? ["revise", "cancel"]
                : []),
            ...(manage && !["completed", "cancelled"].includes(task.state)
                ? ["delegate"]
                : []),
            ...(assigned && task.state === "ready" && !blockedReason ? ["start"] : []),
            ...(assigned && ["in_progress", "rework"].includes(task.state)
                ? ["save-progress", ...(!blockedReason ? ["complete"] : [])]
                : []),
            ...(approval.canDecide ? ["approve", "reject"] : []),
        ];
        return { approval, permittedActions, blockedReason };
    }
    async function taskDetail(actor: StaffActor, id: string): Promise<TaskDetail> {
        const task = await getTask(actor, id), work = await taskWorkState(actor, task);
        const attachments = (await r.readTaskDetailAttachments(id)).results;
        const delegation = task.active_delegation_id
            ? {
                id: task.active_delegation_id,
                delegate_user_id: task.active_delegate_user_id!,
                delegate_name: task.active_delegate_name ?? task.active_delegate_user_id!,
                expires_at: Number(task.active_delegate_expires_at),
                reason: task.active_delegate_reason ?? "",
                version: Number(task.active_delegate_version ?? 1),
            }
            : null;
        const delegationHistory = (await r.readTaskDetailDelegationHistory(actor.companyId, id)).results;
        return { task, ...work, delegation, delegationHistory, attachments };
    }
    async function transitionTask(actor: WorkContext, id: string, input: z.infer<typeof taskActionSchema>, key: string) {
        const operation = `task.${input.action}`, prior = await ports.commands.replay<{
            id: string;
            state: string;
            version: number;
            manifest?: unknown;
        }>(actor, key, operation, id, input);
        if (prior)
            return prior;
        await ports.activeStaff(actor.companyId, actor.userId);
        const detail = await taskDetail(actor, id), task = detail.task;
        if (!detail.permittedActions.includes(input.action))
            throw new BusinessRuleError(403, "TASK_ACTION_UNAVAILABLE", detail.blockedReason ??
                "This action is unavailable for this participant and task state.");
        const attachmentSetIds = input.attachmentSetIds ?? [];
        if (attachmentSetIds.length && !["submit", "save-progress", "complete"].includes(input.action))
            throw new BusinessRuleError(422, "TASK_ATTACHMENT_ACTION", "Bind task evidence only when submitting, saving progress, or completing the task.");
        const prepared = attachmentSetIds.length
            ? await ports.prepareAttachmentBinding(actor, {
                attachmentSetIds,
                bindingContract: {
                    targetType: "staff_task",
                    bindingType: "task_action",
                    contexts: [{ purposeKey: "task_evidence", componentKind: "action", componentRef: `task:${task.state}` }],
                },
                targetId: id,
                expectedTargetVersion: input.expectedVersion,
                commandId: key,
                bindingId: `${id}:${input.action}:${input.expectedVersion + 1}`,
            })
            : null;
        const steps: Effect[] = [
            ...(prepared?.guards ?? []),
            ...(input.action !== "cancel"
                ? [
                    ports.staffGuard(key, actor.companyId, task.assignee_user_id, "task-assignee"),
                ]
                : []),
            taskVersionGuard(key, id, input.expectedVersion, [task.state]),
            ports.staffGuard(key, actor.companyId, actor.userId),
        ];
        let state = task.state;
        if (input.action === "submit" || input.action === "complete") {
            const phase = input.action === "submit" ? "start" : "completion", users: string[] = JSON.parse(phase === "start"
                ? task.start_approvers_json
                : task.completion_approvers_json);
            if (input.action === "submit")
                steps.push(configurationGuard(key, actor, task));
            else if (!input.note.trim() && !isOrdinaryTask(task))
                throw new BusinessRuleError(422, "COMPLETION_NOTE_REQUIRED", "Add a completion note for the reviewer.");
            const route = await ports.taskRoute(task, task.creator_user_id, task.assignee_user_id, phase, users, !!task.source_post_id);
            if (users.length)
                steps.push(...(await ports.openApproval(key, "task", id, phase, task.creator_user_id, task.assignee_user_id, route, input.expectedVersion + 1)).statements);
            state = users.length
                ? phase === "start"
                    ? "start_approval"
                    : "completion_approval"
                : phase === "start"
                    ? "ready"
                    : "completed";
        }
        else if (input.action === "start")
            state = "in_progress";
        else if (input.action === "approve" || input.action === "reject") {
            const decision = await ports.prepareDecision(actor, key, "task", id, input.action === "approve", input.note, input.expectedVersion);
            steps.push(...decision.statements);
            state =
                input.action === "reject"
                    ? decision.instance.phase === "start"
                        ? "draft"
                        : "rework"
                    : decision.final
                        ? decision.instance.phase === "start"
                            ? "ready"
                            : "completed"
                        : task.state;
        }
        else if (input.action === "cancel") {
            if (!input.note.trim())
                throw new BusinessRuleError(422, "REASON_REQUIRED", "Explain why the task is being cancelled.");
            steps.push(configurationGuard(key, actor, task), ...ports.closeApprovals(key, "task", id, actor.userId, "cancel", input.note, input.expectedVersion));
            state = "cancelled";
        }
        steps.push(r.stageTransitionTaskEffect(state, input.action, input.note, ports.clock.now(), id), ...(prepared?.steps ?? []), ports.notification(task, task.assignee_user_id, "task", id, input.expectedVersion + 1, `Task ${state.replaceAll("_", " ")}`));
        if (task.source_post_id) {
            const post = await ports.readablePost(actor, task.source_post_id);
            const followers = await r.readTransitionTaskFollowers(post.id);
            for (const user of new Set([post.author_id, task.creator_user_id, ...followers.results.map(f => f.principal_id)])) {
                if (user === actor.userId)
                    continue;
                try {
                    await ports.readablePost({ companyId: actor.companyId, userId: user }, post.id);
                    steps.push(ports.notification(post, user, 'post', post.id, input.expectedVersion + 1, 'Work on a conversation you follow has progressed'));
                }
                catch (e) {
                    if (!(e instanceof BusinessRuleError) || e.status !== 404)
                        throw e;
                }
            }
        }
        return ports.commands.commit(actor, key, operation, input, id, input.expectedVersion + 1, { id, state, version: input.expectedVersion + 1, ...(prepared ? { manifest: prepared.manifest } : {}) }, steps, scope(task));
    }
    function canAttachTask(actor: StaffActor, task: TaskRow) {
        return ((task.state === "draft" && canConfigureTask(actor, task)) ||
            ((["in_progress", "rework"].includes(task.state) ||
                task.state === "ready" && isOrdinaryTask(task)) &&
                (samePerson(actor, task.assignee_user_id) || isActiveDelegate(actor, task))));
    }
    return { listTasks, getTask, taskSourceReadGuard, canConfigureTask, delegateTask, revokeTaskDelegation, taskVersionGuard, validateTaskConfiguration, configGuards, saveTask, taskWorkState, taskDetail, transitionTask, canAttachTask };
}
