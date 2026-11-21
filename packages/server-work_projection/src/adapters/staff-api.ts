import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { listStaffNotifications } from "@erpx/server-collaboration/public/staff-notifications";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { staffPeople, activeStaff, companyAuthority } from "@erpx/server-operations/public/staff-policy";
import {
  assignLevel,
  headSchema,
  levelAssignmentSchema,
  levelSchema,
  saveLevel,
  staffSettings,
} from "@erpx/server-operations/public/staff-settings";
import {listTasks,taskDetail} from '@erpx/server-operations/public/staff-tasks';
import {
  listProjectMilestones,
  projectMilestoneSchema,
  projectMilestoneTransitionSchema,
} from "@erpx/server-projects/public/project-milestones";
import {
  listProjectWorkLogs,
  projectWorkLogDecisionSchema,
  projectWorkLogSchema,
} from "@erpx/server-projects/public/project-actuals";
import {nativeProjectCommand} from "@erpx/server-projects/public/native-project-commands";
import {
  canEditTemplate,
  getTemplate,
  listTemplates,
} from "@erpx/server-operations/public/routines";
import {rerouteApproval} from '@erpx/server-workflow/public/approvals';
import {changeAppointment} from '@erpx/server-organization/public/department-authority';
import {getProject,listProjects,projectSchema} from "@erpx/server-projects/public/staff-projects";
import {z} from "zod";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {createStaffApiService,type StaffApiPorts} from "@erpx/server-work_projection/application/staff-api";
import type {StaffResource,StaffRequestContext} from "@erpx/server-work_projection/domain/staff-api";
import {workUiStaffFacts as projectsFacts} from "@erpx/server-projects/public/work-ui-staff-facts";
import {workUiStaffFacts as operationsFacts} from "@erpx/server-operations/public/work-ui-staff-facts";
import {workUiStaffFacts as workflowFacts} from "@erpx/server-workflow/public/work-ui-staff-facts";
import {markStaffNotificationRead} from "@erpx/server-collaboration/public/inbox";
import {workUiStaffFacts as people_payrollFacts} from "@erpx/server-people_payroll/public/work-ui-staff-facts";
export function staffHandler(resource:StaffResource){return async(request:Request,context:StaffRequestContext)=>{let db:SqlDatabase;const service=createStaffApiService({requireWorkContext,prepare:async actor=>{db=(await holdingResources(actor.holdingId)).write;},now:Date.now,error:(status,code,message)=>new ApiError(status,code,message),requestPublicOrigin,markNotificationRead:(actor,id,key,now)=>markStaffNotificationRead(db,actor,id,key,now),listStaffNotifications:(...args:Parameters<StaffApiPorts["listStaffNotifications"]>)=>listStaffNotifications(db,...args),staffPeople:(...args:Parameters<StaffApiPorts["staffPeople"]>)=>staffPeople(db,...args),activeStaff:(...args:Parameters<StaffApiPorts["activeStaff"]>)=>activeStaff(db,...args),companyAuthority:(...args:Parameters<StaffApiPorts["companyAuthority"]>)=>companyAuthority(db,...args),assignLevel:(...args:Parameters<StaffApiPorts["assignLevel"]>)=>assignLevel(db,...args),saveLevel:(...args:Parameters<StaffApiPorts["saveLevel"]>)=>saveLevel(db,...args),staffSettings:(...args:Parameters<StaffApiPorts["staffSettings"]>)=>staffSettings(db,...args),getProject:(...args:Parameters<StaffApiPorts["getProject"]>)=>getProject(db,...args),listProjects:(...args:Parameters<StaffApiPorts["listProjects"]>)=>listProjects(db,...args),listTasks:(...args:Parameters<StaffApiPorts["listTasks"]>)=>listTasks(db,...args),taskDetail:(...args:Parameters<StaffApiPorts["taskDetail"]>)=>taskDetail(db,...args),listProjectMilestones:(...args:Parameters<StaffApiPorts["listProjectMilestones"]>)=>listProjectMilestones(db,...args),listProjectWorkLogs:(...args:Parameters<StaffApiPorts["listProjectWorkLogs"]>)=>listProjectWorkLogs(db,...args),projectCommand:({request,holdingSlug,operation,projectId,recordId,input,key,branchSlug,departmentSlug})=>nativeProjectCommand(request,holdingSlug,{operation,projectId,recordId,key,body:input,branchSlug,departmentSlug}),canEditTemplate:(...args:Parameters<StaffApiPorts["canEditTemplate"]>)=>canEditTemplate(db,...args),getTemplate:(...args:Parameters<StaffApiPorts["getTemplate"]>)=>getTemplate(db,...args),listTemplates:(...args:Parameters<StaffApiPorts["listTemplates"]>)=>listTemplates(db,...args),rerouteApproval:(...args:Parameters<StaffApiPorts["rerouteApproval"]>)=>rerouteApproval(db,...args),changeAppointment:(...args:Parameters<StaffApiPorts["changeAppointment"]>)=>changeAppointment(db,...args),headSchema:input=>headSchema.parse(input),levelAssignmentSchema:input=>levelAssignmentSchema.parse(input),levelSchema:input=>levelSchema.parse(input),projectSchema:input=>projectSchema.parse(input),projectMilestoneSchema:input=>projectMilestoneSchema.parse(input),projectMilestoneTransitionSchema:input=>projectMilestoneTransitionSchema.parse(input),projectWorkLogDecisionSchema:input=>projectWorkLogDecisionSchema.parse(input),projectWorkLogSchema:input=>projectWorkLogSchema.parse(input),facts:{projectMembers:(...args:Parameters<StaffApiPorts["facts"]["projectMembers"]>)=>projectsFacts(db).projectMembers(...args),templateSchedules:(...args:Parameters<StaffApiPorts["facts"]["templateSchedules"]>)=>operationsFacts(db).templateSchedules(...args),approvalQueue:(...args:Parameters<StaffApiPorts["facts"]["approvalQueue"]>)=>workflowFacts(db).approvalQueue(...args),headPlacement:(...args:Parameters<StaffApiPorts["facts"]["headPlacement"]>)=>people_payrollFacts(db).headPlacement(...args),betaApproval:(...args:Parameters<StaffApiPorts["facts"]["betaApproval"]>)=>workflowFacts(db).betaApproval(...args)}});try{return Response.json(canonicalDestinationLinks({data:await service(resource,request,context)}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){if(error instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",error.issues[0]?.message??"Invalid input."));if(error instanceof SyntaxError)return errorResponse(new ApiError(422,"INVALID_JSON","Use valid JSON."));return errorResponse(error);}};}
