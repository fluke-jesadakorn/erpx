import {nativeDepartmentReadPorts}from '@erpx/organization-server/adapters/department-authority-reads';
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/identity-access-server/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/people-payroll-server/public/platform-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/workflow-server/public/commands';

import { stageOrganizationSelfCeo } from '@erpx/people-payroll-server/public/organization-authority-people';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import { createDepartmentAuthorityApplication } from '@erpx/organization-server/application/department-authority';
import { nativeOrgStatement } from './native-organization-owner-write';
import { authoritySeatHolderGuard } from './organization-read-tail-sources-r48';
import { authoritySeatExistsSql } from './organization-read-tail-sources-r57';

export {appointmentInput,departmentFlowInput} from '@erpx/organization-server/domain/department-authority';
export type {FlowSelector} from '@erpx/organization-server/domain/department-authority';
function powerGuard(db:SqlDatabase,key:string,actor:WorkContext,power:string){
  if(power==="owner")return assertion(db,key,"owner-appointment-power",`EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.assignments}) assignments WHERE id=? AND user_id=? AND company_id=? AND role='holding_owner' AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`,[actor.assignmentId,actor.userId,actor.companyId]);
  return assertion(db,key,"appointment-power",authoritySeatHolderGuard(people_payrollPlatformSources.hr_people,people_payrollPlatformSources.hr_engagements,`scope_id=? AND s.kind=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND p.status='active' AND p.merged_into_id IS NULL`),[actor.companyId,actor.companyId,power,actor.userId]);
}

function application(db:SqlDatabase,actor:WorkContext){return createDepartmentAuthorityApplication({...nativeDepartmentReadPorts(db,actor),replay:(key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),change:async(input,key,power,scopeId,selfCeo,effectiveAt,now)=>{const operation='authority.appointment.change',recordId=`${input.kind}:${scopeId}`,version=input.expectedVersion+1;
  return executeCommand(db,actor,key,operation,input,recordId,version,{kind:input.kind,scopeId,version,personId:input.personId,positionId:input.positionId},[
    powerGuard(db,key,actor,power),
    assertion(db,key,"appointment-version",`(?=0 AND NOT ${authoritySeatExistsSql('WHERE company_id=? AND kind=? AND scope_id=?')}) OR ${authoritySeatExistsSql('WHERE company_id=? AND kind=? AND scope_id=? AND version=?')}`,[input.expectedVersion,actor.companyId,input.kind,scopeId,actor.companyId,input.kind,scopeId,input.expectedVersion]),
    await nativeOrgStatement(db,actor.companyId,'authority_seat_upsert',[actor.companyId,input.kind,scopeId,version,input.positionId,input.personId,effectiveAt],key,now),
    await nativeOrgStatement(db,actor.companyId,'authority_appointment_insert',[`appointment-${key}`,actor.companyId,input.kind,scopeId,version,input.positionId,input.personId,effectiveAt,actor.userId,input.reason,key,now],key,now),
    ...(selfCeo?[stageOrganizationSelfCeo(db,actor.companyId,input.personId,input.positionId,now)]:[]),
    ...(input.kind==="head" && input.personId ? [await nativeOrgStatement(db,actor.companyId,'department_head_upsert',[scopeId,actor.companyId,input.personId,version,actor.userId,now],key,now)]:[]),
  ],{companyId:actor.companyId,branchId:null,departmentId:input.kind==="head"?scopeId:null});
 },publish:async(input,key,authority,now)=>{const operation='department.flow.publish',recordId=input.departmentId,version=input.expectedVersion+1;
  return executeCommand(db,actor,key,operation,input,recordId,version,{departmentId:recordId,version,steps:input.steps},[
    assertion(db,key,"flow-publisher",authoritySeatHolderGuard(people_payrollPlatformSources.hr_people,people_payrollPlatformSources.hr_engagements,`kind=? AND s.scope_id=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint`),[actor.companyId,authority,authority==="head"?input.departmentId:actor.companyId,actor.userId]),
    assertion(db,key,"flow-version","(SELECT COALESCE(MAX(version),0) FROM department_flow_versions WHERE company_id=? AND department_id=?)=?",[actor.companyId,input.departmentId,input.expectedVersion]),
    await nativeOrgStatement(db,actor.companyId,'department_flow_version_insert',[`department-flow-${key}`,actor.companyId,input.departmentId,version,JSON.stringify(input.steps),actor.userId,input.reason,key,now],key,now),
  ],{companyId:actor.companyId,branchId:null,departmentId:input.departmentId});
 }});}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}


export const changeAppointment=(db:SqlDatabase,actor:WorkContext,raw:unknown,key:string)=>invoke(()=>application(db,actor).change(raw,key));
export const publishDepartmentFlow=(db:SqlDatabase,actor:WorkContext,raw:unknown,key:string)=>invoke(()=>application(db,actor).publish(raw,key));
