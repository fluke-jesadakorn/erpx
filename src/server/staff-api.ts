import { requestPublicOrigin } from "@/server/public-origin";
import { listStaffNotifications } from "./staff-notifications";
import { z } from "zod";
import { requireWorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { ApiError, errorResponse } from "./http";
import { staffPeople, activeStaff, companyAuthority } from "./staff-policy";
import {
  assignLevel,
  headSchema,
  levelAssignmentSchema,
  levelSchema,
  saveLevel,
  staffSettings,
} from "./staff-settings";
import {
  getProject,
  listProjects,
  listTasks,
  projectSchema,
  saveProject,
  saveTask,
  taskActionSchema,
  taskDelegationRevokeSchema,
  taskDelegationSchema,
  taskDetail,
  taskSchema,
  delegateTask,
  revokeTaskDelegation,
  transitionTask,
} from "./staff-tasks";
import {
  listProjectMilestones,
  projectMilestoneSchema,
  projectMilestoneTransitionSchema,
  saveProjectMilestone,
  transitionProjectMilestone,
} from "./project-milestones";
import {
  decideProjectWorkLog,
  listProjectWorkLogs,
  projectWorkLogDecisionSchema,
  projectWorkLogSchema,
  saveProjectWorkLog,
} from "./project-actuals";
import {
  canEditTemplate,
  changeScheduleState,
  getTemplate,
  listTemplates,
  runTemplate,
  saveSchedule,
  saveTemplate,
  scheduleSchema,
  templateSchema,
  type ScheduleRow,
} from "./routines";
import { rerouteApproval } from "./approvals";
import { changeAppointment } from "./department-authority";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export function staffHandler(
  resource:
    | "staff"
    | "staff-tasks"
    | "projects"
    | "routine-templates"
    | "routine-schedules"
    | "approvals",
) {
  return async (request: Request, context: Context) => {
    try {
      const { holdingSlug, path: p = [] } = await context.params,
        actor = await requireWorkContext(holdingSlug),
        { write: db } = await holdingResources(actor.holdingId);
      let data: unknown;
      if (request.method === "GET") {
        if (resource === "staff" && p[0] === "notifications")
          data = await listStaffNotifications(db, actor);
        else if (resource === "staff" && p[0] === "people") {
          const query = new URL(request.url).searchParams;
          await activeStaff(db, actor.companyId, actor.userId);
          data = await staffPeople(db, actor.companyId, {
            departmentId: query.get("departmentId") ?? undefined,
            levelId: query.get("levelId") ?? undefined,
          });
        } else if (resource === "staff" && p[0] === "settings")
          data = await staffSettings(db, actor);
        else if (resource === "staff-tasks")
          data = p[0]
            ? await taskDetail(db, actor, p[0])
            : await listTasks(
                db,
                actor,
                new URL(request.url).searchParams.get("projectId") ?? undefined,
              );
        else if (resource === "projects")
          data = p[0] && p[1] === "milestones"
            ? await listProjectMilestones(db, actor, p[0])
            : p[0] && p[1] === "work-logs"
            ? await listProjectWorkLogs(db, actor, p[0])
            : p[0]
            ? {
                project: await getProject(db, actor, p[0]),
                members: (
                  await db
                    .prepare(
                      "SELECT user_id FROM staff_project_members WHERE project_id=?",
                    )
                    .bind(p[0])
                    .all()
                ).results,
                tasks: await listTasks(db, actor, p[0]),
              }
            : await listProjects(db, actor);
        else if (resource === "routine-templates")
          data = p[0]
            ? {
                template: await getTemplate(db, actor, p[0]),
                schedules: (
                  await db
                    .prepare(
                      "SELECT * FROM routine_schedules WHERE template_id=? AND company_id=? ORDER BY created_at",
                    )
                    .bind(p[0], actor.companyId)
                    .all<ScheduleRow>()
                ).results,
                canEdit: await canEditTemplate(
                  db,
                  actor,
                  await getTemplate(db, actor, p[0]),
                ),
              }
            : await listTemplates(db, actor);
        else if (
          resource === "approvals" &&
          (await companyAuthority(db, actor))
        )
          data = (
            await db
              .prepare(
                "SELECT a.*,s.approver_user_id FROM approval_instances a LEFT JOIN approval_steps s ON s.instance_id=a.id AND s.ordinal=a.current_step WHERE a.company_id=? AND (?=0 OR a.record_type='expense') AND a.state IN ('pending','blocked') ORDER BY a.created_at LIMIT 250",
              )
              .bind(actor.companyId, actor.beta ? 1 : 0)
              .all()
          ).results;
        else throw new ApiError(404, "NOT_FOUND", "Staff resource not found.");
      } else {
        const origin = request.headers.get("origin");
        if (origin && origin !== requestPublicOrigin(request))
          throw new ApiError(
            403,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
          );
        const key = request.headers.get("Idempotency-Key");
        if (!key)
          throw new ApiError(
            422,
            "COMMAND_KEY_REQUIRED",
            "Idempotency-Key is required.",
          );
        const input = await request.json();
        if (
          resource === "staff" &&
          p[0] === "notifications" &&
          p[2] === "read"
        ) {
          await db
            .prepare(
              "UPDATE staff_notifications SET read_at=? WHERE id=? AND company_id=? AND user_id=?",
            )
            .bind(Date.now(), p[1], actor.companyId, actor.userId)
            .run();
          data = { id: p[1] };
        } else if (resource === "staff" && p[0] === "levels")
          data = await saveLevel(
            db,
            actor,
            p[1] ?? null,
            levelSchema.parse(input),
            key,
          );
        else if (
          resource === "staff" &&
          p[0] === "engagements" &&
          p[2] === "level"
        )
          data = await assignLevel(
            db,
            actor,
            p[1],
            levelAssignmentSchema.parse(input),
            key,
          );
        else if (
          resource === "staff" &&
          p[0] === "departments" &&
          p[2] === "head"
        )
          {
            const head=headSchema.parse(input);
            const holder=await db.prepare("SELECT e.position_id FROM hr_engagements e WHERE e.company_id=? AND e.department_id=? AND e.person_id=? AND e.status='active' ORDER BY e.updated_at DESC LIMIT 1").bind(actor.companyId,p[1],head.personId).first<{position_id:string|null}>();
            data=await changeAppointment(db,actor,{kind:"head",departmentId:p[1],positionId:holder?.position_id??null,personId:head.personId,expectedVersion:head.expectedVersion,reason:head.reason},key);
          }
        else if (resource === "staff-tasks" && p[1] === "transition")
          data = await transitionTask(
            db,
            actor,
            p[0],
            taskActionSchema.parse(input),
            key,
          );
        else if (resource === "staff-tasks" && p[1] === "delegate" && p[2] === "revoke")
          data = await revokeTaskDelegation(
            db,
            actor,
            p[0],
            taskDelegationRevokeSchema.parse({ ...input, delegationId: p[3] }),
            key,
          );
        else if (resource === "staff-tasks" && p[1] === "delegate")
          data = await delegateTask(
            db,
            actor,
            p[0],
            taskDelegationSchema.parse(input),
            key,
          );
        else if (
          resource === "staff-tasks" &&
          (p.length < 2 || p[1] === "revise")
        )
          data = await saveTask(
            db,
            actor,
            p[0] ?? null,
            taskSchema.parse(input),
            key,
            p[1] === "revise",
          );
        else if (resource === "projects" && p[1] === "milestones" && p[3] === "transition")
          data = await transitionProjectMilestone(
            db,
            actor,
            p[0],
            p[2],
            projectMilestoneTransitionSchema.parse(input),
            key,
          );
        else if (resource === "projects" && p[1] === "work-logs" && p[3] === "review")
          data = await decideProjectWorkLog(
            db,
            actor,
            p[0],
            p[2],
            projectWorkLogDecisionSchema.parse(input),
            key,
          );
        else if (resource === "projects" && p[1] === "work-logs" && p.length <= 3)
          data = await saveProjectWorkLog(
            db,
            actor,
            p[0],
            projectWorkLogSchema.parse(input),
            key,
          );
        else if (resource === "projects" && p[1] === "milestones" && p.length <= 3)
          data = await saveProjectMilestone(
            db,
            actor,
            p[0],
            p[2] ?? null,
            projectMilestoneSchema.parse(input),
            key,
          );
        else if (resource === "projects" && p.length < 2)
          data = await saveProject(
            db,
            actor,
            p[0] ?? null,
            projectSchema.parse(input),
            key,
          );
        else if (resource === "routine-templates" && p[1] === "run") {
          const body = z
            .object({ dueAt: z.iso.datetime({ offset: true }) })
            .parse(input);
          data = await runTemplate(
            db,
            actor,
            p[0],
            Date.parse(body.dueAt),
            key,
          );
        } else if (resource === "routine-templates" && p[1] === "clone")
          data = await saveTemplate(
            db,
            actor,
            null,
            templateSchema.parse(input),
            key,
            p[0],
          );
        else if (resource === "routine-templates" && p[1] === "schedules")
          data = await saveSchedule(
            db,
            actor,
            p[0],
            p[2] ?? null,
            scheduleSchema.parse(input),
            key,
          );
        else if (resource === "routine-templates" && p.length < 2)
          data = await saveTemplate(
            db,
            actor,
            p[0] ?? null,
            templateSchema.parse(input),
            key,
          );
        else if (
          resource === "routine-schedules" &&
          ["pause", "resume"].includes(p[1])
        ) {
          const body = z
            .object({ expectedVersion: z.number().int().positive() })
            .parse(input);
          data = await changeScheduleState(
            db,
            actor,
            p[0],
            body.expectedVersion,
            p[1] === "resume",
            key,
          );
        } else if (resource === "approvals" && p[1] === "reroute") {
          if (
            actor.beta &&
            !(await db
              .prepare(
                "SELECT 1 FROM approval_instances WHERE id=? AND company_id=? AND record_type='expense'",
              )
              .bind(p[0], actor.companyId)
              .first())
          )
            throw new ApiError(404, "NOT_FOUND", "Approval not found.");
          data = await rerouteApproval(
            db,
            actor,
            p[0],
            z
              .object({
                expectedVersion: z.number().int().positive(),
                instanceVersion: z.number().int().positive(),
                approverUserId: z.string().min(1),
                reason: z.string().trim().min(3).max(2000),
              })
              .parse(input),
            key,
          );
        } else throw new ApiError(404, "NOT_FOUND", "Staff action not found.");
      }
      return Response.json(
        { data },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    } catch (error) {
      if (error instanceof z.ZodError)
        return errorResponse(
          new ApiError(
            422,
            "VALIDATION_FAILED",
            error.issues[0]?.message ?? "Invalid input.",
          ),
        );
      if (error instanceof SyntaxError)
        return errorResponse(
          new ApiError(422, "INVALID_JSON", "Use valid JSON."),
        );
      return errorResponse(error);
    }
  };
}
