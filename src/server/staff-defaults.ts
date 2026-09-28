import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "./authorization";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { executeCommand, replayCommand } from "./commands";
import { requireCompanyAuthority } from "./staff-policy";

export const positionPayInput = z.object({
  payGroupId: z.string().min(1), currency: z.string().length(3),
  monthlyAmount: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER), expectedVersion: z.number().int().min(0),
});
export const levelLeaveInput = z.object({
  annualDays: z.number().min(0).max(365), sickDays: z.number().min(0).max(365),
  expectedVersion: z.number().int().min(0),
});

export function canSeeDefaultPay(actor: WorkContext) {
  return hasPermission(actor, "people.compensation.read");
}

export async function defaultCatalog(db: SqlDatabase, actor: WorkContext) {
  const leave = (await db.prepare(`SELECT d.level_id,d.version,d.policy_version_id,p.name,p.effective_from,
      max(CASE WHEN t.code='annual' THEN r.grant_units END) annual_days,
      max(CASE WHEN t.code='sick' THEN r.grant_units END) sick_days
    FROM level_leave_defaults d JOIN leave_policy_versions p ON p.id=d.policy_version_id
    JOIN leave_policy_rules r ON r.policy_version_id=p.id JOIN leave_types t ON t.id=r.leave_type_id
    WHERE d.company_id=? AND d.version=(SELECT max(x.version) FROM level_leave_defaults x WHERE x.company_id=d.company_id AND x.level_id=d.level_id)
    GROUP BY d.level_id,d.version,d.policy_version_id,p.name,p.effective_from`).bind(actor.companyId).all<{
      level_id:string;version:number;policy_version_id:string;name:string;effective_from:string;annual_days:number;sick_days:number
    }>()).results;
  const pay = canSeeDefaultPay(actor) ? (await db.prepare(`SELECT d.position_id,d.version,d.pay_group_id,d.currency,d.monthly_amount_minor,g.name pay_group_name
    FROM position_pay_defaults d JOIN hr_pay_groups g ON g.id=d.pay_group_id JOIN organization_positions p ON p.id=d.position_id
    WHERE d.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
      AND d.version=(SELECT max(x.version) FROM position_pay_defaults x WHERE x.company_id=d.company_id AND x.position_id=d.position_id)`)
    .bind(actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<{position_id:string;version:number;pay_group_id:string;currency:string;monthly_amount_minor:number;pay_group_name:string}>()).results : [];
  return { pay, leave };
}

export async function savePositionPayDefault(db: SqlDatabase, actor: WorkContext, positionId: string, input: z.infer<typeof positionPayInput>, key: string) {
  actor = requirePermission(actor, "people.manage");
  if (!canSeeDefaultPay(actor)) throw new ApiError(403,"PAY_ACCESS_REQUIRED","Compensation access is required.");
  const id=`position-pay-${key}`, old=await replayCommand(db,actor,key,"position.pay-default.save",id,input);
  if(old)return old;
  const position=await db.prepare("SELECT id FROM organization_positions WHERE id=? AND company_id=? AND active=1 AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)").bind(positionId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first();
  const group=await db.prepare("SELECT id FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active' AND frequency='monthly' AND currency=?").bind(input.payGroupId,actor.companyId,input.currency).first();
  if(!position||!group)throw new ApiError(422,"DEFAULT_PAY_INVALID","Choose an active position and matching monthly pay group.");
  const current=await db.prepare("SELECT coalesce(max(version),0) version FROM position_pay_defaults WHERE company_id=? AND position_id=?").bind(actor.companyId,positionId).first<{version:number}>();
  if(Number(current?.version??0)!==input.expectedVersion)throw new ApiError(409,"DEFAULT_CHANGED","Refresh the position default and try again.");
  return executeCommand(db,actor,key,"position.pay-default.save",input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1},[
    db.prepare("INSERT INTO position_pay_defaults(id,company_id,position_id,version,pay_group_id,currency,monthly_amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
      .bind(id,actor.companyId,positionId,input.expectedVersion+1,input.payGroupId,input.currency,input.monthlyAmount,actor.userId,Date.now()),
  ]);
}

export async function saveLevelLeaveDefault(db: SqlDatabase, actor: WorkContext, levelId: string, input: z.infer<typeof levelLeaveInput>, key: string) {
  actor=requirePermission(actor,"leave.policy.manage");
  await requireCompanyAuthority(db,actor,"leave.policy.manage");
  const id=`level-leave-${key}`,old=await replayCommand(db,actor,key,"level.leave-default.save",id,input);
  if(old)return old;
  const level=await db.prepare("SELECT level_number FROM staff_levels WHERE id=? AND company_id=? AND active=1").bind(levelId,actor.companyId).first<{level_number:number}>();
  if(!level)throw new ApiError(404,"LEVEL_NOT_FOUND","Choose an active staff level.");
  const types=(await db.prepare("SELECT id,code FROM leave_types WHERE company_id=? AND code IN ('annual','sick') AND active=true AND tracked=true").bind(actor.companyId).all<{id:string;code:string}>()).results;
  if(types.length!==2)throw new ApiError(422,"LEAVE_TYPES_REQUIRED","Tracked annual and sick leave types are required.");
  const current=await db.prepare("SELECT coalesce(max(version),0) version FROM level_leave_defaults WHERE company_id=? AND level_id=?").bind(actor.companyId,levelId).first<{version:number}>();
  if(Number(current?.version??0)!==input.expectedVersion)throw new ApiError(409,"DEFAULT_CHANGED","Refresh the level default and try again.");
  const version=input.expectedVersion+1,policyId=`policy-${key}`,today=new Date().toISOString().slice(0,10),now=Date.now();
  const policyKey=`level-${level.level_number}-default`;
  return executeCommand(db,actor,key,"level.leave-default.save",input,id,version,{id,version,policyVersionId:policyId},[
    db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,workweek_json,created_by,created_at) VALUES(?,?,?,?,?,'active',?,'UTC','[1,2,3,4,5]'::jsonb,?,?)")
      .bind(policyId,actor.companyId,policyKey,version,`Level ${level.level_number} default leave`,today,actor.userId,now),
    ...types.map(type=>db.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units,accrual_units,employee_eligible,contractor_eligible) VALUES(?,?,'annual',?,0,true,false)")
      .bind(policyId,type.id,type.code==="annual"?input.annualDays:input.sickDays)),
    db.prepare("INSERT INTO level_leave_defaults(id,company_id,level_id,version,policy_version_id,created_by,created_at) VALUES(?,?,?,?,?,?,?)")
      .bind(id,actor.companyId,levelId,version,policyId,actor.userId,now),
  ]);
}

