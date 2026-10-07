import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { listStaffNotifications } from "@/server/modules/collaboration/public/staff-notifications";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { staffPeople, activeStaff, companyAuthority } from "@/server/modules/operations/public/staff-policy";
import {
  assignLevel,
  headSchema,
  levelAssignmentSchema,
  levelSchema,
  saveLevel,
  staffSettings,
} from "@/server/modules/operations/public/staff-settings";
import {
  listTasks,
  saveTask,
  taskActionSchema,
  taskDelegationRevokeSchema,
  taskDelegationSchema,
  taskDetail,
  taskSchema,
  delegateTask,
  revokeTaskDelegation,
  transitionTask,
} from "@/server/modules/operations/public/staff-tasks";
import {
  listProjectMilestones,
  projectMilestoneSchema,
  projectMilestoneTransitionSchema,
  saveProjectMilestone,
  transitionProjectMilestone,
} from "@/server/modules/projects/public/project-milestones";
import {
  decideProjectWorkLog,
  listProjectWorkLogs,
  projectWorkLogDecisionSchema,
  projectWorkLogSchema,
  saveProjectWorkLog,
} from "@/server/modules/projects/public/project-actuals";
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
} from "@/server/modules/operations/public/routines";
import { rerouteApproval } from "@/server/modules/workflow/public/approvals";
import { changeAppointment } from "@/server/modules/organization/public/department-authority";
import {getProject,listProjects,projectSchema,saveProject} from "../../projects/public/staff-projects";
import {z} from "zod";
import {holdingResources} from "../../runtime/public/runtime";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError,errorResponse} from "../../runtime/public/http";
import {createStaffApiService,type StaffApiPorts} from "../application/staff-api";
import type {StaffResource,StaffRequestContext} from "../domain/staff-api";
import {workUiStaffFacts as projectsFacts} from "../../projects/public/work-ui-staff-facts";
import {workUiStaffFacts as operationsFacts} from "../../operations/public/work-ui-staff-facts";
import {workUiStaffFacts as workflowFacts} from "../../workflow/public/work-ui-staff-facts";
import {workUiStaffFacts as collaborationFacts} from "../../collaboration/public/work-ui-staff-facts";
import {workUiStaffFacts as people_payrollFacts} from "../../people_payroll/public/work-ui-staff-facts";
export function staffHandler(resource:StaffResource){return async(request:Request,context:StaffRequestContext)=>{let db:SqlDatabase;const service=createStaffApiService({requireWorkContext,prepare:async actor=>{db=(await holdingResources(actor.holdingId)).write;},now:Date.now,error:(status,code,message)=>new ApiError(status,code,message),requestPublicOrigin,listStaffNotifications:(...args:Parameters<StaffApiPorts["listStaffNotifications"]>)=>listStaffNotifications(db,...args),staffPeople:(...args:Parameters<StaffApiPorts["staffPeople"]>)=>staffPeople(db,...args),activeStaff:(...args:Parameters<StaffApiPorts["activeStaff"]>)=>activeStaff(db,...args),companyAuthority:(...args:Parameters<StaffApiPorts["companyAuthority"]>)=>companyAuthority(db,...args),assignLevel:(...args:Parameters<StaffApiPorts["assignLevel"]>)=>assignLevel(db,...args),saveLevel:(...args:Parameters<StaffApiPorts["saveLevel"]>)=>saveLevel(db,...args),staffSettings:(...args:Parameters<StaffApiPorts["staffSettings"]>)=>staffSettings(db,...args),getProject:(...args:Parameters<StaffApiPorts["getProject"]>)=>getProject(db,...args),listProjects:(...args:Parameters<StaffApiPorts["listProjects"]>)=>listProjects(db,...args),listTasks:(...args:Parameters<StaffApiPorts["listTasks"]>)=>listTasks(db,...args),saveProject:(...args:Parameters<StaffApiPorts["saveProject"]>)=>saveProject(db,...args),saveTask:(...args:Parameters<StaffApiPorts["saveTask"]>)=>saveTask(db,...args),taskDetail:(...args:Parameters<StaffApiPorts["taskDetail"]>)=>taskDetail(db,...args),delegateTask:(...args:Parameters<StaffApiPorts["delegateTask"]>)=>delegateTask(db,...args),revokeTaskDelegation:(...args:Parameters<StaffApiPorts["revokeTaskDelegation"]>)=>revokeTaskDelegation(db,...args),transitionTask:(...args:Parameters<StaffApiPorts["transitionTask"]>)=>transitionTask(db,...args),listProjectMilestones:(...args:Parameters<StaffApiPorts["listProjectMilestones"]>)=>listProjectMilestones(db,...args),saveProjectMilestone:(...args:Parameters<StaffApiPorts["saveProjectMilestone"]>)=>saveProjectMilestone(db,...args),transitionProjectMilestone:(...args:Parameters<StaffApiPorts["transitionProjectMilestone"]>)=>transitionProjectMilestone(db,...args),decideProjectWorkLog:(...args:Parameters<StaffApiPorts["decideProjectWorkLog"]>)=>decideProjectWorkLog(db,...args),listProjectWorkLogs:(...args:Parameters<StaffApiPorts["listProjectWorkLogs"]>)=>listProjectWorkLogs(db,...args),saveProjectWorkLog:(...args:Parameters<StaffApiPorts["saveProjectWorkLog"]>)=>saveProjectWorkLog(db,...args),canEditTemplate:(...args:Parameters<StaffApiPorts["canEditTemplate"]>)=>canEditTemplate(db,...args),changeScheduleState:(...args:Parameters<StaffApiPorts["changeScheduleState"]>)=>changeScheduleState(db,...args),getTemplate:(...args:Parameters<StaffApiPorts["getTemplate"]>)=>getTemplate(db,...args),listTemplates:(...args:Parameters<StaffApiPorts["listTemplates"]>)=>listTemplates(db,...args),runTemplate:(...args:Parameters<StaffApiPorts["runTemplate"]>)=>runTemplate(db,...args),saveSchedule:(...args:Parameters<StaffApiPorts["saveSchedule"]>)=>saveSchedule(db,...args),saveTemplate:(...args:Parameters<StaffApiPorts["saveTemplate"]>)=>saveTemplate(db,...args),rerouteApproval:(...args:Parameters<StaffApiPorts["rerouteApproval"]>)=>rerouteApproval(db,...args),changeAppointment:(...args:Parameters<StaffApiPorts["changeAppointment"]>)=>changeAppointment(db,...args),headSchema:input=>headSchema.parse(input),levelAssignmentSchema:input=>levelAssignmentSchema.parse(input),levelSchema:input=>levelSchema.parse(input),projectSchema:input=>projectSchema.parse(input),taskActionSchema:input=>taskActionSchema.parse(input),taskDelegationRevokeSchema:input=>taskDelegationRevokeSchema.parse(input),taskDelegationSchema:input=>taskDelegationSchema.parse(input),taskSchema:input=>taskSchema.parse(input),projectMilestoneSchema:input=>projectMilestoneSchema.parse(input),projectMilestoneTransitionSchema:input=>projectMilestoneTransitionSchema.parse(input),projectWorkLogDecisionSchema:input=>projectWorkLogDecisionSchema.parse(input),projectWorkLogSchema:input=>projectWorkLogSchema.parse(input),scheduleSchema:input=>scheduleSchema.parse(input),templateSchema:input=>templateSchema.parse(input),facts:{projectMembers:(...args:Parameters<StaffApiPorts["facts"]["projectMembers"]>)=>projectsFacts(db).projectMembers(...args),templateSchedules:(...args:Parameters<StaffApiPorts["facts"]["templateSchedules"]>)=>operationsFacts(db).templateSchedules(...args),approvalQueue:(...args:Parameters<StaffApiPorts["facts"]["approvalQueue"]>)=>workflowFacts(db).approvalQueue(...args),notificationRead:(...args:Parameters<StaffApiPorts["facts"]["notificationRead"]>)=>collaborationFacts(db).notificationRead(...args),headPlacement:(...args:Parameters<StaffApiPorts["facts"]["headPlacement"]>)=>people_payrollFacts(db).headPlacement(...args),betaApproval:(...args:Parameters<StaffApiPorts["facts"]["betaApproval"]>)=>workflowFacts(db).betaApproval(...args)}});try{return Response.json({data:await service(resource,request,context)},{headers:{"Cache-Control":"private, no-store"}});}catch(error){if(error instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",error.issues[0]?.message??"Invalid input."));if(error instanceof SyntaxError)return errorResponse(new ApiError(422,"INVALID_JSON","Use valid JSON."));return errorResponse(error);}};}
