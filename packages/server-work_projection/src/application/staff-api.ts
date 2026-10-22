import {z} from "zod";
import type {WorkContext,requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import type {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import type {listStaffNotifications} from "@erpx/server-collaboration/public/staff-notifications";
import type {} from "@erpx/server-identity_access/public/authorization";
import type {staffPeople,activeStaff,companyAuthority} from "@erpx/server-operations/public/staff-policy";
import type {assignLevel,headSchema,levelAssignmentSchema,levelSchema,saveLevel,staffSettings} from "@erpx/server-operations/public/staff-settings";
import type {listTasks,saveTask,taskActionSchema,taskDelegationRevokeSchema,taskDelegationSchema,taskDetail,taskSchema,delegateTask,revokeTaskDelegation,transitionTask} from "@erpx/server-operations/public/staff-tasks";
import type {listProjectMilestones,projectMilestoneSchema,projectMilestoneTransitionSchema,saveProjectMilestone,transitionProjectMilestone} from "@erpx/server-projects/public/project-milestones";
import type {decideProjectWorkLog,listProjectWorkLogs,projectWorkLogDecisionSchema,projectWorkLogSchema,saveProjectWorkLog} from "@erpx/server-projects/public/project-actuals";
import type {canEditTemplate,changeScheduleState,endSchedule,getTemplate,listTemplates,runTemplate,saveSchedule,saveTemplate,scheduleSchema,templateSchema,ScheduleRow} from "@erpx/server-operations/public/routines";
import type {rerouteApproval} from "@erpx/server-workflow/public/approvals";
import type {changeAppointment} from "@erpx/server-organization/public/department-authority";
import type {getProject,listProjects,projectSchema,saveProject} from "@erpx/server-projects/public/staff-projects";
import type {StaffResource,StaffRequestContext} from "@erpx/server-work_projection/domain/staff-api";
type WithoutDatabase<F> = F extends (...args:infer A)=>infer R ? A extends [unknown,...infer Tail] ? (...args:Tail)=>R:never:never;
export type StaffApiPorts={requireWorkContext:typeof requireWorkContext;prepare(actor:WorkContext):Promise<void>;now():number;error(status:number,code:string,message:string):Error;requestPublicOrigin:typeof requestPublicOrigin;listStaffNotifications:WithoutDatabase<typeof listStaffNotifications>;
staffPeople:WithoutDatabase<typeof staffPeople>;
activeStaff:WithoutDatabase<typeof activeStaff>;
companyAuthority:WithoutDatabase<typeof companyAuthority>;
assignLevel:WithoutDatabase<typeof assignLevel>;
saveLevel:WithoutDatabase<typeof saveLevel>;
staffSettings:WithoutDatabase<typeof staffSettings>;
getProject:WithoutDatabase<typeof getProject>;
listProjects:WithoutDatabase<typeof listProjects>;
listTasks:WithoutDatabase<typeof listTasks>;
saveProject:WithoutDatabase<typeof saveProject>;
saveTask:WithoutDatabase<typeof saveTask>;
taskDetail:WithoutDatabase<typeof taskDetail>;
delegateTask:WithoutDatabase<typeof delegateTask>;
revokeTaskDelegation:WithoutDatabase<typeof revokeTaskDelegation>;
transitionTask:WithoutDatabase<typeof transitionTask>;
listProjectMilestones:WithoutDatabase<typeof listProjectMilestones>;
saveProjectMilestone:WithoutDatabase<typeof saveProjectMilestone>;
transitionProjectMilestone:WithoutDatabase<typeof transitionProjectMilestone>;
decideProjectWorkLog:WithoutDatabase<typeof decideProjectWorkLog>;
listProjectWorkLogs:WithoutDatabase<typeof listProjectWorkLogs>;
saveProjectWorkLog:WithoutDatabase<typeof saveProjectWorkLog>;
canEditTemplate:WithoutDatabase<typeof canEditTemplate>;
changeScheduleState:WithoutDatabase<typeof changeScheduleState>;
endSchedule:WithoutDatabase<typeof endSchedule>;
getTemplate:WithoutDatabase<typeof getTemplate>;
listTemplates:WithoutDatabase<typeof listTemplates>;
runTemplate:WithoutDatabase<typeof runTemplate>;
saveSchedule:WithoutDatabase<typeof saveSchedule>;
saveTemplate:WithoutDatabase<typeof saveTemplate>;
rerouteApproval:WithoutDatabase<typeof rerouteApproval>;
changeAppointment:WithoutDatabase<typeof changeAppointment>;headSchema:typeof headSchema.parse;
levelAssignmentSchema:typeof levelAssignmentSchema.parse;
levelSchema:typeof levelSchema.parse;
projectSchema:typeof projectSchema.parse;
taskActionSchema:typeof taskActionSchema.parse;
taskDelegationRevokeSchema:typeof taskDelegationRevokeSchema.parse;
taskDelegationSchema:typeof taskDelegationSchema.parse;
taskSchema:typeof taskSchema.parse;
projectMilestoneSchema:typeof projectMilestoneSchema.parse;
projectMilestoneTransitionSchema:typeof projectMilestoneTransitionSchema.parse;
projectWorkLogDecisionSchema:typeof projectWorkLogDecisionSchema.parse;
projectWorkLogSchema:typeof projectWorkLogSchema.parse;
scheduleSchema:typeof scheduleSchema.parse;
templateSchema:typeof templateSchema.parse;facts:{projectMembers(value:string):Promise<{results:Record<string,unknown>[]}>;
templateSchedules(value:string,companyId:string):Promise<{results:ScheduleRow[]}>;
approvalQueue(companyId:string,value:0 | 1):Promise<{results:Record<string,unknown>[]}>;
notificationRead(now:number,value:string,companyId:string,userId:string):Promise<unknown>;
headPlacement(companyId:string,value:string,personId:string):Promise<{position_id:string|null}|null>;
betaApproval(value:string,companyId:string):Promise<Record<string,unknown>|null>;};};
export function createStaffApiService(ports:StaffApiPorts){return async function execute(resource:StaffResource,request:Request,context:StaffRequestContext){
const {holdingSlug,path:p=[]}=await context.params;const actor=await ports.requireWorkContext(holdingSlug);await ports.prepare(actor);
      let data: unknown;
      if (request.method === "GET") {
        if (resource === "staff" && p[0] === "notifications")
          data = await ports.listStaffNotifications( actor);
        else if (resource === "staff" && p[0] === "people") {
          const query = new URL(request.url).searchParams;
          await ports.activeStaff( actor.companyId, actor.userId);
          data = await ports.staffPeople( actor.companyId, {
            departmentId: query.get("departmentId") ?? undefined,
            levelId: query.get("levelId") ?? undefined,
          });
        } else if (resource === "staff" && p[0] === "settings")
          data = await ports.staffSettings( actor);
        else if (resource === "staff-tasks")
          data = p[0]
            ? await ports.taskDetail( actor, p[0])
            : await ports.listTasks(
                actor,
                new URL(request.url).searchParams.get("projectId") ?? undefined,
              );
        else if (resource === "projects")
          data = p[0] && p[1] === "milestones"
            ? await ports.listProjectMilestones( actor, p[0])
            : p[0] && p[1] === "work-logs"
            ? await ports.listProjectWorkLogs( actor, p[0])
            : p[0]
            ? {
                project: await ports.getProject( actor, p[0]),
                members: (
                  await ports.facts.projectMembers(p[0])
                ).results,
                tasks: await ports.listTasks( actor, p[0]),
              }
            : await ports.listProjects( actor);
        else if (resource === "routine-templates")
          data = p[0]
            ? {
                template: await ports.getTemplate( actor, p[0]),
                schedules: (
                  await ports.facts.templateSchedules(p[0],actor.companyId)
                ).results,
                canEdit: await ports.canEditTemplate(
                  actor,
                  await ports.getTemplate( actor, p[0]),
                ),
              }
            : await ports.listTemplates( actor);
        else if (
          resource === "approvals" &&
          (await ports.companyAuthority( actor))
        )
          data = (
            await ports.facts.approvalQueue(actor.companyId,actor.beta ? 1 : 0)
          ).results;
        else throw ports.error(404, "NOT_FOUND", "Staff resource not found.");
      } else {
        const origin = request.headers.get("origin");
        if (origin && origin !== ports.requestPublicOrigin(request))
          throw ports.error(
            403,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
          );
        const key = request.headers.get("Idempotency-Key");
        if (!key)
          throw ports.error(
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
          await ports.facts.notificationRead(ports.now(),p[1],actor.companyId,actor.userId);
          data = { id: p[1] };
        } else if (resource === "staff" && p[0] === "levels")
          data = await ports.saveLevel(
            actor,
            p[1] ?? null,
            ports.levelSchema(input),
            key,
          );
        else if (
          resource === "staff" &&
          p[0] === "engagements" &&
          p[2] === "level"
        )
          data = await ports.assignLevel(
            actor,
            p[1],
            ports.levelAssignmentSchema(input),
            key,
          );
        else if (
          resource === "staff" &&
          p[0] === "departments" &&
          p[2] === "head"
        )
          {
            const head=ports.headSchema(input);
            const holder=await ports.facts.headPlacement(actor.companyId,p[1],head.personId);
            data=await ports.changeAppointment(actor,{kind:"head",departmentId:p[1],positionId:holder?.position_id??null,personId:head.personId,expectedVersion:head.expectedVersion,reason:head.reason},key);
          }
        else if (resource === "staff-tasks" && p[1] === "transition")
          data = await ports.transitionTask(
            actor,
            p[0],
            ports.taskActionSchema(input),
            key,
          );
        else if (resource === "staff-tasks" && p[1] === "delegate" && p[2] === "revoke")
          data = await ports.revokeTaskDelegation(
            actor,
            p[0],
            ports.taskDelegationRevokeSchema({ ...input, delegationId: p[3] }),
            key,
          );
        else if (resource === "staff-tasks" && p[1] === "delegate")
          data = await ports.delegateTask(
            actor,
            p[0],
            ports.taskDelegationSchema(input),
            key,
          );
        else if (
          resource === "staff-tasks" &&
          (p.length < 2 || p[1] === "revise")
        )
          data = await ports.saveTask(
            actor,
            p[0] ?? null,
            ports.taskSchema(input),
            key,
            p[1] === "revise",
          );
        else if (resource === "projects" && p[1] === "milestones" && p[3] === "transition")
          data = await ports.transitionProjectMilestone(
            actor,
            p[0],
            p[2],
            ports.projectMilestoneTransitionSchema(input),
            key,
          );
        else if (resource === "projects" && p[1] === "work-logs" && p[3] === "review")
          data = await ports.decideProjectWorkLog(
            actor,
            p[0],
            p[2],
            ports.projectWorkLogDecisionSchema(input),
            key,
          );
        else if (resource === "projects" && p[1] === "work-logs" && p.length <= 3)
          data = await ports.saveProjectWorkLog(
            actor,
            p[0],
            ports.projectWorkLogSchema(input),
            key,
          );
        else if (resource === "projects" && p[1] === "milestones" && p.length <= 3)
          data = await ports.saveProjectMilestone(
            actor,
            p[0],
            p[2] ?? null,
            ports.projectMilestoneSchema(input),
            key,
          );
        else if (resource === "projects" && p.length < 2)
          data = await ports.saveProject(
            actor,
            p[0] ?? null,
            ports.projectSchema(input),
            key,
          );
        else if (resource === "routine-templates" && p[1] === "run") {
          const body = z
            .object({ dueAt: z.iso.datetime({ offset: true }) })
            .parse(input);
          data = await ports.runTemplate(
            actor,
            p[0],
            Date.parse(body.dueAt),
            key,
          );
        } else if (resource === "routine-templates" && p[1] === "clone")
          data = await ports.saveTemplate(
            actor,
            null,
            ports.templateSchema(input),
            key,
            p[0],
          );
        else if (resource === "routine-templates" && p[1] === "schedules")
          data = await ports.saveSchedule(
            actor,
            p[0],
            p[2] ?? null,
            ports.scheduleSchema(input),
            key,
          );
        else if (resource === "routine-templates" && p.length < 2)
          data = await ports.saveTemplate(
            actor,
            p[0] ?? null,
            ports.templateSchema(input),
            key,
          );
        else if (
          resource === "routine-schedules" &&
          ["pause", "resume", "end"].includes(p[1])
        ) {
          const body = z
            .object({ expectedVersion: z.number().int().positive() })
            .parse(input);
          data = p[1]==="end"?await ports.endSchedule(actor,p[0],body.expectedVersion,key):await ports.changeScheduleState(actor,p[0],body.expectedVersion,p[1]==="resume",key);
        } else if (resource === "approvals" && p[1] === "reroute") {
          if (
            actor.beta &&
            !(await ports.facts.betaApproval(p[0],actor.companyId))
          )
            throw ports.error(404, "NOT_FOUND", "Approval not found.");
          data = await ports.rerouteApproval(
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
        } else throw ports.error(404, "NOT_FOUND", "Staff action not found.");
      }
return data;
};}
