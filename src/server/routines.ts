import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { z } from "zod";
import { nextOccurrence, type RoutineRule } from "../lib/routine-calendar";
import {
  taskFieldsSchema,
  validateTaskConfiguration,
  configGuards,
} from "./staff-tasks";
import {
  activeStaff,
  companyAuthority,
  staffGuard,
  notification,
  type RecordScope,
  type StaffActor,
} from "./staff-policy";
import { taskRoute, openApproval } from "./approvals";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";

export const templateSchema = taskFieldsSchema
  .extend({
    published: z.boolean().default(true),
    expectedVersion: z.number().int().positive().default(1),
  })
  .refine((v) => v.startApprovers.length + v.completionApprovers.length > 0, {
    message: "Configure at least one start or completion approver.",
  });
export const scheduleSchema = z
  .object({
    frequency: z.enum(["daily", "weekly", "monthly"]),
    timezone: z
      .string()
      .max(80)
      .default("Asia/Bangkok")
      .refine((v) => {
        try {
          new Intl.DateTimeFormat("en", { timeZone: v });
          return true;
        } catch {
          return false;
        }
      }, "Choose a valid IANA timezone."),
    occurrenceTime: z
      .string()
      .regex(/^([01]\d|2[0-3]):[0-5]\d$/)
      .default("09:00"),
    startDate: z.iso.date(),
    endDate: z.iso.date().nullable().default(null),
    weekday: z.number().int().min(0).max(6).nullable().default(null),
    monthDay: z.number().int().min(1).max(31).nullable().default(null),
    dueOffsetHours: z.number().int().min(0).max(8760).default(24),
    expectedVersion: z.number().int().positive().default(1),
  })
  .refine((v) => !v.endDate || v.endDate >= v.startDate, {
    message: "End date must follow start date.",
  });
