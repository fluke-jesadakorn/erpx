import { readListPage } from "./list-page";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { organizationGuard, validateStaffOrganization } from "./organization";
import { requirePermission } from "./iam-permissions";
import "server-only";

import { z } from "zod";
import type { WorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { ApiError } from "@/server/http";
import { sha256 } from "@/server/crypto";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import { calculatePayrollAmounts } from "@/lib/payroll-math";
import { assertion,executeCommand,replayCommand } from "./commands";
import { directoryPerson, canReadCompensation,canManagePeople } from "./people-directory";
import { prepareJournal } from "./finance";

const country = z.string().length(2).transform((value) => value.toUpperCase());
const currency = z.string().length(3).transform((value) => value.toUpperCase());
const date = z.string().date();

export const createPersonSchema = z.object({
  legalName: z.string().trim().min(1).max(200),
  preferredName: z.string().trim().max(160).optional(),
  email: z.string().email().optional(),
  phone: z.string().trim().max(60).optional(),
  countryCode: country,
  userId: z.never().optional(),
});

export const createEngagementSchema = z.object({
  personId: z.string().min(1),
  branchId: z.string().nullable().optional(),
  departmentId: z.string().nullable().optional(),
  workerType: z.enum(["employee", "contractor"]),
  employmentType: z.enum(["salaried", "hourly", "shift", "contractor"]),
  startDate: date,
  endDate: date.nullable().optional(),
  managerPersonId: z.string().nullable().optional(),
  jobTitle: z.string().trim().min(1).max(160),
  currency,
  payGroupId: z.string().nullable().optional(),
});

export const compensationSchema = z.object({
  engagementId: z.string().min(1),
  effectiveFrom: date,
  effectiveTo: date.nullable().optional(),
  payFrequency: z.enum(["weekly", "fortnightly", "semimonthly", "monthly", "off_cycle"]),
  baseAmount: z.string().regex(/^\d+(\.\d{1,6})?$/).default("0"),
  hourlyRate: z.string().regex(/^\d+(\.\d{1,6})?$/).default("0"),
  expectedHours: z.number().min(0).max(744).default(0),
  currency,
  components: z.array(z.object({
    code: z.string().min(1).max(80),
    label: z.string().min(1).max(160),
    kind: z.enum(["earning", "deduction", "employer"]),
    amount: z.string().regex(/^\d+(\.\d{1,6})?$/),
    taxable: z.boolean().default(false),
  })).max(50).default([]),
});
export const compensationTransitionSchema = z.object({
  status: z.enum(["approved", "superseded"]),
  expectedVersion: z.number().int().positive(),
});
export const engagementTransitionSchema = z.object({
  status: z.enum(["active", "suspended", "ended"]),
  expectedVersion: z.number().int().positive(),
  endDate: date.nullable().optional(),
});

export const payGroupSchema = z.object({
  name: z.string().trim().min(1).max(120),
  countryCode: country,
  currency,
  frequency: z.enum(["weekly", "fortnightly", "semimonthly", "monthly"]),
  timezone: z.string().min(1).max(80).default("UTC"),
  overtimeMultiplierBps:z.number().int().min(10000).max(50000).default(15000),
  prorationMethod:z.enum(["calendar_days","none"]).default("calendar_days"),
});

export const countryPackSchema = z.object({
  countryCode: country,
  version: z.string().trim().min(1).max(40),
  rules: z.record(z.string(), z.unknown()),
  exportFormats: z.array(z.string().max(80)).max(20).default([]),
});
export const countryPackTransitionSchema = z.object({
  status: z.enum(["validated", "active", "superseded"]),
  proof: z.string().trim().min(1).max(2000),
});

export const payrollRunSchema = z.object({
  payGroupId: z.string().min(1),
  countryPackId: z.string().nullable().optional(),
  periodStart: date,
  periodEnd: date,
  payDate: date,
  currency,
  mode: z.enum(["preview", "final_pay", "production"]).default("preview"),
});

export const paymentBatchSchema = z.object({
  sourceType: z.enum(["payroll", "expense", "contractor"]),
  sourceId: z.string().min(1),
  formatCode: z.string().trim().min(1).max(80).default("generic-csv"),
});

export const timeEntrySchema = z.object({
  engagementId: z.string().min(1),
  workDate: date,
  regularHours: z.number().min(0).max(24),
  overtimeHours: z.number().min(0).max(24).default(0),
  note: z.string().max(500).default(""),
});

export const paymentConfirmationSchema = z.object({
  rows: z.array(z.object({
    reference: z.string().min(1),
    status: z.enum(["confirmed", "failed"]),
    confirmationRef: z.string().max(160).optional(),
  })).min(1),
});

export const payrollPostSchema = z.object({
  chartId: z.string().min(1),
  periodId: z.string().min(1),
  entryDate: date,
  expenseAccountId: z.string().min(1),
  netPayableAccountId: z.string().min(1),
  taxPayableAccountId: z.string().min(1).optional(),
  employerContributionAccountId: z.string().min(1).optional(),
  expectedVersion: z.number().int().positive(),
});

export const payrollCorrectionRequestSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export const payrollCorrectionDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  action: z.enum(["approve", "reject", "cancel"]),
  reason: z.string().trim().min(3).max(2000),
});
export const finalPayPreparationSchema = z.object({
  taskId: z.string().min(1),
  expectedTaskVersion: z.number().int().positive(),
  payGroupId: z.string().min(1),
  periodStart: date,
  periodEnd: date,
  payDate: date,
  currency,
  reason: z.string().trim().min(3).max(2000),
});
export const payrollAdvanceRequestSchema = z.object({
  engagementId: z.string().min(1),
  amount: z.string().regex(/^\d+(\.\d{1,2})?$/),
  installmentCount: z.number().int().min(1).max(24),
  firstDeductionPeriod: date,
  currency,
  reason: z.string().trim().min(3).max(2000),
});
export const payrollAdvanceDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  action: z.enum(["approve", "reject", "cancel"]),
  reason: z.string().trim().min(3).max(2000),
});


function scopeWhere(actor: WorkContext, alias = "p") {
  if (actor.departmentId) return { sql: `${alias}.company_id=? AND ${alias}.department_id=?`, args: [actor.companyId, actor.departmentId] };
  if (actor.branchId) return { sql: `${alias}.company_id=? AND ${alias}.branch_id=?`, args: [actor.companyId, actor.branchId] };
  return { sql: `${alias}.company_id=?`, args: [actor.companyId] };
}

async function command(write: SqlDatabase, actor:WorkContext, id: string, operation: string, input: unknown) {
  const digest = await sha256(JSON.stringify({input,userId:actor.userId,assignmentId:actor.assignmentId,role:actor.role,authority:actor.authority}));
  const current=await write.prepare("SELECT record_id FROM command_receipts WHERE command_id=?").bind(id).first<{record_id:string}>();
  if(current)return {digest,result:await replayCommand(write,actor,id,operation,current.record_id,input)};
  const old=await write.prepare("SELECT 1 FROM hr_command_receipts WHERE command_id=?").bind(id).first();
  if(old)throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This legacy command has no work context. Review its result before retrying.");
  return {digest,result:null};
}

function receipt(write: SqlDatabase, id: string, operation: string, digest: string, result: unknown) {
  return write.prepare("INSERT INTO hr_command_receipts(command_id,operation,request_hash,result_json,created_at) VALUES(?,?,?,?,?)")
    .bind(id, operation, digest, JSON.stringify(result), Date.now());
}

export async function listPeople(actor: WorkContext,filters:{levelId?:string;departmentId?:string}={}) {
  const { write } = await holdingResources(actor.holdingId);
  const rows = await write.prepare(`SELECT p.id,p.user_id,p.legal_name,p.preferred_name,p.email,p.country_code,p.status,p.version,
      e.id AS engagement_id,e.job_title,e.worker_type,e.employment_type,e.status AS engagement_status,e.start_date,e.currency,
      e.level_id,e.department_id,e.position_id,e.manager_person_id,(SELECT name FROM staff_levels WHERE id=e.level_id) level_name,n.display_name AS branch_name
    FROM hr_people p LEFT JOIN hr_engagements e ON e.id=(SELECT x.id FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1)
    LEFT JOIN organization_nodes n ON n.id=e.branch_id
    WHERE p.company_id=? AND p.merged_into_id IS NULL AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)
    AND (?::text IS NULL OR (?='unset' AND e.level_id IS NULL) OR e.level_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY p.legal_name,p.id`).bind(actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,filters.levelId??null,filters.levelId??null,filters.levelId??null,filters.departmentId??null,filters.departmentId??null).all();
  return rows.results;
}

export async function getPerson(actor: WorkContext, personId: string) {
  const { write } = await holdingResources(actor.holdingId);
  const person = await directoryPerson(actor,personId);
  const privateAccess = canReadCompensation(actor,person.user_id);
  const compensation = privateAccess ? await write.prepare("SELECT base_amount_minor,hourly_rate_minor,pay_frequency,effective_from AS compensation_effective_from FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (effective_to IS NULL OR effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY effective_from DESC LIMIT 1").bind(person.engagement_id).first() : null;
  const [leave,time] = privateAccess ? await Promise.all([
    write.prepare("SELECT l.id,l.number,l.start_date,l.days,l.state,l.decision_note FROM leave_requests l JOIN employees e ON e.id=l.employee_id WHERE e.person_id=? AND l.company_id=? AND (?::text IS NULL OR l.branch_id=?) ORDER BY l.start_date DESC LIMIT 24").bind(personId,actor.companyId,actor.branchId,actor.branchId).all(),
    write.prepare("SELECT id,version,submitted_by,approved_by,work_date,regular_hours,overtime_hours,status,note,rejection_reason FROM hr_time_entries WHERE engagement_id=? ORDER BY work_date DESC LIMIT 24").bind(person.engagement_id).all(),
  ]) : [{results:[]},{results:[]}];
  if (!privateAccess) { person.phone=null; }
  const revisions=privateAccess?(await write.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? ORDER BY effective_from DESC,created_at DESC").bind(person.engagement_id).all()).results:[];
  return {person:{...person,...compensation},leave:leave.results,time:time.results,privateAccess,revisions};
}

export async function peopleSummary(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId);
  const scope = scopeWhere(actor, "e");
  const [people, active, contractors, runs, pending] = await Promise.all([
    write.prepare(`SELECT COUNT(DISTINCT p.id) AS count FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id WHERE ${scope.sql} AND p.status!='archived' AND p.merged_into_id IS NULL`).bind(...scope.args).first<{ count: number }>(),
    write.prepare(`SELECT COUNT(*) AS count FROM hr_engagements e WHERE ${scope.sql} AND e.status='active'`).bind(...scope.args).first<{ count: number }>(),
    write.prepare(`SELECT COUNT(*) AS count FROM hr_engagements e WHERE ${scope.sql} AND e.worker_type='contractor' AND e.status='active'`).bind(...scope.args).first<{ count: number }>(),
    (!actor.branchId&&!actor.departmentId&&hasPermission(actor,"payroll.read")) ? write.prepare("SELECT COUNT(*) AS count FROM hr_payroll_runs WHERE company_id=? AND status IN ('draft','calculating','preview')").bind(actor.companyId).first<{ count: number }>() : Promise.resolve(null),
    write.prepare(`SELECT COUNT(*) AS count FROM leave_requests e WHERE ${scope.sql} AND e.state='pending_approval' AND (?=1 OR e.created_by=?)`).bind(...scope.args,canManagePeople(actor)?1:0,actor.userId).first<{ count: number }>(),
  ]);
  return { people: people?.count ?? 0, active: active?.count ?? 0, contractors: contractors?.count ?? 0, payrollRuns: runs?.count ?? 0, pendingLeave: pending?.count ?? 0 };
}

