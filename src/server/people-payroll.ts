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
  mode: z.enum(["preview", "production"]).default("preview"),
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
      e.level_id,e.department_id,(SELECT name FROM staff_levels WHERE id=e.level_id) level_name,n.display_name AS branch_name
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
  await executeCommand(write,actor,id,"people.pay-group.create",input,groupId,1,result,[write.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(groupId,actor.companyId,input.name,input.countryCode,input.currency,input.frequency,input.timezone,actor.userId,now,now),receipt(write,id,"people.pay-group.create",c.digest,result)]); return result;
}

export async function createCountryPack(actor: WorkContext, input: z.infer<typeof countryPackSchema>, id: string) {
  actor = requirePermission(actor,"payroll.prepare"); const { write }=await holdingResources(actor.holdingId); const c=await command(write,actor,id,"people.country-pack.create",input); if(c.result)return c.result;
  const packId=`pack-${crypto.randomUUID()}`; const now=Date.now(); const result={id:packId,countryCode:input.countryCode,version:input.version,status:"testing"};
  await executeCommand(write,actor,id,"people.country-pack.create",input,packId,1,result,[write.prepare("INSERT INTO hr_country_packs(id,country_code,version,status,rules_json,export_formats_json,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(packId,input.countryCode,input.version,"testing",JSON.stringify(input.rules),JSON.stringify(input.exportFormats),actor.userId,now,now),receipt(write,id,"people.country-pack.create",c.digest,result)]); return result;
}

export async function listCountryPacks(actor: WorkContext){actor = requirePermission(actor,"payroll.configuration.read");const {write}=await holdingResources(actor.holdingId);return (await write.prepare("SELECT id,country_code,version,status,effective_from,export_formats_json,validated_by,updated_at FROM hr_country_packs ORDER BY country_code,version DESC").all()).results;}
export async function transitionCountryPack(actor: WorkContext,packId:string,input:z.infer<typeof countryPackTransitionSchema>,commandId:string){actor = requirePermission(actor,"people.compensation.approve");const {write}=await holdingResources(actor.holdingId);const prior=await replayCommand(write,actor,commandId,`people.country-pack.${input.status}`,packId,input);if(prior)return prior;const pack=await write.prepare("SELECT status FROM hr_country_packs WHERE id=?").bind(packId).first<{status:string}>();if(!pack)throw new ApiError(404,"COUNTRY_PACK_NOT_FOUND","Country pack not found.");const allowed=(pack.status==="testing"&&input.status==="validated")||(pack.status==="validated"&&input.status==="active")||(pack.status==="active"&&input.status==="superseded");if(!allowed)throw new ApiError(409,"INVALID_COUNTRY_PACK_TRANSITION","Country pack status must progress testing → validated → active.");const now=Date.now();await executeCommand(write,actor,commandId,`people.country-pack.${input.status}`,input,packId,1,{id:packId,status:input.status},[assertion(write,commandId,"pack-status","EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status=?)",[packId,pack.status]),write.prepare("UPDATE hr_country_packs SET status=?,approval_evidence=?,validated_by=?,effective_from=CASE WHEN ?='active' THEN COALESCE(effective_from,?) ELSE effective_from END,updated_at=? WHERE id=? AND status=?").bind(input.status,input.proof,actor.userId,input.status,new Date().toISOString().slice(0,10),now,packId,pack.status)]);return {id:packId,status:input.status};}

async function calculateItem(write: SqlDatabase, run: { id: string; period_start: string; period_end: string; currency: string }, engagement: { id: string; version:number; person_id: string; worker_type: string; employment_type: string; start_date: string; end_date: string | null; job_title: string }) {
  const compensation = await write.prepare(`SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1`).bind(engagement.id,run.period_start,run.period_end).first<Record<string, string | number | null>>();
  const time = await write.prepare("SELECT id,version,regular_hours,overtime_hours FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id").bind(engagement.id,run.period_start,run.period_end).all<{id:string;version:number;regular_hours:number;overtime_hours:number}>();
  const hours = {regular:time.results.reduce((sum,t)=>sum+t.regular_hours,0),overtime:time.results.reduce((sum,t)=>sum+t.overtime_hours,0),entries:time.results};
  const base = Number(compensation?.base_amount_minor ?? 0); const hourly = Number(compensation?.hourly_rate_minor ?? 0); const regular = Number(hours?.regular ?? 0); const overtime = Number(hours?.overtime ?? 0);
  const components = compensation?.components_json ? JSON.parse(String(compensation.components_json)) as Array<{ code:string; label:string; kind:string; amount:string }> : [];
  const calculated = calculatePayrollAmounts({ employmentType: engagement.employment_type as "salaried"|"hourly"|"shift"|"contractor", baseMinor: base, hourlyMinor: hourly, regularHours: regular, overtimeHours: overtime, components: components.filter((component): component is {code:string;label:string;kind:"earning"|"deduction"|"employer";amount:string} => ["earning","deduction","employer"].includes(component.kind)) });
  const { gross, deductions, employer, net } = calculated; const inputSnapshot={engagement,compensation,hours,period:{start:run.period_start,end:run.period_end}}; const result={grossMinor:gross,deductionsMinor:deductions,employerCostMinor:employer,netMinor:net,explanation:{baseMinor:base,hourlyRateMinor:hourly,regularHours:regular,overtimeHours:overtime,components:calculated.applied}};
  return { itemId:`payitem-${crypto.randomUUID()}`, engagementId:engagement.id, personId:engagement.person_id, workerType:engagement.worker_type, inputSnapshot, result, gross, deductions, employer, net };
}

export async function createPayrollRun(actor:WorkContext,input:z.infer<typeof payrollRunSchema>,key:string) {
  actor = requirePermission(actor,"payroll.prepare");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");const {write}=await holdingResources(actor.holdingId),id=`payrun-${key}`;const previous=await replayCommand(write,actor,key,"people.payroll.create",id,input);if(previous)return previous;
  if(input.mode==="production")throw new ApiError(409,"PAYROLL_ENGINE_UNAVAILABLE","Production payroll requires an implemented and validated statutory calculation adapter. The global calculator supports gross-pay previews.");
  if(input.periodEnd<input.periodStart||input.payDate<input.periodEnd)throw new ApiError(422,"INVALID_PAYROLL_PERIOD","Review the period and pay date.");
  const now=Date.now();return executeCommand(write,actor,key,"people.payroll.create",input,id,1,{id,status:"calculating",mode:"preview",version:1},[
    assertion(write,key,"pay-group","EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND currency=? AND status='active')",[input.payGroupId,actor.companyId,input.currency]),
    assertion(write,key,"duplicate-period","NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND status!='cancelled' AND period_start<=? AND period_end>=?)",[actor.companyId,input.payGroupId,input.periodEnd,input.periodStart]),
    write.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'preview','calculating',?,?,?)").bind(id,actor.companyId,input.payGroupId,input.countryPackId??null,input.periodStart,input.periodEnd,input.payDate,input.currency,actor.userId,now,now),
  ]);
}
export async function calculatePayrollRun(actor:WorkContext,runId:string,key=crypto.randomUUID()) {
  actor = requirePermission(actor,"payroll.prepare");const {write}=await holdingResources(actor.holdingId);const previous=await replayCommand(write,actor,key,"people.payroll.calculate",runId,{});if(previous)return previous;
  const run=await write.prepare("SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<{id:string;pay_group_id:string;period_start:string;period_end:string;currency:string;mode:string;status:string;version:number}>();if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if(!["draft","calculating","preview","failed"].includes(run.status)||run.mode!=="preview")throw new ApiError(409,"PAYROLL_LOCKED","Only an unapproved preview can be recalculated.");
  // A run is company-wide; a branch preparer may not read or calculate other branches.
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a holding-level payroll assignment for company-wide runs.");
  const engagements=await write.prepare("SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,e.end_date,e.job_title FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.status='active' AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) ORDER BY e.id").bind(actor.companyId,run.pay_group_id,run.currency,run.period_end,run.period_start).all<{id:string;version:number;person_id:string;worker_type:string;employment_type:string;start_date:string;end_date:string|null;job_title:string}>();
  if(!engagements.results.length)throw new ApiError(422,"PAYROLL_EMPTY","Assign active employees to this pay group before calculating.");
  if(engagements.results.some(e=>e.start_date>run.period_start||(e.end_date&&e.end_date<run.period_end)))throw new ApiError(422,"PRORATION_REQUIRED","A partial-period engagement needs a prorated pay calculation before this preview can proceed.");
  const items=await Promise.all(engagements.results.map(e=>calculateItem(write,run,e)));
  if(items.some(i=>!i.inputSnapshot.compensation||i.inputSnapshot.compensation.currency!==run.currency))throw new ApiError(422,"PAYROLL_INPUT_INCOMPLETE","Every employee needs approved compensation in the pay-group currency covering the full period.");
  if(items.some(i=>i.net<0))throw new ApiError(422,"NEGATIVE_NET_PAY","Review deductions exceeding gross pay.");
  const total=items.reduce((a,i)=>({gross:a.gross+i.gross,deductions:a.deductions+i.deductions,employer:a.employer+i.employer,net:a.net+i.net}),{gross:0,deductions:0,employer:0,net:0});if(Object.values(total).some(v=>!Number.isSafeInteger(v)))throw new ApiError(422,"MONEY_OVERFLOW","Payroll exceeds supported precision.");
  const digest=await sha256(JSON.stringify(items.map(i=>i.inputSnapshot))),now=Date.now();
  return executeCommand(write,actor,key,"people.payroll.calculate",{},runId,run.version+1,{id:runId,status:"preview",version:run.version+1,totals:{...total,headcount:items.length}},[
    assertion(write,key,"run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND version=? AND status IN ('draft','calculating','preview','failed'))",[runId,run.version]),
    ...items.flatMap((i,index)=>[
      assertion(write,key,`engagement-${index}`,"EXISTS(SELECT 1 FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.version=? AND e.status='active' AND p.status='active')",[i.engagementId,i.inputSnapshot.engagement.version]),
      assertion(write,key,`time-count-${index}`,"(SELECT COUNT(*) FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved')=?",[i.engagementId,run.period_start,run.period_end,i.inputSnapshot.hours.entries.length]),
      ...i.inputSnapshot.hours.entries.map(t=>assertion(write,key,`time-${index}-${t.id}`,"EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND version=? AND status='approved')",[t.id,t.version])),
    ]),
    ...items.map((i,index)=>assertion(write,key,`compensation-${index}`,"EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status IN ('active','approved'))",[i.inputSnapshot.compensation!.id,i.inputSnapshot.compensation!.version])),
    write.prepare("DELETE FROM hr_payroll_items WHERE run_id=?").bind(runId),
    write.prepare("UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=?,input_digest=?,version=version+1,updated_at=?,error_message=NULL WHERE id=?").bind(JSON.stringify({...total,headcount:items.length}),digest,now,runId),
    ...items.map((i,index)=>write.prepare("INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(`payitem-${key}-${index}`,runId,i.engagementId,i.workerType,JSON.stringify(i.inputSnapshot),JSON.stringify(i.result),i.gross,i.deductions,i.employer,i.net,run.currency,now)),
  ]);
}

export async function getPayrollRun(actor: WorkContext, runId: string) { if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment."); actor = requirePermission(actor,"payroll.read"); const {write}=await holdingResources(actor.holdingId); const run=await write.prepare("SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first(); if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found."); const items=await write.prepare("SELECT i.*,p.legal_name,p.id person_id FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE i.run_id=? ORDER BY p.legal_name").bind(runId).all(); return {run,items:items.results}; }
export async function listPayrollRuns(actor: WorkContext) { actor = requirePermission(actor,"payroll.read"); if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment."); const {write}=await holdingResources(actor.holdingId); return (await write.prepare("SELECT * FROM hr_payroll_runs WHERE company_id=? ORDER BY pay_date DESC,created_at DESC").bind(actor.companyId).all()).results; }

export async function postPayroll(actor: WorkContext) {
  requirePermission(actor,"payroll.calculate");
  throw new ApiError(409,"PAYROLL_ENGINE_UNAVAILABLE","No statutory payroll posting adapter is installed. Reviewed global previews cannot create payable obligations.");

}

export async function approvePayrollRun(actor:WorkContext,runId:string,expectedVersion:number,key:string) {
  actor = requirePermission(actor,"payroll.approve");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");const {write}=await holdingResources(actor.holdingId);const input={expectedVersion};const previous=await replayCommand(write,actor,key,"people.payroll.approve",runId,input);if(previous)return previous;
  return executeCommand(write,actor,key,"people.payroll.approve",input,runId,expectedVersion+1,{id:runId,status:"approved",version:expectedVersion+1},[
    assertion(write,key,"run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status='preview' AND created_by!=? AND input_digest IS NOT NULL)",[runId,actor.companyId,expectedVersion,actor.userId]),
    write.prepare("UPDATE hr_payroll_runs SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,Date.now(),runId),
    write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) SELECT ?,?,?,?,?,?,?,input_digest,? FROM hr_payroll_runs WHERE id=?").bind(crypto.randomUUID(),key,runId,"people.payroll.approve",actor.userId,actor.assignmentId,expectedVersion,Date.now(),runId),
  ]);
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
