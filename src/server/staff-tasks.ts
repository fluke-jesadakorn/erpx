import {samePerson} from "@/lib/iam";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { readablePost } from "./social-source";
import { taskSourceStatements, type TaskSource } from "./social-task";
import { z } from "zod";
import { assertion, executeCommand, replayCommand } from "./commands";
import {
  approvalView,
  closeApprovals,
  openApproval,
  participantSql,
  prepareDecision,
  taskRoute,
  type ApprovalView,
} from "./approvals";
import {
  activeStaff,
  staffGuard,
  notification,
  type RecordScope,
  type StaffActor,
} from "./staff-policy";
import { ApiError } from "./http";
import type { WorkContext } from "./authorization";
import { prepareAttachmentBinding } from "./attachments/service";

const users = z.array(z.string().min(1).max(160)).max(20);
export const taskFieldsSchema = z.object({
  title: z.string().trim().min(2).max(200),
  description: z.string().trim().max(10000).default(""),
  assigneeUserId: z.string().min(1),
  projectId: z.string().min(1).nullable().default(null),
  startApprovers: users.default([]),
  completionApprovers: users.default([]),
});
export const taskSchema = taskFieldsSchema
  .extend({
    dueAt: z.iso.datetime({ offset: true }).nullable().default(null),
    expectedVersion: z.number().int().positive().default(1),
    reason: z.string().trim().max(2000).default(""),
  })
  .refine((v) => !!v.dueAt || v.startApprovers.length + v.completionApprovers.length === 0, {
    message: "Set a due date for work that requires approval.",
    path: ["dueAt"],
  });