export type TemplateRow = RecordScope & {
  id: string;
  title: string;
  description: string;
  creator_user_id: string;
  assignee_user_id: string;
  project_id: string | null;
  start_approvers_json: string;
  completion_approvers_json: string;
  published: number;
  version: number;
  created_at: number;
  updated_at: number;
};
export type ScheduleRow = RoutineRule & {
  id: string;
  template_id: string;
  company_id: string;
  due_offset_hours: number;
  state: "active" | "paused" | "ended";
  next_occurrence_at: number | null;
  paused_at: number | null;
  last_generated_at: number | null;
  last_error: string | null;
  version: number;
  created_at: number;
  updated_at: number;
};
const eventScope = (r: RecordScope) => ({
  companyId: r.company_id,
  branchId: r.branch_id,
  departmentId: r.department_id,
});
export async function listTemplates(db: SqlDatabase, actor: StaffActor) {
  const owner = await companyAuthority(db, actor, ["holding_owner"]);
  return (
    await db
      .prepare(
        "SELECT * FROM routine_templates WHERE company_id=? AND (published=1 OR creator_user_id=? OR ?=1) ORDER BY updated_at DESC,id",
      )
      .bind(actor.companyId, actor.userId, owner ? 1 : 0)
      .all<TemplateRow>()
  ).results;
}
export async function getTemplate(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
) {
  const row = await db
    .prepare("SELECT * FROM routine_templates WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<TemplateRow>();
  if (
    !row ||
    (!row.published &&
      row.creator_user_id !== actor.userId &&
      !(await companyAuthority(db, actor, ["holding_owner"])))
  )
    throw new ApiError(
      404,
      "TEMPLATE_NOT_FOUND",
      "Routine template not found.",
    );
  return row;
}
export async function canEditTemplate(
  db: SqlDatabase,
  actor: StaffActor,
  row: TemplateRow,
) {
  return (
    row.creator_user_id === actor.userId ||
    (await companyAuthority(db, actor, ["holding_owner"]))
  );
}
function templateAuthorityGuard(
  db: SqlDatabase,
  key: string,
  actor: StaffActor,
  id: string,
) {
  const now = Date.now();
  return assertion(
    db,
    key,
    "template-authority",
    "EXISTS(SELECT 1 FROM routine_templates t WHERE t.id=? AND t.company_id=? AND (t.creator_user_id=? OR EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=? AND a.company_id=t.company_id AND a.role='holding_owner' AND a.scope_kind='holding' AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?)))",
    [id, actor.companyId, actor.userId, actor.userId, now, now],
  );
}
export async function saveTemplate(
  db: SqlDatabase,
  actor: StaffActor,
  id: string | null,
  input: z.infer<typeof templateSchema>,
  key: string,
  cloneId?: string,
) {
  const recordId = id ?? `routine-${key}`,
    operation = cloneId
      ? "routine.clone"
      : id
        ? "routine.update"
        : "routine.create",
    commandInput = { ...input, cloneId: cloneId ?? null };
  const prior = await replayCommand(
    db,
    actor,
    key,
    operation,
    recordId,
    commandInput,
  );
  if (prior) return prior;
  const staff = await activeStaff(db, actor.companyId, actor.userId),
    current = id ? await getTemplate(db, actor, id) : null;
  if (cloneId) await getTemplate(db, actor, cloneId);
  if (current && !(await canEditTemplate(db, actor, current)))
    throw new ApiError(
      403,
      "TEMPLATE_OWNER_REQUIRED",
      "Only the creator or company owner edits this source. Clone it to make your own version.",
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
    { id: recordId, version },
    [
      staffGuard(db, key, actor.companyId, actor.userId),
      ...configGuards(db, key, actor, input),
      ...(current
        ? [
            templateAuthorityGuard(db, key, actor, recordId),
            assertion(
              db,
              key,
              "template-version",
              "EXISTS(SELECT 1 FROM routine_templates WHERE id=? AND version=?)",
              [recordId, input.expectedVersion],
            ),
          ]
        : []),
      current
        ? db
            .prepare(
              "UPDATE routine_templates SET title=?,description=?,assignee_user_id=?,project_id=?,start_approvers_json=?,completion_approvers_json=?,published=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(
              input.title,
              input.description,
              input.assigneeUserId,
              input.projectId,
              JSON.stringify(input.startApprovers),
              JSON.stringify(input.completionApprovers),
              input.published ? 1 : 0,
              now,
              recordId,
            )
        : db
            .prepare(
              "INSERT INTO routine_templates(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,project_id,start_approvers_json,completion_approvers_json,published,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
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
              input.projectId,
              JSON.stringify(input.startApprovers),
              JSON.stringify(input.completionApprovers),
              input.published ? 1 : 0,
              now,
              now,
            ),
    ],
    eventScope(current ?? staff),
  );
}
export async function saveSchedule(
  db: SqlDatabase,
  actor: StaffActor,
  templateId: string,
  id: string | null,
  input: z.infer<typeof scheduleSchema>,
  key: string,
) {
  const recordId = id ?? `schedule-${key}`,
    operation = id ? "routine.schedule.update" : "routine.schedule.create";
  const prior = await replayCommand(db, actor, key, operation, recordId, input);
  if (prior) return prior;
  const template = await getTemplate(db, actor, templateId);
  if (!(await canEditTemplate(db, actor, template)))
    throw new ApiError(
      403,
      "TEMPLATE_OWNER_REQUIRED",
      "The source owner manages its schedules.",
    );
  const rule: RoutineRule = {
      frequency: input.frequency,
      timezone: input.timezone,
      occurrence_time: input.occurrenceTime,
      start_date: input.startDate,
      end_date: input.endDate,
      weekday: input.weekday,
      month_day: input.monthDay,
    },
    now = Date.now(),
    next = nextOccurrence(rule, now - 1),
    version = id ? input.expectedVersion + 1 : 1;
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    recordId,
    version,
    { id: recordId, state: "paused", version },
    [
      templateAuthorityGuard(db, key, actor, templateId),
      ...(id
        ? [
            assertion(
              db,
              key,
              "schedule-version",
              "EXISTS(SELECT 1 FROM routine_schedules WHERE id=? AND template_id=? AND company_id=? AND version=? AND state='paused')",
              [recordId, templateId, actor.companyId, input.expectedVersion],
            ),
          ]
        : []),
      id
        ? db
            .prepare(
              "UPDATE routine_schedules SET frequency=?,timezone=?,occurrence_time=?,start_date=?,end_date=?,weekday=?,month_day=?,due_offset_hours=?,next_occurrence_at=?,last_error=NULL,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(
              rule.frequency,
              rule.timezone,
              rule.occurrence_time,
              rule.start_date,
              rule.end_date,
              rule.weekday,
              rule.month_day,
              input.dueOffsetHours,
              next,
              now,
              recordId,
            )
        : db
            .prepare(
              "INSERT INTO routine_schedules(id,template_id,company_id,frequency,timezone,occurrence_time,start_date,end_date,weekday,month_day,due_offset_hours,next_occurrence_at,paused_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              recordId,
              templateId,
              actor.companyId,
              rule.frequency,
              rule.timezone,
              rule.occurrence_time,
              rule.start_date,
              rule.end_date,
              rule.weekday,
              rule.month_day,
              input.dueOffsetHours,
              next,
              now,
              now,
              now,
            ),
      ...(!id
        ? [
            db
              .prepare(
                "INSERT INTO routine_schedule_pauses(id,schedule_id,started_at) VALUES(?,?,?)",
              )
              .bind(key, recordId, now),
          ]
        : []),
    ],
    eventScope(template),
  );
}
export async function changeScheduleState(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  version: number,
  resume: boolean,
  key: string,
) {
  const input = { expectedVersion: version, resume },
    operation = resume ? "routine.schedule.resume" : "routine.schedule.pause";
  const prior = await replayCommand(db, actor, key, operation, id, input);
  if (prior) return prior;
  const row = await db
    .prepare("SELECT * FROM routine_schedules WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<ScheduleRow>();
  if (!row)
    throw new ApiError(404, "SCHEDULE_NOT_FOUND", "Schedule not found.");
  const template = await getTemplate(db, actor, row.template_id);
  if (!(await canEditTemplate(db, actor, template)))
    throw new ApiError(
      403,
      "TEMPLATE_OWNER_REQUIRED",
      "The source owner manages its schedules.",
    );
  if (resume && !template.published)
    throw new ApiError(
      409,
      "TEMPLATE_UNPUBLISHED",
      "Publish the template before enabling its schedule.",
    );
  const now = Date.now();
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    id,
    version + 1,
    { id, state: resume ? "active" : "paused", version: version + 1 },
    [
      templateAuthorityGuard(db, key, actor, template.id),
      assertion(
        db,
        key,
        "schedule-version",
        "EXISTS(SELECT 1 FROM routine_schedules WHERE id=? AND version=? AND state=?)",
        [id, version, resume ? "paused" : "active"],
      ),
      db
        .prepare(
          "UPDATE routine_schedules SET state=?,paused_at=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(resume ? "active" : "paused", resume ? null : now, now, id),
      resume
        ? db
            .prepare(
              "UPDATE routine_schedule_pauses SET ended_at=? WHERE schedule_id=? AND ended_at IS NULL",
            )
            .bind(now, id)
        : db
            .prepare(
              "INSERT INTO routine_schedule_pauses(id,schedule_id,started_at) VALUES(?,?,?)",
            )
            .bind(key, id, now),
    ],
    eventScope(template),
  );
}
/** Immutable task snapshot. Both manual runs and scheduled occurrences use this preparation. */
async function occurrenceStatements(
  db: SqlDatabase,
  template: TemplateRow,
  id: string,
  key: string,
  dueAt: number,
  creator: string,
  schedule: ScheduleRow | null,
  occurrenceAt: number | null,
) {
  const now = Date.now(),
    start: string[] = JSON.parse(template.start_approvers_json),
    completion: string[] = JSON.parse(template.completion_approvers_json);
  const startRoute = await taskRoute(
      db,
      template,
      creator,
      template.assignee_user_id,
      "start",
      start,
    ),
    completionRoute = await taskRoute(
      db,
      template,
      creator,
      template.assignee_user_id,
      "completion",
      completion,
    );
  let blocked = startRoute.blockedReason ?? completionRoute.blockedReason;
  if (
    template.project_id &&
    !(await db
      .prepare("SELECT 1 FROM staff_projects WHERE id=? AND company_id=?")
      .bind(template.project_id, template.company_id)
      .first())
  )
    blocked ??= "The project is unavailable.";
  const state = blocked ? "draft" : start.length ? "start_approval" : "ready";
  const steps: SqlStatement[] = [
    assertion(
      db,
      key,
      "template-version",
      "EXISTS(SELECT 1 FROM routine_templates WHERE id=? AND version=? AND published=1)",
      [template.id, template.version],
    ),
    db
      .prepare(
        "INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,due_at,project_id,start_approvers_json,completion_approvers_json,state,blocked_reason,template_id,template_version,schedule_id,occurrence_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        id,
        template.company_id,
        template.branch_id,
        template.department_id,
        template.title,
        template.description,
        creator,
        template.assignee_user_id,
        dueAt,
        template.project_id,
        template.start_approvers_json,
        template.completion_approvers_json,
        state,
        blocked,
        template.id,
        template.version,
        schedule?.id ?? null,
        occurrenceAt,
        now,
        now,
      ),
    notification(
      db,
      template,
      template.assignee_user_id,
      "task",
      id,
      1,
      blocked
        ? "Routine task blocked: correction required"
        : "Routine task assigned to you",
    ),
  ];
  if (!blocked && start.length)
    steps.push(
      ...(
        await openApproval(
          db,
          key,
          "task",
          id,
          "start",
          creator,
          template.assignee_user_id,
          startRoute,
          1,
        )
      ).statements,
    );
  if (blocked) {
    steps.push(
      notification(
        db,
        template,
        creator,
        "task",
        id,
        1,
        `Routine blocked: ${blocked}`,
      ),
    );
    console.warn(
      JSON.stringify({
        event: "routine_blocked",
        taskId: id,
        templateId: template.id,
        companyId: template.company_id,
        reason: blocked,
      }),
    );
  }
  return { steps, state };
}
export async function runTemplate(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  dueAt: number,
  key: string,
) {
  const taskId = `task-${key}`,
    input = { templateId: id, dueAt },
    prior = await replayCommand(db, actor, key, "routine.run", taskId, input);
  if (prior) return prior;
  await activeStaff(db, actor.companyId, actor.userId);
  const template = await getTemplate(db, actor, id);
  if (!template.published)
    throw new ApiError(
      409,
      "TEMPLATE_UNPUBLISHED",
      "Publish the template before running it.",
    );
  // A runner is the creator of the occurrence and must have access to the chosen project.
  if (template.project_id) {
    const { getProject } = await import("./staff-tasks");
    await getProject(db, actor, template.project_id);
  }
  const prepared = await occurrenceStatements(
    db,
    template,
    taskId,
    key,
    dueAt,
    actor.userId,
    null,
    null,
  );
  return executeCommand(
    db,
    actor,
    key,
    "routine.run",
    input,
    taskId,
    1,
    { id: taskId, state: prepared.state, version: 1 },
    [staffGuard(db, key, actor.companyId, actor.userId), ...prepared.steps],
    eventScope(template),
  );
}
export async function generateOccurrences(
  db: SqlDatabase,
  now = Date.now(),
  limit = 20,
) {
  let generated = 0,
    examined = 0;
  const schedules = (
    await db
      .prepare(
        "SELECT * FROM routine_schedules WHERE state='active' AND next_occurrence_at<=? ORDER BY next_occurrence_at,id LIMIT ?",
      )
      .bind(now, Math.min(limit, 20))
      .all<ScheduleRow>()
  ).results;
  for (const schedule of schedules) {
    while (
      schedule.next_occurrence_at !== null &&
      schedule.next_occurrence_at <= now &&
      examined < limit
    ) {
      examined++;
      const at = schedule.next_occurrence_at;
      const pause = await db
        .prepare(
          "SELECT ended_at FROM routine_schedule_pauses WHERE schedule_id=? AND started_at<=? AND (ended_at IS NULL OR ended_at>?) ORDER BY started_at DESC LIMIT 1",
        )
        .bind(schedule.id, at, at)
        .first<{ ended_at: number | null }>();
      const next = nextOccurrence(schedule, pause?.ended_at ?? at),
        key = `occurrence-${schedule.id}-${at}`,
        id = `task-${schedule.id}-${at}`;
      try {
        const template = await db
          .prepare(
            "SELECT * FROM routine_templates WHERE id=? AND company_id=? AND published=1",
          )
          .bind(schedule.template_id, schedule.company_id)
          .first<TemplateRow>();
        if (!template) throw new Error("TEMPLATE_UNAVAILABLE");
        const prior = await db
          .prepare(
            "SELECT id FROM staff_tasks WHERE schedule_id=? AND occurrence_at=?",
          )
          .bind(schedule.id, at)
          .first();
        const prepared =
          pause || prior
            ? null
            : await occurrenceStatements(
                db,
                template,
                id,
                key,
                at + schedule.due_offset_hours * 3600000,
                template.creator_user_id,
                schedule,
                at,
              );
        await db.batch([
          assertion(
            db,
            key,
            "schedule-cursor",
            "EXISTS(SELECT 1 FROM routine_schedules WHERE id=? AND version=? AND state='active' AND next_occurrence_at=?)",
            [schedule.id, schedule.version, at],
          ),
          ...(prepared?.steps ?? []),
          db
            .prepare(
              "UPDATE routine_schedules SET next_occurrence_at=?,state=CASE WHEN ?::text IS NULL THEN 'ended' ELSE state END,last_generated_at=CASE WHEN ?=1 THEN ? ELSE last_generated_at END,last_error=NULL,updated_at=? WHERE id=?",
            )
            .bind(next, next, prepared ? 1 : 0, at, now, schedule.id),
          ...(prepared
            ? [
                db
                  .prepare(
                    "INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)",
                  )
                  .bind(
                    key,
                    key,
                    "system",
                    "scheduler",
                    "routine.occurrence",
                    "staff_task",
                    id,
                    1,
                    "Generate scheduled occurrence",
                    now,
                  ),
                db
                  .prepare(
                    "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)",
                  )
                  .bind(
                    key,
                    key,
                    "task.generated",
                    "staff_task",
                    id,
                    1,
                    JSON.stringify({
                      id,
                      state: prepared.state,
                      version: 1,
                      ...eventScope(template),
                    }),
                    now,
                  ),
              ]
            : []),
          db
            .prepare("DELETE FROM command_assertions WHERE command_id=?")
            .bind(key),
        ]);
        if (prepared) generated++;
        schedule.next_occurrence_at = next;
      } catch (error) {
        // A competing generator can win the cursor without making this a generation failure.
        const latest = await db
          .prepare(
            "SELECT next_occurrence_at,version,state FROM routine_schedules WHERE id=?",
          )
          .bind(schedule.id)
          .first<
            Pick<ScheduleRow, "next_occurrence_at" | "version" | "state">
          >();
        if (
          latest?.next_occurrence_at !== at ||
          latest?.version !== schedule.version ||
          latest?.state !== "active"
        )
          break;
        const message =
          error instanceof Error ? error.message : "Generation failed";
        await db
          .prepare(
            "UPDATE routine_schedules SET last_error=?,updated_at=? WHERE id=? AND version=?",
          )
          .bind(message.slice(0, 500), now, schedule.id, schedule.version)
          .run();
        console.error(
          JSON.stringify({
            event: "routine_generation_failed",
            scheduleId: schedule.id,
            companyId: schedule.company_id,
            occurrenceAt: at,
            lagMs: now - at,
            error: message,
          }),
        );
        break;
      }
    }
  }
  const lag = await db
    .prepare(
      "SELECT MIN(next_occurrence_at) oldest FROM routine_schedules WHERE state='active' AND next_occurrence_at<=?",
    )
    .bind(now)
    .first<{ oldest: number | null }>();
  console.log(
    JSON.stringify({
      event: "routine_generation",
      generated,
      examined,
      lagMs: lag?.oldest == null ? 0 : now - lag.oldest,
    }),
  );
  return { generated, examined };
}
