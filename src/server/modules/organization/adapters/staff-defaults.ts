import { platformOwnerProjectionSources as people_payrollPlatformSources } from "../../people_payroll/public/platform-owner-projection-sources";
import "server-only";
import {hasPermission} from "@/lib/permissions";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {requirePermission} from "../../identity_access/public/iam-permissions";
import {executeCommand,replayCommand} from "../../workflow/public/commands";
import {requireCompanyAuthority} from "../../operations/public/staff-policy";
import {stageOrganizationLevelLeavePolicy,stageOrganizationPayRecommendation,stageOrganizationLeaveRecommendation} from "../../people_payroll/public/organization-defaults";
import {ApiError} from "../../runtime/public/http";
import {BusinessRuleError} from "../../shared";
import {createStaffDefaultApplication} from "../application/staff-defaults";
import type {PositionPayDefault,LevelLeaveDefault,DefaultRecommendation,PayDefaultFact,LeaveDefaultFact,ExistingLeaveFact} from "../domain/staff-defaults";
export {positionPayInput,levelLeaveInput} from "../domain/staff-defaults";
export const canSeeDefaultPay=(actor:WorkContext)=>hasPermission(actor,'people.compensation.read');
async function catalog(db:SqlDatabase,actor:WorkContext){
  const leave = (await db.prepare(`SELECT d.level_id,d.version,d.policy_version_id,p.name,p.effective_from,
      max(CASE WHEN t.code='annual' THEN r.grant_units END) annual_days,
      max(CASE WHEN t.code='sick' THEN r.grant_units END) sick_days
    FROM level_leave_defaults d JOIN (${people_payrollPlatformSources.leave_policy_versions}) p ON p.id=d.policy_version_id
    JOIN (${people_payrollPlatformSources.leave_policy_rules}) r ON r.policy_version_id=p.id JOIN (${people_payrollPlatformSources.leave_types}) t ON t.id=r.leave_type_id
    WHERE d.company_id=? AND d.version=(SELECT max(x.version) FROM level_leave_defaults x WHERE x.company_id=d.company_id AND x.level_id=d.level_id)
    GROUP BY d.level_id,d.version,d.policy_version_id,p.name,p.effective_from`).bind(actor.companyId).all<{
      level_id:string;version:number;policy_version_id:string;name:string;effective_from:string;annual_days:number;sick_days:number
    }>()).results;
  const pay = canSeeDefaultPay(actor) ? (await db.prepare(`SELECT d.position_id,d.version,d.pay_group_id,d.currency,d.monthly_amount_minor,g.name pay_group_name
    FROM position_pay_defaults d JOIN (${people_payrollPlatformSources.hr_pay_groups}) g ON g.id=d.pay_group_id JOIN organization_positions p ON p.id=d.position_id
    WHERE d.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
      AND d.version=(SELECT max(x.version) FROM position_pay_defaults x WHERE x.company_id=d.company_id AND x.position_id=d.position_id)`)
    .bind(actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<{position_id:string;version:number;pay_group_id:string;currency:string;monthly_amount_minor:number;pay_group_name:string}>()).results : [];
  return { pay, leave };
}
function application(db:SqlDatabase,actor:WorkContext){return createStaffDefaultApplication({clock:{now:()=>Date.now()},canSeePay:canSeeDefaultPay(actor),async authorizePeople(){actor=requirePermission(actor,'people.manage');},async authorizeLeavePolicy(){actor=requirePermission(actor,'leave.policy.manage');},authorizeCompanyLeavePolicy:async()=>{await requireCompanyAuthority(db,actor,'leave.policy.manage');},catalog:()=>catalog(db,actor),replay:(key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),
 position:async(id)=>!!await db.prepare("SELECT id FROM organization_positions WHERE id=? AND company_id=? AND active=1 AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)").bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first(),
 payGroup:async(id,currency)=>!!await db.prepare(`SELECT id FROM (${people_payrollPlatformSources.hr_pay_groups}) hr_pay_groups WHERE id=? AND company_id=? AND status='active' AND frequency='monthly' AND currency=?`).bind(id,actor.companyId,currency).first(),
 payVersion:async(id)=>Number((await db.prepare('SELECT coalesce(max(version),0) version FROM position_pay_defaults WHERE company_id=? AND position_id=?').bind(actor.companyId,id).first<{version:number}>())?.version??0),
 level:(id)=>db.prepare('SELECT level_number FROM staff_levels WHERE id=? AND company_id=? AND active=1').bind(id,actor.companyId).first<{level_number:number}>(),
 leaveTypes:async()=>(await db.prepare(`SELECT id,code FROM (${people_payrollPlatformSources.leave_types}) leave_types WHERE company_id=? AND code IN ('annual','sick') AND active=true AND tracked=true`).bind(actor.companyId).all<{id:string;code:string}>()).results,
 leaveVersion:async(id)=>Number((await db.prepare('SELECT coalesce(max(version),0) version FROM level_leave_defaults WHERE company_id=? AND level_id=?').bind(actor.companyId,id).first<{version:number}>())?.version??0),
 savePay:(m)=>executeCommand(db,actor,m.key,'position.pay-default.save',m.input,m.id,m.input.expectedVersion+1,{id:m.id,version:m.input.expectedVersion+1},[db.prepare('INSERT INTO position_pay_defaults(id,company_id,position_id,version,pay_group_id,currency,monthly_amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)').bind(m.id,actor.companyId,m.positionId,m.input.expectedVersion+1,m.input.payGroupId,m.input.currency,m.input.monthlyAmount,actor.userId,m.now)]),
 saveLeave:(m)=>{const version=m.input.expectedVersion+1,policyId=`policy-${m.key}`;return executeCommand(db,actor,m.key,'level.leave-default.save',m.input,m.id,version,{id:m.id,version,policyVersionId:policyId},[...stageOrganizationLevelLeavePolicy(db,actor,{policyId,version,levelNumber:m.levelNumber,today:m.today,types:m.types,annualDays:m.input.annualDays,sickDays:m.input.sickDays,now:m.now}),db.prepare('INSERT INTO level_leave_defaults(id,company_id,level_id,version,policy_version_id,created_by,created_at) VALUES(?,?,?,?,?,?,?)').bind(m.id,actor.companyId,m.levelId,version,policyId,actor.userId,m.now)]);},
 priorCompensation:async(id)=>!!await db.prepare(`SELECT 1 FROM (${people_payrollPlatformSources.hr_compensation_revisions}) hr_compensation_revisions WHERE engagement_id=? LIMIT 1`).bind(id).first(),
 payDefault:(id)=>db.prepare(`SELECT d.* FROM position_pay_defaults d JOIN organization_positions p ON p.id=d.position_id WHERE d.company_id=? AND d.position_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY d.version DESC LIMIT 1`).bind(actor.companyId,id,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<PayDefaultFact>(),
 leaveDefault:(id)=>db.prepare(`SELECT d.policy_version_id,p.effective_from FROM level_leave_defaults d JOIN (${people_payrollPlatformSources.leave_policy_versions}) p ON p.id=d.policy_version_id WHERE d.company_id=? AND d.level_id=? ORDER BY d.version DESC LIMIT 1`).bind(actor.companyId,id).first<LeaveDefaultFact>(),
 existingLeave:(id,startDate)=>db.prepare(`SELECT a.id,a.effective_from,a.effective_to,p.policy_key FROM (${people_payrollPlatformSources.leave_policy_assignments}) a JOIN (${people_payrollPlatformSources.leave_policy_versions}) p ON p.id=a.policy_version_id WHERE a.company_id=? AND a.engagement_id=? AND (a.effective_to IS NULL OR a.effective_to>=?) ORDER BY a.effective_from DESC LIMIT 1`).bind(actor.companyId,id,startDate).first<ExistingLeaveFact>(),
 stagePay:async(options,pay,now)=>stageOrganizationPayRecommendation(db,actor,{engagementId:options.engagementId,startDate:options.startDate,commandId:options.commandId,payGroupId:pay.pay_group_id,amount:pay.monthly_amount_minor,currency:pay.currency,now}),
 stageLeave:async(options,recommendation,now)=>stageOrganizationLeaveRecommendation(db,actor,{engagementId:options.engagementId,commandId:options.commandId,...recommendation,now}),
 });}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export const defaultCatalog=(db:SqlDatabase,a:WorkContext)=>invoke(()=>application(db,a).catalog());
export const savePositionPayDefault=(db:SqlDatabase,a:WorkContext,id:string,input:PositionPayDefault,key:string)=>invoke(()=>application(db,a).savePay(id,input,key));
export const saveLevelLeaveDefault=(db:SqlDatabase,a:WorkContext,id:string,input:LevelLeaveDefault,key:string)=>invoke(()=>application(db,a).saveLeave(id,input,key));
export const recommendationStatements=(db:SqlDatabase,a:WorkContext,options:DefaultRecommendation)=>invoke(()=>application(db,a).recommendation(options));