export async function createPerson(actor: WorkContext, input: z.infer<typeof createPersonSchema>, id: string) {
  actor = requirePermission(actor,"people.manage"); const { write } = await holdingResources(actor.holdingId); const c = await command(write, actor, id, "people.person.create", input); if (c.result) return c.result;
  const personId = `person-${crypto.randomUUID()}`; const now = Date.now();
  const result = { id: personId, legalName: input.legalName, status: "active", version: 1 };
  await executeCommand(write,actor,id,"people.person.create",input,personId,1,result,[
    write.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,preferred_name,email,phone,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(personId,input.userId ?? null,actor.companyId,input.legalName,input.preferredName ?? null,input.email ?? null,input.phone ?? null,input.countryCode,actor.userId,now,now),
    receipt(write,id,"people.person.create",c.digest,result),
  ]); return result;
}

export async function createEngagement(actor: WorkContext, input: z.infer<typeof createEngagementSchema>, id: string) {
  actor = requirePermission(actor,"people.manage"); const { write } = await holdingResources(actor.holdingId); const c = await command(write, actor, id, "people.engagement.create", input); if (c.result) return c.result;
  const person = await write.prepare("SELECT id FROM hr_people WHERE id=? AND company_id=? AND status!='archived'").bind(input.personId,actor.companyId).first(); if (!person) throw new ApiError(404,"PERSON_NOT_FOUND","Person not found.");
  if (input.endDate && input.endDate < input.startDate) throw new ApiError(422,"INVALID_DATES","End date must be on or after start date.");
  if (input.workerType === "employee" && input.employmentType === "contractor") throw new ApiError(422,"INVALID_WORKER_TYPE","Employee engagements cannot use contractor employment type.");
  const engagementId = `eng-${crypto.randomUUID()}`; const now = Date.now(); const result = { id: engagementId, personId: input.personId, status: "onboarding", version: 1 };
  await executeCommand(write,actor,id,"people.engagement.create",input,engagementId,1,result,[
    write.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,end_date,manager_person_id,job_title,currency,pay_group_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(engagementId,input.personId,actor.companyId,input.branchId ?? actor.branchId,input.departmentId ?? actor.departmentId,input.workerType,input.employmentType,"onboarding",input.startDate,input.endDate ?? null,input.managerPersonId ?? null,input.jobTitle,input.currency,input.payGroupId ?? null,actor.userId,now,now),
    receipt(write,id,"people.engagement.create",c.digest,result),
  ]); return result;
}

export async function createCompensation(actor:WorkContext,input:z.infer<typeof compensationSchema>,key:string) {
  actor = requirePermission(actor,"people.manage");const {write}=await holdingResources(actor.holdingId),id=`comp-${key}`;const previous=await replayCommand(write,actor,key,"people.compensation.create",id,input);if(previous)return previous;
  const e=await write.prepare("SELECT e.* FROM hr_engagements e WHERE e.id=? AND e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(input.engagementId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<Record<string,string|number|null>>();if(!e)throw new ApiError(404,"ENGAGEMENT_NOT_FOUND","Engagement not found in your scope.");
  if(e.currency!==input.currency)throw new ApiError(422,"CURRENCY_MISMATCH","Compensation must use the engagement currency.");if(input.effectiveTo&&input.effectiveTo<input.effectiveFrom)throw new ApiError(422,"INVALID_DATES","Effective end must follow the start date.");
  const base=decimalToMinor(input.baseAmount),hourly=decimalToMinor(input.hourlyRate);for(const c of input.components)decimalToMinor(c.amount);if(new Set(input.components.map(c=>c.code)).size!==input.components.length)throw new ApiError(422,"DUPLICATE_COMPONENT","Each compensation component needs a distinct code.");
  const now=Date.now();return executeCommand(write,actor,key,"people.compensation.create",input,id,1,{id,status:"draft",version:1},[
    assertion(write,key,"engagement","EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND currency=?)",[input.engagementId,e.version,input.currency]),
    write.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,effective_to,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)").bind(id,input.engagementId,input.effectiveFrom,input.effectiveTo??null,input.payFrequency,base,hourly,input.expectedHours,input.currency,JSON.stringify(input.components),actor.userId,now,now),
  ]);
}
export async function payrollOptions(actor:WorkContext){actor = requirePermission(actor,"people.payroll.details");const {write}=await holdingResources(actor.holdingId);return (await write.prepare("SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND status='active' ORDER BY name").bind(actor.companyId).all()).results;}
export const assignPayGroupSchema=z.object({payGroupId:z.string().nullable(),expectedVersion:z.number().int().positive()});
export async function assignPayGroup(actor:WorkContext,engagementId:string,input:z.infer<typeof assignPayGroupSchema>,key:string){
  actor = requirePermission(actor,"people.manage");const {write}=await holdingResources(actor.holdingId);
  const previous=await replayCommand(write,actor,key,"people.pay-group.assign",engagementId,input);if(previous)return previous;
  const missing=await write.prepare("SELECT 1 FROM hr_engagements e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.id=? AND e.company_id=? AND e.status='active' AND (l.id IS NULL OR l.active!=1)").bind(engagementId,actor.companyId).first();
  if(missing)throw new ApiError(422,"LEVEL_REQUIRED","Assign an active staff level before updating this engagement.");
  return executeCommand(write,actor,key,"people.pay-group.assign",input,engagementId,input.expectedVersion+1,{id:engagementId,version:input.expectedVersion+1},[
    assertion(write,key,"engagement","EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))",[engagementId,actor.companyId,input.expectedVersion,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]),
    assertion(write,key,"active-level","NOT EXISTS(SELECT 1 FROM hr_engagements e LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.id=? AND e.status='active' AND (l.id IS NULL OR l.active!=1))",[engagementId]),
    assertion(write,key,"pay-group","?::text IS NULL OR EXISTS(SELECT 1 FROM hr_pay_groups p JOIN hr_engagements e ON e.id=? WHERE p.id=? AND p.company_id=e.company_id AND p.currency=e.currency AND p.status='active')",[input.payGroupId,engagementId,input.payGroupId]),
    write.prepare("UPDATE hr_engagements SET pay_group_id=?,version=version+1,updated_at=? WHERE id=?").bind(input.payGroupId,Date.now(),engagementId),
  ]);
}

