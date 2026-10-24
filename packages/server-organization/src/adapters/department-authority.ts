import {nativeDepartmentReadPorts}from '@erpx/server-organization/adapters/department-authority-reads';
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/server-identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';

import { stageOrganizationSelfCeo } from '@erpx/server-people_payroll/public/organization-authority-people';
import {BusinessRuleError} from '@erpx/server-shared/index';
import { createDepartmentAuthorityApplication } from '@erpx/server-organization/application/department-authority';

export {appointmentInput,departmentFlowInput} from '@erpx/server-organization/domain/department-authority';
export type {FlowSelector} from '@erpx/server-organization/domain/department-authority';
function powerGuard(db:SqlDatabase,key:string,actor:WorkContext,power:string){
  if(power==="owner")return assertion(db,key,"owner-appointment-power",`EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.assignments}) assignments WHERE id=? AND user_id=? AND company_id=? AND role='holding_owner' AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`,[actor.assignmentId,actor.userId,actor.companyId]);
  return assertion(db,key,"appointment-power",`EXISTS(SELECT 1 FROM authority_seats s JOIN (${people_payrollPlatformSources.hr_people}) p ON p.id=s.person_id AND p.company_id=s.company_id JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' WHERE s.company_id=? AND s.scope_id=? AND s.kind=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND p.status='active' AND p.merged_into_id IS NULL)`,[actor.companyId,actor.companyId,power,actor.userId]);
}

function application(db:SqlDatabase,actor:WorkContext){return createDepartmentAuthorityApplication({...nativeDepartmentReadPorts(db,actor),replay:(key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),change:async(input,key,power,scopeId,selfCeo,effectiveAt,now)=>{const operation='authority.appointment.change',recordId=`${input.kind}:${scopeId}`,version=input.expectedVersion+1;
  return executeCommand(db,actor,key,operation,input,recordId,version,{kind:input.kind,scopeId,version,personId:input.personId,positionId:input.positionId},[
    powerGuard(db,key,actor,power),
    assertion(db,key,"appointment-version",`(?=0 AND NOT EXISTS(SELECT 1 FROM authority_seats WHERE company_id=? AND kind=? AND scope_id=?)) OR EXISTS(SELECT 1 FROM authority_seats WHERE company_id=? AND kind=? AND scope_id=? AND version=?)`,[input.expectedVersion,actor.companyId,input.kind,scopeId,actor.companyId,input.kind,scopeId,input.expectedVersion]),
    db.prepare(`INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,?,?,?,?,?,?)
      ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=excluded.version,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=excluded.effective_at`)
      .bind(actor.companyId,input.kind,scopeId,version,input.positionId,input.personId,effectiveAt),
    db.prepare(`INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,command_id,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,'explicit',?,?)`).bind(`appointment-${key}`,actor.companyId,input.kind,scopeId,version,input.positionId,input.personId,effectiveAt,actor.userId,input.reason,key,now),
    ...(selfCeo?[stageOrganizationSelfCeo(db,actor.companyId,input.personId,input.positionId,now)]:[]),
    ...(input.kind==="head" && input.personId ? [db.prepare(`INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at)
      VALUES(?,?,?,?,?,?) ON CONFLICT(department_id) DO UPDATE SET person_id=excluded.person_id,version=department_heads.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at`)
      .bind(scopeId,actor.companyId,input.personId,version,actor.userId,now)]:[]),
  ],{companyId:actor.companyId,branchId:null,departmentId:input.kind==="head"?scopeId:null});
 },publish:async(input,key,authority,now)=>{const operation='department.flow.publish',recordId=input.departmentId,version=input.expectedVersion+1;
  return executeCommand(db,actor,key,operation,input,recordId,version,{departmentId:recordId,version,steps:input.steps},[
    assertion(db,key,"flow-publisher",`EXISTS(SELECT 1 FROM authority_seats s JOIN (${people_payrollPlatformSources.hr_people}) p ON p.id=s.person_id AND p.company_id=s.company_id JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' WHERE s.company_id=? AND s.kind=? AND s.scope_id=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`,[actor.companyId,authority,authority==="head"?input.departmentId:actor.companyId,actor.userId]),
    assertion(db,key,"flow-version","(SELECT COALESCE(MAX(version),0) FROM department_flow_versions WHERE company_id=? AND department_id=?)=?",[actor.companyId,input.departmentId,input.expectedVersion]),
    db.prepare("INSERT INTO department_flow_versions(id,company_id,department_id,version,steps_json,published_by,reason,command_id,created_at) VALUES(?,?,?,?,?::jsonb,?,?,?,?)")
      .bind(`department-flow-${key}`,actor.companyId,input.departmentId,version,JSON.stringify(input.steps),actor.userId,input.reason,key,now),
  ],{companyId:actor.companyId,branchId:null,departmentId:input.departmentId});
 }});}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}


export const changeAppointment=(db:SqlDatabase,actor:WorkContext,raw:unknown,key:string)=>invoke(()=>application(db,actor).change(raw,key));
export const publishDepartmentFlow=(db:SqlDatabase,actor:WorkContext,raw:unknown,key:string)=>invoke(()=>application(db,actor).publish(raw,key));
