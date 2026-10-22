import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/server-identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import {activeStaff,staffPeople,eligibleRoleUsers,type ApprovalRoute} from '@erpx/server-operations/public/staff-policy';
import {organizationAuthorityActorPerson,organizationAuthorityAppointee,organizationAuthoritySelectorPerson,stageOrganizationSelfCeo} from '@erpx/server-people_payroll/public/organization-authority-people';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createDepartmentAuthorityApplication,type DepartmentAuthorityPorts} from '@erpx/server-organization/application/department-authority';
import type {AuthoritySeat,DepartmentPolicy,DepartmentRoute} from '@erpx/server-organization/domain/department-authority';
export {appointmentInput,departmentFlowInput} from '@erpx/server-organization/domain/department-authority';
export type {FlowSelector} from '@erpx/server-organization/domain/department-authority';
function powerGuard(db:SqlDatabase,key:string,actor:WorkContext,power:string){
  if(power==="owner")return assertion(db,key,"owner-appointment-power",`EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.assignments}) assignments WHERE id=? AND user_id=? AND company_id=? AND role='holding_owner' AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`,[actor.assignmentId,actor.userId,actor.companyId]);
  return assertion(db,key,"appointment-power",`EXISTS(SELECT 1 FROM authority_seats s JOIN (${people_payrollPlatformSources.hr_people}) p ON p.id=s.person_id AND p.company_id=s.company_id JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' WHERE s.company_id=? AND s.scope_id=? AND s.kind=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND p.status='active' AND p.merged_into_id IS NULL)`,[actor.companyId,actor.companyId,power,actor.userId]);
}

function application(db:SqlDatabase,actor:WorkContext){const ports:DepartmentAuthorityPorts<Record<string,unknown>>={clock:{now:()=>Date.now()},actor,
 personForActor:()=>organizationAuthorityActorPerson(db,actor.companyId,actor.userId),
 currentPowerKinds:async(personId,now)=>(await db.prepare(`SELECT DISTINCT s.kind FROM authority_seats s JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=s.person_id AND e.company_id=s.company_id AND e.status='active' WHERE s.company_id=? AND s.scope_id=? AND s.person_id=? AND s.effective_at<=? AND s.kind IN ('ceo','hr')`).bind(actor.companyId,actor.companyId,personId,now).all<{kind:string}>()).results.map(x=>x.kind),
 seat:(kind,scope)=>db.prepare('SELECT * FROM authority_seats WHERE company_id=? AND kind=? AND scope_id=?').bind(actor.companyId,kind,scope).first<AuthoritySeat>(),
 overview:async()=>({seats:(await db.prepare(`SELECT s.*,p.legal_name person_name,pos.title position_title,n.display_name department_name FROM authority_seats s LEFT JOIN (${people_payrollPlatformSources.hr_people}) p ON p.id=s.person_id LEFT JOIN organization_positions pos ON pos.id=s.position_id LEFT JOIN organization_nodes n ON n.id=s.scope_id AND s.kind='head' WHERE s.company_id=? ORDER BY CASE s.kind WHEN 'ceo' THEN 0 WHEN 'hr' THEN 1 ELSE 2 END,n.display_name`).bind(actor.companyId).all()).results,history:(await db.prepare('SELECT * FROM authority_appointments WHERE company_id=? ORDER BY created_at DESC,id DESC LIMIT 200').bind(actor.companyId).all()).results,policies:(await db.prepare(`SELECT DISTINCT ON (department_id) department_id,version,steps_json,published_by,reason,created_at FROM department_flow_versions WHERE company_id=? ORDER BY department_id,version DESC`).bind(actor.companyId).all()).results,departments:(await db.prepare("SELECT id,display_name FROM organization_nodes WHERE company_id=? AND kind='department' AND status='active' ORDER BY display_name").bind(actor.companyId).all<{id:string;display_name:string}>()).results}),
 activeStaff:userId=>activeStaff(db,actor.companyId,userId),staffPeople:()=>staffPeople(db,actor.companyId),replay:(key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),
 departmentActive:async(id)=>!!await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active'").bind(id,actor.companyId).first(),
 positionValid:async(id,kind,scope)=>!!await db.prepare("SELECT id FROM organization_positions WHERE id=? AND company_id=? AND active=1 AND (?<>'ceo' OR is_ceo=1) AND (?<>'head' OR department_id=?)").bind(id,actor.companyId,kind,kind,scope).first(),
 appointeeUser:(input,scope,self)=>organizationAuthorityAppointee(db,actor.companyId,input,scope,self),selectorUser:id=>organizationAuthoritySelectorPerson(db,actor.companyId,id),
 flowVersion:async(id)=>(await db.prepare('SELECT COALESCE(MAX(version),0) version FROM department_flow_versions WHERE company_id=? AND department_id=?').bind(actor.companyId,id).first<{version:number}>())?.version??0,
 policy:id=>db.prepare('SELECT id,version,steps_json FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1').bind(actor.companyId,id).first<DepartmentPolicy>(),
 legacyHead:async(id)=>(await db.prepare('SELECT person_id FROM department_heads WHERE company_id=? AND department_id=?').bind(actor.companyId,id).first<{person_id:string}>())?.person_id??null,
 legacyHrUsers:scope=>eligibleRoleUsers(db,scope,'hr_admin'),
 change:async(input,key,power,scopeId,selfCeo,effectiveAt,now)=>{const operation='authority.appointment.change',recordId=`${input.kind}:${scopeId}`,version=input.expectedVersion+1;
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
 }};return createDepartmentAuthorityApplication(ports);}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function nativeRoute(route:DepartmentRoute):ApprovalRoute{const {checks,...facts}=route;return {...facts,...(checks?{checks:checks.map(c=>c.kind==='department-policy'?{query:'EXISTS(SELECT 1 FROM department_flow_versions WHERE id=? AND company_id=? AND version=?)',args:[c.id,c.companyId,c.version]}:{query:`EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_engagements}) hr_engagements WHERE id=? AND company_id=? AND version=? AND status='active')`,args:[c.id,c.companyId,c.version]})}:{})};}
