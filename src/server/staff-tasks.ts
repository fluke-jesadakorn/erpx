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
    dueAt: z.iso.datetime({ offset: true }),
    expectedVersion: z.number().int().positive().default(1),
    reason: z.string().trim().max(2000).default(""),
  })
  .refine((v) => v.startApprovers.length + v.completionApprovers.length > 0, {
    message: "Configure at least one start or completion approver.",
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
  due_at: number;
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
};
export type TaskDetail = {
  task: TaskRow;
  approval: ApprovalView;
  permittedActions: string[];
  blockedReason: string | null;
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
const taskSelect = `SELECT t.*,o.post_id source_post_id,p.title project_title,p.owner_user_id project_owner_id,COALESCE(a.legal_name,t.assignee_user_id) assignee_name,COALESCE(c.legal_name,t.creator_user_id) creator_name FROM staff_tasks t LEFT JOIN social_task_origins o ON o.task_id=t.id LEFT JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id LEFT JOIN hr_people a ON a.user_id=t.assignee_user_id AND a.company_id=t.company_id AND a.merged_into_id IS NULL LEFT JOIN hr_people c ON c.user_id=t.creator_user_id AND c.company_id=t.company_id AND c.merged_into_id IS NULL`;
const taskAccess = `(t.creator_user_id=? OR t.assignee_user_id=? OR p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members pm WHERE pm.project_id=t.project_id AND pm.user_id=?) OR EXISTS(SELECT 1 FROM erpx_json_each(t.start_approvers_json) WHERE value=?) OR EXISTS(SELECT 1 FROM erpx_json_each(t.completion_approvers_json) WHERE value=?) OR ${participantSql("task", "t")})`;
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
    )
    .first<TaskRow>();
  if (!row) throw new ApiError(404, "TASK_NOT_FOUND", "Task not found.");
  if(row.source_post_id)await readablePost(db,actor,row.source_post_id);
  return row;
}
export function canConfigureTask(actor: StaffActor, task: TaskRow) {
  return (
    task.creator_user_id === actor.userId ||
    task.project_owner_id === actor.userId
  );
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
    "EXISTS(SELECT 1 FROM staff_tasks t LEFT JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id WHERE t.id=? AND t.company_id=? AND (t.creator_user_id=? OR p.owner_user_id=?))",
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
  if (current && current.owner_user_id !== actor.userId)
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
  if (revision && !input.reason.trim())
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
  );
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
    { id: recordId, state: "draft", version },
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
              "UPDATE staff_tasks SET title=?,description=?,assignee_user_id=?,due_at=?,project_id=?,start_approvers_json=?,completion_approvers_json=?,state='draft',completion_note='',blocked_reason=NULL,revision=revision+?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(
              input.title,
              input.description,
              input.assigneeUserId,
              Date.parse(input.dueAt),
              input.projectId,
              JSON.stringify(input.startApprovers),
              JSON.stringify(input.completionApprovers),
              revision ? 1 : 0,
              now,
              recordId,
            )
        : db
            .prepare(
              "INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,due_at,project_id,start_approvers_json,completion_approvers_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
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
              Date.parse(input.dueAt),
              input.projectId,
              JSON.stringify(input.startApprovers),
              JSON.stringify(input.completionApprovers),
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
export async function taskDetail(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
): Promise<TaskDetail> {
  const task = await getTask(db, actor, id),
    approval = await approvalView(db, "task", id, actor),
    manage = canConfigureTask(actor, task),
    assigned = task.assignee_user_id === actor.userId;
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
      );
      blockedReason ??= route.blockedReason;
    }
  }
  const permittedActions = [
    ...(manage && task.state === "draft"
      ? ["edit", ...(!blockedReason ? ["submit"] : [])]
      : []),
    ...(manage && !["completed", "cancelled"].includes(task.state)
      ? ["revise", "cancel"]
      : []),
    ...(assigned && task.state === "ready" && !blockedReason ? ["start"] : []),
    ...(assigned && ["in_progress", "rework"].includes(task.state)
      ? ["save-progress", ...(!blockedReason ? ["complete"] : [])]
      : []),
    ...(approval.canDecide ? ["approve", "reject"] : []),
  ];
  const attachments = (
    await db
      .prepare(
        "SELECT id,original_name,bytes,media_type FROM attachments WHERE owner_type='staff_task' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at",
      )
      .bind(id)
      .all<TaskDetail["attachments"][number]>()
  ).results;
  return { task, approval, permittedActions, blockedReason, attachments };
}
export async function transitionTask(
  db: SqlDatabase,
  actor: StaffActor,
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
  const steps: SqlStatement[] = [
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
    else if (!input.note.trim())
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
    { id, state, version: input.expectedVersion + 1 },
    steps,
    scope(task),
  );
}
export function canAttachTask(actor: StaffActor, task: TaskRow) {
  return (
    (task.state === "draft" && canConfigureTask(actor, task)) ||
    (["in_progress", "rework"].includes(task.state) &&
      task.assignee_user_id === actor.userId)
  );
}