export const projectSchema = z.object({
  title: z.string().trim().min(2).max(200),
  description: z.string().trim().max(10000).default(""),
  memberUserIds: z.array(z.string().min(1).max(160)).max(200).default([]),
  expectedVersion: z.number().int().positive().default(1),
});
export const taskActionSchema = z.object({
  action: z.enum([
    "submit",
    "start",
    "save-progress",
    "complete",
    "approve",
    "reject",
    "cancel",
  ]),
  expectedVersion: z.number().int().positive(),
  note: z.string().trim().max(10000).default(""),
  attachmentSetIds: z.array(z.string().min(1)).max(1).default([]),
});
export const taskDelegationSchema = z.object({
  delegateUserId: z.string().min(1),
  expiresAt: z.iso.datetime({ offset: true }),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export const taskDelegationRevokeSchema = z.object({
  delegationId: z.string().min(1),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export type TaskRow = RecordScope & {
  source_post_id?: string | null;
  id: string;
  title: string;
  description: string;
  creator_user_id: string;
  assignee_user_id: string;
  assignee_name: string;
  creator_name: string;
  project_id: string | null;
  project_title: string | null;
  project_owner_id: string | null;
  due_at: number | null;
  completion_note: string;
  start_approvers_json: string;
  completion_approvers_json: string;
  state: string;
  blocked_reason: string | null;
  version: number;
  revision: number;
  updated_at: number;
  template_id: string | null;
  template_version: number | null;
  schedule_id: string | null;
  occurrence_at: number | null;
  active_delegation_id: string | null;
  active_delegate_user_id: string | null;
  active_delegate_name: string | null;
  active_delegate_expires_at: number | null;
  active_delegate_reason?: string | null;
  active_delegate_version?: number | null;
};
function isOrdinaryTask(task: TaskRow) {
  return task.template_id === null && task.schedule_id === null &&
    (JSON.parse(task.start_approvers_json) as string[]).length === 0 &&
    (JSON.parse(task.completion_approvers_json) as string[]).length === 0;
}
export type TaskDetail = {
  task: TaskRow;
  approval: ApprovalView;
  permittedActions: string[];
  blockedReason: string | null;
  delegation: {
    id: string;
    delegate_user_id: string;
    delegate_name: string;
    expires_at: number;
    reason: string;
    version: number;
  } | null;
  delegationHistory: {
    id: string;
    delegate_name: string;
    delegator_name: string;
    starts_at: number;
    expires_at: number;
    reason: string;
    status: "active" | "expired" | "revoked";
    revoked_by_name: string | null;
    revoked_at: number | null;
    revoke_reason: string | null;
  }[];
  attachments: {
    id: string;
    original_name: string;
    bytes: number;
    media_type: string;
  }[];
};
export type ProjectRow = RecordScope & {
  id: string;
  title: string;
  description: string;
  owner_user_id: string;
  version: number;
  updated_at: number;
};
const scope = (r: RecordScope) => ({
  companyId: r.company_id,
  branchId: r.branch_id,
  departmentId: r.department_id,
});
const taskSelect = `SELECT t.*,o.post_id source_post_id,p.title project_title,p.owner_user_id project_owner_id,COALESCE(a.legal_name,t.assignee_user_id) assignee_name,COALESCE(c.legal_name,t.creator_user_id) creator_name,d.id active_delegation_id,d.delegate_user_id active_delegate_user_id,COALESCE(da.legal_name,d.delegate_user_id) active_delegate_name,d.expires_at active_delegate_expires_at,d.reason active_delegate_reason,d.version active_delegate_version FROM staff_tasks t LEFT JOIN social_task_origins o ON o.task_id=t.id LEFT JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id LEFT JOIN LATERAL (SELECT sd.id,sd.delegate_user_id,sd.expires_at,sd.reason,sd.version FROM staff_task_delegations sd WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active' AND sd.starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND sd.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) d ON TRUE LEFT JOIN hr_people a ON a.user_id=t.assignee_user_id AND a.company_id=t.company_id AND a.merged_into_id IS NULL LEFT JOIN hr_people c ON c.user_id=t.creator_user_id AND c.company_id=t.company_id AND c.merged_into_id IS NULL LEFT JOIN hr_people da ON da.user_id=d.delegate_user_id AND da.company_id=t.company_id AND da.merged_into_id IS NULL`;
const taskAccess = `(iam_same_person(t.creator_user_id,?,t.company_id) OR iam_same_person(t.assignee_user_id,?,t.company_id) OR iam_same_person(d.delegate_user_id,?,t.company_id) OR iam_same_person(p.owner_user_id,?,t.company_id) OR EXISTS(SELECT 1 FROM staff_project_members pm WHERE pm.project_id=t.project_id AND pm.user_id=?) OR EXISTS(SELECT 1 FROM erpx_json_each(t.start_approvers_json) WHERE value=?) OR EXISTS(SELECT 1 FROM erpx_json_each(t.completion_approvers_json) WHERE value=?) OR ${participantSql("task", "t")})`;
export async function listTasks(
  db: SqlDatabase,
  actor: StaffActor,
  projectId?: string,
) {
  const rows = await db
    .prepare(
      `${taskSelect} WHERE t.company_id=? AND ${taskAccess} AND (?::text IS NULL OR t.project_id=?) ORDER BY t.updated_at DESC,t.id LIMIT 250`,
    )
    .bind(
      actor.companyId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      projectId ?? null,
      projectId ?? null,
    )
    .all<TaskRow>();
  const visible:TaskRow[]=[];for(const row of rows.results){try{if(row.source_post_id)await readablePost(db,actor,row.source_post_id);visible.push(row);}catch(e){if(!(e instanceof ApiError)||e.status!==404)throw e;}}
  return visible;
}
export async function getTask(db: SqlDatabase, actor: StaffActor, id: string) {
  const row = await db
    .prepare(`${taskSelect} WHERE t.id=? AND t.company_id=? AND ${taskAccess}`)
    .bind(
      id,
      actor.companyId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
    )
    .first<TaskRow>();
  if (!row) throw new ApiError(404, "TASK_NOT_FOUND", "Task not found.");
  if(row.source_post_id)await readablePost(db,actor,row.source_post_id);
  return row;
}
export function canConfigureTask(actor: StaffActor, task: TaskRow) {
  return (
    samePerson(actor,task.creator_user_id) ||
    samePerson(actor,task.project_owner_id)
  );
}
function isActiveDelegate(actor: StaffActor, task: TaskRow) {
  return Boolean(
    task.active_delegate_user_id &&
      samePerson(actor, task.active_delegate_user_id),
  );
}

export async function delegateTask(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  input: z.infer<typeof taskDelegationSchema>,
  key: string,
) {
  const prior = await replayCommand(db, actor, key, "task.delegate", id, input);
  if (prior) return prior;
  const task = await getTask(db, actor, id);
  if (!canConfigureTask(actor, task))
    throw new ApiError(403, "TASK_DELEGATION_REQUIRED", "Only the task creator or project owner can delegate this task.");
  if (["completed", "cancelled"].includes(task.state))
    throw new ApiError(409, "TASK_TERMINAL", "Completed or cancelled tasks cannot be delegated.");
  if (samePerson(actor, input.delegateUserId) || samePerson({ ...actor, userId: input.delegateUserId }, task.assignee_user_id))
    throw new ApiError(422, "TASK_DELEGATE_INVALID", "Choose a different active staff member as the temporary delegate.");
  const approvers = [
    ...JSON.parse(task.start_approvers_json) as string[],
    ...JSON.parse(task.completion_approvers_json) as string[],
  ];
  if (approvers.some((userId) => userId === input.delegateUserId))
    throw new ApiError(422, "TASK_DELEGATE_INDEPENDENCE", "A configured approver cannot also receive temporary task ownership.");
  const expiresAt = Date.parse(input.expiresAt), now = Date.now();
  if (!Number.isFinite(expiresAt) || expiresAt <= now)
    throw new ApiError(422, "TASK_DELEGATE_EXPIRY", "Delegation expiry must be in the future.");
  if (expiresAt > now + 90 * 86_400_000)
    throw new ApiError(422, "TASK_DELEGATE_EXPIRY", "Delegation expiry cannot be more than 90 days away.");
  await activeStaff(db, actor.companyId, input.delegateUserId);
  const delegationId = `task-delegation-${key}`;
  const result = { id, delegationId, version: input.expectedVersion + 1, expiresAt };
  return executeCommand(db, actor, key, "task.delegate", input, id, result.version, result, [
    staffGuard(db, key, actor.companyId, actor.userId),
    staffGuard(db, key, actor.companyId, input.delegateUserId, "delegate-staff"),
    configurationGuard(db, key, actor, task),
    taskVersionGuard(db, key, id, input.expectedVersion, [task.state]),
    assertion(db, key, "task-delegation-unique", `NOT EXISTS(SELECT 1 FROM staff_task_delegations WHERE company_id=? AND task_id=? AND status='active' AND starts_at<=? AND expires_at>?)`, [actor.companyId, id, now, now]),
    db.prepare("INSERT INTO staff_task_delegations(id,company_id,task_id,delegator_user_id,delegate_user_id,starts_at,expires_at,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(delegationId, actor.companyId, id, actor.userId, input.delegateUserId, now, expiresAt, input.reason, now, now),
    db.prepare("UPDATE staff_tasks SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, id, actor.companyId),
    notification(db, task, input.delegateUserId, "task", id, result.version, `Task delegated to you until ${new Date(expiresAt).toISOString()}`),
  ], scope(task));
}

export async function revokeTaskDelegation(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  input: z.infer<typeof taskDelegationRevokeSchema>,
  key: string,
) {
  const recordId = `${id}:${input.delegationId}`;
  const prior = await replayCommand(db, actor, key, "task.delegate-revoke", recordId, input);
  if (prior) return prior;
  const task = await getTask(db, actor, id);
  if (!canConfigureTask(actor, task))
    throw new ApiError(403, "TASK_DELEGATION_REQUIRED", "Only the task creator or project owner can revoke this delegation.");
  const delegation = await db.prepare("SELECT * FROM staff_task_delegations WHERE id=? AND company_id=? AND task_id=? AND status='active' AND starts_at<=? AND expires_at>?").bind(input.delegationId, actor.companyId, id, Date.now(), Date.now()).first<{id:string;delegate_user_id:string;version:number}>();
  if (!delegation) throw new ApiError(404, "TASK_DELEGATION_NOT_FOUND", "Active task delegation not found.");
  const result = { id, delegationId: delegation.id, version: input.expectedVersion + 1, status: "revoked" };
  return executeCommand(db, actor, key, "task.delegate-revoke", input, recordId, result.version, result, [
    staffGuard(db, key, actor.companyId, actor.userId),
    configurationGuard(db, key, actor, task),
    taskVersionGuard(db, key, id, input.expectedVersion, [task.state]),
    assertion(db, key, "task-delegation-active", "EXISTS(SELECT 1 FROM staff_task_delegations WHERE id=? AND company_id=? AND task_id=? AND status='active' AND starts_at<=? AND expires_at>?)", [delegation.id, actor.companyId, id, Date.now(), Date.now()]),
    db.prepare("UPDATE staff_task_delegations SET status='revoked',revoked_by=?,revoke_reason=?,revoked_at=?,updated_at=?,version=version+1 WHERE id=? AND company_id=?").bind(actor.userId, input.reason, Date.now(), Date.now(), delegation.id, actor.companyId),
    db.prepare("UPDATE staff_tasks SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(Date.now(), id, actor.companyId),
    notification(db, task, delegation.delegate_user_id, "task", id, result.version, "Task delegation revoked; the accountable assignee resumes ownership."),
  ], scope(task));
}
function configurationGuard(
  db: SqlDatabase,
  key: string,
  actor: StaffActor,
  task: TaskRow,
) {
  return assertion(
    db,
    key,
    "task-configure",
    "EXISTS(SELECT 1 FROM staff_tasks t LEFT JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id WHERE t.id=? AND t.company_id=? AND (iam_same_person(t.creator_user_id,?,t.company_id) OR iam_same_person(p.owner_user_id,?,t.company_id)))",
    [task.id, actor.companyId, actor.userId, actor.userId],
  );
}
export function taskVersionGuard(
  db: SqlDatabase,
  key: string,
  id: string,
  version: number,
  states: string[],
) {
  return assertion(
    db,
    key,
    "task-version",
    `EXISTS(SELECT 1 FROM staff_tasks WHERE id=? AND version=? AND state IN (${states.map(() => "?").join(",")}))`,
    [id, version, ...states],
  );
}
export async function getProject(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
) {
  const row = await db
    .prepare(
      "SELECT p.* FROM staff_projects p WHERE p.id=? AND p.company_id=? AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))",
    )
    .bind(id, actor.companyId, actor.userId, actor.userId)
    .first<ProjectRow>();
  if (!row) throw new ApiError(404, "PROJECT_NOT_FOUND", "Project not found.");
  return row;
}
export async function listProjects(db: SqlDatabase, actor: StaffActor) {
  return (
    await db
      .prepare(
        "SELECT p.* FROM staff_projects p WHERE p.company_id=? AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)) ORDER BY p.updated_at DESC",
      )
      .bind(actor.companyId, actor.userId, actor.userId)
      .all<ProjectRow>()
  ).results;
}
export async function saveProject(
  db: SqlDatabase,
  actor: StaffActor,
  id: string | null,
  input: z.infer<typeof projectSchema>,
  key: string,
) {
  const recordId = id ?? `project-${key}`,
    operation = id ? "project.update" : "project.create";
  const prior = await replayCommand(db, actor, key, operation, recordId, input);
  if (prior) return prior;
  const staff = await activeStaff(db, actor.companyId, actor.userId),
    current = id ? await getProject(db, actor, id) : null;
  if (current && !samePerson(actor,current.owner_user_id))
    throw new ApiError(
      403,
      "PROJECT_OWNER_REQUIRED",
      "Only the project owner manages this project and its membership.",
    );
  const members = [...new Set(input.memberUserIds)];
  for (const user of members) await activeStaff(db, actor.companyId, user);
  const now = Date.now(),
    version = current ? input.expectedVersion + 1 : 1;
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    recordId,
    version,
    { id: recordId, version },
    [
      staffGuard(db, key, actor.companyId, actor.userId),
      ...(current
        ? [
            assertion(
              db,
              key,
              "project-version",
              "EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND version=? AND owner_user_id=?)",
              [recordId, input.expectedVersion, actor.userId],
            ),
          ]
        : []),
      current
        ? db
            .prepare(
              "UPDATE staff_projects SET title=?,description=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(input.title, input.description, now, recordId)
        : db
            .prepare(
              "INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,description,owner_user_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              recordId,
              actor.companyId,
              staff.branch_id,
              staff.department_id,
              input.title,
              input.description,
              actor.userId,
              now,
              now,
            ),
      db
        .prepare("DELETE FROM staff_project_members WHERE project_id=?")
        .bind(recordId),
      ...members.flatMap((user, i) => [
        staffGuard(db, key, actor.companyId, user, `member-${i}`),
        db
          .prepare(
            "INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) VALUES(?,?,?,?)",
          )
          .bind(recordId, user, actor.userId, now),
      ]),
    ],
    scope(current ?? staff),
  );
}
export async function validateTaskConfiguration(
  db: SqlDatabase,
  actor: StaffActor,
  input: z.infer<typeof taskFieldsSchema>,
  creator = actor.userId,
  serviceTask = false,
) {
  const staff = await activeStaff(db, actor.companyId, input.assigneeUserId);
  if (input.projectId) await getProject(db, actor, input.projectId);
  const recordScope: RecordScope = {
    company_id: actor.companyId,
    branch_id: staff.branch_id,
    department_id: staff.department_id,
  };
  for (const [phase, list] of [
    ["start", input.startApprovers],
    ["completion", input.completionApprovers],
  ] as const) {
    const route = await taskRoute(
      db,
      recordScope,
      creator,
      input.assigneeUserId,
      phase,
      list,
      serviceTask,
    );
    if (route.blockedReason)
      throw new ApiError(422, "TASK_ROUTE_INVALID", route.blockedReason);
  }
}
export function configGuards(
  db: SqlDatabase,
  key: string,
  actor: StaffActor,
  input: z.infer<typeof taskFieldsSchema>,
) {
  return [
    ...[
      ...new Set([
        input.assigneeUserId,
        ...input.startApprovers,
        ...input.completionApprovers,
      ]),
    ].map((user, i) =>
      staffGuard(db, key, actor.companyId, user, `config-user-${i}`),
    ),
    ...(input.projectId
      ? [
          assertion(
            db,
            key,
            "task-project",
            "EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)))",
            [input.projectId, actor.companyId, actor.userId, actor.userId],
          ),
        ]
      : []),
  ];
}
export async function saveTask(
  db: SqlDatabase,
  actor: StaffActor,
  id: string | null,
  input: z.infer<typeof taskSchema>,
  key: string,
  revision = false,
  source?: TaskSource,
) {
  const recordId = id ?? (source ? `task-post-${source.postId}` : `task-${key}`),
    operation = revision ? "task.revise" : id ? "task.update" : "task.create";
  const commandInput = source ? {...input, source} : input;
  const prior = await replayCommand(db, actor, key, operation, recordId, commandInput);
  if (prior) return prior;
  const sourceSteps = await taskSourceStatements(db,actor,key,recordId,[input.assigneeUserId,...input.startApprovers,...input.completionApprovers],source,input.projectId);
  const staff = await activeStaff(db, actor.companyId, actor.userId),
    current = id ? await getTask(db, actor, id) : null;
  if (current && !canConfigureTask(actor, current))
    throw new ApiError(
      403,
      "TASK_CREATOR_REQUIRED",
      "The creator or project owner manages task configuration.",
    );
  const ordinary = input.startApprovers.length + input.completionApprovers.length === 0;
  const wasReviewed = current &&
    (JSON.parse(current.start_approvers_json) as string[]).length +
      (JSON.parse(current.completion_approvers_json) as string[]).length > 0;
  if (wasReviewed && ordinary)
    throw new ApiError(422, "TASK_REVIEW_REQUIRED", "A reviewed task must keep at least one approval step.");
  if (revision && !ordinary && !input.reason.trim())
    throw new ApiError(
      422,
      "REVISION_REASON_REQUIRED",
      "Explain the material revision; approvals will restart.",
    );
  await validateTaskConfiguration(
    db,
    actor,
    input,
    current?.creator_user_id ?? actor.userId,
    !!(source || current?.source_post_id),
  );
  const nextState = ordinary ? "ready" : "draft";
  const dueAt = input.dueAt ? Date.parse(input.dueAt) : null;
  const now = Date.now(),
    version = current ? input.expectedVersion + 1 : 1;
  return executeCommand(
    db,
    actor,
    key,
    operation,
    commandInput,
    recordId,
    version,
    { id: recordId, state: nextState, version },
    [
      staffGuard(db, key, actor.companyId, actor.userId),
      ...configGuards(db, key, actor, input),
      ...(current
        ? [
            configurationGuard(db, key, actor, current),
            taskVersionGuard(
              db,
              key,
              recordId,
              input.expectedVersion,
              revision
                ? [
                    "draft",
                    "start_approval",
                    "ready",
                    "in_progress",
                    "completion_approval",
                    "rework",
                  ]
                : ["draft"],
            ),
          ]
        : []),
      ...(revision
        ? closeApprovals(
            db,
            key,
            "task",
            recordId,
            actor.userId,
            "revise",
            input.reason,
            input.expectedVersion,
          )
        : []),
      current
        ? db
            .prepare(
              "UPDATE staff_tasks SET title=?,description=?,assignee_user_id=?,due_at=?,project_id=?,start_approvers_json=?,completion_approvers_json=?,state=?,completion_note='',blocked_reason=NULL,revision=revision+?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(
              input.title,
              input.description,
              input.assigneeUserId,
              dueAt,
              input.projectId,
              JSON.stringify(input.startApprovers),
              JSON.stringify(input.completionApprovers),
              nextState,
              revision ? 1 : 0,
              now,
              recordId,
            )
        : db
            .prepare(
              "INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,due_at,project_id,start_approvers_json,completion_approvers_json,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              recordId,
              actor.companyId,
              staff.branch_id,
              staff.department_id,
              input.title,
              input.description,
              actor.userId,
              input.assigneeUserId,
              dueAt,
              input.projectId,
              JSON.stringify(input.startApprovers),
              JSON.stringify(input.completionApprovers),
              nextState,
              now,
              now,
            ),
      ...sourceSteps,
      notification(
        db,
        current ?? staff,
        input.assigneeUserId,
        "task",
        recordId,
        version,
        revision ? "Task revised; approvals restart" : "Task assigned to you",
      ),
    ],
    scope(current ?? staff),
  );
}
export type TaskWorkState = Pick<TaskDetail, "approval" | "permittedActions" | "blockedReason">;

/** The caller must load the authorized row with getTask or listTasks first. */
export async function taskWorkState(
  db: SqlDatabase,
  actor: StaffActor,
  task: TaskRow,
): Promise<TaskWorkState> {
  const approval = await approvalView(db, "task", task.id, actor),
    manage = canConfigureTask(actor, task),
    assigned = samePerson(actor,task.assignee_user_id) || isActiveDelegate(actor, task);
  let blockedReason = approval.blockedReason ?? task.blocked_reason;
  if (!["completed", "cancelled"].includes(task.state)) {
    const phases: ("start" | "completion")[] =
      task.state === "draft"
        ? ["start", "completion"]
        : ["ready", "in_progress", "rework", "start_approval"].includes(
              task.state,
            )
          ? ["completion"]
          : [];
    for (const phase of phases) {
      const route = await taskRoute(
        db,
        task,
        task.creator_user_id,
        task.assignee_user_id,
        phase,
        JSON.parse(
          phase === "start"
            ? task.start_approvers_json
            : task.completion_approvers_json,
        ),
        !!task.source_post_id,
      );
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

export async function taskDetail(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
): Promise<TaskDetail> {
  const task = await getTask(db, actor, id),
    work = await taskWorkState(db, actor, task);
  const attachments = (
    await db
      .prepare(
        "SELECT id,original_name,bytes,media_type FROM attachments WHERE owner_type='staff_task' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at",
      )
      .bind(id)
      .all<TaskDetail["attachments"][number]>()
  ).results;
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
  const delegationHistory = (await db.prepare(`
    SELECT d.id,COALESCE(recipient.legal_name,d.delegate_user_id) delegate_name,
      COALESCE(grantor.legal_name,d.delegator_user_id) delegator_name,
      d.starts_at,d.expires_at,d.reason,d.revoked_at,d.revoke_reason,
      COALESCE(revoker.legal_name,d.revoked_by) revoked_by_name,
      CASE WHEN d.status='revoked' THEN 'revoked'
        WHEN d.expires_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint THEN 'expired'
        ELSE 'active' END status
    FROM staff_task_delegations d
    LEFT JOIN hr_people recipient ON recipient.company_id=d.company_id AND recipient.user_id=d.delegate_user_id AND recipient.merged_into_id IS NULL
    LEFT JOIN hr_people grantor ON grantor.company_id=d.company_id AND grantor.user_id=d.delegator_user_id AND grantor.merged_into_id IS NULL
    LEFT JOIN hr_people revoker ON revoker.company_id=d.company_id AND revoker.user_id=d.revoked_by AND revoker.merged_into_id IS NULL
    WHERE d.company_id=? AND d.task_id=? ORDER BY d.created_at DESC,d.id DESC LIMIT 50
  `).bind(actor.companyId, id).all<TaskDetail["delegationHistory"][number]>()).results;
  return { task, ...work, delegation, delegationHistory, attachments };
}
export async function transitionTask(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  input: z.infer<typeof taskActionSchema>,
  key: string,
) {
  const operation = `task.${input.action}`,
    prior = await replayCommand(db, actor, key, operation, id, input);
  if (prior) return prior;
  await activeStaff(db, actor.companyId, actor.userId);
  const detail = await taskDetail(db, actor, id),
    task = detail.task;
  if (!detail.permittedActions.includes(input.action))
    throw new ApiError(
      403,
      "TASK_ACTION_UNAVAILABLE",
      detail.blockedReason ??
        "This action is unavailable for this participant and task state.",
    );
  const attachmentSetIds=input.attachmentSetIds??[];
  if (attachmentSetIds.length && !["submit", "save-progress", "complete"].includes(input.action))
    throw new ApiError(
      422,
      "TASK_ATTACHMENT_ACTION",
      "Bind task evidence only when submitting, saving progress, or completing the task.",
    );
  const prepared = attachmentSetIds.length
    ? await prepareAttachmentBinding(db, actor, {
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
  const steps: SqlStatement[] = [
    ...(prepared?.guards ?? []),
    ...(input.action !== "cancel"
      ? [
          staffGuard(
            db,
            key,
            actor.companyId,
            task.assignee_user_id,
            "task-assignee",
          ),
        ]
      : []),
    taskVersionGuard(db, key, id, input.expectedVersion, [task.state]),
    staffGuard(db, key, actor.companyId, actor.userId),
  ];
  let state = task.state;
  if (input.action === "submit" || input.action === "complete") {
    const phase = input.action === "submit" ? "start" : "completion",
      users: string[] = JSON.parse(
        phase === "start"
          ? task.start_approvers_json
          : task.completion_approvers_json,
      );
    if (input.action === "submit")
      steps.push(configurationGuard(db, key, actor, task));
    else if (!input.note.trim() && !isOrdinaryTask(task))
      throw new ApiError(
        422,
        "COMPLETION_NOTE_REQUIRED",
        "Add a completion note for the reviewer.",
      );
    const route = await taskRoute(
      db,
      task,
      task.creator_user_id,
      task.assignee_user_id,
      phase,
      users,
      !!task.source_post_id,
    );
    if (users.length)
      steps.push(
        ...(
          await openApproval(
            db,
            key,
            "task",
            id,
            phase,
            task.creator_user_id,
            task.assignee_user_id,
            route,
            input.expectedVersion + 1,
          )
        ).statements,
      );
    state = users.length
      ? phase === "start"
        ? "start_approval"
        : "completion_approval"
      : phase === "start"
        ? "ready"
        : "completed";
  } else if (input.action === "start") state = "in_progress";
  else if (input.action === "approve" || input.action === "reject") {
    const decision = await prepareDecision(
      db,
      actor,
      key,
      "task",
      id,
      input.action === "approve",
      input.note,
      input.expectedVersion,
    );
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
  } else if (input.action === "cancel") {
    if (!input.note.trim())
      throw new ApiError(
        422,
        "REASON_REQUIRED",
        "Explain why the task is being cancelled.",
      );
    steps.push(
      configurationGuard(db, key, actor, task),
      ...closeApprovals(
        db,
        key,
        "task",
        id,
        actor.userId,
        "cancel",
        input.note,
        input.expectedVersion,
      ),
    );
    state = "cancelled";
  }
  steps.push(
    db
      .prepare(
        "UPDATE staff_tasks SET state=?,completion_note=CASE WHEN ? IN ('complete','save-progress') THEN ? ELSE completion_note END,blocked_reason=NULL,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(state, input.action, input.note, Date.now(), id),
    ...(prepared?.steps ?? []),
    notification(
      db,
      task,
      task.assignee_user_id,
      "task",
      id,
      input.expectedVersion + 1,
      `Task ${state.replaceAll("_", " ")}`,
    ),
  );
  if(task.source_post_id){
    const post=await readablePost(db,actor,task.source_post_id);
    const followers=await db.prepare("SELECT DISTINCT principal_id FROM social_follows WHERE target_type='post' AND target_id=?").bind(post.id).all<{principal_id:string}>();
    for(const user of new Set([post.author_id,task.creator_user_id,...followers.results.map(f=>f.principal_id)])){
      if(user===actor.userId)continue;
      try{await readablePost(db,{companyId:actor.companyId,userId:user},post.id);steps.push(notification(db,post,user,'post',post.id,input.expectedVersion+1,'Work on a conversation you follow has progressed'));}catch(e){if(!(e instanceof ApiError)||e.status!==404)throw e;}
    }
  }
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    { id, state, version: input.expectedVersion + 1, ...(prepared ? { manifest: prepared.manifest } : {}) },
    steps,
    scope(task),
  );
}
export function canAttachTask(actor: StaffActor, task: TaskRow) {
  return (
    (task.state === "draft" && canConfigureTask(actor, task)) ||
    ((["in_progress", "rework"].includes(task.state) ||
      task.state === "ready" && isOrdinaryTask(task)) &&
      (samePerson(actor,task.assignee_user_id) || isActiveDelegate(actor, task)))
  );
}