export async function recommendationStatements(db: SqlDatabase, actor: WorkContext, options: {
  engagementId:string; positionId:string|null; levelId:string|null; priorLevelId:string|null;
  startDate:string; currency:string; acceptPay:boolean; acceptLeave:boolean; commandId:string;
}): Promise<SqlStatement[]> {
  const statements:SqlStatement[]=[], now=Date.now();
  if(options.acceptPay && options.positionId){
    if(!canSeeDefaultPay(actor))throw new ApiError(403,"PAY_ACCESS_REQUIRED","Compensation access is required.");
    const prior=await db.prepare("SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? LIMIT 1").bind(options.engagementId).first();
    const pay=await db.prepare(`SELECT d.* FROM position_pay_defaults d JOIN organization_positions p ON p.id=d.position_id
      WHERE d.company_id=? AND d.position_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY d.version DESC LIMIT 1`)
      .bind(actor.companyId,options.positionId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<{pay_group_id:string;currency:string;monthly_amount_minor:number;version:number}>();
    if(!prior && pay && pay.currency===options.currency){
      statements.push(db.prepare("UPDATE hr_engagements SET pay_group_id=? WHERE id=?").bind(pay.pay_group_id,options.engagementId));
      statements.push(db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?,?,'monthly',?,0,0,?,'[]','draft',?,?,?)")
        .bind(`comp-default-${options.commandId}`,options.engagementId,options.startDate,pay.monthly_amount_minor,pay.currency,actor.userId,now,now));
    }
  }
  if(options.acceptLeave && options.levelId){
    const policy=await db.prepare("SELECT d.policy_version_id,p.effective_from FROM level_leave_defaults d JOIN leave_policy_versions p ON p.id=d.policy_version_id WHERE d.company_id=? AND d.level_id=? ORDER BY d.version DESC LIMIT 1").bind(actor.companyId,options.levelId).first<{policy_version_id:string;effective_from:string}>();
    if(policy){
      const existing=(await db.prepare(`SELECT a.id,a.effective_from,a.effective_to,p.policy_key FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id
        WHERE a.company_id=? AND a.engagement_id=? AND (a.effective_to IS NULL OR a.effective_to>=?) ORDER BY a.effective_from DESC LIMIT 1`)
        .bind(actor.companyId,options.engagementId,options.startDate).first<{id:string;effective_from:string;effective_to:string|null;policy_key:string}>());
      const managed=!existing || existing.policy_key==="legacy-untracked" || /^level-[1-6]-default$/.test(existing.policy_key);
      if(managed && (!existing || options.priorLevelId!==options.levelId || existing.policy_key==="legacy-untracked")){
        const changed=Boolean(options.priorLevelId && options.priorLevelId!==options.levelId);
        const year=Number(options.startDate.slice(0,4));
        const effective=changed?`${year+1}-01-01`:options.startDate<policy.effective_from?policy.effective_from:options.startDate;
        if(existing && existing.effective_from>=effective)throw new ApiError(409,"LEAVE_ASSIGNMENT_OVERLAP","Review the current leave assignment before accepting this default.");
        const end=changed?`${year}-12-31`:new Date(Date.parse(`${effective}T00:00:00Z`)-86400000).toISOString().slice(0,10);
        if(existing && existing.effective_from<=end && (!existing.effective_to || existing.effective_to>end))statements.push(db.prepare("UPDATE leave_policy_assignments SET effective_to=?,version=version+1 WHERE id=?").bind(end,existing.id));
        statements.push(db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,?,?,?)")
          .bind(`leave-default-${options.commandId}`,actor.companyId,options.engagementId,policy.policy_version_id,effective,actor.userId,now));
      }
    }
  }
  return statements;
}
