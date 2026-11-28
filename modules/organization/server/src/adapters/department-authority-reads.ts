
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/people-payroll-server/public/platform-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {ApiError} from '@erpx/platform-runtime-server/public/http';

import {activeStaff,staffPeople,eligibleRoleUsers,type ApprovalRoute} from '@erpx/operations-server/public/staff-policy';
import { organizationAuthorityActorPerson, organizationAuthorityAppointee, organizationAuthoritySelectorPerson } from '@erpx/people-payroll-server/public/organization-authority-people';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';

import type {AuthoritySeat,DepartmentPolicy,DepartmentRoute} from '@erpx/organization-server/domain/department-authority';
import {createDepartmentAuthorityReads,type DepartmentAuthorityReadPorts}from '@erpx/organization-server/application/department-authority';
import {nativeOrgAll,nativeOrgOne,nativeOrgSql}from './native-organization-owner-write';

/** Owner-local department authority reads are native: the SQL for every kind
 * lives in `erpx-server-organization::adapters::organization_owner_write` and
 * this adapter sends only a typed kind with positional values on the caller's
 * unit. The three cross-owner reads (hr_people/hr_engagements owned by
 * people_payroll) stay behind that owner's projection port and are deliberately
 * not duplicated here. */
export function nativeDepartmentReadPorts(db:SqlDatabase,actor:WorkContext):DepartmentAuthorityReadPorts<Record<string,unknown>>{return{clock:{now:()=>Date.now()},actor,personForActor:()=>organizationAuthorityActorPerson(db,actor.companyId,actor.userId),currentPowerKinds:async(personId,now)=>(await db.prepare(`SELECT DISTINCT s.kind FROM authority_seats s JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=s.person_id AND e.company_id=s.company_id AND e.status='active' WHERE s.company_id=? AND s.scope_id=? AND s.person_id=? AND s.effective_at<=? AND s.kind IN ('ceo','hr')`).bind(actor.companyId,actor.companyId,personId,now).all<{kind:string}>()).results.map(x=>x.kind),seat:(kind,scope)=>nativeOrgOne<AuthoritySeat>(db,actor.companyId,'dept_auth_read_seat',[actor.companyId,kind,scope]),overview:async()=>({seats:(await db.prepare(`SELECT s.*,p.legal_name person_name,pos.title position_title,n.display_name department_name FROM authority_seats s LEFT JOIN (${people_payrollPlatformSources.hr_people}) p ON p.id=s.person_id LEFT JOIN organization_positions pos ON pos.id=s.position_id LEFT JOIN organization_nodes n ON n.id=s.scope_id AND s.kind='head' WHERE s.company_id=? ORDER BY CASE s.kind WHEN 'ceo' THEN 0 WHEN 'hr' THEN 1 ELSE 2 END,n.display_name`).bind(actor.companyId).all()).results,history:await nativeOrgAll(db,actor.companyId,'dept_auth_read_history',[actor.companyId]),policies:await nativeOrgAll(db,actor.companyId,'dept_auth_read_policies',[actor.companyId]),departments:await nativeOrgAll<{id:string;display_name:string}>(db,actor.companyId,'dept_auth_read_departments',[actor.companyId])}),activeStaff:userId=>activeStaff(db,actor.companyId,userId),staffPeople:()=>staffPeople(db,actor.companyId),departmentActive:async(id)=>(await nativeOrgAll(db,actor.companyId,'dept_auth_read_department_active',[id,actor.companyId])).length>0,positionValid:async(id,kind,scope)=>(await nativeOrgAll(db,actor.companyId,'dept_auth_read_position_valid',[id,actor.companyId,kind,kind,scope])).length>0,appointeeUser:(input,scope,self)=>organizationAuthorityAppointee(db,actor.companyId,input,scope,self),selectorUser:id=>organizationAuthoritySelectorPerson(db,actor.companyId,id),flowVersion:async(id)=>(await nativeOrgOne<{version:number}>(db,actor.companyId,'dept_auth_read_flow_version',[actor.companyId,id]))?.version??0,policy:id=>nativeOrgOne<DepartmentPolicy>(db,actor.companyId,'dept_auth_read_policy',[actor.companyId,id]),legacyHead:async(id)=>(await nativeOrgOne<{person_id:string}>(db,actor.companyId,'dept_auth_read_legacy_head',[actor.companyId,id]))?.person_id??null,legacyHrUsers:scope=>eligibleRoleUsers(db,scope,'hr_admin')};}
function application(db:SqlDatabase,actor:WorkContext){return createDepartmentAuthorityReads(nativeDepartmentReadPorts(db,actor));}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
async function nativeRoute(db:SqlDatabase,companyId:string,route:DepartmentRoute):Promise<ApprovalRoute>{const {checks,...facts}=route;
const mapped=checks?await Promise.all(checks.map(async c=>c.kind==='department-policy'?await nativeOrgSql(companyId,'dept_auth_read_policy_check',[c.id,c.companyId,c.version]):{query:`EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_engagements}) hr_engagements WHERE id=? AND company_id=? AND version=? AND status='active')`,args:[c.id,c.companyId,c.version]})):undefined;
return {...facts,...(mapped?{checks:mapped}:{})};}
export const appointmentOverview=(db:SqlDatabase,actor:WorkContext)=>invoke(()=>application(db,actor).overview());
const routeActor=(companyId:string)=>({companyId} as WorkContext);

export const departmentRoute=(db:SqlDatabase,companyId:string,userId:string,excluded:string[]=[],proposed?:import("@erpx/organization-server/domain/department-authority").FlowSelector[])=>invoke(async()=>nativeRoute(db,companyId,await application(db,routeActor(companyId)).departmentRoute(userId,excluded,proposed)));

export const departmentStepEligible=(db:SqlDatabase,companyId:string,userId:string,snapshot:Record<string,unknown>)=>invoke(()=>application(db,routeActor(companyId)).stepEligible(userId,snapshot));

export const appointedHrEligible=(db:SqlDatabase,companyId:string,userId:string,personId:unknown)=>invoke(()=>application(db,routeActor(companyId)).hrEligible(userId,personId));

export const peopleRequestRoute=(db:SqlDatabase,companyId:string,userId:string,excluded:string[]=[])=>invoke(async()=>nativeRoute(db,companyId,await application(db,routeActor(companyId)).peopleRoute(userId,excluded)));

export const previewDepartmentFlow=(db:SqlDatabase,actor:WorkContext,raw:unknown)=>invoke(()=>application(db,actor).preview(raw));