export async function transitionCompensation(actor:WorkContext,compensationId:string,input:z.infer<typeof compensationTransitionSchema>,key:string) {
  actor = requirePermission(actor,"people.compensation.approve");const {write}=await holdingResources(actor.holdingId);const previous=await replayCommand(write,actor,key,"people.compensation.transition",compensationId,input);if(previous)return previous;
  const row=await write.prepare("SELECT c.*,e.person_id,e.branch_id,e.department_id FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id WHERE c.id=? AND e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(compensationId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<Record<string,string|number|null>>();if(!row)throw new ApiError(404,"COMPENSATION_NOT_FOUND","Compensation not found in your scope.");
  if(samePerson(actor,row.created_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different person must approve compensation.");
  return executeCommand(write,actor,key,"people.compensation.transition",input,compensationId,input.expectedVersion+1,{id:compensationId,status:input.status,version:input.expectedVersion+1},[
    assertion(write,key,"revision","EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status=?)",[compensationId,input.expectedVersion,input.status==="approved"?"draft":"approved"]),
    ...(input.status==="approved"?[assertion(write,key,"overlap","NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? AND id!=? AND status IN ('active','approved') AND effective_from<=COALESCE(?,'9999-12-31') AND COALESCE(effective_to,'9999-12-31')>=?)",[row.engagement_id,compensationId,row.effective_to,row.effective_from])]:[]),
    write.prepare("UPDATE hr_compensation_revisions SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(input.status,actor.userId,Date.now(),compensationId),
    write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,compensationId,`people.compensation.${input.status}`,actor.userId,actor.assignmentId,input.expectedVersion,await sha256(JSON.stringify(input)),Date.now()),
  ]);
}

export async function transitionEngagement(actor: WorkContext, engagementId: string, input: z.infer<typeof engagementTransitionSchema>, key: string) {
  actor = requirePermission(actor,"people.manage");
  if (!actor.beta) throw new ApiError(409,"LIFECYCLE_REQUIRED","Use the person lifecycle workflow to review verified accounts and the effective time.");
  const { write } = await holdingResources(actor.holdingId);
  const previous=await replayCommand(write,actor,key,"people.engagement.transition",engagementId,input);if(previous)return previous;
  const guard=await organizationGuard(write,actor.companyId,key);
  const row = await write.prepare("SELECT e.*,p.user_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(engagementId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<{id:string;person_id:string;user_id:string|null;status:string;version:number;start_date:string;end_date:string|null;branch_id:string|null;department_id:string|null;position_id:string|null;level_id:string|null;manager_person_id:string|null}>();
  if(!row)throw new ApiError(404,"ENGAGEMENT_NOT_FOUND","Engagement not found in your scope.");
  if(input.status==="active")await validateStaffOrganization(write,actor.companyId,row,true);
  if(row.user_id===actor.userId&&input.status!=="active")throw new ApiError(409,"SELF_DEACTIVATION","Another HR administrator must deactivate your engagement.");
  const valid=(row.status==="onboarding"&&input.status==="active")||(["active","suspended"].includes(row.status)&&input.status!==row.status);
  if(!valid)throw new ApiError(409,"INVALID_ENGAGEMENT_TRANSITION","Engagement cannot move through that transition.");
  if(input.status==="ended"&&!input.endDate)throw new ApiError(422,"END_DATE_REQUIRED","Enter the final date of engagement.");
  if(input.endDate&&input.endDate<row.start_date)throw new ApiError(422,"INVALID_DATES","End date must be on or after start date.");
  const now=Date.now();
  return executeCommand(write,actor,key,"people.engagement.transition",input,engagementId,input.expectedVersion+1,{id:engagementId,status:input.status,version:input.expectedVersion+1},[
    guard,
    assertion(write,key,"engagement","EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status=?)",[engagementId,actor.companyId,input.expectedVersion,row.status]),
    ...(input.status==="active"?[assertion(write,key,"active-level","EXISTS(SELECT 1 FROM hr_engagements e JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id WHERE e.id=? AND l.active=1)",[engagementId])]:[]),
    write.prepare("UPDATE hr_engagements SET status=?,end_date=CASE WHEN ?='ended' THEN ? ELSE end_date END,version=version+1,updated_at=? WHERE id=?").bind(input.status,input.status,input.endDate??null,now,engagementId),
    write.prepare("UPDATE employees SET status=CASE WHEN EXISTS(SELECT 1 FROM hr_engagements WHERE person_id=? AND status='active') THEN 'active' ELSE 'inactive' END WHERE person_id=?").bind(row.person_id,row.person_id),
    ...(input.status==="active"||!row.user_id?[]:[write.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND NOT EXISTS(SELECT 1 FROM hr_engagements WHERE person_id=? AND status='active')").bind(now,row.user_id,actor.companyId,row.person_id)]),
  ]);
}


export async function createTimeEntry(actor:WorkContext,input:z.infer<typeof timeEntrySchema>,key:string){
  const {write}=await holdingResources(actor.holdingId),id=`time-${key}`;const previous=await replayCommand(write,actor,key,"people.time.submit",id,input);if(previous)return previous;
  const e=await write.prepare("SELECT e.*,p.user_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.company_id=? AND e.status='active' AND p.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(input.engagementId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<Record<string,string|number|null>>();if(!e)throw new ApiError(404,"ENGAGEMENT_NOT_FOUND","Active employee placement not found in your scope.");
  if(e.user_id!==actor.userId&&!canManagePeople(actor))throw new ApiError(403,"TIME_OWNER_REQUIRED","Submit time for your own employee record.");
  if(input.regularHours+input.overtimeHours>24||input.regularHours+input.overtimeHours<=0)throw new ApiError(422,"INVALID_HOURS","Enter a positive total of at most 24 hours per day.");
  if(input.workDate<String(e.start_date)||(e.end_date&&input.workDate>String(e.end_date)))throw new ApiError(422,"DATE_OUTSIDE_ENGAGEMENT","Work date must fall within the engagement.");
  return executeCommand(write,actor,key,"people.time.submit",input,id,1,{id,status:"submitted",version:1},[
    assertion(write,key,"engagement","EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')",[input.engagementId,e.version]),
    write.prepare("INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,overtime_hours,note,status,submitted_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'submitted',?,?,?)").bind(id,input.engagementId,actor.companyId,input.workDate,input.regularHours,input.overtimeHours,input.note,actor.userId,Date.now(),Date.now()),
  ]);
}
export const timeEntryUpdateSchema = timeEntrySchema.omit({engagementId:true}).extend({expectedVersion:z.number().int().positive()});
export const timeEntryReviewSchema = z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(500)});
export async function updateTimeEntry(actor:WorkContext,entryId:string,input:z.infer<typeof timeEntryUpdateSchema>,key:string){
  const {write}=await holdingResources(actor.holdingId);
  const previous=await replayCommand(write,actor,key,"people.time.update",entryId,input);if(previous)return previous;
  const entry=await write.prepare("SELECT t.*,e.start_date,e.end_date FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE t.id=? AND t.company_id=? AND t.submitted_by=? AND e.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(entryId,actor.companyId,actor.userId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<Record<string,string|number|null>>();
  if(!entry)throw new ApiError(404,"TIME_ENTRY_NOT_FOUND","Time entry not found in your editable records.");
  if(input.regularHours+input.overtimeHours>24||input.regularHours+input.overtimeHours<=0)throw new ApiError(422,"INVALID_HOURS","Enter a positive total of at most 24 hours per day.");
  if(input.workDate<String(entry.start_date)||(entry.end_date&&input.workDate>String(entry.end_date)))throw new ApiError(422,"DATE_OUTSIDE_ENGAGEMENT","Work date must fall within the engagement.");
  return executeCommand(write,actor,key,"people.time.update",input,entryId,input.expectedVersion+1,{id:entryId,status:"submitted",version:input.expectedVersion+1},[
    assertion(write,key,"editable-time","EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND version=? AND status IN ('draft','submitted','rejected') AND submitted_by=?)",[entryId,input.expectedVersion,actor.userId]),
    write.prepare("UPDATE hr_time_entries SET work_date=?,regular_hours=?,overtime_hours=?,note=?,status='submitted',approved_by=NULL,rejection_reason=NULL,version=version+1,updated_at=? WHERE id=?").bind(input.workDate,input.regularHours,input.overtimeHours,input.note,Date.now(),entryId),
  ]);
}
export async function rejectTimeEntry(actor:WorkContext,entryId:string,input:z.infer<typeof timeEntryReviewSchema>,key:string){
  actor = requirePermission(actor,"people.time.manage");const {write}=await holdingResources(actor.holdingId);
  return executeCommand(write,actor,key,"people.time.reject",input,entryId,input.expectedVersion+1,{id:entryId,status:"rejected",version:input.expectedVersion+1},[
    assertion(write,key,"reviewable-time","EXISTS(SELECT 1 FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status='submitted' AND t.submitted_by!=? AND (p.user_id IS NULL OR p.user_id!=?) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?))",[entryId,actor.companyId,input.expectedVersion,actor.userId,actor.userId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]),
    write.prepare("UPDATE hr_time_entries SET status='rejected',rejection_reason=?,version=version+1,updated_at=? WHERE id=?").bind(input.reason,Date.now(),entryId),
    write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,entryId,"people.time.reject",actor.userId,actor.assignmentId,input.expectedVersion,await sha256(JSON.stringify(input)),Date.now()),
  ]);
}

export async function approveTimeEntry(actor:WorkContext,entryId:string,expectedVersion:number,key:string){
  actor = requirePermission(actor,"people.time.manage");const {write}=await holdingResources(actor.holdingId);
  return executeCommand(write,actor,key,"people.time.approve",{expectedVersion},entryId,expectedVersion+1,{id:entryId,status:"approved",version:expectedVersion+1},[
    assertion(write,key,"time","EXISTS(SELECT 1 FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status='submitted' AND t.submitted_by!=? AND (p.user_id IS NULL OR p.user_id!=?) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?))",[entryId,actor.companyId,expectedVersion,actor.userId,actor.userId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]),
    write.prepare("UPDATE hr_time_entries SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,Date.now(),entryId),
    write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,entryId,"people.time.approve",actor.userId,actor.assignmentId,expectedVersion,key,Date.now()),
  ]);
}

export async function createPayGroup(actor: WorkContext, input: z.infer<typeof payGroupSchema>, id: string) {
  actor = requirePermission(actor,"payroll.prepare"); const { write } = await holdingResources(actor.holdingId); const c=await command(write,actor,id,"people.pay-group.create",input); if(c.result)return c.result;
  const groupId=`paygrp-${crypto.randomUUID()}`; const now=Date.now(); const result={id:groupId,...input,status:"active"};
  await executeCommand(write,actor,id,"people.pay-group.create",input,groupId,1,result,[write.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(groupId,actor.companyId,input.name,input.countryCode,input.currency,input.frequency,input.timezone,input.overtimeMultiplierBps,input.prorationMethod,actor.userId,now,now),receipt(write,id,"people.pay-group.create",c.digest,result)]); return result;
}

export async function createCountryPack(actor: WorkContext, input: z.infer<typeof countryPackSchema>, id: string) {
  actor = requirePermission(actor,"payroll.prepare"); const { write }=await holdingResources(actor.holdingId); const c=await command(write,actor,id,"people.country-pack.create",input); if(c.result)return c.result;
  const packId=`pack-${crypto.randomUUID()}`; const now=Date.now(); const result={id:packId,countryCode:input.countryCode,version:input.version,status:"testing"};
  await executeCommand(write,actor,id,"people.country-pack.create",input,packId,1,result,[write.prepare("INSERT INTO hr_country_packs(id,country_code,version,status,rules_json,export_formats_json,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(packId,input.countryCode,input.version,"testing",JSON.stringify(input.rules),JSON.stringify(input.exportFormats),actor.userId,now,now),receipt(write,id,"people.country-pack.create",c.digest,result)]); return result;
}

export async function listCountryPacks(actor: WorkContext){actor = requirePermission(actor,"payroll.configuration.read");const {write}=await holdingResources(actor.holdingId);return (await write.prepare("SELECT id,country_code,version,status,effective_from,export_formats_json,validated_by,updated_at FROM hr_country_packs ORDER BY country_code,version DESC").all()).results;}
export async function transitionCountryPack(actor: WorkContext,packId:string,input:z.infer<typeof countryPackTransitionSchema>,commandId:string){actor = requirePermission(actor,"people.compensation.approve");const {write}=await holdingResources(actor.holdingId);const prior=await replayCommand(write,actor,commandId,`people.country-pack.${input.status}`,packId,input);if(prior)return prior;const pack=await write.prepare("SELECT status FROM hr_country_packs WHERE id=?").bind(packId).first<{status:string}>();if(!pack)throw new ApiError(404,"COUNTRY_PACK_NOT_FOUND","Country pack not found.");const allowed=(pack.status==="testing"&&input.status==="validated")||(pack.status==="validated"&&input.status==="active")||(pack.status==="active"&&input.status==="superseded");if(!allowed)throw new ApiError(409,"INVALID_COUNTRY_PACK_TRANSITION","Country pack status must progress testing → validated → active.");const now=Date.now();await executeCommand(write,actor,commandId,`people.country-pack.${input.status}`,input,packId,1,{id:packId,status:input.status},[assertion(write,commandId,"pack-status","EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status=?)",[packId,pack.status]),write.prepare("UPDATE hr_country_packs SET status=?,approval_evidence=?,validated_by=?,effective_from=CASE WHEN ?='active' THEN COALESCE(effective_from,?) ELSE effective_from END,updated_at=? WHERE id=? AND status=?").bind(input.status,input.proof,actor.userId,input.status,new Date().toISOString().slice(0,10),now,packId,pack.status)]);return {id:packId,status:input.status};}

async function calculateItem(write: SqlDatabase, run: { id: string; company_id: string; period_start: string; period_end: string; currency: string }, engagement: { id: string; version:number; person_id: string; worker_type: string; employment_type: string; start_date: string; end_date: string | null; job_title: string },policy:{overtimeMultiplierBps:number;prorationMethod:"calendar_days"|"none"}) {
  const compensation = await write.prepare(`SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1`).bind(engagement.id,run.period_end,run.period_start).first<Record<string, string | number | null>>();
  const time = await write.prepare("SELECT id,version,regular_hours,overtime_hours FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id").bind(engagement.id,run.period_start,run.period_end).all<{id:string;version:number;regular_hours:number;overtime_hours:number}>();
  const handoffs=(await write.prepare('SELECT id,request_id,request_version,minutes,work_date FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ? ORDER BY id').bind(engagement.id,run.period_start,run.period_end).all<{id:string;request_id:string;request_version:number;minutes:number;work_date:string}>()).results;
  const hours = {handoffs,regular:time.results.reduce((sum,t)=>sum+t.regular_hours,0),overtime:time.results.reduce((sum,t)=>sum+t.overtime_hours,0)+handoffs.reduce((sum,h)=>sum+h.minutes/60,0),entries:time.results};
  const base = Number(compensation?.base_amount_minor ?? 0); const hourly = Number(compensation?.hourly_rate_minor ?? 0); const regular = Number(hours?.regular ?? 0); const overtime = Number(hours?.overtime ?? 0);
  const components = compensation?.components_json ? JSON.parse(String(compensation.components_json)) as Array<{ code:string; label:string; kind:string; amount:string }> : [];
  const benefitInputs=(await write.prepare("SELECT b.*,p.name plan_name FROM benefit_payroll_inputs b JOIN benefit_elections e ON e.id=b.election_id JOIN benefit_plans p ON p.id=e.plan_id WHERE b.engagement_id=? AND b.currency=? AND b.status='released' AND b.effective_from<=? AND (b.effective_to IS NULL OR b.effective_to>=?) ORDER BY b.id").bind(engagement.id,run.currency,run.period_end,run.period_start).all<{id:string;plan_name:string;employee_deduction_minor:number;employer_cost_minor:number;source_election_version:number}>()).results;
  for(const benefit of benefitInputs){
    if(Number(benefit.employee_deduction_minor)>0)components.push({code:`benefit-deduction:${benefit.id}`,label:`Benefit · ${benefit.plan_name}`,kind:"deduction",amount:minorToDecimal(Number(benefit.employee_deduction_minor))});
    if(Number(benefit.employer_cost_minor)>0)components.push({code:`benefit-employer:${benefit.id}`,label:`Benefit employer cost · ${benefit.plan_name}`,kind:"employer",amount:minorToDecimal(Number(benefit.employer_cost_minor))});
  }
  const advanceRepayments=(await write.prepare(`SELECT r.id,r.version,r.amount_minor,r.due_period_start,r.due_period_end,a.reason
      FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id
      WHERE r.company_id=? AND a.engagement_id=? AND a.status='approved' AND r.status IN ('open','reserved')
        AND r.due_period_start<=? AND r.due_period_end>=? AND (r.payroll_run_id IS NULL OR r.payroll_run_id=?)
      ORDER BY r.sequence`).bind(run.company_id,engagement.id,run.period_end,run.period_start,run.id).all<{id:string;version:number;amount_minor:number;due_period_start:string;due_period_end:string;reason:string}>()).results;
  for(const repayment of advanceRepayments){
    components.push({code:`payroll-advance:${repayment.id}`,label:`Payroll advance repayment · ${repayment.due_period_start}`,kind:"deduction",amount:minorToDecimal(Number(repayment.amount_minor))});
  }
  const day=86400000,periodDays=Math.floor((Date.parse(run.period_end)-Date.parse(run.period_start))/day)+1,activeStart=engagement.start_date>run.period_start?engagement.start_date:run.period_start,activeEnd=engagement.end_date&&engagement.end_date<run.period_end?engagement.end_date:run.period_end,activeDays=Math.floor((Date.parse(activeEnd)-Date.parse(activeStart))/day)+1,partial=activeDays!==periodDays;
  if(partial&&policy.prorationMethod==="none")throw new ApiError(422,"PRORATION_REQUIRED","This pay group does not permit automatic proration for partial-period employment.");
  const calculated = calculatePayrollAmounts({ employmentType: engagement.employment_type as "salaried"|"hourly"|"shift"|"contractor", baseMinor: base, hourlyMinor: hourly, regularHours: regular, overtimeHours: overtime,overtimeMultiplierBps:policy.overtimeMultiplierBps,...(partial&&policy.prorationMethod==="calendar_days"?{prorationNumerator:activeDays,prorationDenominator:periodDays}:{}), components: components.filter((component): component is {code:string;label:string;kind:"earning"|"deduction"|"employer";amount:string} => ["earning","deduction","employer"].includes(component.kind)) });
  const { gross, deductions, employer, net } = calculated; const inputSnapshot={engagement,compensation,hours,benefitInputs,advanceRepayments,period:{start:run.period_start,end:run.period_end},policy:{...policy,activeDays,periodDays}}; const result={grossMinor:gross,deductionsMinor:deductions,employerCostMinor:employer,netMinor:net,explanation:{baseMinor:base,hourlyRateMinor:hourly,regularHours:regular,overtimeHours:overtime,overtimeMultiplierBps:policy.overtimeMultiplierBps,activeDays,periodDays,components:calculated.applied}};
  return { itemId:`payitem-${crypto.randomUUID()}`, engagementId:engagement.id, personId:engagement.person_id, workerType:engagement.worker_type, inputSnapshot, result, gross, deductions, employer, net };
}

export async function createPayrollRun(actor:WorkContext,input:z.infer<typeof payrollRunSchema>,key:string) {
  actor = requirePermission(actor,"payroll.prepare");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");const {write}=await holdingResources(actor.holdingId),id=`payrun-${key}`;const previous=await replayCommand(write,actor,key,"people.payroll.create",id,input);if(previous)return previous;
  if(input.mode==="production")throw new ApiError(409,"PAYROLL_ENGINE_UNAVAILABLE","Production payroll requires an implemented and validated statutory calculation adapter. The global calculator supports gross-pay previews.");
  if(input.periodEnd<input.periodStart||input.payDate<input.periodEnd)throw new ApiError(422,"INVALID_PAYROLL_PERIOD","Review the period and pay date.");
  const now=Date.now();return executeCommand(write,actor,key,"people.payroll.create",input,id,1,{id,status:"calculating",mode:"preview",version:1},[
    assertion(write,key,"pay-group","EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND currency=? AND status='active')",[input.payGroupId,actor.companyId,input.currency]),
    assertion(write,key,"duplicate-period","NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND mode!='final_pay' AND status!='cancelled' AND period_start<=? AND period_end>=?)",[actor.companyId,input.payGroupId,input.periodEnd,input.periodStart]),
    write.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'preview','calculating',?,?,?)").bind(id,actor.companyId,input.payGroupId,input.countryPackId??null,input.periodStart,input.periodEnd,input.payDate,input.currency,actor.userId,now,now),
  ]);
}

export async function prepareFinalPay(actor:WorkContext,input:z.infer<typeof finalPayPreparationSchema>,key:string){
  actor=requirePermission(actor,"payroll.prepare");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Final payroll requires a holding-level assignment.");
  const {write}=await holdingResources(actor.holdingId),prepId=`final-pay-${key}`,runId=`payrun-final-${key}`,previous=await replayCommand(write,actor,key,"people.payroll.final-pay.prepare",prepId,input);if(previous)return previous;
  if(input.periodEnd<input.periodStart||input.payDate<input.periodEnd)throw new ApiError(422,"INVALID_PAYROLL_PERIOD","Review the final-pay period and pay date.");
  const task=await write.prepare(`SELECT t.id,t.version,t.status,t.assignee_id,t.clearance_type,t.case_id,c.person_id,c.definition,e.id engagement_id,e.pay_group_id,e.currency,e.end_date
    FROM people_preparation_tasks t JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id
    JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
    WHERE t.id=? AND t.company_id=?`).bind(input.taskId,actor.companyId).first<{id:string;version:number;status:string;assignee_id:string|null;clearance_type:string|null;case_id:string;person_id:string;definition:string;engagement_id:string;pay_group_id:string|null;currency:string;end_date:string|null}>();
  if(!task)throw new ApiError(404,"CLEARANCE_TASK_NOT_FOUND","Final payroll clearance task not found.");
  if(task.clearance_type!=="payroll"||task.status!=="pending"||task.assignee_id!==actor.userId)throw new ApiError(403,"FINAL_PAY_OWNER_REQUIRED","Only the assigned Payroll clearance owner can prepare final pay.");
  if(task.version!==input.expectedTaskVersion)throw new ApiError(409,"CLEARANCE_TASK_CHANGED","Refresh the clearance task before preparing final pay.");
  if(task.pay_group_id!==input.payGroupId||task.currency!==input.currency)throw new ApiError(422,"FINAL_PAY_POLICY_MISMATCH","Final pay must use the engagement pay group and currency.");
  const definition=typeof task.definition==="string"?JSON.parse(task.definition) as {lastWorkingDate?:string}:task.definition;
  const lastWorking=definition.lastWorkingDate??task.end_date;if(!lastWorking||input.periodEnd>lastWorking)throw new ApiError(422,"FINAL_PAY_AFTER_LAST_DAY","Final-pay period cannot extend beyond the last working day.");
  const now=Date.now(),result={id:prepId,taskId:input.taskId,runId,status:"prepared",version:1};
  return executeCommand(write,actor,key,"people.payroll.final-pay.prepare",input,prepId,1,result,[
    assertion(write,key,"task-version","EXISTS(SELECT 1 FROM people_preparation_tasks WHERE id=? AND company_id=? AND version=? AND status='pending' AND clearance_type='payroll' AND assignee_id=?)",[input.taskId,actor.companyId,input.expectedTaskVersion,actor.userId]),
    assertion(write,key,"engagement-pay-group","EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND pay_group_id=? AND currency=?)",[task.engagement_id,actor.companyId,input.payGroupId,input.currency]),
    assertion(write,key,"one-final-pay","NOT EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations WHERE task_id=? AND status!='cancelled')",[input.taskId]),
    write.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'final_pay','calculating',?,?,?)").bind(runId,actor.companyId,input.payGroupId,null,task.engagement_id,input.periodStart,input.periodEnd,input.payDate,input.currency,actor.userId,now,now),
    write.prepare("INSERT INTO hr_payroll_final_pay_preparations(id,company_id,task_id,case_id,engagement_id,run_id,reason,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(prepId,actor.companyId,input.taskId,task.case_id,task.engagement_id,runId,input.reason,actor.userId,now,now),
    write.prepare("INSERT INTO hr_payroll_final_pay_events(id,preparation_id,task_id,run_id,company_id,action,actor_id,task_version,evidence_json,created_at) VALUES(?,?,?,?,?,'prepared',?,?,?,?)").bind(`final-pay-event-${key}`,prepId,input.taskId,runId,actor.companyId,actor.userId,input.expectedTaskVersion,JSON.stringify({periodStart:input.periodStart,periodEnd:input.periodEnd,payDate:input.payDate,currency:input.currency,reason:input.reason}),now),
  ]);
}

function advancePeriodStart(first:string,months:number){const value=new Date(`${first}T00:00:00Z`);value.setUTCMonth(value.getUTCMonth()+months);return value.toISOString().slice(0,10);}
function advancePeriodEnd(first:string,months:number){const value=new Date(`${first}T00:00:00Z`);value.setUTCMonth(value.getUTCMonth()+months+1);value.setUTCDate(0);return value.toISOString().slice(0,10);}

export async function requestPayrollAdvance(actor:WorkContext,input:z.infer<typeof payrollAdvanceRequestSchema>,key:string){
  const {write}=await holdingResources(actor.holdingId),id=`payroll-advance-${key}`,previous=await replayCommand(write,actor,key,"people.payroll.advance.request",id,input);if(previous)return previous;
  const engagement=await write.prepare(`SELECT e.id,e.company_id,e.currency,e.status,e.pay_group_id,p.id person_id,p.user_id,p.legal_name
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    WHERE e.id=? AND e.company_id=? AND p.status='active'`).bind(input.engagementId,actor.companyId).first<{id:string;company_id:string;currency:string;status:string;pay_group_id:string|null;person_id:string;user_id:string;legal_name:string}>();
  if(!engagement)throw new ApiError(404,"ENGAGEMENT_NOT_FOUND","Employee engagement not found in this Company.");
  const payrollOwner=hasPermission(actor,"payroll.prepare"),ownRequest=hasPermission(actor,"self.requests")&&samePerson(actor,engagement.user_id);
  if(!payrollOwner&&!ownRequest)throw new ApiError(403,"PAYROLL_ADVANCE_REQUEST_FORBIDDEN","Request your own advance or use an authorized Payroll preparation role.");
  if(!["active","suspended"].includes(engagement.status))throw new ApiError(409,"ENGAGEMENT_NOT_ACTIVE","Only an active employee can request a payroll advance.");
  if(!engagement.pay_group_id)throw new ApiError(409,"PAY_GROUP_REQUIRED","Assign a pay group before requesting an advance.");
  if(input.currency!==engagement.currency)throw new ApiError(422,"CURRENCY_MISMATCH","Advance currency must match the engagement payroll currency.");
  let amountMinor:number;try{amountMinor=decimalToMinor(input.amount);}catch{throw new ApiError(422,"INVALID_ADVANCE_AMOUNT","Enter a valid advance amount.");}
  if(amountMinor<=0)throw new ApiError(422,"INVALID_ADVANCE_AMOUNT","Advance amount must be greater than zero.");
  const now=Date.now(),result={id,engagementId:engagement.id,personId:engagement.person_id,status:"pending",version:1,amountMinor};
  return executeCommand(write,actor,key,"people.payroll.advance.request",input,id,1,result,[
    assertion(write,key,"engagement","EXISTS(SELECT 1 FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.company_id=? AND e.currency=? AND e.status IN ('active','suspended') AND p.status='active')",[input.engagementId,actor.companyId,input.currency]),
    assertion(write,key,"no-pending-advance","NOT EXISTS(SELECT 1 FROM hr_payroll_advances WHERE engagement_id=? AND status='pending')",[input.engagementId]),
    write.prepare("INSERT INTO hr_payroll_advances(id,company_id,engagement_id,person_id,amount_minor,installment_count,first_deduction_period,currency,reason,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,engagement.id,engagement.person_id,amountMinor,input.installmentCount,input.firstDeductionPeriod,input.currency,input.reason,actor.userId,now,now),
    write.prepare("INSERT INTO hr_payroll_advance_events(id,advance_id,company_id,action,actor_id,advance_version,evidence_json,created_at) VALUES(?,?,?,'requested',?,?,?,?)").bind(`payroll-advance-event-${key}`,id,actor.companyId,actor.userId,1,JSON.stringify({amountMinor,installmentCount:input.installmentCount,firstDeductionPeriod:input.firstDeductionPeriod,currency:input.currency,reason:input.reason}),now),
    write.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT 'payroll-advance-review-'||md5(?||':'||a.user_id),?,a.user_id,'payroll_advance',?,1,'Payroll advance needs an independent decision',?
      FROM assignments a WHERE a.company_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'payroll.approve') AND NOT iam_same_person(a.user_id,?,a.company_id)
      GROUP BY a.user_id ON CONFLICT DO NOTHING`).bind(id,actor.companyId,id,now,actor.companyId,now,actor.userId),
  ]);
}

export async function decidePayrollAdvance(actor:WorkContext,advanceId:string,input:z.infer<typeof payrollAdvanceDecisionSchema>,key:string){
  if(input.action!=="cancel")actor=requirePermission(actor,"payroll.approve");else if(!hasPermission(actor,"self.requests")&&!hasPermission(actor,"payroll.prepare"))throw new ApiError(403,"PAYROLL_ADVANCE_CANCEL_FORBIDDEN","Only the requester or Payroll owner can cancel this advance.");
  const {write}=await holdingResources(actor.holdingId),previous=await replayCommand(write,actor,key,`people.payroll.advance.${input.action}`,advanceId,input);if(previous)return previous;
  const row=await write.prepare("SELECT a.*,p.user_id FROM hr_payroll_advances a JOIN hr_people p ON p.id=a.person_id WHERE a.id=? AND a.company_id=?").bind(advanceId,actor.companyId).first<Record<string,string|number|null>>();
  if(!row)throw new ApiError(404,"PAYROLL_ADVANCE_NOT_FOUND","Payroll advance not found.");
  if(row.status!=="pending"||Number(row.version)!==input.expectedVersion)throw new ApiError(409,"PAYROLL_ADVANCE_CHANGED","This advance changed. Reload before deciding.");
  if(input.action==="cancel"&&!samePerson(actor,row.requested_by)&&!hasPermission(actor,"payroll.prepare"))throw new ApiError(403,"PAYROLL_ADVANCE_OWNER_REQUIRED","Only the requester or Payroll owner can cancel this advance.");
  if(input.action!=="cancel"&&samePerson(actor,row.requested_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different person must decide the payroll advance.");
  const status=input.action==="approve"?"approved":input.action==="reject"?"rejected":"cancelled",version=input.expectedVersion+1,now=Date.now();
  const statements:SqlStatement[]=[
    assertion(write,key,"advance-version","EXISTS(SELECT 1 FROM hr_payroll_advances WHERE id=? AND company_id=? AND status='pending' AND version=?)",[advanceId,actor.companyId,input.expectedVersion]),
    write.prepare("UPDATE hr_payroll_advances SET status=?,version=version+1,approved_by=CASE WHEN ?='approved' THEN ? ELSE approved_by END,review_reason=?,reviewed_at=?,updated_at=? WHERE id=?").bind(status,status,actor.userId,input.reason,now,now,advanceId),
    write.prepare("INSERT INTO hr_payroll_advance_events(id,advance_id,company_id,action,actor_id,advance_version,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`payroll-advance-event-${key}`,advanceId,actor.companyId,input.action==="approve"?"approved":status,actor.userId,version,JSON.stringify({reason:input.reason,status}),now),
    write.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,?,?,?,?,?,?,? ON CONFLICT DO NOTHING").bind(`payroll-advance-result-${key}`,actor.companyId,row.requested_by,"payroll_advance",advanceId,version,status==="approved"?"Payroll advance approved; deductions will appear in payroll":"Payroll advance decision recorded",now),
  ];
  if(input.action==="approve"){
    const total=Number(row.amount_minor),count=Number(row.installment_count),base=Math.floor(total/count),remainder=total%count;
    for(let index=0;index<count;index++)statements.push(write.prepare("INSERT INTO hr_payroll_advance_repayments(id,advance_id,company_id,sequence,due_period_start,due_period_end,amount_minor,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`payroll-advance-repayment-${key}-${index+1}`,advanceId,actor.companyId,index+1,advancePeriodStart(String(row.first_deduction_period),index),advancePeriodEnd(String(row.first_deduction_period),index),base+(index<remainder?1:0),now));
  }
  return executeCommand(write,actor,key,`people.payroll.advance.${input.action}`,input,advanceId,version,{id:advanceId,status,version},statements);
}

export async function payrollAdvancePage(actor:WorkContext){
  const privileged=hasPermission(actor,"payroll.read");if(!privileged&&!hasPermission(actor,"self.requests"))throw new ApiError(403,"PAYROLL_ADVANCE_READ_FORBIDDEN","Payroll advance access is not available for this role.");
  const {write}=await holdingResources(actor.holdingId),rows=(await write.prepare(`SELECT a.*,p.legal_name,e.job_title,e.currency,e.pay_group_id FROM hr_payroll_advances a JOIN hr_people p ON p.id=a.person_id JOIN hr_engagements e ON e.id=a.engagement_id WHERE a.company_id=? AND (${privileged?"TRUE":"p.user_id=?"}) ORDER BY a.created_at DESC,a.id DESC`).bind(...(privileged?[actor.companyId]:[actor.companyId,actor.userId])).all()).results;
  const options=(await write.prepare(`SELECT e.id engagement_id,p.legal_name,e.currency,e.pay_group_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.status IN ('active','suspended') AND e.pay_group_id IS NOT NULL AND p.status='active' AND ${privileged?"TRUE":"p.user_id=?"} ORDER BY p.legal_name,e.id`).bind(...(privileged?[actor.companyId]:[actor.companyId,actor.userId])).all()).results;
  return {items:rows,options};
}

export async function payrollAdvanceAlertTarget(db:SqlDatabase,companyId:string,userId:string,advanceId:string,now=Date.now()){
  return (await db.prepare(`SELECT a.id FROM hr_payroll_advances a JOIN hr_people p ON p.id=a.person_id
    WHERE a.id=? AND a.company_id=? AND (p.user_id=? OR EXISTS(
      SELECT 1 FROM assignments x WHERE x.company_id=a.company_id AND x.user_id=? AND x.scope_kind='holding' AND x.revoked_at IS NULL
      AND iam_assignment_active(x.id,?) AND iam_role_allows(x.role,x.company_id,'payroll.read')
    )) LIMIT 1`).bind(advanceId,companyId,userId,userId,now).first<string>("id"))??null;
}
export async function calculatePayrollRun(actor:WorkContext,runId:string,key=crypto.randomUUID()) {
  actor = requirePermission(actor,"payroll.prepare");const {write}=await holdingResources(actor.holdingId);const previous=await replayCommand(write,actor,key,"people.payroll.calculate",runId,{});if(previous)return previous;
  const run=await write.prepare("SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<{id:string;company_id:string;pay_group_id:string;period_start:string;period_end:string;currency:string;mode:string;status:string;version:number}>();if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if(!["draft","calculating","preview","failed"].includes(run.status)||!["preview","final_pay"].includes(run.mode))throw new ApiError(409,"PAYROLL_LOCKED","Only an unapproved payroll preview can be recalculated.");
  // A run is company-wide; a branch preparer may not read or calculate other branches.
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a holding-level payroll assignment for company-wide runs.");
  const engagements=await write.prepare("SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,e.end_date,e.job_title FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id").bind(actor.companyId,run.pay_group_id,run.currency,run.period_end,run.period_start,(run as Record<string,unknown>).scope_engagement_id??null,(run as Record<string,unknown>).scope_engagement_id??null).all<{id:string;version:number;person_id:string;worker_type:string;employment_type:string;start_date:string;end_date:string|null;job_title:string}>();
  if(!engagements.results.length)throw new ApiError(422,"PAYROLL_EMPTY","Assign active employees to this pay group before calculating.");
  const group=await write.prepare("SELECT overtime_multiplier_bps,proration_method FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(run.pay_group_id,actor.companyId).first<{overtime_multiplier_bps:number;proration_method:"calendar_days"|"none"}>();if(!group)throw new ApiError(409,"PAY_GROUP_CHANGED","The pay group is no longer active.");
  const policy={overtimeMultiplierBps:Number(group.overtime_multiplier_bps),prorationMethod:group.proration_method};
  const items=await Promise.all(engagements.results.map(e=>calculateItem(write,run,e,policy)));
  if(items.some(i=>!i.inputSnapshot.compensation||i.inputSnapshot.compensation.currency!==run.currency))throw new ApiError(422,"PAYROLL_INPUT_INCOMPLETE","Every employee needs approved compensation in the pay-group currency covering the full period.");
  if(items.some(i=>i.net<0))throw new ApiError(422,"NEGATIVE_NET_PAY","Review deductions exceeding gross pay.");
  const total=items.reduce((a,i)=>({gross:a.gross+i.gross,deductions:a.deductions+i.deductions,employer:a.employer+i.employer,net:a.net+i.net}),{gross:0,deductions:0,employer:0,net:0});if(Object.values(total).some(v=>!Number.isSafeInteger(v)))throw new ApiError(422,"MONEY_OVERFLOW","Payroll exceeds supported precision.");
  const digest=await sha256(JSON.stringify(items.map(i=>i.inputSnapshot))),now=Date.now();
  return executeCommand(write,actor,key,"people.payroll.calculate",{},runId,run.version+1,{id:runId,status:"preview",version:run.version+1,totals:{...total,headcount:items.length}},[
    assertion(write,key,"run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND version=? AND status IN ('draft','calculating','preview','failed'))",[runId,run.version]),
    ...items.flatMap((i,index)=>[
      assertion(write,key,`engagement-${index}`,"EXISTS(SELECT 1 FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.version=? AND e.status IN ('active','ended') AND p.status='active')",[i.engagementId,i.inputSnapshot.engagement.version]),
      assertion(write,key,`time-count-${index}`,"(SELECT COUNT(*) FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved')=?",[i.engagementId,run.period_start,run.period_end,i.inputSnapshot.hours.entries.length]),
      assertion(write,key,`handoff-count-${index}`,"(SELECT COUNT(*) FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ?)=?",[i.engagementId,run.period_start,run.period_end,i.inputSnapshot.hours.handoffs.length]),
      ...i.inputSnapshot.hours.entries.map(t=>assertion(write,key,`time-${index}-${t.id}`,"EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND version=? AND status='approved')",[t.id,t.version])),
      ...i.inputSnapshot.advanceRepayments.map(repayment=>assertion(write,key,`advance-repayment-${index}-${repayment.id}`,"EXISTS(SELECT 1 FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id WHERE r.id=? AND r.version=? AND r.status IN ('open','reserved') AND a.status='approved' AND a.engagement_id=? AND a.company_id=?)",[repayment.id,repayment.version,i.engagementId,actor.companyId])),
    ]),
    ...items.map((i,index)=>assertion(write,key,`compensation-${index}`,"EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status IN ('active','approved'))",[i.inputSnapshot.compensation!.id,i.inputSnapshot.compensation!.version])),
    write.prepare("DELETE FROM hr_payroll_items WHERE run_id=?").bind(runId),
    write.prepare("UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=?,input_digest=?,version=version+1,updated_at=?,error_message=NULL WHERE id=?").bind(JSON.stringify({...total,headcount:items.length}),digest,now,runId),
    ...items.map((i,index)=>write.prepare("INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(`payitem-${key}-${index}`,runId,i.engagementId,i.workerType,JSON.stringify(i.inputSnapshot),JSON.stringify(i.result),i.gross,i.deductions,i.employer,i.net,run.currency,now)),
  ]);
}

export async function workPayrollRows(actor: WorkContext) {
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  actor = requirePermission(actor,"payroll.read");
  const { write } = await holdingResources(actor.holdingId);
  return (await write.prepare("SELECT r.* FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id WHERE r.company_id=? ORDER BY r.updated_at DESC").bind(actor.companyId).all<Record<string,string|number|null>>()).results;
}
export async function getPayrollRun(actor: WorkContext, runId: string) { if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment."); const canReadPayroll=hasPermission(actor,"payroll.read"),canReadPayment=hasPermission(actor,"payroll.payment.read"); if(!canReadPayroll&&!canReadPayment)requirePermission(actor,"payroll.read"); const paymentOnly=!canReadPayroll; const {write}=await holdingResources(actor.holdingId); const run=await write.prepare("SELECT r.*,g.overtime_multiplier_bps,g.proration_method FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id WHERE r.id=? AND r.company_id=?").bind(runId,actor.companyId).first(); if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found."); const [items,exports,corrections,paymentBatch]=await Promise.all([write.prepare("SELECT i.*,p.legal_name,p.id person_id FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE i.run_id=? ORDER BY p.legal_name").bind(runId).all(),write.prepare("SELECT id,run_revision,checksum_sha256,created_at FROM hr_payroll_review_exports WHERE run_id=? ORDER BY run_revision DESC").bind(runId).all(),write.prepare("SELECT * FROM hr_payroll_corrections WHERE run_id=? AND company_id=? ORDER BY created_at DESC,id DESC").bind(runId,actor.companyId).all(),write.prepare("SELECT * FROM hr_payment_batches WHERE company_id=? AND source_type='payroll' AND source_id=? AND status<>'cancelled' ORDER BY created_at DESC,id DESC LIMIT 1").bind(actor.companyId,runId).first()]); const paymentItems=paymentBatch ? (await write.prepare("SELECT * FROM hr_payment_batch_items WHERE batch_id=? ORDER BY reference").bind(String(paymentBatch.id)).all()).results : []; return {run:paymentOnly?{...run,totals_json:"{}",input_digest:null,error_message:null}:run,items:paymentOnly?[]:items.results,exports:paymentOnly?[]:exports.results,corrections:paymentOnly?[]:corrections.results,paymentBatch,paymentItems}; }
export async function listPayrollRuns(actor: WorkContext) { actor = requirePermission(actor,"payroll.read"); if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment."); const {write}=await holdingResources(actor.holdingId); return (await write.prepare("SELECT * FROM hr_payroll_runs WHERE company_id=? ORDER BY pay_date DESC,created_at DESC").bind(actor.companyId).all()).results; }

export async function payrollPostingOptions(actor: WorkContext) {
  actor = requirePermission(actor,"finance.approve");
  if (actor.branchId || actor.departmentId) throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const { write } = await holdingResources(actor.holdingId);
  const [charts, periods] = await Promise.all([
    write.prepare("SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status='active' ORDER BY name").bind(actor.companyId).all(),
    write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(actor.companyId).all(),
  ]);
  const chartIds = charts.results.map(row => String((row as Record<string,unknown>).id));
  const accounts = chartIds.length ? await write.prepare(`SELECT id,chart_id,code,name,category,currency FROM finance_accounts WHERE company_id=? AND status='active' AND chart_id IN (${chartIds.map(() => "?").join(",")}) ORDER BY code`).bind(actor.companyId,...chartIds).all() : { results: [] };
  return { charts: charts.results, periods: periods.results, accounts: accounts.results };
}

export async function postPayroll(actor: WorkContext, runId: string, input: z.infer<typeof payrollPostSchema>, key: string) {
  actor = requirePermission(actor,"finance.approve");
  if (actor.branchId || actor.departmentId) throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(write, actor, key, "people.payroll.post", runId, input);
  if (previous) return previous;
  const run = await write.prepare(`SELECT r.*, COALESCE(SUM(i.gross_minor),0) gross_minor,
      COALESCE(SUM(i.deductions_minor),0) deductions_minor,
      COALESCE(SUM(i.employer_cost_minor),0) employer_minor,
      COALESCE(SUM(i.net_minor),0) net_minor,
      COUNT(i.id) item_count,
      COUNT(i.id) FILTER (WHERE i.status NOT IN ('calculated','approved','posted')) invalid_items
    FROM hr_payroll_runs r LEFT JOIN hr_payroll_items i ON i.run_id=r.id
    WHERE r.id=? AND r.company_id=?
    GROUP BY r.id`).bind(runId, actor.companyId).first<Record<string, string | number | null>>();
  if (!run) throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if (String(run.status) !== "approved") throw new ApiError(409,"PAYROLL_NOT_APPROVED","An independently approved payroll run is required before GL posting.");
  if (Number(run.version) !== input.expectedVersion) throw new ApiError(409,"VERSION_CONFLICT","The payroll run changed. Refresh before posting.");
  if (Number(run.item_count) <= 0 || Number(run.invalid_items) > 0) throw new ApiError(409,"PAYROLL_ITEMS_INVALID","Every payroll item must be calculated or approved before posting.");
  if (Number(run.deductions_minor) > 0 && !input.taxPayableAccountId) throw new ApiError(422,"TAX_PAYABLE_ACCOUNT_REQUIRED","Select the liability account that holds payroll deductions.");
  if (Number(run.employer_minor) > 0 && !input.employerContributionAccountId) throw new ApiError(422,"EMPLOYER_ACCOUNT_REQUIRED","Select the liability account that holds employer contributions.");
  const gross = Number(run.gross_minor), deductions = Number(run.deductions_minor), employer = Number(run.employer_minor), net = Number(run.net_minor);
  if (gross < 0 || deductions < 0 || employer < 0 || net <= 0 || gross + employer !== net + deductions + employer) throw new ApiError(409,"PAYROLL_TOTALS_INVALID","Payroll totals do not balance to a payable amount.");
  const lines = [
    { accountId: input.expenseAccountId, description: "Payroll gross and employer cost", debit: minorToDecimal(gross + employer), credit: "0" },
    { accountId: input.netPayableAccountId, description: "Net pay payable to employees", debit: "0", credit: minorToDecimal(net) },
    ...(deductions > 0 ? [{ accountId: input.taxPayableAccountId!, description: "Payroll deductions payable", debit: "0", credit: minorToDecimal(deductions) }] : []),
    ...(employer > 0 ? [{ accountId: input.employerContributionAccountId!, description: "Employer contributions payable", debit: "0", credit: minorToDecimal(employer) }] : []),
  ];
  const journal = await prepareJournal(write, actor, {
    chartId: input.chartId,
    periodId: input.periodId,
    entryDate: input.entryDate,
    currency: String(run.currency),
    memo: `Payroll ${String(run.period_start)} to ${String(run.period_end)}`,
    sourceType: "payroll",
    sourceId: runId,
    isOpening: false,
    lines,
  }, `${key}-journal`, "posted");
  const now = Date.now(), result = { id: runId, status: "posted", version: input.expectedVersion + 1, journalId: journal.result.id, netPayableMinor: net };
  return executeCommand(write, actor, key, "people.payroll.post", input, runId, input.expectedVersion + 1, result, [
    assertion(write, key, "payroll-run", "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND input_digest IS NOT NULL)", [runId, actor.companyId, input.expectedVersion]),
    assertion(write, key, "payroll-items", "NOT EXISTS(SELECT 1 FROM hr_payroll_items WHERE run_id=? AND status NOT IN ('calculated','approved','posted'))", [runId]),
    assertion(write, key, "payroll-posting-unique", "NOT EXISTS(SELECT 1 FROM hr_payroll_postings WHERE run_id=?)", [runId]),
    ...journal.steps,
    write.prepare("UPDATE hr_payroll_runs SET status='posted',posted_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='approved' AND version=?").bind(actor.userId, now, runId, actor.companyId, input.expectedVersion),
    write.prepare("UPDATE hr_payroll_items SET status='posted' WHERE run_id=? AND status IN ('calculated','approved')").bind(runId),
    write.prepare("INSERT INTO hr_payroll_postings(id,run_id,journal_id,created_at) VALUES(?,?,?,?)").bind(`payroll-posting-${key}`, runId, journal.result.id, now),
  ]);
}

export async function approvePayrollRun(actor:WorkContext,runId:string,expectedVersion:number,key:string) {
  actor = requirePermission(actor,"payroll.approve");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");const {write}=await holdingResources(actor.holdingId);const input={expectedVersion};const previous=await replayCommand(write,actor,key,"people.payroll.approve",runId,input);if(previous)return previous;
  return executeCommand(write,actor,key,"people.payroll.approve",input,runId,expectedVersion+1,{id:runId,status:"approved",version:expectedVersion+1},[
    assertion(write,key,"run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status='preview' AND created_by!=? AND input_digest IS NOT NULL)",[runId,actor.companyId,expectedVersion,actor.userId]),
    write.prepare("UPDATE hr_payroll_runs SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,Date.now(),runId),
    write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) SELECT ?,?,?,?,?,?,?,input_digest,? FROM hr_payroll_runs WHERE id=?").bind(crypto.randomUUID(),key,runId,"people.payroll.approve",actor.userId,actor.assignmentId,expectedVersion,Date.now(),runId),
  ]);
}

export async function requestPayrollCorrection(actor:WorkContext,runId:string,input:z.infer<typeof payrollCorrectionRequestSchema>,key:string){
  actor=requirePermission(actor,"payroll.prepare");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write}=await holdingResources(actor.holdingId),id=`payroll-correction-${key}`,previous=await replayCommand(write,actor,key,"people.payroll.correction.request",id,input);if(previous)return previous;
  const run=await write.prepare("SELECT id,status,version,revision,input_digest,totals_json FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<{id:string;status:string;version:number;revision:number;input_digest:string|null;totals_json:string}>();
  if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");if(run.status!=="approved"||run.version!==input.expectedVersion||!run.input_digest)throw new ApiError(409,"PAYROLL_CORRECTION_SOURCE_CHANGED","Only the current approved payroll revision can be returned for correction.");
  const now=Date.now(),result={id,runId,status:"pending",version:1};
  return executeCommand(write,actor,key,"people.payroll.correction.request",input,id,1,result,[
    assertion(write,key,"approved-run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND revision=? AND input_digest=?)",[runId,actor.companyId,input.expectedVersion,run.revision,run.input_digest]),
    write.prepare("INSERT INTO hr_payroll_corrections(id,run_id,company_id,run_version,run_revision,input_digest,totals_json,reason,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(id,runId,actor.companyId,run.version,run.revision,run.input_digest,run.totals_json,input.reason,actor.userId,now,now),
    write.prepare("INSERT INTO hr_payroll_correction_events(id,correction_id,run_id,company_id,action,actor_id,run_version,evidence_json,created_at) VALUES(?,?,?,?, 'requested',?,?,?,?)").bind(`payroll-correction-event-${key}`,id,runId,actor.companyId,actor.userId,run.version,JSON.stringify({reason:input.reason,runRevision:run.revision,inputDigest:run.input_digest,totals:JSON.parse(run.totals_json)}),now),
    write.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT 'payroll-correction-review-'||md5(?||':'||a.user_id),?,a.user_id,'payroll_correction',?,1,'Approved payroll needs an independent correction decision',?
      FROM assignments a WHERE a.company_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'payroll.approve') AND NOT iam_same_person(a.user_id,?,a.company_id)
      GROUP BY a.user_id ON CONFLICT DO NOTHING`).bind(id,actor.companyId,id,now,actor.companyId,now,actor.userId),
  ]);
}

export async function decidePayrollCorrection(actor:WorkContext,correctionId:string,input:z.infer<typeof payrollCorrectionDecisionSchema>,key:string){
  const permission=input.action==="cancel"?"payroll.prepare":"payroll.approve";actor=requirePermission(actor,permission);if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write}=await holdingResources(actor.holdingId),previous=await replayCommand(write,actor,key,`people.payroll.correction.${input.action}`,correctionId,input);if(previous)return previous;
  const row=await write.prepare("SELECT c.*,r.status run_status,r.version current_run_version,r.revision current_run_revision,r.input_digest current_input_digest FROM hr_payroll_corrections c JOIN hr_payroll_runs r ON r.id=c.run_id WHERE c.id=? AND c.company_id=?").bind(correctionId,actor.companyId).first<Record<string,string|number|null>>();
  if(!row)throw new ApiError(404,"PAYROLL_CORRECTION_NOT_FOUND","Payroll correction request not found.");if(row.status!=="pending"||Number(row.version)!==input.expectedVersion)throw new ApiError(409,"PAYROLL_CORRECTION_CHANGED","This correction request changed. Reload before deciding.");
  if(input.action==="cancel"&&!samePerson(actor,row.requested_by))throw new ApiError(403,"PAYROLL_CORRECTION_OWNER_REQUIRED","Only the requester can cancel this correction request.");
  if(input.action!=="cancel"&&samePerson(actor,row.requested_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different person must decide the payroll correction.");
  if(row.run_status!=="approved"||Number(row.current_run_version)!==Number(row.run_version)||Number(row.current_run_revision)!==Number(row.run_revision)||row.current_input_digest!==row.input_digest)throw new ApiError(409,"PAYROLL_CORRECTION_SOURCE_CHANGED","The approved payroll evidence changed. Start a new correction review.");
  const status=input.action==="cancel"?"cancelled":input.action==="approve"?"approved":"rejected",now=Date.now(),version=input.expectedVersion+1,result={id:correctionId,runId:String(row.run_id),status,version,runStatus:input.action==="approve"?"calculating":"approved"};
  const statements:SqlStatement[]=[
    assertion(write,key,"correction-version","EXISTS(SELECT 1 FROM hr_payroll_corrections WHERE id=? AND company_id=? AND status='pending' AND version=?)",[correctionId,actor.companyId,input.expectedVersion]),
    assertion(write,key,"run-evidence","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND revision=? AND input_digest=?)",[row.run_id,actor.companyId,row.run_version,row.run_revision,row.input_digest]),
    write.prepare("UPDATE hr_payroll_corrections SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=?,updated_at=? WHERE id=?").bind(status,actor.userId,input.reason,now,now,correctionId),
    write.prepare("INSERT INTO hr_payroll_correction_events(id,correction_id,run_id,company_id,action,actor_id,run_version,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(`payroll-correction-event-${key}`,correctionId,row.run_id,actor.companyId,status,actor.userId,row.run_version,JSON.stringify({reason:input.reason,runRevision:row.run_revision,inputDigest:row.input_digest}),now),
    write.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(`payroll-correction-result-${key}`,actor.companyId,row.requested_by,"payroll_correction",correctionId,version,input.action==="approve"?"Payroll correction approved; recalculate the run with current inputs":input.action==="reject"?"Payroll correction rejected; review the decision reason":"Payroll correction request cancelled",now),
  ];
  if(input.action==="approve")statements.push(write.prepare("UPDATE hr_payroll_runs SET status='calculating',approved_by=NULL,version=version+1,error_message=?,updated_at=? WHERE id=?").bind(`Correction approved: ${input.reason}`,now,row.run_id));
  return executeCommand(write,actor,key,`people.payroll.correction.${input.action}`,input,correctionId,version,result,statements);
}

export async function payrollCorrectionAlertTarget(db:SqlDatabase,companyId:string,userId:string,correctionId:string,now=Date.now()){
  return (await db.prepare(`SELECT c.run_id FROM hr_payroll_corrections c JOIN assignments a ON a.company_id=c.company_id AND a.user_id=?
    WHERE c.id=? AND c.company_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'payroll.read') LIMIT 1`).bind(userId,correctionId,companyId,now).first<string>("run_id"))??null;
}

export async function exportPayrollReview(actor:WorkContext,runId:string,key:string){
  actor=requirePermission(actor,"payroll.read");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write,files}=await holdingResources(actor.holdingId),previous=await replayCommand(write,actor,key,"people.payroll.review-export",runId,{});if(previous)return previous;
  const run=await write.prepare("SELECT id,status,revision,input_digest,currency,period_start,period_end FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<{id:string;status:string;revision:number;input_digest:string|null;currency:string;period_start:string;period_end:string}>();
  if(!run||!["preview","approved"].includes(run.status)||!run.input_digest)throw new ApiError(409,"PAYROLL_REVIEW_NOT_READY","Calculate a payroll preview before exporting review evidence.");
  const items=await write.prepare("SELECT p.legal_name,i.worker_type,i.gross_minor,i.deductions_minor,i.employer_cost_minor,i.net_minor,i.currency,i.input_snapshot_json FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE i.run_id=? ORDER BY p.legal_name").bind(runId).all<{legal_name:string;worker_type:string;gross_minor:number;deductions_minor:number;employer_cost_minor:number;net_minor:number;currency:string;input_snapshot_json:string}>();
  const csv=["employee,worker_type,gross,deductions,employer_cost,net,currency,input_snapshot",...items.results.map(item=>[item.legal_name,item.worker_type,minorToDecimal(item.gross_minor),minorToDecimal(item.deductions_minor),minorToDecimal(item.employer_cost_minor),minorToDecimal(item.net_minor),item.currency,item.input_snapshot_json].map(value=>`"${String(value).replaceAll('"','""')}"`).join(","))].join("\n"),checksum=await sha256(csv),fileKey=`${actor.holdingId}/people/payroll/${runId}/review-${run.revision}.csv`,exportId=`payroll-review-${key}`,now=Date.now();
  await files.put(fileKey,csv,{httpMetadata:{contentType:"text/csv"},customMetadata:{sha256:checksum,inputDigest:run.input_digest,period:`${run.period_start}:${run.period_end}`}});
  return executeCommand(write,actor,key,"people.payroll.review-export",{},runId,Number(run.revision),{id:runId,revision:run.revision,checksum},[
    assertion(write,key,"run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND revision=? AND input_digest=? AND status IN ('preview','approved'))",[runId,actor.companyId,run.revision,run.input_digest]),
    write.prepare("INSERT INTO hr_payroll_review_exports(id,run_id,run_revision,file_key,checksum_sha256,created_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(exportId,runId,run.revision,fileKey,checksum,actor.userId,now),
  ]);
}

export async function getPayrollReviewFile(actor:WorkContext,runId:string,revision:number){
  actor=requirePermission(actor,"payroll.read");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write,files}=await holdingResources(actor.holdingId),row=await write.prepare("SELECT e.file_key,e.checksum_sha256 FROM hr_payroll_review_exports e JOIN hr_payroll_runs r ON r.id=e.run_id WHERE e.run_id=? AND e.run_revision=? AND r.company_id=?").bind(runId,revision,actor.companyId).first<{file_key:string;checksum_sha256:string}>();if(!row)throw new ApiError(404,"PAYROLL_REVIEW_EXPORT_NOT_FOUND","Payroll review export not found.");const object=await files.get(row.file_key);if(!object)throw new ApiError(503,"PAYROLL_REVIEW_EXPORT_UNAVAILABLE","Payroll review export is temporarily unavailable.");return {object,checksum:row.checksum_sha256};
}

export async function createPaymentBatch(actor: WorkContext, input: z.infer<typeof paymentBatchSchema>, commandId: string) {
  actor = requirePermission(actor,"payroll.payment.prepare"); const {write}=await holdingResources(actor.holdingId); const c=await command(write,actor,commandId,"people.payment-batch.create",input); if(c.result)return c.result;
  const duplicate = await write.prepare("SELECT id FROM hr_payment_batches WHERE company_id=? AND source_type=? AND source_id=? AND status NOT IN ('cancelled') LIMIT 1").bind(actor.companyId, input.sourceType, input.sourceId).first();
  if (duplicate) throw new ApiError(409, "PAYMENT_BATCH_EXISTS", "An outstanding payment batch already exists for this obligation.");
  let currency = "";
  let items: { results: Array<{ id: string; engagement_id: string | null; person_id: string | null; legal_name: string; preferred_name: string | null; net_minor: number; currency: string }> };
  if (input.sourceType === "payroll") {
    const source = await write.prepare("SELECT id,status,currency FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(input.sourceId,actor.companyId).first<{id:string;status:string;currency:string}>();
    if (!source || source.status !== "posted") throw new ApiError(409,"PAYROLL_NOT_POSTED","Payroll must be posted before payment export.");
    currency = source.currency;
    items = await write.prepare(`SELECT i.id,i.engagement_id,e.person_id,p.legal_name,p.preferred_name,i.net_minor,i.currency
      FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id
      WHERE i.run_id=? AND i.net_minor>0 AND i.status='posted' ORDER BY i.id`).bind(input.sourceId).all<{id:string;engagement_id:string;person_id:string;legal_name:string;preferred_name:string|null;net_minor:number;currency:string}>();
  } else if (input.sourceType === "expense") {
    throw new ApiError(409,"EXPENSE_PAYABLE_REQUIRED","Record reimbursement against the posted expense payable.");
  } else {
    const invoice = await write.prepare(`SELECT i.id,i.status,i.currency,i.total_minor,p.name AS legal_name
      FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
      WHERE i.id=? AND i.company_id=? AND i.kind='payable' AND p.kind='supplier' AND p.person_id IS NULL AND i.source_type!='expense_claim'`).bind(input.sourceId, actor.companyId).first<{id:string;status:string;currency:string;total_minor:number;legal_name:string}>();
    if (!invoice || invoice.status !== "posted") throw new ApiError(409,"CONTRACTOR_INVOICE_NOT_POSTED","A posted contractor payable is required before payment export.");
    currency = invoice.currency;
    items = { results: [{ id: invoice.id, engagement_id: null, person_id: null, legal_name: invoice.legal_name, preferred_name: null, net_minor: Number(invoice.total_minor), currency: invoice.currency }] };
  }
  if(!items.results.length)throw new ApiError(409,"PAYMENT_ITEMS_EMPTY","There are no approved payable items to export.");
  const total=items.results.reduce((sum,item)=>sum+item.net_minor,0); if (total <= 0) throw new ApiError(422, "PAYMENT_TOTAL_INVALID", "Payment amount must be greater than zero."); const batchId=`payment-${crypto.randomUUID()}`; const now=Date.now(); const statements:SqlStatement[]=[write.prepare("INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,status,format_code,total_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(batchId,actor.companyId,input.sourceType,input.sourceId,currency,"draft",input.formatCode,total,actor.userId,now,now)];
  if(input.sourceType==='contractor')statements.push(
    assertion(write,commandId,'supplier-payment-reservation',"NOT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=? AND status IN ('reserved','exported','unknown','confirmed'))",[input.sourceId]),
    assertion(write,commandId,'contractor-batch-unique',"NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE company_id=? AND source_type='contractor' AND source_id=? AND status<>'cancelled' AND id<>?)",[actor.companyId,input.sourceId,batchId]),
    write.prepare('UPDATE finance_invoices SET version=version+1,updated_at=? WHERE id=? AND company_id=?').bind(now,input.sourceId,actor.companyId),
  );
  for(const item of items.results)statements.push(write.prepare("INSERT INTO hr_payment_batch_items(id,batch_id,engagement_id,person_id,reference,payee_name,amount_minor,currency) VALUES(?,?,?,?,?,?,?,?)").bind(`payitem-${crypto.randomUUID()}`,batchId,item.engagement_id,item.person_id,`PAY-${input.sourceId}-${item.id}`,item.preferred_name??item.legal_name,item.net_minor,item.currency));
  const result={id:batchId,status:"draft",totalMinor:total,itemCount:items.results.length}; statements.push(receipt(write,commandId,"people.payment-batch.create",c.digest,result));
  await executeCommand(write,actor,commandId,"people.payment-batch.create",input,batchId,1,result,statements); return result;
}

export async function approvePaymentBatch(actor: WorkContext,batchId:string,expectedVersion:number,commandId:string){actor = requirePermission(actor,"finance.pay");const {write}=await holdingResources(actor.holdingId);const prior=await replayCommand(write,actor,commandId,"people.payment-batch.approve",batchId,{expectedVersion});if(prior)return prior;const batch=await write.prepare("SELECT status,version,created_by FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId,actor.companyId).first<{status:string;version:number;created_by:string}>();if(!batch)throw new ApiError(404,"PAYMENT_BATCH_NOT_FOUND","Payment batch not found.");if(samePerson(actor,batch.created_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different person must approve the payment batch.");if(batch.status!=="draft"||batch.version!==expectedVersion)throw new ApiError(409,"PAYMENT_BATCH_NOT_APPROVABLE","Refresh the payment batch before approving it.");const now=Date.now();await executeCommand(write,actor,commandId,"people.payment-batch.approve",{expectedVersion},batchId,1,{id:batchId,status:"approved",version:expectedVersion+1},[assertion(write,commandId,"batch-version","EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND version=? AND status='draft' AND created_by!=?)",[batchId,actor.companyId,expectedVersion,actor.userId]),write.prepare("UPDATE hr_payment_batches SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='draft' AND version=?").bind(actor.userId,now,batchId,actor.companyId,expectedVersion),write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),commandId,batchId,"people.payment-batch.approve",actor.userId,actor.assignmentId,expectedVersion,commandId,now)]);return {id:batchId,status:"approved",version:expectedVersion+1};}

export async function exportPaymentBatch(actor: WorkContext,batchId:string,commandId:string){actor = requirePermission(actor,"payroll.payment.execute");const {write,files}=await holdingResources(actor.holdingId);const prior=await replayCommand(write,actor,commandId,"people.payment-batch.export",batchId,{});if(prior)return prior;const batch=await write.prepare("SELECT * FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId,actor.companyId).first<{id:string;status:string;source_type:string;source_id:string;format_code:string;currency:string;total_minor:number}>();if(!batch)throw new ApiError(404,"PAYMENT_BATCH_NOT_FOUND","Payment batch not found.");if(batch.status!=="approved")throw new ApiError(409,"PAYMENT_BATCH_LOCKED","An independently approved payment batch is required before export.");const items=await write.prepare("SELECT reference,payee_name,amount_minor,currency,status FROM hr_payment_batch_items WHERE batch_id=? ORDER BY reference").bind(batchId).all<{reference:string;payee_name:string;amount_minor:number;currency:string;status:string}>();const lines=["reference,payee,amount,currency,status",...items.results.map((item)=>[item.reference,`\"${item.payee_name.replaceAll('"','""')}\"`,minorToDecimal(item.amount_minor),item.currency,item.status].join(","))].join("\n");const key=`${actor.holdingId}/people/payments/${batchId}.csv`;const checksum=await sha256(lines);await files.put(key,lines,{httpMetadata:{contentType:"text/csv"},customMetadata:{sha256:checksum,format:batch.format_code}});await executeCommand(write,actor,commandId,"people.payment-batch.export",{},batchId,1,{id:batchId,status:"exported",fileKey:key,checksum,itemCount:items.results.length},[assertion(write,commandId,"batch-status","EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND status='approved')",[batchId,actor.companyId]),write.prepare("UPDATE hr_payment_batches SET status='exported',file_key=?,file_checksum=?,exported_at=?,version=version+1,updated_at=? WHERE id=?").bind(key,checksum,Date.now(),Date.now(),batchId)]);return {id:batchId,status:"exported",fileKey:key,checksum,itemCount:items.results.length};}

export async function confirmPaymentBatch(actor:WorkContext,batchId:string,input:z.infer<typeof paymentConfirmationSchema>,commandId:string) {
 actor=requirePermission(actor,"finance.pay");const {write}=await holdingResources(actor.holdingId);
 const previous=await replayCommand(write,actor,commandId,"people.payment-batch.confirm",batchId,input);if(previous)return previous;
 const batch=await write.prepare("SELECT status,version FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId,actor.companyId).first<{status:string;version:number}>();
 if(!batch)throw new ApiError(404,"PAYMENT_BATCH_NOT_FOUND","Payment batch not found.");
 if(!["exported","partially_settled"].includes(batch.status))throw new ApiError(409,"PAYMENT_BATCH_NOT_CONFIRMABLE","Export the payment batch first.");
 const items=(await write.prepare("SELECT reference,status FROM hr_payment_batch_items WHERE batch_id=?").bind(batchId).all<{reference:string;status:string}>()).results;
 if(new Set(input.rows.map(r=>r.reference)).size!==input.rows.length || input.rows.some(r=>!items.some(i=>i.reference===r.reference&&["outstanding","failed"].includes(i.status))))throw new ApiError(409,"PAYMENT_ITEMS_CHANGED","Review the current outstanding payment items.");
 const remaining=items.filter(i=>i.status!=="settled"&&!input.rows.some(r=>r.reference===i.reference&&r.status==="confirmed")).length;
 const status=remaining?"partially_settled":"settled",now=Date.now();
 return executeCommand(write,actor,commandId,"people.payment-batch.confirm",input,batchId,batch.version+1,{id:batchId,status,confirmed:input.rows.length,failed:input.rows.filter(r=>r.status==="failed").length},[
 assertion(write,commandId,"payment-batch-version","EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND version=? AND status=?)",[batchId,actor.companyId,batch.version,batch.status]),
 ...input.rows.map(row=>write.prepare("UPDATE hr_payment_batch_items SET status=?,confirmation_ref=?,settled_at=? WHERE batch_id=? AND reference=? AND status IN ('outstanding','failed')").bind(row.status==="confirmed"?"settled":"failed",row.confirmationRef??null,row.status==="confirmed"?now:null,batchId,row.reference)),
 write.prepare("UPDATE hr_payment_batches SET status=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(status,now,batchId,actor.companyId)
 ]);
}

export async function getPaymentFile(actor: WorkContext,batchId:string){actor = requirePermission(actor,"payroll.payment.read");const {write,files}=await holdingResources(actor.holdingId);const batch=await write.prepare("SELECT file_key,file_checksum FROM hr_payment_batches WHERE id=? AND company_id=? AND status IN ('exported','partially_settled','settled')").bind(batchId,actor.companyId).first<{file_key:string|null;file_checksum:string|null}>();if(!batch?.file_key)throw new ApiError(404,"PAYMENT_FILE_NOT_FOUND","Payment export is not available.");const object=await files.get(batch.file_key);if(!object)throw new ApiError(503,"PAYMENT_FILE_UNAVAILABLE","Payment export is temporarily unavailable.");return {object,checksum:batch.file_checksum};}

export async function payrollRunsPage(actor:WorkContext,cursor?:string) {
 actor=requirePermission(actor,"payroll.read");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
 const {write}=await holdingResources(actor.holdingId);
 return readListPage<Record<string,string|number|null> & {id:string;page_key:string}>(write,{
  sql:"SELECT *,pay_date||':'||created_at::text page_key FROM hr_payroll_runs WHERE company_id=?",args:[actor.companyId],cursor,descending:true,scope:[actor.companyId,'payroll'],
 });
}
