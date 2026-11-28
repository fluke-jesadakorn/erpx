import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import { listStaffNotifications } from "@erpx/collaboration-server/public/staff-notifications";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { staffPeople, activeStaff, companyAuthority } from "@erpx/operations-server/public/staff-policy";
import {
  assignLevel,
  headSchema,
  levelAssignmentSchema,
  levelSchema,
  saveLevel,
  staffSettings,
} from "@erpx/operations-server/public/staff-settings";
import {listTasks,taskDetail} from '@erpx/operations-server/public/staff-tasks';
import {
  nativeProjectMilestones,
  projectMilestoneSchema,
  projectMilestoneTransitionSchema,
} from "@erpx/projects-server/public/project-milestones";
import {
  listProjectWorkLogs,
  projectWorkLogDecisionSchema,
  projectWorkLogSchema,
} from "@erpx/projects-server/public/project-actuals";
import {nativeProjectCommand} from "@erpx/projects-server/public/native-project-commands";
import {rerouteApproval} from '@erpx/workflow-server/public/approvals';
import {changeAppointment} from '@erpx/organization-server/public/department-authority';
import {getProject,listProjects,projectSchema} from "@erpx/projects-server/public/staff-projects";
import {z} from "zod";
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {createStaffApiService,type StaffApiPorts} from "@erpx/work-projection-server/application/staff-api";
import type {StaffResource,StaffRequestContext} from "@erpx/work-projection-server/domain/staff-api";
import {workUiStaffFacts as projectsFacts} from "@erpx/projects-server/public/work-ui-staff-facts";
import {workUiStaffFacts as workflowFacts} from "@erpx/workflow-server/public/work-ui-staff-facts";
import {markStaffNotificationRead} from "@erpx/collaboration-server/public/inbox";
import {workUiStaffFacts as people_payrollFacts} from "@erpx/people-payroll-server/public/work-ui-staff-facts";
export function staffHandler(resource:StaffResource){return async(request:Request,context:StaffRequestContext)=>{let db:SqlDatabase;const service=createStaffApiService({requireWorkContext,prepare:async actor=>{db=(await holdingResources(actor.holdingId)).write;},now:Date.now,error:(status,code,message)=>new ApiError(status,code,message),requestPublicOrigin,markNotificationRead:(actor,id,key,now)=>markStaffNotificationRead(db,actor,id,key,now),listStaffNotifications:(...args:Parameters<StaffApiPorts["listStaffNotifications"]>)=>listStaffNotifications(db,...args),staffPeople:(...args:Parameters<StaffApiPorts["staffPeople"]>)=>staffPeople(db,...args),activeStaff:(...args:Parameters<StaffApiPorts["activeStaff"]>)=>activeStaff(db,...args),companyAuthority:(...args:Parameters<StaffApiPorts["companyAuthority"]>)=>companyAuthority(db,...args),assignLevel:(...args:Parameters<StaffApiPorts["assignLevel"]>)=>assignLevel(db,...args),saveLevel:(...args:Parameters<StaffApiPorts["saveLevel"]>)=>saveLevel(db,...args),staffSettings:(...args:Parameters<StaffApiPorts["staffSettings"]>)=>staffSettings(db,...args),getProject:(...args:Parameters<StaffApiPorts["getProject"]>)=>getProject(db,...args),listProjects:(...args:Parameters<StaffApiPorts["listProjects"]>)=>listProjects(db,...args),listTasks:(...args:Parameters<StaffApiPorts["listTasks"]>)=>listTasks(db,...args),taskDetail:(...args:Parameters<StaffApiPorts["taskDetail"]>)=>taskDetail(db,...args),listProjectMilestones:(...args:Parameters<StaffApiPorts["listProjectMilestones"]>)=>nativeProjectMilestones(db,...args),listProjectWorkLogs:(...args:Parameters<StaffApiPorts["listProjectWorkLogs"]>)=>listProjectWorkLogs(db,...args),projectCommand:({request,holdingSlug,operation,projectId,recordId,input,key,branchSlug,departmentSlug})=>nativeProjectCommand(request,holdingSlug,{operation,projectId,recordId,key,body:input,branchSlug,departmentSlug}),rerouteApproval:(...args:Parameters<StaffApiPorts["rerouteApproval"]>)=>rerouteApproval(db,...args),changeAppointment:(...args:Parameters<StaffApiPorts["changeAppointment"]>)=>changeAppointment(db,...args),headSchema:input=>headSchema.parse(input),levelAssignmentSchema:input=>levelAssignmentSchema.parse(input),levelSchema:input=>levelSchema.parse(input),projectSchema:input=>projectSchema.parse(input),projectMilestoneSchema:input=>projectMilestoneSchema.parse(input),projectMilestoneTransitionSchema:input=>projectMilestoneTransitionSchema.parse(input),projectWorkLogDecisionSchema:input=>projectWorkLogDecisionSchema.parse(input),projectWorkLogSchema:input=>projectWorkLogSchema.parse(input),facts:{projectMembers:(...args:Parameters<StaffApiPorts["facts"]["projectMembers"]>)=>projectsFacts(db).projectMembers(...args),approvalQueue:(...args:Parameters<StaffApiPorts["facts"]["approvalQueue"]>)=>workflowFacts(db).approvalQueue(...args),headPlacement:(...args:Parameters<StaffApiPorts["facts"]["headPlacement"]>)=>people_payrollFacts(db).headPlacement(...args),betaApproval:(...args:Parameters<StaffApiPorts["facts"]["betaApproval"]>)=>workflowFacts(db).betaApproval(...args)}});try{return Response.json(canonicalDestinationLinks({data:await service(resource,request,context)}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){if(error instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",error.issues[0]?.message??"Invalid input."));if(error instanceof SyntaxError)return errorResponse(new ApiError(422,"INVALID_JSON","Use valid JSON."));return errorResponse(error);}};}