export const appointmentOverview=(db:SqlDatabase,actor:WorkContext)=>invoke(()=>application(db,actor).overview());
export const changeAppointment=(db:SqlDatabase,actor:WorkContext,raw:unknown,key:string)=>invoke(()=>application(db,actor).change(raw,key));
export const publishDepartmentFlow=(db:SqlDatabase,actor:WorkContext,raw:unknown,key:string)=>invoke(()=>application(db,actor).publish(raw,key));
const routeActor=(companyId:string)=>({companyId} as WorkContext);
export const departmentRoute=(db:SqlDatabase,companyId:string,userId:string,excluded:string[]=[],proposed?:import("@erpx/server-organization/domain/department-authority").FlowSelector[])=>invoke(async()=>nativeRoute(await application(db,routeActor(companyId)).departmentRoute(userId,excluded,proposed)));
export const departmentStepEligible=(db:SqlDatabase,companyId:string,userId:string,snapshot:Record<string,unknown>)=>invoke(()=>application(db,routeActor(companyId)).stepEligible(userId,snapshot));
export const appointedHrEligible=(db:SqlDatabase,companyId:string,userId:string,personId:unknown)=>invoke(()=>application(db,routeActor(companyId)).hrEligible(userId,personId));
export const peopleRequestRoute=(db:SqlDatabase,companyId:string,userId:string,excluded:string[]=[])=>invoke(async()=>nativeRoute(await application(db,routeActor(companyId)).peopleRoute(userId,excluded)));
export const previewDepartmentFlow=(db:SqlDatabase,actor:WorkContext,raw:unknown)=>invoke(()=>application(db,actor).preview(raw));
