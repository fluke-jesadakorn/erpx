import { readListPage } from "./list-page";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { organizationGuard, validateStaffOrganization } from "./organization";
import { requirePermission } from "./iam-permissions";
import "server-only";

import { z } from "zod";
import type { WorkContext } from "@/server/authorization";
import { holdingResources, runtimeEnv } from "@/server/runtime";
import { ApiError } from "@/server/http";
import { sha256 } from "@/server/crypto";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import { calculatePayrollAmounts } from "@/lib/payroll-math";
import {
  calculateThaiHotelPayrollFixture,
  isThaiHotelPayrollMockEnabled,
  thaiHotelPayrollMockId,
} from "./simulation-only-adapters";
import { assertion,executeCommand,replayCommand } from "./commands";
import { directoryPerson, canReadCompensation,canManagePeople } from "./people-directory";
import { prepareJournal } from "./finance";
import { payrollAccountingLines, payrollNetCostShares, type PayrollPostingItem } from "./payroll-accounting";
import { assertPayrollSourceFresh, payrollSourceGuards } from "./payroll-readiness";
import { parseSyntheticPayrollBankResultCsv } from "@/lib/payroll-bank-result";
import { prepareAttachmentBinding } from "./attachments/service";
import { snapshotMoney } from "./company-money";
import { applyThaiStatutory, assertStatutoryRunReady, recognizedThaiPack, thaiEvidenceForEmployee, thaiEvidenceGuards, thaiPackForRun, type PayrollStatutoryRun } from "./payroll-statutory";
import {preparePayslipIssue} from "./payroll-payslips";
export { cancelPayrollRun, payrollCancelSchema } from "./payroll-cancellation";
import { calculatePayrollAdjustmentRun } from "./payroll-adjustments";
import { assertPayrollRecoveryDeductionLimits } from "./payroll-recoveries";
import {preparePayrollDeductionConsumption} from "./payroll-deductions";
export { createPayrollAdjustment, editPayrollAdjustment, payrollAdjustmentCreateSchema, payrollAdjustmentEditSchema } from "./payroll-adjustments";

const country = z.string().length(2).transform((value) => value.toUpperCase());
const currency = z.string().length(3).transform((value) => value.toUpperCase());
const date = z.string().date();

function payrollCsvCell(value:unknown){
  const raw=String(value);
  const safe=/^[\s\u0000-\u001f]*[=+\-@]/u.test(raw)?`'${raw}`:raw;
  return `"${safe.replaceAll('"','""')}"`;
}

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
  employmentType: z.enum(["salaried", "daily", "hourly", "shift", "contractor"]),
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
  dailyRate:z.string().regex(/^\d+(\.\d{1,6})?$/).default("0"),
  standardDailyHours:z.number().positive().max(24).default(8),
  expectedHours: z.number().min(0).max(744).default(0),
  currency,
  components: z.array(z.object({
    code: z.string().min(1).max(80),
    label: z.string().min(1).max(160),
    kind: z.enum(["earning", "deduction", "employer"]),
    amount: z.string().regex(/^\d+(\.\d{1,6})?$/),
    taxable: z.boolean().default(false),
    incomeTiming: z.enum(["regular", "irregular", "non_taxable"]).optional(),
    socialSecurityWage: z.boolean().optional(),
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

export const payrollCostVerificationSchema=z.object({expectedVersion:z.number().int().positive(),evidenceRef:z.string().trim().min(8).max(500)}).strict();

export async function payrollCostAssignments(actor:WorkContext,engagementId:string){
  actor=requirePermission(actor,"payroll.prepare");
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company Payroll assignment to review dated cost units.");
  const {write}=await holdingResources(actor.holdingId);
  const engagement=await write.prepare("SELECT id FROM hr_engagements WHERE id=? AND company_id=?").bind(engagementId,actor.companyId).first();
  if(!engagement)throw new ApiError(404,"ENGAGEMENT_NOT_FOUND","Engagement not found.");
  return (await write.prepare(`SELECT a.id,a.effective_from,a.effective_to,a.branch_id,a.department_id,a.evidence_status,a.source,a.version,
    v.evidence_ref,v.verified_by,v.created_at verified_at FROM hr_payroll_cost_assignments a
    LEFT JOIN hr_payroll_cost_assignment_verifications v ON v.assignment_id=a.id
    WHERE a.engagement_id=? AND a.company_id=? AND a.status='active' ORDER BY a.effective_from,a.id`)
    .bind(engagementId,actor.companyId).all()).results;
}

export async function verifyPayrollCostAssignment(actor:WorkContext,assignmentId:string,input:z.infer<typeof payrollCostVerificationSchema>,commandId:string){
  actor=requirePermission(actor,"payroll.prepare");
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company Payroll assignment to verify historical cost units.");
  const {write}=await holdingResources(actor.holdingId);
  const prior=await replayCommand(write,actor,commandId,"people.cost-assignment.verify",assignmentId,input);if(prior)return prior;
  const assignment=await write.prepare(`SELECT a.id,a.engagement_id,a.effective_from,a.effective_to,a.branch_id,a.department_id,a.evidence_status,a.source,a.version,
    e.start_date,e.branch_id current_branch_id,e.department_id current_department_id FROM hr_payroll_cost_assignments a
    JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.status='active'`)
    .bind(assignmentId,actor.companyId).first<{id:string;engagement_id:string;effective_from:string;effective_to:string|null;branch_id:string|null;department_id:string|null;evidence_status:string;source:string;version:number;start_date:string;current_branch_id:string|null;current_department_id:string|null}>();
  if(!assignment)throw new ApiError(404,"PAYROLL_COST_ASSIGNMENT_NOT_FOUND","Dated cost assignment not found.");
  if(assignment.evidence_status!=="legacy_unverified"||assignment.source!=="migration"||assignment.version!==input.expectedVersion)
    throw new ApiError(409,"PAYROLL_COST_ASSIGNMENT_CHANGED","Only an unchanged imported interval can be verified here.");
  const history=await write.prepare("SELECT 1 FROM staff_organization_history WHERE engagement_id=? AND company_id=? LIMIT 1").bind(assignment.engagement_id,actor.companyId).first();
  if(history||assignment.branch_id!==assignment.current_branch_id||assignment.department_id!==assignment.current_department_id)
    throw new ApiError(409,"PAYROLL_COST_HISTORY_RECONSTRUCTION_REQUIRED","Recorded organization changes need dated reconstruction before this interval can be verified.");
  const now=Date.now(),result={id:assignmentId,status:"verified",version:input.expectedVersion+1};
  return executeCommand(write,actor,commandId,"people.cost-assignment.verify",input,assignmentId,input.expectedVersion+1,result,[
    assertion(write,commandId,"assignment-version","EXISTS(SELECT 1 FROM hr_payroll_cost_assignments a JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.status='active' AND a.evidence_status='legacy_unverified' AND a.source='migration' AND a.version=? AND a.branch_id IS NOT DISTINCT FROM e.branch_id AND a.department_id IS NOT DISTINCT FROM e.department_id AND NOT EXISTS(SELECT 1 FROM staff_organization_history h WHERE h.engagement_id=a.engagement_id AND h.company_id=a.company_id))",[assignmentId,actor.companyId,input.expectedVersion]),
    write.prepare("UPDATE hr_payroll_cost_assignments SET evidence_status='verified',source='hr_attestation',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(now,assignmentId,actor.companyId,input.expectedVersion),
    write.prepare("INSERT INTO hr_payroll_cost_assignment_verifications(id,company_id,assignment_id,verified_by,evidence_ref,previous_version,created_at) VALUES(?,?,?,?,?,?,?)").bind(`cost-verification-${commandId}`,actor.companyId,assignmentId,actor.userId,input.evidenceRef,input.expectedVersion,now),
    write.prepare("SELECT set_config('erpx.payroll_cost_repair',?,true)").bind(assignmentId),
    write.prepare(`UPDATE hr_time_entries t SET cost_assignment_id=a.id,cost_branch_id=a.branch_id,cost_department_id=a.department_id,version=t.version+1,updated_at=?
      FROM hr_payroll_cost_assignments a WHERE a.id=? AND a.company_id=? AND t.engagement_id=a.engagement_id AND t.status='approved'
      AND t.work_date>=a.effective_from AND (a.effective_to IS NULL OR t.work_date<=a.effective_to)
      AND t.cost_assignment_id IS NULL`).bind(now,assignmentId,actor.companyId),
  ]);
}

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
  expectedVersion: z.number().int().positive().default(1),
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
  bankAccountId: z.string().min(1).optional(),
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
    bankTransactionId: z.string().min(1).optional(),
    bankVersion: z.number().int().positive().optional(),
  })).min(1),
});

export const payrollPostSchema = z.object({
  chartId: z.string().min(1).optional(),
  periodId: z.string().min(1).optional(),
  entryDate: date.optional(),
  accountMappingId: z.string().min(1).optional(),
  expenseAccountId: z.string().min(1).optional(),
  netPayableAccountId: z.string().min(1).optional(),
  taxPayableAccountId: z.string().min(1).optional(),
  employerContributionAccountId: z.string().min(1).optional(),
  employerExpenseAccountId: z.string().min(1).optional(),
  socialSecurityPayableAccountId: z.string().min(1).optional(),
  advanceReceivableAccountId: z.string().min(1).optional(),
  benefitPayableAccountId: z.string().min(1).optional(),
  otherDeductionPayableAccountId: z.string().min(1).optional(),
  serviceChargeExpenseAccountId: z.string().min(1).optional(),
  overtimeExpenseAccountId: z.string().min(1).optional(),
  bonusExpenseAccountId: z.string().min(1).optional(),
  expectedVersion: z.number().int().positive(),
});

export const payrollAccountMappingSchema = payrollPostSchema.omit({periodId:true,entryDate:true,expectedVersion:true,accountMappingId:true}).required({chartId:true,expenseAccountId:true,netPayableAccountId:true});

export const payrollCorrectionRequestSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
  attachmentSetIds: z.array(z.string().min(1)).max(20).default([]),
});
export const payrollCorrectionDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  action: z.enum(["approve", "reject", "cancel"]),
  reason: z.string().trim().min(3).max(2000),
});
export const finalPayPreparationSchema = z.object({
  taskId: z.string().min(1),
  countryPackId: z.string().min(1).optional(),
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
  actor = requirePermission(actor,"people.manage");
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

export async function getPerson(actor: WorkContext, personId: string, engagementId?: string) {
  const { write } = await holdingResources(actor.holdingId);
  const person = await directoryPerson(actor,personId,engagementId);
  const privateAccess = canReadCompensation(actor,person.user_id);
  if (!canManagePeople(actor) && !privateAccess) throw new ApiError(403,"PERSON_DETAILS_FORBIDDEN","Open the Holding directory to view this person's work contact details.");
  const compensation = privateAccess ? await write.prepare("SELECT base_amount_minor,hourly_rate_minor,pay_frequency,effective_from AS compensation_effective_from FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (effective_to IS NULL OR effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY effective_from DESC LIMIT 1").bind(person.engagement_id).first() : null;
  const [leave,time] = privateAccess ? await Promise.all([
    write.prepare("SELECT l.id,l.number,l.start_date,l.days,l.state,l.decision_note FROM leave_requests l JOIN employees e ON e.id=l.employee_id WHERE e.person_id=? AND l.company_id=? AND (?::text IS NULL OR l.branch_id=?) ORDER BY l.start_date DESC LIMIT 24").bind(personId,actor.companyId,actor.branchId,actor.branchId).all(),
    write.prepare("SELECT id,version,submitted_by,approved_by,work_date,regular_hours,overtime_hours,status,note,rejection_reason FROM hr_time_entries WHERE engagement_id=? ORDER BY work_date DESC LIMIT 24").bind(person.engagement_id).all(),
  ]) : [{results:[]},{results:[]}];
  if (!privateAccess) { person.phone=null; }
  const revisions=privateAccess?(await write.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? ORDER BY effective_from DESC,created_at DESC").bind(person.engagement_id).all()).results:[];
  const recommendedPay=privateAccess&&person.position_id?(await write.prepare("SELECT currency,monthly_amount_minor,version FROM position_pay_defaults WHERE company_id=? AND position_id=? ORDER BY version DESC LIMIT 1").bind(actor.companyId,person.position_id).first<{currency:string;monthly_amount_minor:number;version:number}>()):null;
  const recommendedLeave=person.level_id?(await write.prepare(`SELECT p.name,d.version,
    max(CASE WHEN t.code='annual' THEN r.grant_units END) annual_days,
    max(CASE WHEN t.code='sick' THEN r.grant_units END) sick_days
    FROM level_leave_defaults d JOIN leave_policy_versions p ON p.id=d.policy_version_id
    JOIN leave_policy_rules r ON r.policy_version_id=p.id JOIN leave_types t ON t.id=r.leave_type_id
    WHERE d.company_id=? AND d.level_id=? AND d.version=(SELECT max(x.version) FROM level_leave_defaults x WHERE x.company_id=d.company_id AND x.level_id=d.level_id)
    GROUP BY p.name,d.version`).bind(actor.companyId,person.level_id).first<{name:string;version:number;annual_days:number;sick_days:number}>()):null;
  const leaveAssignment=person.engagement_id?(await write.prepare(`SELECT p.name,p.version,a.effective_from,a.effective_to FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id WHERE a.company_id=? AND a.engagement_id=? AND a.effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (a.effective_to IS NULL OR a.effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY a.effective_from DESC LIMIT 1`).bind(actor.companyId,person.engagement_id).first<{name:string;version:number;effective_from:string;effective_to:string|null}>()):null;
  return {person:{...person,...compensation},leave:leave.results,time:time.results,privateAccess,revisions,recommendedPay,recommendedLeave,leaveAssignment};
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

export async function payrollSummary(actor:WorkContext,payMonth?:string) {
  actor=requirePermission(actor,"payroll.read");
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company payroll assignment for the payroll summary.");
  if(payMonth&&!/^\d{4}-(0[1-9]|1[0-2])$/.test(payMonth))throw new ApiError(422,"INVALID_PAY_MONTH","Use a calendar payroll month.");
  const {write}=await holdingResources(actor.holdingId);
  return (await write.prepare(`SELECT substring(r.pay_date FROM 1 FOR 7) pay_month,r.currency,
    COUNT(DISTINCT r.id) run_count,COUNT(DISTINCT i.engagement_id) headcount,
    SUM(i.gross_minor) gross_minor,SUM(i.deductions_minor) deductions_minor,SUM(i.employer_cost_minor) employer_cost_minor,SUM(i.net_minor) net_minor
    FROM hr_payroll_runs r JOIN hr_payroll_items i ON i.run_id=r.id
    WHERE r.company_id=? AND r.status='posted' AND (?::text IS NULL OR substring(r.pay_date FROM 1 FOR 7)=?)
    GROUP BY substring(r.pay_date FROM 1 FOR 7),r.currency ORDER BY pay_month DESC,r.currency LIMIT 120`)
    .bind(actor.companyId,payMonth??null,payMonth??null).all<{pay_month:string;currency:string;run_count:number;headcount:number;gross_minor:number;deductions_minor:number;employer_cost_minor:number;net_minor:number}>()).results;
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
  const base=decimalToMinor(input.baseAmount),hourly=decimalToMinor(input.hourlyRate),daily=decimalToMinor(input.dailyRate);for(const c of input.components)decimalToMinor(c.amount);if(new Set(input.components.map(c=>c.code)).size!==input.components.length)throw new ApiError(422,"DUPLICATE_COMPONENT","Each compensation component needs a distinct code.");
  const amounts=[{kind:"base",amount:base},{kind:"hourly_rate",amount:hourly},{kind:"daily_rate",amount:daily},...input.components.map(component=>({kind:`component:${component.code}`,amount:decimalToMinor(component.amount)}))];
  const snapshots=await Promise.all(amounts.map(amount=>snapshotMoney(write,actor,{sourceType:"hr_compensation_revision",sourceId:id,amountKind:amount.kind,transactionDate:input.effectiveFrom,currency:input.currency,currencyScale:2,amountUnits:String(amount.amount)})));
  const now=Date.now();return executeCommand(write,actor,key,"people.compensation.create",input,id,1,{id,status:"draft",version:1},[
    assertion(write,key,"engagement","EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND currency=?)",[input.engagementId,e.version,input.currency]),
    write.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,effective_to,pay_frequency,base_amount_minor,hourly_rate_minor,daily_rate_minor,standard_daily_hours,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)").bind(id,input.engagementId,input.effectiveFrom,input.effectiveTo??null,input.payFrequency,base,hourly,daily,input.standardDailyHours,input.expectedHours,input.currency,JSON.stringify(input.components),actor.userId,now,now),
    ...snapshots.map(snapshot=>snapshot.statement),
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
export const timeEntryUpdateSchema = timeEntrySchema.omit({engagementId:true}).extend({
  expectedVersion:z.number().int().positive(),
  attachmentSetIds:z.array(z.string().min(1)).max(20).default([]),
});
export const timeEntryReviewSchema = z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(500)});
export async function updateTimeEntry(actor:WorkContext,entryId:string,input:z.infer<typeof timeEntryUpdateSchema>,key:string){
  const {write}=await holdingResources(actor.holdingId);
  const previous=await replayCommand(write,actor,key,"people.time.update",entryId,input);if(previous)return previous;
  const entry=await write.prepare("SELECT t.*,e.start_date,e.end_date FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE t.id=? AND t.company_id=? AND t.submitted_by=? AND e.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(entryId,actor.companyId,actor.userId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<Record<string,string|number|null>>();
  if(!entry)throw new ApiError(404,"TIME_ENTRY_NOT_FOUND","Time entry not found in your editable records.");
  if(input.attachmentSetIds.length&&entry.status!=="rejected")throw new ApiError(409,"TIME_CORRECTION_EVIDENCE_STATE","Correction proof can be bound only while resubmitting a returned time entry.");
  if(input.regularHours+input.overtimeHours>24||input.regularHours+input.overtimeHours<=0)throw new ApiError(422,"INVALID_HOURS","Enter a positive total of at most 24 hours per day.");
  if(input.workDate<String(entry.start_date)||(entry.end_date&&input.workDate>String(entry.end_date)))throw new ApiError(422,"DATE_OUTSIDE_ENGAGEMENT","Work date must fall within the engagement.");
  const attachmentBinding=input.attachmentSetIds.length?await prepareAttachmentBinding(write,actor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:"time_entry",bindingType:"time_entry_correction",contexts:[{purposeKey:"corrected_entry_proof",componentKind:"action",componentRef:"correction-resubmission"}]},targetId:entryId,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:entryId}):null;
  return executeCommand(write,actor,key,"people.time.update",input,entryId,input.expectedVersion+1,{id:entryId,status:"submitted",version:input.expectedVersion+1},[
    ...(attachmentBinding?.guards??[]),
    assertion(write,key,"editable-time","EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND version=? AND status IN ('draft','submitted','rejected') AND submitted_by=?)",[entryId,input.expectedVersion,actor.userId]),
    write.prepare("UPDATE hr_time_entries SET work_date=?,regular_hours=?,overtime_hours=?,note=?,status='submitted',approved_by=NULL,rejection_reason=NULL,version=version+1,updated_at=? WHERE id=?").bind(input.workDate,input.regularHours,input.overtimeHours,input.note,Date.now(),entryId),
    ...(attachmentBinding?.steps??[]),
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
  const packId=`pack-${crypto.randomUUID()}`,now=Date.now();
  const recognized=await recognizedThaiPack(input.rules,input.countryCode,input.version).catch(error=>{if(error instanceof ApiError&&error.code==="PAYROLL_ADAPTER_RULES_UNSUPPORTED")return null;throw error;});
  const result={id:packId,countryCode:input.countryCode,version:input.version,status:"testing",recordVersion:1,adapterId:recognized?.adapterId??null};
  await executeCommand(write,actor,id,"people.country-pack.create",input,packId,1,result,[write.prepare("INSERT INTO hr_country_packs(id,country_code,version,status,rules_json,export_formats_json,created_by,created_at,updated_at,adapter_id,source_digest,effective_from,effective_to) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(packId,input.countryCode,input.version,"testing",JSON.stringify(input.rules),JSON.stringify(input.exportFormats),actor.userId,now,now,recognized?.adapterId??null,recognized?.sourceDigest??null,recognized?.rules.effectiveFrom??null,recognized?.rules.effectiveTo??null),receipt(write,id,"people.country-pack.create",c.digest,result)]); return result;
}

export async function listCountryPacks(actor: WorkContext){actor = requirePermission(actor,"payroll.configuration.read");const {write}=await holdingResources(actor.holdingId);return (await write.prepare("SELECT id,country_code,version,status,effective_from,effective_to,adapter_id,source_digest,record_version,export_formats_json,validated_by,updated_at FROM hr_country_packs ORDER BY country_code,version DESC").all()).results;}
export async function transitionCountryPack(actor: WorkContext,packId:string,input:z.infer<typeof countryPackTransitionSchema>,commandId:string){
  actor = requirePermission(actor,"people.compensation.approve");const {write}=await holdingResources(actor.holdingId);const prior=await replayCommand(write,actor,commandId,`people.country-pack.${input.status}`,packId,input);if(prior)return prior;
  const pack=await write.prepare("SELECT status,country_code,version,rules_json,created_by,record_version,source_digest,adapter_id FROM hr_country_packs WHERE id=?").bind(packId).first<{status:string;country_code:string;version:string;rules_json:string;created_by:string;record_version:number;source_digest:string|null;adapter_id:string|null}>();
  if(!pack)throw new ApiError(404,"COUNTRY_PACK_NOT_FOUND","Country pack not found.");
  if(pack.record_version!==input.expectedVersion)throw new ApiError(409,"COUNTRY_PACK_CHANGED","The rule pack changed. Reload before deciding.");
  if(samePerson(actor,pack.created_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different person must review the statutory rule pack.");
  const allowed=(pack.status==="testing"&&input.status==="validated")||(pack.status==="validated"&&input.status==="active")||(pack.status==="active"&&input.status==="superseded");if(!allowed)throw new ApiError(409,"INVALID_COUNTRY_PACK_TRANSITION","Country pack status must progress testing → validated → active.");
  const recognized=await recognizedThaiPack(JSON.parse(pack.rules_json),pack.country_code,pack.version);
  if(pack.source_digest!==recognized.sourceDigest||pack.adapter_id!==recognized.adapterId)throw new ApiError(409,"PAYROLL_RULE_EVIDENCE_MISSING","Create a new rule revision with its retained source digest.");
  const now=Date.now(),recordVersion=pack.record_version+1,result={id:packId,status:input.status,recordVersion};
  await executeCommand(write,actor,commandId,`people.country-pack.${input.status}`,input,packId,recordVersion,result,[assertion(write,commandId,"pack-status","EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status=? AND record_version=? AND source_digest=?)",[packId,pack.status,pack.record_version,pack.source_digest]),write.prepare("UPDATE hr_country_packs SET status=?,approval_evidence=?,validated_by=?,record_version=record_version+1,updated_at=? WHERE id=?").bind(input.status,input.proof,actor.userId,now,packId),write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),commandId,packId,`people.country-pack.${input.status}`,actor.userId,actor.assignmentId,pack.record_version,recognized.sourceDigest,now)]);
  return result;
}

async function calculateItem(write: SqlDatabase, run: PayrollStatutoryRun, engagement: { id: string; version:number; person_id: string; worker_type: string; employment_type: string; start_date: string; end_date: string | null; job_title: string },policy:{overtimeMultiplierBps:number;prorationMethod:"calendar_days"|"none"},useSimulationAdapter:boolean) {
  const payee=await write.prepare("SELECT id,version,legal_name,preferred_name FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL").bind(engagement.person_id,run.company_id).first<{id:string;version:number;legal_name:string;preferred_name:string|null}>();
  if(!payee)throw new ApiError(422,"PAYROLL_PAYEE_INVALID","Verify the employee identity before calculating payroll.");
  const compensation = await write.prepare(`SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1`).bind(engagement.id,run.period_end,run.period_start).first<Record<string, string | number | null>>();
  const time = await write.prepare("SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id").bind(engagement.id,run.period_start,run.period_end).all<{id:string;version:number;work_date:string;regular_hours:number;overtime_hours:number;cost_assignment_id:string|null;cost_branch_id:string|null;cost_department_id:string|null}>();
  const handoffs=(await write.prepare('SELECT id,request_id,request_version,minutes,work_date FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ? ORDER BY id').bind(engagement.id,run.period_start,run.period_end).all<{id:string;request_id:string;request_version:number;minutes:number;work_date:string}>()).results;
  const hours = {handoffs,regular:time.results.reduce((sum,t)=>sum+t.regular_hours,0),overtime:time.results.reduce((sum,t)=>sum+t.overtime_hours,0)+handoffs.reduce((sum,h)=>sum+h.minutes/60,0),entries:time.results};
  const base = Number(compensation?.base_amount_minor ?? 0); const hourly = Number(compensation?.hourly_rate_minor ?? 0); const regular = Number(hours?.regular ?? 0); const overtime = Number(hours?.overtime ?? 0);
  const daily=Number(compensation?.daily_rate_minor??0),dailyHours=Number(compensation?.standard_daily_hours??8),regularByDay=new Map<string,number>();
  for(const entry of time.results)regularByDay.set(entry.work_date,(regularByDay.get(entry.work_date)??0)+entry.regular_hours);
  if(engagement.employment_type==="daily"&&(daily<=0||[...regularByDay.values()].some(value=>value>dailyHours)))throw new ApiError(422,"DAILY_PAYROLL_TIME_INVALID","Daily pay requires a positive approved rate and regular hours within the reviewed daily schedule; classify additional hours as overtime.");
  const regularDays=[...regularByDay.values()].reduce((sum,value)=>sum+value/dailyHours,0);
  const components = compensation?.components_json ? JSON.parse(String(compensation.components_json)) as Array<{ code:string; label:string; kind:string; amount:string; taxable?:boolean; incomeTiming?:"regular"|"irregular"|"non_taxable"; socialSecurityWage?:boolean }> : [];
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
  const monthlyFinalSalary=run.mode==="final_pay"&&engagement.employment_type==="salaried"&&compensation?.pay_frequency==="monthly"&&!useSimulationAdapter;
  if(monthlyFinalSalary&&run.period_start.slice(0,7)!==run.period_end.slice(0,7))throw new ApiError(422,"FINAL_PAY_CALENDAR_MONTH_REQUIRED","Prepare monthly salary final pay within one calendar month using the reviewed calendar-day proration policy.");
  const day=86400000,periodDays=monthlyFinalSalary?new Date(Date.UTC(Number(run.period_start.slice(0,4)),Number(run.period_start.slice(5,7)),0)).getUTCDate():Math.floor((Date.parse(run.period_end)-Date.parse(run.period_start))/day)+1,activeStart=engagement.start_date>run.period_start?engagement.start_date:run.period_start,activeEnd=engagement.end_date&&engagement.end_date<run.period_end?engagement.end_date:run.period_end,activeDays=Math.floor((Date.parse(activeEnd)-Date.parse(activeStart))/day)+1,partial=activeDays!==periodDays;
  const costAssignments=(await write.prepare("SELECT id,effective_from,effective_to,branch_id,department_id,evidence_status,version FROM hr_payroll_cost_assignments WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from,id").bind(engagement.id,run.company_id,activeEnd,activeStart).all<{id:string;effective_from:string;effective_to:string|null;branch_id:string|null;department_id:string|null;evidence_status:string;version:number}>()).results;
  const assignmentOn=(workDate:string)=>costAssignments.filter(row=>row.effective_from<=workDate&&(row.effective_to===null||row.effective_to>=workDate));
  for(let dateMs=Date.parse(activeStart);dateMs<=Date.parse(activeEnd);dateMs+=day){const rows=assignmentOn(new Date(dateMs).toISOString().slice(0,10));if(rows.length!==1||rows[0].evidence_status!=="verified")throw new ApiError(422,"PAYROLL_COST_ALLOCATION_INCOMPLETE","Verify dated branch and department assignments for every work day before payroll review.");}
  for(const entry of time.results){const rows=assignmentOn(entry.work_date);if(rows.length!==1||rows[0].id!==entry.cost_assignment_id||rows[0].branch_id!==entry.cost_branch_id||rows[0].department_id!==entry.cost_department_id)throw new ApiError(422,"PAYROLL_TIME_COST_EVIDENCE_MISSING","Approve time again after its dated cost unit is verified.");}
  if(partial&&policy.prorationMethod==="none")throw new ApiError(422,"PRORATION_REQUIRED","This pay group does not permit automatic proration for partial-period employment.");
  const normalizedComponents = components.filter((component): component is {code:string;label:string;kind:"earning"|"deduction"|"employer";amount:string} => ["earning","deduction","employer"].includes(component.kind));
  const mockContext = useSimulationAdapter
    ? components
        .filter((component) => component.code.startsWith("MOCK_FIXTURE:") && component.kind === "ytd_context")
        .map((component) => ({
          ...component,
          label: `MOCK ONLY · ${component.label}`,
          amountMinor: decimalToMinor(component.amount),
          includedInTotals: false,
        }))
    : [];
  const payrollInput = { employmentType: engagement.employment_type as "salaried"|"daily"|"hourly"|"shift"|"contractor", baseMinor: base, hourlyMinor: hourly,dailyMinor:daily,regularDays, regularHours: regular, overtimeHours: overtime,overtimeMultiplierBps:policy.overtimeMultiplierBps,...(partial&&policy.prorationMethod==="calendar_days"?{prorationNumerator:activeDays,prorationDenominator:periodDays}:{}) };
  const mockComponents = useSimulationAdapter
    ? normalizedComponents
        .filter((component) => component.code.startsWith("MOCK_FIXTURE:"))
        .map((component) => ({ ...component, fixtureId: component.code }))
    : [];
  const ordinaryComponents = useSimulationAdapter
    ? normalizedComponents.filter((component) => !component.code.startsWith("MOCK_FIXTURE:"))
    : normalizedComponents;
  const grossCalculation = useSimulationAdapter
    ? calculateThaiHotelPayrollFixture({ payroll: { ...payrollInput, components: ordinaryComponents }, fixtures: mockComponents })
    : calculatePayrollAmounts({ ...payrollInput, components: ordinaryComponents });
  await assertPayrollRecoveryDeductionLimits(write,run,advanceRepayments,grossCalculation.gross,grossCalculation.deductions);
  const thaiEvidence=(run.mode==="production" || (run.mode==="final_pay" && run.country_pack_id))?await thaiEvidenceForEmployee(write,run,engagement.id):null;
  const calculated=thaiEvidence?applyThaiStatutory(grossCalculation,thaiEvidence,run,grossCalculation.overtimePay):grossCalculation;
  const { gross, deductions, employer, net } = calculated;
  const inputSnapshot={engagement,payee,compensation,hours,benefitInputs,advanceRepayments,costAssignments,...(thaiEvidence?{thaiStatutory:thaiEvidence}:{}),period:{start:run.period_start,end:run.period_end},policy:{...policy,activeDays,periodDays,...(useSimulationAdapter?{calculationAdapter:thaiHotelPayrollMockId,statutoryCalculation:false}:thaiEvidence?{calculationAdapter:"thai-payroll-v1",statutoryCalculation:true}:{})}};
  const result={grossMinor:gross,deductionsMinor:deductions,employerCostMinor:employer,netMinor:net,...("statutory" in calculated?{statutory:calculated.statutory}:{}),...("adapter" in calculated?{adapter:calculated.adapter}:{}),explanation:{baseMinor:base,hourlyRateMinor:hourly,dailyRateMinor:daily,standardDailyHours:dailyHours,regularDays,regularHours:regular,overtimeHours:overtime,overtimeMinor:grossCalculation.overtimePay,overtimeMultiplierBps:policy.overtimeMultiplierBps,activeDays,periodDays,components:[...calculated.applied,...mockContext]}};
  return { itemId:`payitem-${crypto.randomUUID()}`, engagementId:engagement.id, personId:engagement.person_id, workerType:engagement.worker_type, inputSnapshot, result, gross, deductions, employer, net };
}
export { calculateItem as calculatePayrollItemForSystem };

export async function createPayrollRun(actor:WorkContext,input:z.infer<typeof payrollRunSchema>,key:string) {
  actor = requirePermission(actor,"payroll.prepare");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");const {write}=await holdingResources(actor.holdingId),id=`payrun-${key}`;const previous=await replayCommand(write,actor,key,"people.payroll.create",id,input);if(previous)return previous;
  if(input.mode==="final_pay")throw new ApiError(422,"FINAL_PAY_CLEARANCE_REQUIRED","Prepare final payroll from the assigned HR clearance task.");
  const selectedPack=input.mode==="production"?await thaiPackForRun(write,{id,company_id:actor.companyId,pay_group_id:input.payGroupId,country_pack_id:input.countryPackId,period_start:input.periodStart,period_end:input.periodEnd,pay_date:input.payDate,currency:input.currency,mode:input.mode}):null;
  if(input.periodEnd<input.periodStart||input.payDate<input.periodEnd)throw new ApiError(422,"INVALID_PAYROLL_PERIOD","Review the period and pay date.");
  const now=Date.now();return executeCommand(write,actor,key,"people.payroll.create",input,id,1,{id,status:"calculating",mode:input.mode,version:1},[
    assertion(write,key,"pay-group","EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND currency=? AND status='active')",[input.payGroupId,actor.companyId,input.currency]),
    assertion(write,key,"duplicate-period","NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND mode!='final_pay' AND adjustment_of_run_id IS NULL AND status!='cancelled' AND period_start<=? AND period_end>=?)",[actor.companyId,input.payGroupId,input.periodEnd,input.periodStart]),
    ...(selectedPack?[assertion(write,key,"statutory-pack","EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status='active' AND record_version=? AND source_digest=?)",[selectedPack.row.id,selectedPack.row.record_version,selectedPack.sourceDigest])]:[]),
    write.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'calculating',?,?,?)").bind(id,actor.companyId,input.payGroupId,input.countryPackId??null,input.periodStart,input.periodEnd,input.payDate,input.currency,input.mode,actor.userId,now,now),
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
  const selectedPack=input.countryPackId?await thaiPackForRun(write,{id:runId,company_id:actor.companyId,pay_group_id:input.payGroupId,country_pack_id:input.countryPackId,period_start:input.periodStart,period_end:input.periodEnd,pay_date:input.payDate,currency:input.currency,mode:"final_pay"}):null;
  const now=Date.now(),result={id:prepId,taskId:input.taskId,runId,status:"prepared",version:1};
  return executeCommand(write,actor,key,"people.payroll.final-pay.prepare",input,prepId,1,result,[
    assertion(write,key,"task-version","EXISTS(SELECT 1 FROM people_preparation_tasks WHERE id=? AND company_id=? AND version=? AND status='pending' AND clearance_type='payroll' AND assignee_id=?)",[input.taskId,actor.companyId,input.expectedTaskVersion,actor.userId]),
    assertion(write,key,"engagement-pay-group","EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND pay_group_id=? AND currency=?)",[task.engagement_id,actor.companyId,input.payGroupId,input.currency]),
    assertion(write,key,"one-final-pay","NOT EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations WHERE task_id=? AND status!='cancelled')",[input.taskId]),
    ...(selectedPack?[assertion(write,key,"statutory-pack","EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status='active' AND record_version=? AND source_digest=?)",[selectedPack.row.id,selectedPack.row.record_version,selectedPack.sourceDigest])]:[]),
    write.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'final_pay','calculating',?,?,?)").bind(runId,actor.companyId,input.payGroupId,input.countryPackId??null,task.engagement_id,input.periodStart,input.periodEnd,input.payDate,input.currency,actor.userId,now,now),
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
  const run=await write.prepare("SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<{id:string;company_id:string;pay_group_id:string;period_start:string;period_end:string;pay_date:string;country_pack_id:string|null;currency:string;mode:string;status:string;version:number;adjustment_of_run_id:string|null}>();if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if(!["draft","calculating","preview","failed"].includes(run.status)||!["preview","final_pay","production"].includes(run.mode))throw new ApiError(409,"PAYROLL_LOCKED","Only an unapproved payroll preview can be recalculated.");
  // A run is company-wide; a branch preparer may not read or calculate other branches.
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a holding-level payroll assignment for company-wide runs.");
  if(run.adjustment_of_run_id)return calculatePayrollAdjustmentRun(write,actor,run,key);
  const engagements=await write.prepare("SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,e.end_date,e.job_title,e.branch_id,e.department_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id").bind(actor.companyId,run.pay_group_id,run.currency,run.period_end,run.period_start,(run as Record<string,unknown>).scope_engagement_id??null,(run as Record<string,unknown>).scope_engagement_id??null).all<{id:string;version:number;person_id:string;worker_type:string;employment_type:string;start_date:string;end_date:string|null;job_title:string;branch_id:string|null;department_id:string|null}>();
  if(!engagements.results.length)throw new ApiError(422,"PAYROLL_EMPTY","Assign active employees to this pay group before calculating.");
  const group=await write.prepare("SELECT overtime_multiplier_bps,proration_method FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(run.pay_group_id,actor.companyId).first<{overtime_multiplier_bps:number;proration_method:"calendar_days"|"none"}>();if(!group)throw new ApiError(409,"PAY_GROUP_CHANGED","The pay group is no longer active.");
  const policy={overtimeMultiplierBps:Number(group.overtime_multiplier_bps),prorationMethod:group.proration_method};
  const useSimulationAdapter=run.mode==="preview" && isThaiHotelPayrollMockEnabled(actor.holdingSlug);
  const items=await Promise.all(engagements.results.map(e=>calculateItem(write,run,e,policy,useSimulationAdapter)));
  if(items.some(i=>!i.inputSnapshot.compensation||i.inputSnapshot.compensation.currency!==run.currency))throw new ApiError(422,"PAYROLL_INPUT_INCOMPLETE","Every employee needs approved compensation in the pay-group currency covering the full period.");
  if(items.some(i=>i.net<0))throw new ApiError(422,"NEGATIVE_NET_PAY","Review deductions exceeding gross pay.");
  const total=items.reduce((a,i)=>({gross:a.gross+i.gross,deductions:a.deductions+i.deductions,employer:a.employer+i.employer,net:a.net+i.net}),{gross:0,deductions:0,employer:0,net:0});if(Object.values(total).some(v=>!Number.isSafeInteger(v)))throw new ApiError(422,"MONEY_OVERFLOW","Payroll exceeds supported precision.");
  const digest=await sha256(JSON.stringify(items.map(i=>i.inputSnapshot))),now=Date.now();
  return executeCommand(write,actor,key,"people.payroll.calculate",{},runId,run.version+1,{id:runId,status:"preview",version:run.version+1,totals:{...total,headcount:items.length}},[
    assertion(write,key,"run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND version=? AND status IN ('draft','calculating','preview','failed'))",[runId,run.version]),
    ...items.flatMap(i=>i.inputSnapshot.thaiStatutory?thaiEvidenceGuards(write,key,i.inputSnapshot.thaiStatutory,run,i.engagementId):[]),
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
export async function listPayrollRuns(actor: WorkContext) { const canReadPayroll=hasPermission(actor,"payroll.read"),canReadPayment=hasPermission(actor,"payroll.payment.read");if(!canReadPayroll&&!canReadPayment)requirePermission(actor,"payroll.read");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");const {write}=await holdingResources(actor.holdingId);if(!canReadPayroll)return (await write.prepare("SELECT r.id,r.period_start,r.period_end,r.pay_date,r.currency,r.status,r.version,b.id payment_batch_id,b.status payment_status,b.total_minor net_payable_minor FROM hr_payroll_runs r LEFT JOIN hr_payment_batches b ON b.source_type='payroll' AND b.source_id=r.id AND b.company_id=r.company_id AND b.status<>'cancelled' WHERE r.company_id=? AND r.status='posted' ORDER BY r.pay_date DESC,r.created_at DESC").bind(actor.companyId).all()).results;return (await write.prepare("SELECT * FROM hr_payroll_runs WHERE company_id=? ORDER BY pay_date DESC,created_at DESC").bind(actor.companyId).all()).results; }

export async function payrollPostingOptions(actor: WorkContext) {
  actor = requirePermission(actor,"finance.approve");
  if (actor.branchId || actor.departmentId) throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const { write } = await holdingResources(actor.holdingId);
  const [charts, periods, mappings, groups] = await Promise.all([
    write.prepare("SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status='active' ORDER BY name").bind(actor.companyId).all(),
    write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(actor.companyId).all(),
    write.prepare("SELECT id,pay_group_id,chart_id,accounts_json,status,version,created_by,approved_by FROM hr_payroll_account_mappings WHERE company_id=? AND status IN ('draft','approved') ORDER BY pay_group_id,created_at DESC").bind(actor.companyId).all(),
    write.prepare("SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND status='active' ORDER BY name").bind(actor.companyId).all(),
  ]);
  const chartIds = charts.results.map(row => String((row as Record<string,unknown>).id));
  const accounts = chartIds.length ? await write.prepare(`SELECT id,chart_id,code,name,category,currency FROM finance_accounts WHERE company_id=? AND status='active' AND chart_id IN (${chartIds.map(() => "?").join(",")}) ORDER BY code`).bind(actor.companyId,...chartIds).all() : { results: [] };
  return { charts: charts.results, periods: periods.results, accounts: accounts.results, mappings: mappings.results, groups: groups.results };
}

type ResolvedPayrollPost = z.infer<typeof payrollPostSchema> & {chartId:string;periodId:string;entryDate:string;expenseAccountId:string;netPayableAccountId:string;resolvedMappingId:string|null;resolvedMappingVersion:number|null};

async function resolvePayrollPost(write:SqlDatabase,actor:WorkContext,runId:string,input:z.infer<typeof payrollPostSchema>):Promise<ResolvedPayrollPost>{
  const run=await write.prepare("SELECT pay_group_id,period_end FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<{pay_group_id:string;period_end:string}>();
  if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  const mapping=await write.prepare(`SELECT id,chart_id,accounts_json,version FROM hr_payroll_account_mappings WHERE company_id=? AND pay_group_id=? AND status='approved' ${input.accountMappingId?"AND id=?":""} ORDER BY created_at DESC LIMIT 1`).bind(...(input.accountMappingId?[actor.companyId,run.pay_group_id,input.accountMappingId]:[actor.companyId,run.pay_group_id])).first<{id:string;chart_id:string;accounts_json:string;version:number}>();
  if(!mapping)throw new ApiError(409,"PAYROLL_ACCOUNT_MAPPING_REQUIRED","An independently approved pay-group account mapping is required before payroll posting.");
  const saved=JSON.parse(mapping.accounts_json) as z.infer<typeof payrollAccountMappingSchema>;
  if(input.chartId!==undefined&&input.chartId!==mapping.chart_id)throw new ApiError(422,"PAYROLL_MAPPING_OVERRIDE_FORBIDDEN","The chart must match the approved pay-group mapping.");
  for(const [name,value] of Object.entries(input))if(name.endsWith("AccountId")&&name!=="accountMappingId"&&value!==undefined&&value!==saved[name as keyof typeof saved])
    throw new ApiError(422,"PAYROLL_MAPPING_OVERRIDE_FORBIDDEN","Payroll accounts must match the approved pay-group mapping.");
  const merged={...saved,...input};
  const chartId=mapping.chart_id;
  if(!merged.expenseAccountId||!merged.netPayableAccountId)throw new ApiError(422,"PAYROLL_ACCOUNT_MAPPING_REQUIRED","The approved mapping needs wage expense and net payable accounts.");
  const dateHint=input.entryDate??run.period_end;
  let entryDate=dateHint,periodId=input.periodId;
  if(!periodId){
    let periods=(await write.prepare("SELECT id,start_date FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=? ORDER BY start_date").bind(actor.companyId,dateHint,dateHint).all<{id:string;start_date:string}>()).results;
    if(!periods.length&&!input.entryDate){
      const next=(await write.prepare("SELECT id,start_date FROM finance_periods WHERE company_id=? AND state='open' AND start_date>? ORDER BY start_date LIMIT 1").bind(actor.companyId,run.period_end).all<{id:string;start_date:string}>()).results;
      if(next.length){entryDate=next[0].start_date;periods=(await write.prepare("SELECT id,start_date FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?").bind(actor.companyId,entryDate,entryDate).all<{id:string;start_date:string}>()).results;}
    }
    if(periods.length!==1)throw new ApiError(409,"PAYROLL_ACCOUNTING_PERIOD_REQUIRED","Select one open accounting period for payroll accrual.");
    periodId=periods[0].id;
  }
  return {...merged,chartId,periodId,entryDate,expenseAccountId:merged.expenseAccountId,netPayableAccountId:merged.netPayableAccountId,resolvedMappingId:mapping.id,resolvedMappingVersion:mapping.version};
}

export async function savePayrollAccountMapping(actor:WorkContext,payGroupId:string,input:z.infer<typeof payrollAccountMappingSchema>,key:string){
  actor=requirePermission(actor,"finance.approve");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company finance assignment for payroll mapping.");
  const {write}=await holdingResources(actor.holdingId),id=`payroll-mapping-${key}`,previous=await replayCommand(write,actor,key,"people.payroll.mapping.save",id,input);if(previous)return previous;
  const group=await write.prepare("SELECT id,currency FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(payGroupId,actor.companyId).first<{id:string;currency:string}>();if(!group)throw new ApiError(404,"PAY_GROUP_NOT_FOUND","Active pay group not found.");
  const chart=await write.prepare("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(input.chartId,actor.companyId).first<{id:string;base_currency:string}>();if(!chart||chart.base_currency!==group.currency)throw new ApiError(422,"PAYROLL_CHART_CURRENCY","Payroll chart must use the pay-group currency.");
  const ids=[...new Set(Object.entries(input).filter(([name])=>name.endsWith("AccountId")).map(([,value])=>value).filter((value):value is string=>typeof value==="string"))];
  const accounts=await write.prepare(`SELECT id,currency,category FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${ids.map(()=>"?").join(",")})`).bind(actor.companyId,input.chartId,...ids).all<{id:string;currency:string;category:string}>();
  if(accounts.results.length!==ids.length||accounts.results.some(account=>account.currency!==group.currency))throw new ApiError(422,"PAYROLL_ACCOUNT_INVALID","Every mapped account must be active in the selected chart and currency.");
  const categories=new Map(accounts.results.map(row=>[row.id,row.category]));
  for(const [name,value] of Object.entries(input))if(name.endsWith("AccountId")&&typeof value==="string"){
    const required=name==="advanceReceivableAccountId"?"asset":name.endsWith("ExpenseAccountId")||name==="expenseAccountId"?"expense":"liability";
    if(categories.get(value)!==required)throw new ApiError(422,"PAYROLL_ACCOUNT_INVALID",`${name} must point to an active ${required} account.`);
  }
  const now=Date.now();return executeCommand(write,actor,key,"people.payroll.mapping.save",input,id,1,{id,payGroupId,status:"draft",version:1},[write.prepare("INSERT INTO hr_payroll_account_mappings(id,company_id,pay_group_id,chart_id,accounts_json,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'draft',?,?,?)").bind(id,actor.companyId,payGroupId,input.chartId,JSON.stringify(input),actor.userId,now,now)]);
}

export async function approvePayrollAccountMapping(actor:WorkContext,mappingId:string,expectedVersion:number,key:string){
  actor=requirePermission(actor,"finance.approve");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company finance assignment for payroll mapping.");
  const {write}=await holdingResources(actor.holdingId),input={expectedVersion},previous=await replayCommand(write,actor,key,"people.payroll.mapping.approve",mappingId,input);if(previous)return previous;
  const row=await write.prepare("SELECT id,pay_group_id,created_by,status,version FROM hr_payroll_account_mappings WHERE id=? AND company_id=?").bind(mappingId,actor.companyId).first<{id:string;pay_group_id:string;created_by:string;status:string;version:number}>();
  if(!row)throw new ApiError(404,"PAYROLL_MAPPING_NOT_FOUND","Payroll account mapping not found.");
  if(row.status!=="draft"||row.version!==expectedVersion)throw new ApiError(409,"PAYROLL_MAPPING_CHANGED","Refresh this mapping before approval.");
  if(samePerson(actor,row.created_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different Finance approver must approve the mapping.");
  const now=Date.now();return executeCommand(write,actor,key,"people.payroll.mapping.approve",input,mappingId,expectedVersion+1,{id:mappingId,status:"approved",version:expectedVersion+1},[
    assertion(write,key,"mapping-review","EXISTS(SELECT 1 FROM hr_payroll_account_mappings WHERE id=? AND company_id=? AND status='draft' AND version=? AND NOT iam_same_person(created_by,?,company_id))",[mappingId,actor.companyId,expectedVersion,actor.userId]),
    write.prepare("UPDATE hr_payroll_account_mappings SET status='superseded',version=version+1,updated_at=? WHERE company_id=? AND pay_group_id=? AND status='approved'").bind(now,actor.companyId,row.pay_group_id),
    write.prepare("UPDATE hr_payroll_account_mappings SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,now,mappingId),
  ]);
}

async function preparePayrollAccrual(write: SqlDatabase, actor: WorkContext, runId: string, requested: z.infer<typeof payrollPostSchema>, key: string) {
  const input=await resolvePayrollPost(write,actor,runId,requested);
  const run = await write.prepare(`SELECT r.*, COUNT(i.id) item_count,
      COUNT(i.id) FILTER (WHERE i.status NOT IN ('calculated','approved','posted')) invalid_items
    FROM hr_payroll_runs r LEFT JOIN hr_payroll_items i ON i.run_id=r.id
    WHERE r.id=? AND r.company_id=? GROUP BY r.id`).bind(runId, actor.companyId).first<Record<string,string|number|null>>();
  if (!run) throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if (String(run.status) !== "approved") throw new ApiError(409,"PAYROLL_NOT_APPROVED","An independently approved payroll run is required before GL posting.");
  if (Number(run.version) !== input.expectedVersion) throw new ApiError(409,"VERSION_CONFLICT","The payroll run changed. Refresh before posting.");
  if (Number(run.item_count) <= 0 || Number(run.invalid_items) > 0) throw new ApiError(409,"PAYROLL_ITEMS_INVALID","Every payroll item must be calculated or approved before posting.");
  await assertPayrollSourceFresh(write,runId);
  if (input.entryDate < String(run.period_end)) throw new ApiError(422,"PAYROLL_ACCRUAL_DATE_INVALID","Recognize payroll expense at period end or later in an open adjustment period.");
  const items = (await write.prepare("SELECT id,worker_type,gross_minor,deductions_minor,employer_cost_minor,net_minor,input_snapshot_json,result_json FROM hr_payroll_items WHERE run_id=? ORDER BY id").bind(runId).all<PayrollPostingItem & {worker_type:string}>()).results;
  if(items.some(item=>item.worker_type!=="employee"))throw new ApiError(409,"CONTRACTOR_PAYABLE_ROUTE","Contractor amounts must be handled through Finance payables, not posted as payroll.");
  const accrual = payrollAccountingLines(items,input);
  const expected = JSON.parse(String(run.totals_json)) as {gross:number;deductions:number;employer:number;net:number};
  if (accrual.totals.grossMinor !== expected.gross || accrual.totals.deductionsMinor !== expected.deductions || accrual.totals.employerMinor !== expected.employer || accrual.totals.netMinor !== expected.net)
    throw new ApiError(409,"PAYROLL_TOTALS_INVALID","Approved run totals changed after review.");
  const journal = await prepareJournal(write, actor, {
    chartId: input.chartId, periodId: input.periodId, entryDate: input.entryDate,
    currency: String(run.currency),
    memo: `Payroll ${String(run.period_start)} to ${String(run.period_end)}${input.entryDate > String(run.period_end) ? " · adjustment for closed work period" : ""}`,
    sourceType: "payroll", sourceId: runId, isOpening: false, lines: accrual.lines,
  }, key, "posted");
  const roles = new Map<string,string>();
  const setRole=(id:string,role:string)=>{const previous=roles.get(id);if(previous&&previous!==role)throw new ApiError(422,"PAYROLL_ACCOUNT_INVALID","An account cannot serve incompatible payroll expense, liability or asset roles.");roles.set(id,role);};
  for (const id of [input.expenseAccountId,input.employerExpenseAccountId,input.overtimeExpenseAccountId,input.bonusExpenseAccountId,input.serviceChargeExpenseAccountId].filter((id):id is string=>!!id)) setRole(id,"expense");
  for (const id of [input.netPayableAccountId,input.taxPayableAccountId,input.employerContributionAccountId,input.socialSecurityPayableAccountId,input.benefitPayableAccountId,input.otherDeductionPayableAccountId].filter((id):id is string=>!!id)) setRole(id,"liability");
  if(input.advanceReceivableAccountId)setRole(input.advanceReceivableAccountId,"asset");
  const ids=[...roles.keys()];
  const valid=await write.prepare(`SELECT id,category,currency FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${ids.map(()=>"?").join(",")})`).bind(actor.companyId,input.chartId,...ids).all<{id:string;category:string;currency:string}>();
  if(valid.results.length!==ids.length||valid.results.some(row=>row.category!==roles.get(row.id)||row.currency!==run.currency))throw new ApiError(422,"PAYROLL_ACCOUNT_INVALID","Payroll mapping needs active wage expense, payable liability, and advance asset accounts in the run currency.");
  return {run,accrual,journal,input};
}

export async function previewPayrollAccounting(actor: WorkContext, runId: string, input: z.infer<typeof payrollPostSchema>) {
  actor = requirePermission(actor,"finance.approve");
  if (actor.branchId || actor.departmentId) throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write}=await holdingResources(actor.holdingId);
  const {run,accrual,input:resolved}=await preparePayrollAccrual(write,actor,runId,input,`preview-${crypto.randomUUID()}`);
  return {runId,runVersion:Number(run.version),entryDate:resolved.entryDate,periodId:resolved.periodId,accountMappingId:resolved.resolvedMappingId,accountMappingVersion:resolved.resolvedMappingVersion,lines:accrual.lines,totals:accrual.totals,categories:accrual.categories};
}

export async function postPayroll(actor: WorkContext, runId: string, input: z.infer<typeof payrollPostSchema>, key: string) {
  actor = requirePermission(actor,"finance.approve");
  if (actor.branchId || actor.departmentId) throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(write, actor, key, "people.payroll.post", runId, input);
  if (previous) return previous;
  const {run,accrual,journal,input:resolved}=await preparePayrollAccrual(write,actor,runId,input,`${key}-journal`);
  if(run.mode==="production" || run.country_pack_id || (await runtimeEnv()).ERPX_ENVIRONMENT==="production")await assertStatutoryRunReady(write,runId);
  const moneySnapshot=(await snapshotMoney(write,actor,{sourceType:"payroll_run",sourceId:runId,amountKind:"net_payable",transactionDate:String(run.pay_date),currency:String(run.currency),currencyScale:2,amountUnits:String(accrual.totals.netMinor)})).statement;
  const payslips=await preparePayslipIssue(write,actor,runId,String(journal.result.id),key,input.expectedVersion+1);
  const deductions=await preparePayrollDeductionConsumption(write,actor,runId,String(journal.result.id),resolved.advanceReceivableAccountId,key);
  const now = Date.now(), result = { id: runId, status: "posted", version: input.expectedVersion + 1, journalId: journal.result.id, netPayableMinor: accrual.totals.netMinor,payslipIds:payslips.ids };
  return executeCommand(write, actor, key, "people.payroll.post", input, runId, input.expectedVersion + 1, result, [
    assertion(write, key, "payroll-run", "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND input_digest IS NOT NULL)", [runId, actor.companyId, input.expectedVersion]),
    ...(resolved.resolvedMappingId?[assertion(write,key,"payroll-mapping","EXISTS(SELECT 1 FROM hr_payroll_account_mappings WHERE id=? AND company_id=? AND pay_group_id=(SELECT pay_group_id FROM hr_payroll_runs WHERE id=?) AND status='approved' AND version=?)",[resolved.resolvedMappingId,actor.companyId,runId,resolved.resolvedMappingVersion])]:[]),
    ...deductions.guards,
    ...await payrollSourceGuards(write,key,runId),
    assertion(write, key, "payroll-items", "NOT EXISTS(SELECT 1 FROM hr_payroll_items WHERE run_id=? AND status NOT IN ('calculated','approved','posted'))", [runId]),
    assertion(write, key, "payroll-posting-unique", "NOT EXISTS(SELECT 1 FROM hr_payroll_postings WHERE run_id=?)", [runId]),
    ...payslips.guards,
    ...journal.steps,
    write.prepare("UPDATE hr_payroll_runs SET status='posted',posted_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='approved' AND version=?").bind(actor.userId, now, runId, actor.companyId, input.expectedVersion),
    write.prepare("UPDATE hr_payroll_items SET status='posted' WHERE run_id=? AND status IN ('calculated','approved')").bind(runId),
    write.prepare("INSERT INTO hr_payroll_postings(id,run_id,journal_id,account_mapping_id,account_mapping_version,created_at) VALUES(?,?,?,?,?,?)").bind(`payroll-posting-${key}`, runId, journal.result.id, resolved.resolvedMappingId, resolved.resolvedMappingVersion, now),
    ...deductions.steps,
    ...payslips.steps,
    ...(moneySnapshot?[moneySnapshot]:[]),
  ]);
}

export async function approvePayrollRun(actor:WorkContext,runId:string,expectedVersion:number,key:string) {
  actor = requirePermission(actor,"payroll.approve");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");const {write}=await holdingResources(actor.holdingId);const input={expectedVersion};const previous=await replayCommand(write,actor,key,"people.payroll.approve",runId,input);if(previous)return previous;
  const submitted=await write.prepare("SELECT submitted_by FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<{submitted_by:string|null}>();
  if(submitted?.submitted_by&&samePerson(actor,submitted.submitted_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different person must approve the submitted payroll revision.");
  await assertPayrollSourceFresh(write,runId);
  return executeCommand(write,actor,key,"people.payroll.approve",input,runId,expectedVersion+1,{id:runId,status:"approved",version:expectedVersion+1},[
    assertion(write,key,"run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status='in_review' AND submitted_by IS NOT NULL AND NOT iam_same_person(submitted_by,?,company_id) AND submitted_digest=input_digest AND input_digest IS NOT NULL)",[runId,actor.companyId,expectedVersion,actor.userId]),
    ...await payrollSourceGuards(write,key,runId),
    write.prepare("UPDATE hr_payroll_runs SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,Date.now(),runId),
    write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) SELECT ?,?,?,?,?,?,?,input_digest,? FROM hr_payroll_runs WHERE id=?").bind(crypto.randomUUID(),key,runId,"people.payroll.approve",actor.userId,actor.assignmentId,expectedVersion,Date.now(),runId),
  ]);
}

export async function requestPayrollCorrection(actor:WorkContext,runId:string,input:z.infer<typeof payrollCorrectionRequestSchema>,key:string){
  actor=requirePermission(actor,"payroll.prepare");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write}=await holdingResources(actor.holdingId),id=`payroll-correction-${key}`,previous=await replayCommand(write,actor,key,"people.payroll.correction.request",id,input);if(previous)return previous;
  const run=await write.prepare("SELECT id,status,version,revision,input_digest,totals_json FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<{id:string;status:string;version:number;revision:number;input_digest:string|null;totals_json:string}>();
  if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");if(run.status!=="approved"||run.version!==input.expectedVersion||!run.input_digest)throw new ApiError(409,"PAYROLL_CORRECTION_SOURCE_CHANGED","Only the current approved payroll revision can be returned for correction.");
  const attachmentBinding=input.attachmentSetIds.length?await prepareAttachmentBinding(write,actor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:"payroll_run",bindingType:"payroll_correction_request",contexts:[{purposeKey:"correction_support",componentKind:"action",componentRef:"correction-request"}]},targetId:runId,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:id}):null;
  const now=Date.now(),result={id,runId,status:"pending",version:1};
  return executeCommand(write,actor,key,"people.payroll.correction.request",input,id,1,result,[
    ...(attachmentBinding?.guards??[]),
    assertion(write,key,"approved-run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND revision=? AND input_digest=?)",[runId,actor.companyId,input.expectedVersion,run.revision,run.input_digest]),
    write.prepare("INSERT INTO hr_payroll_corrections(id,run_id,company_id,run_version,run_revision,input_digest,totals_json,reason,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(id,runId,actor.companyId,run.version,run.revision,run.input_digest,run.totals_json,input.reason,actor.userId,now,now),
    write.prepare("INSERT INTO hr_payroll_correction_events(id,correction_id,run_id,company_id,action,actor_id,run_version,evidence_json,created_at) VALUES(?,?,?,?, 'requested',?,?,?,?)").bind(`payroll-correction-event-${key}`,id,runId,actor.companyId,actor.userId,run.version,JSON.stringify({reason:input.reason,runRevision:run.revision,inputDigest:run.input_digest,totals:JSON.parse(run.totals_json)}),now),
    write.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT 'payroll-correction-review-'||md5(?||':'||a.user_id),?,a.user_id,'payroll_correction',?,1,'Approved payroll needs an independent correction decision',?
      FROM assignments a WHERE a.company_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'payroll.approve') AND NOT iam_same_person(a.user_id,?,a.company_id)
      GROUP BY a.user_id ON CONFLICT DO NOTHING`).bind(id,actor.companyId,id,now,actor.companyId,now,actor.userId),
    ...(attachmentBinding?.steps??[]),
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
  if(!run||!["preview","in_review","approved"].includes(run.status)||!run.input_digest)throw new ApiError(409,"PAYROLL_REVIEW_NOT_READY","Calculate a payroll preview before exporting review evidence.");
  await assertPayrollSourceFresh(write,runId);
  const existing=await write.prepare("SELECT checksum_sha256 FROM hr_payroll_review_exports WHERE run_id=? AND run_revision=?").bind(runId,run.revision).first<{checksum_sha256:string}>();
  if(existing)return {id:runId,revision:run.revision,checksum:existing.checksum_sha256};
  const items=await write.prepare("SELECT p.legal_name,i.worker_type,i.gross_minor,i.deductions_minor,i.employer_cost_minor,i.net_minor,i.currency,i.input_snapshot_json FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE i.run_id=? ORDER BY p.legal_name").bind(runId).all<{legal_name:string;worker_type:string;gross_minor:number;deductions_minor:number;employer_cost_minor:number;net_minor:number;currency:string;input_snapshot_json:string}>();
  const csv=["employee,worker_type,gross,deductions,employer_cost,net,currency,input_snapshot",...items.results.map(item=>[item.legal_name,item.worker_type,minorToDecimal(item.gross_minor),minorToDecimal(item.deductions_minor),minorToDecimal(item.employer_cost_minor),minorToDecimal(item.net_minor),item.currency,item.input_snapshot_json].map(payrollCsvCell).join(","))].join("\n"),checksum=await sha256(csv),fileKey=`${actor.holdingId}/people/payroll/${runId}/review-${run.revision}.csv`,exportId=`payroll-review-${key}`,now=Date.now();
  await files.put(fileKey,csv,{httpMetadata:{contentType:"text/csv"},customMetadata:{sha256:checksum,inputDigest:run.input_digest,period:`${run.period_start}:${run.period_end}`}});
  return executeCommand(write,actor,key,"people.payroll.review-export",{},runId,Number(run.revision),{id:runId,revision:run.revision,checksum},[
    assertion(write,key,"run","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND revision=? AND input_digest=? AND status IN ('preview','in_review','approved'))",[runId,actor.companyId,run.revision,run.input_digest]),
    ...await payrollSourceGuards(write,key,runId),
    write.prepare("INSERT INTO hr_payroll_review_exports(id,run_id,run_revision,file_key,checksum_sha256,created_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(exportId,runId,run.revision,fileKey,checksum,actor.userId,now),
  ]);
}

export async function getPayrollReviewFile(actor:WorkContext,runId:string,revision:number){
  actor=requirePermission(actor,"payroll.read");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write,files}=await holdingResources(actor.holdingId),row=await write.prepare("SELECT e.file_key,e.checksum_sha256 FROM hr_payroll_review_exports e JOIN hr_payroll_runs r ON r.id=e.run_id WHERE e.run_id=? AND e.run_revision=? AND r.company_id=?").bind(runId,revision,actor.companyId).first<{file_key:string;checksum_sha256:string}>();if(!row)throw new ApiError(404,"PAYROLL_REVIEW_EXPORT_NOT_FOUND","Payroll review export not found.");const object=await files.get(row.file_key);if(!object)throw new ApiError(503,"PAYROLL_REVIEW_EXPORT_UNAVAILABLE","Payroll review export is temporarily unavailable.");return {object,checksum:row.checksum_sha256};
}

export async function createPaymentBatch(actor: WorkContext, input: z.infer<typeof paymentBatchSchema>, commandId: string) {
  actor = requirePermission(actor,"payroll.payment.prepare"); const {write}=await holdingResources(actor.holdingId); const c=await command(write,actor,commandId,"people.payment-batch.create",input); if(c.result)return c.result;
  if(input.sourceType==="payroll"&&(actor.branchId||actor.departmentId))throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company assignment to prepare payroll payment.");
  if(input.sourceType==="payroll"&&(await runtimeEnv()).ERPX_ENVIRONMENT==="production")await assertStatutoryRunReady(write,input.sourceId);
  const duplicate = await write.prepare("SELECT id FROM hr_payment_batches WHERE company_id=? AND source_type=? AND source_id=? AND status NOT IN ('cancelled') LIMIT 1").bind(actor.companyId, input.sourceType, input.sourceId).first();
  if (duplicate) throw new ApiError(409, "PAYMENT_BATCH_EXISTS", "An outstanding payment batch already exists for this obligation.");
  let currency = "";
  let items: { results: Array<{ id: string; engagement_id: string | null; person_id: string | null; legal_name: string; preferred_name: string | null; net_minor: number; currency: string; input_snapshot_json?:string }> };
  if (input.sourceType === "payroll") {
    const source = await write.prepare("SELECT id,status,currency FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(input.sourceId,actor.companyId).first<{id:string;status:string;currency:string}>();
    if (!source || source.status !== "posted") throw new ApiError(409,"PAYROLL_NOT_POSTED","Payroll must be posted before payment export.");
    currency = source.currency;
    const posted=(await write.prepare("SELECT id,engagement_id,net_minor,currency,input_snapshot_json FROM hr_payroll_items WHERE run_id=? AND net_minor>0 AND status='posted' ORDER BY engagement_id").bind(input.sourceId).all<{id:string;engagement_id:string;net_minor:number;currency:string;input_snapshot_json:string}>()).results;
    items={results:posted.map(item=>{
      const snapshot=JSON.parse(item.input_snapshot_json) as {engagement?:{person_id:string};payee?:{id:string;legal_name:string;preferred_name:string|null}};
      const payee=snapshot.payee;
      if(!payee?.id||!payee.legal_name||payee.id!==snapshot.engagement?.person_id)throw new ApiError(409,"PAYROLL_PAYEE_REVIEW_REQUIRED","The posted payroll item lacks an approved employee payment identity.");
      return {...item,person_id:payee.id,legal_name:payee.legal_name,preferred_name:payee.preferred_name};
    })};
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
  if(input.sourceType==="payroll"&&input.bankAccountId){
    const bank=await write.prepare("SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND category='asset' AND currency=? AND status='active'").bind(input.bankAccountId,actor.companyId,currency).first();
    if(!bank)throw new ApiError(422,"BANK_ACCOUNT_INVALID","Select an active bank asset account in the payroll currency.");
  }
  const total=items.results.reduce((sum,item)=>sum+item.net_minor,0); if (total <= 0) throw new ApiError(422, "PAYMENT_TOTAL_INVALID", "Payment amount must be greater than zero."); const batchId=`payment-${crypto.randomUUID()}`; const now=Date.now(); const statements:SqlStatement[]=[
    ...(input.sourceType==="payroll"?[assertion(write,commandId,"posted-payroll-source","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted')",[input.sourceId,actor.companyId])]:[]),
    ...items.results.filter((item):item is typeof item & {input_snapshot_json:string}=>!!item.input_snapshot_json).map((item,index)=>assertion(write,commandId,`posted-payment-item-${index}`,"EXISTS(SELECT 1 FROM hr_payroll_items WHERE id=? AND run_id=? AND status='posted' AND net_minor=? AND input_snapshot_json=?)",[item.id,input.sourceId,item.net_minor,item.input_snapshot_json])),
    write.prepare("INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,status,format_code,total_minor,bank_account_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(batchId,actor.companyId,input.sourceType,input.sourceId,currency,"draft",input.formatCode,total,input.sourceType==="payroll"?input.bankAccountId??null:null,actor.userId,now,now)];
  if(input.sourceType==='contractor')statements.push(
    assertion(write,commandId,'supplier-payment-reservation',"NOT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=? AND status IN ('reserved','exported','unknown','confirmed'))",[input.sourceId]),
    assertion(write,commandId,'contractor-batch-unique',"NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE company_id=? AND source_type='contractor' AND source_id=? AND status<>'cancelled' AND id<>?)",[actor.companyId,input.sourceId,batchId]),
    write.prepare('UPDATE finance_invoices SET version=version+1,updated_at=? WHERE id=? AND company_id=?').bind(now,input.sourceId,actor.companyId),
  );
  for(const item of items.results){
    const reviewed=item.input_snapshot_json?(JSON.parse(item.input_snapshot_json) as {payee?:{legal_name:string;preferred_name:string|null}}).payee:null;
    statements.push(write.prepare("INSERT INTO hr_payment_batch_items(id,batch_id,engagement_id,person_id,reference,payee_name,amount_minor,currency) VALUES(?,?,?,?,?,?,?,?)").bind(`payitem-${crypto.randomUUID()}`,batchId,item.engagement_id,item.person_id,`PAY-${input.sourceId}-${item.id}`,reviewed?(reviewed.preferred_name??reviewed.legal_name):(item.preferred_name??item.legal_name),item.net_minor,item.currency));
  }
  const result={id:batchId,status:"draft",totalMinor:total,itemCount:items.results.length}; statements.push(receipt(write,commandId,"people.payment-batch.create",c.digest,result));
  await executeCommand(write,actor,commandId,"people.payment-batch.create",input,batchId,1,result,statements); return result;
}

export async function preparePaymentRetry(actor:WorkContext,batchId:string,expectedVersion:number,commandId:string){
  actor=requirePermission(actor,"payroll.payment.prepare");
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company assignment to retry payroll payments.");
  const {write}=await holdingResources(actor.holdingId),input={expectedVersion};
  const prior=await replayCommand(write,actor,commandId,"people.payment-batch.retry",batchId,input);if(prior)return prior;
  const batch=await write.prepare("SELECT source_type,source_id,status,version FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId,actor.companyId).first<{source_type:string;source_id:string;status:string;version:number}>();
  if(!batch)throw new ApiError(404,"PAYMENT_BATCH_NOT_FOUND","Payment batch not found.");
  if(batch.source_type!=="payroll"||batch.status!=="partially_settled"||batch.version!==expectedVersion)throw new ApiError(409,"PAYMENT_RETRY_NOT_READY","Refresh the partially settled payroll batch before preparing a retry.");
  if((await runtimeEnv()).ERPX_ENVIRONMENT==="production")await assertStatutoryRunReady(write,batch.source_id);
  const failed=await write.prepare("SELECT COUNT(*) count FROM hr_payment_batch_items WHERE batch_id=? AND status='failed'").bind(batchId).first<{count:number}>();
  if(!Number(failed?.count))throw new ApiError(409,"PAYMENT_RETRY_NOT_READY","Record at least one bank rejection before preparing a retry.");
  const now=Date.now();const result={id:batchId,status:"draft",version:expectedVersion+1,retryItems:Number(failed?.count??0)};
  return executeCommand(write,actor,commandId,"people.payment-batch.retry",input,batchId,result.version,result,[
    assertion(write,commandId,"batch-retry-version","EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND source_type='payroll' AND status='partially_settled' AND version=?)",[batchId,actor.companyId,expectedVersion]),
    assertion(write,commandId,"batch-retry-items","EXISTS(SELECT 1 FROM hr_payment_batch_items WHERE batch_id=? AND status='failed')",[batchId]),
    write.prepare("UPDATE hr_payment_batches SET status='draft',retry_requested_by=?,approved_by=NULL,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(actor.userId,now,batchId,actor.companyId,expectedVersion),
  ]);
}

export async function approvePaymentBatch(actor: WorkContext,batchId:string,expectedVersion:number,commandId:string){actor = requirePermission(actor,"finance.pay");const {write}=await holdingResources(actor.holdingId);const prior=await replayCommand(write,actor,commandId,"people.payment-batch.approve",batchId,{expectedVersion});if(prior)return prior;const batch=await write.prepare("SELECT status,version,source_type,created_by,retry_requested_by FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId,actor.companyId).first<{status:string;version:number;source_type:string;created_by:string;retry_requested_by:string|null}>();if(!batch)throw new ApiError(404,"PAYMENT_BATCH_NOT_FOUND","Payment batch not found.");if(batch.source_type==="payroll"&&(actor.branchId||actor.departmentId))throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company assignment to approve payroll payment.");if(samePerson(actor,batch.retry_requested_by??batch.created_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different person must approve the payment batch.");if(batch.status!=="draft"||batch.version!==expectedVersion)throw new ApiError(409,"PAYMENT_BATCH_NOT_APPROVABLE","Refresh the payment batch before approving it.");const now=Date.now();await executeCommand(write,actor,commandId,"people.payment-batch.approve",{expectedVersion},batchId,1,{id:batchId,status:"approved",version:expectedVersion+1},[assertion(write,commandId,"batch-version","EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND version=? AND status='draft' AND NOT iam_same_person(COALESCE(retry_requested_by,created_by),?,company_id))",[batchId,actor.companyId,expectedVersion,actor.userId]),write.prepare("UPDATE hr_payment_batches SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='draft' AND version=?").bind(actor.userId,now,batchId,actor.companyId,expectedVersion),write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),commandId,batchId,"people.payment-batch.approve",actor.userId,actor.assignmentId,expectedVersion,commandId,now)]);return {id:batchId,status:"approved",version:expectedVersion+1};}

export async function exportPaymentBatch(actor:WorkContext,batchId:string,commandId:string){return exportPaymentBatchV2(actor,batchId,commandId);}

async function exportPaymentBatchV2(actor:WorkContext,batchId:string,commandId:string){
  actor=requirePermission(actor,"payroll.payment.execute");
  const {write,files}=await holdingResources(actor.holdingId);
  const prior=await replayCommand(write,actor,commandId,"people.payment-batch.export",batchId,{});
  if(prior)return prior;
  const batch=await write.prepare("SELECT id,status,version,source_type,source_id,format_code,retry_requested_by FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId,actor.companyId).first<{id:string;status:string;version:number;source_type:string;source_id:string;format_code:string;retry_requested_by:string|null}>();
  if(!batch)throw new ApiError(404,"PAYMENT_BATCH_NOT_FOUND","Payment batch not found.");
  if(batch.source_type==="payroll"&&(actor.branchId||actor.departmentId))throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company assignment to export payroll payment.");
  if(batch.source_type==="payroll"&&(await runtimeEnv()).ERPX_ENVIRONMENT==="production")await assertStatutoryRunReady(write,batch.source_id);
  if(batch.status!=="approved")throw new ApiError(409,"PAYMENT_BATCH_LOCKED","An independently approved payment batch is required before export.");
  const eligibleStatus=batch.retry_requested_by?"failed":"outstanding";
  const items=(await write.prepare("SELECT id,reference,person_id,payee_name,amount_minor,currency,status FROM hr_payment_batch_items WHERE batch_id=? AND status=? ORDER BY reference").bind(batchId,eligibleStatus).all<{id:string;reference:string;person_id:string|null;payee_name:string;amount_minor:number;currency:string;status:string}>()).results;
  if(!items.length)throw new ApiError(409,"PAYMENT_ITEMS_EMPTY","No outstanding payment instructions remain.");
  const payrollItemGuards:SqlStatement[]=[];
  if(batch.source_type==="payroll"){
    for(const [index,item] of items.entries()){
      const posted=await write.prepare("SELECT id,engagement_id,net_minor,currency,input_snapshot_json FROM hr_payroll_items WHERE run_id=? AND status='posted' AND ?=('PAY-'||?||'-'||id)").bind(batch.source_id,item.reference,batch.source_id).first<{id:string;engagement_id:string;net_minor:number;currency:string;input_snapshot_json:string}>();
      if(!posted)throw new ApiError(409,"PAYROLL_PAYEE_REVIEW_REQUIRED","The payment instruction no longer matches a posted payroll item.");
      const snapshot=JSON.parse(posted.input_snapshot_json) as {payee?:{id:string;legal_name:string;preferred_name:string|null}};
      const payee=snapshot.payee;
      if(!payee?.id||item.person_id!==payee.id||item.payee_name!==(payee.preferred_name??payee.legal_name)||item.amount_minor!==posted.net_minor||item.currency!==posted.currency)throw new ApiError(409,"PAYROLL_PAYEE_REVIEW_REQUIRED","The payment instruction differs from the approved payroll identity or amount.");
      payrollItemGuards.push(assertion(write,commandId,`payment-item-${index}`,"EXISTS(SELECT 1 FROM hr_payment_batch_items b JOIN hr_payroll_items p ON p.run_id=? AND p.id=? WHERE b.id=? AND b.batch_id=? AND b.status=? AND b.reference=? AND b.person_id=? AND b.payee_name=? AND b.amount_minor=? AND b.currency=? AND p.status='posted' AND p.net_minor=? AND p.currency=? AND p.input_snapshot_json=?)",[batch.source_id,posted.id,item.id,batchId,eligibleStatus,item.reference,item.person_id,item.payee_name,item.amount_minor,item.currency,posted.net_minor,posted.currency,posted.input_snapshot_json]));
    }
  }
  const total=items.reduce((sum,item)=>sum+Number(item.amount_minor),0);
  if(!Number.isSafeInteger(total)||total<=0)throw new ApiError(422,"PAYMENT_TOTAL_INVALID","Export amount is invalid.");
  const csv=["reference,payee,amount,currency,status",...items.map(item=>[item.reference,item.payee_name,minorToDecimal(item.amount_minor),item.currency,"outstanding"].map(payrollCsvCell).join(","))].join("\n");
  const fileKey=actor.holdingId+"/people/payments/"+batchId+"/attempt-"+batch.version+".csv";
  const checksum=await sha256(csv),now=Date.now();
  await files.put(fileKey,csv,{httpMetadata:{contentType:"text/csv"},customMetadata:{sha256:checksum,format:batch.format_code,synthetic:"true"}});
  const result={id:batchId,status:"exported",fileKey,checksum,itemCount:items.length,totalMinor:total,synthetic:true};
  return executeCommand(write,actor,commandId,"people.payment-batch.export",{},batchId,batch.version+1,result,[
    assertion(write,commandId,"batch-export-version","EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND status='approved' AND version=?)",[batchId,actor.companyId,batch.version]),
    assertion(write,commandId,"batch-export-items","(SELECT COUNT(*) FROM hr_payment_batch_items WHERE batch_id=? AND status=?)=?",[batchId,eligibleStatus,items.length]),
    ...payrollItemGuards,
    ...(batch.source_type==="payroll"?[assertion(write,commandId,"posted-payroll-source","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted')",[batch.source_id,actor.companyId])]:[]),
    write.prepare("INSERT INTO hr_payment_batch_exports(id,company_id,batch_id,batch_version,file_key,checksum_sha256,item_count,total_minor,exported_by,exported_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind("payment-export-"+commandId,actor.companyId,batchId,batch.version,fileKey,checksum,items.length,total,actor.userId,now),
    write.prepare("UPDATE hr_payment_batches SET status='exported',file_key=?,file_checksum=?,exported_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(fileKey,checksum,now,now,batchId,actor.companyId,batch.version),
  ]);
}

export async function confirmPaymentBatch(actor:WorkContext,batchId:string,input:z.infer<typeof paymentConfirmationSchema>,commandId:string) {
 actor=requirePermission(actor,"finance.pay");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company finance assignment to confirm payroll payments.");const {write}=await holdingResources(actor.holdingId);
 const previous=await replayCommand(write,actor,commandId,"people.payment-batch.confirm",batchId,input);if(previous)return previous;
 const batch=await write.prepare("SELECT id,status,version,source_type,source_id,currency,bank_account_id FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId,actor.companyId).first<{id:string;status:string;version:number;source_type:string;source_id:string;currency:string;bank_account_id:string|null}>();
 if(!batch)throw new ApiError(404,"PAYMENT_BATCH_NOT_FOUND","Payment batch not found.");
 if(batch.source_type==="payroll"&&(await runtimeEnv()).ERPX_ENVIRONMENT==="production")await assertStatutoryRunReady(write,batch.source_id);
 if(!["exported","partially_settled"].includes(batch.status))throw new ApiError(409,"PAYMENT_BATCH_NOT_CONFIRMABLE","Export the payment batch first.");
 const items=(await write.prepare("SELECT id,reference,status,amount_minor,engagement_id FROM hr_payment_batch_items WHERE batch_id=?").bind(batchId).all<{id:string;reference:string;status:string;amount_minor:number;engagement_id:string|null}>()).results;
 if(new Set(input.rows.map(r=>r.reference)).size!==input.rows.length || input.rows.some(r=>!items.some(i=>i.reference===r.reference&&["outstanding","failed"].includes(i.status))))throw new ApiError(409,"PAYMENT_ITEMS_CHANGED","Review the current outstanding payment items.");
 const remaining=items.filter(i=>i.status!=="settled"&&!input.rows.some(r=>r.reference===i.reference&&r.status==="confirmed")).length;
 const status=remaining?"partially_settled":"settled",now=Date.now(),steps:SqlStatement[]=[assertion(write,commandId,"payment-batch-version","EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND version=? AND status=?)",[batchId,actor.companyId,batch.version,batch.status])];
 if(batch.source_type==="payroll"){
   const confirmed=input.rows.filter(row=>row.status==="confirmed");
   if(confirmed.length&&!batch.bank_account_id)throw new ApiError(422,"PAYROLL_BANK_ACCOUNT_REQUIRED","Select the bank ledger account before confirming payroll payments.");
   const posting=await write.prepare("SELECT p.journal_id,j.chart_id,r.pay_date FROM hr_payroll_postings p JOIN hr_payroll_runs r ON r.id=p.run_id JOIN finance_journals j ON j.id=p.journal_id WHERE p.run_id=? AND r.company_id=? AND j.status='posted'").bind(batch.source_id,actor.companyId).first<{journal_id:string;chart_id:string;pay_date:string}>();
   if(!posting)throw new ApiError(409,"PAYROLL_POSTING_MISSING","A posted payroll accrual is required before bank settlement.");
   const payables=(await write.prepare("SELECT DISTINCT account_id FROM finance_journal_lines WHERE journal_id=? AND description IN ('Net payroll payable','Net pay payable to employees')").bind(posting.journal_id).all<{account_id:string}>()).results;
   if(payables.length!==1)throw new ApiError(409,"PAYROLL_PAYABLE_MAPPING_MISSING","The posted payroll journal needs one net-payable control account.");
   const bankAccount=confirmed.length?await write.prepare("SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category='asset' AND status='active'").bind(batch.bank_account_id,actor.companyId,posting.chart_id,batch.currency).first():null;
   if(confirmed.length&&!bankAccount)throw new ApiError(422,"BANK_ACCOUNT_INVALID","The bank ledger account must be active in the payroll chart and currency.");
   if(new Set(confirmed.map(row=>row.bankTransactionId)).size!==confirmed.length)throw new ApiError(409,"BANK_EVIDENCE_REUSED","Each confirmed employee payment needs its own bank transaction.");
   for(const [index,row] of confirmed.entries()){
     if(!row.bankTransactionId||!row.bankVersion||!row.confirmationRef)throw new ApiError(422,"BANK_EVIDENCE_REQUIRED","Provide the exact bank transaction, version and reference for each confirmed payment.");
     const item=items.find(candidate=>candidate.reference===row.reference)!;
     const transaction=await write.prepare("SELECT t.id,t.version,t.amount_minor,t.currency,t.match_status,t.transaction_date,t.external_id,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?").bind(row.bankTransactionId,actor.companyId).first<{id:string;version:number;amount_minor:number;currency:string;match_status:string;transaction_date:string;external_id:string;bank_account:string}>();
     if(!transaction||transaction.version!==row.bankVersion||!['unmatched','proposed'].includes(transaction.match_status)||transaction.bank_account!==batch.bank_account_id||transaction.currency!==batch.currency||Number(transaction.amount_minor)!==-Number(item.amount_minor)||transaction.external_id!==row.confirmationRef||transaction.transaction_date<posting.pay_date)
       throw new ApiError(422,"BANK_EVIDENCE_MISMATCH","Match the bank account, exact debit, currency, reference, date and transaction version.");
     const periods=(await write.prepare("SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?").bind(actor.companyId,transaction.transaction_date,transaction.transaction_date).all<{id:string}>()).results;
     if(periods.length!==1)throw new ApiError(409,"PAYROLL_SETTLEMENT_PERIOD_REQUIRED","One open accounting period must contain the bank transaction date.");
     const payrollItem=await write.prepare("SELECT id,gross_minor,deductions_minor,employer_cost_minor,net_minor,input_snapshot_json,result_json FROM hr_payroll_items WHERE run_id=? AND engagement_id=? AND net_minor=?").bind(batch.source_id,item.engagement_id,item.amount_minor).first<PayrollPostingItem>();
     if(!payrollItem)throw new ApiError(409,"PAYROLL_PAYMENT_ITEM_CHANGED","The payable amount no longer matches the posted payroll item.");
     const payableLines=payrollNetCostShares(payrollItem).map(share=>({accountId:payables[0].account_id,debit:minorToDecimal(share.amountMinor),credit:"0",description:"Net payroll payable settled",dimensions:share.dimensions}));
     if(payableLines.reduce((sum,line)=>sum+decimalToMinor(line.debit),0)!==Number(item.amount_minor))throw new ApiError(409,"PAYROLL_COST_ALLOCATION_INVALID","The payable cost-unit allocation does not match the confirmed bank debit.");
     const journal=await prepareJournal(write,actor,{chartId:posting.chart_id,periodId:periods[0].id,entryDate:transaction.transaction_date,currency:batch.currency,memo:`Payroll payment ${item.reference} · ${row.confirmationRef}`,sourceType:"payroll_payment",sourceId:item.id,isOpening:false,lines:[...payableLines,{accountId:batch.bank_account_id!,debit:"0",credit:minorToDecimal(item.amount_minor),description:"Confirmed payroll bank debit",dimensions:{}}]},`${commandId}-settlement-${index}`,"posted");
     steps.push(assertion(write,commandId,`bank-transaction-${index}`,"EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND s.bank_account=? AND t.currency=? AND t.amount_minor=? AND t.external_id=?)",[transaction.id,actor.companyId,row.bankVersion,batch.bank_account_id,batch.currency,-item.amount_minor,row.confirmationRef]));
     steps.push(...journal.steps,write.prepare("INSERT INTO hr_payroll_payment_settlements(id,company_id,batch_id,batch_item_id,bank_transaction_id,journal_id,confirmed_by,confirmed_at) VALUES(?,?,?,?,?,?,?,?)").bind(`payroll-settlement-${commandId}-${index}`,actor.companyId,batchId,item.id,transaction.id,journal.result.id,actor.userId,now),write.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payroll_payment',matched_id=?,version=version+1 WHERE id=? AND version=?").bind(item.id,transaction.id,row.bankVersion));
   }
 }
 steps.push(...input.rows.map(row=>write.prepare("UPDATE hr_payment_batch_items SET status=?,confirmation_ref=?,settled_at=? WHERE batch_id=? AND reference=? AND status IN ('outstanding','failed')").bind(row.status==="confirmed"?"settled":"failed",row.confirmationRef??null,row.status==="confirmed"?now:null,batchId,row.reference)),write.prepare("UPDATE hr_payment_batches SET status=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(status,now,batchId,actor.companyId));
 return executeCommand(write,actor,commandId,"people.payment-batch.confirm",input,batchId,batch.version+1,{id:batchId,status,confirmed:input.rows.filter(r=>r.status==="confirmed").length,failed:input.rows.filter(r=>r.status==="failed").length},steps);
}

export async function importSyntheticPaymentResults(actor:WorkContext,batchId:string,csv:string,commandId:string){
  const parsed=parseSyntheticPayrollBankResultCsv(csv);
  return confirmPaymentBatch(actor,batchId,paymentConfirmationSchema.parse(parsed),commandId);
}

export async function getPaymentFile(actor: WorkContext,batchId:string){actor = requirePermission(actor,"payroll.payment.read");const {write,files}=await holdingResources(actor.holdingId);const batch=await write.prepare("SELECT file_key,file_checksum FROM hr_payment_batches WHERE id=? AND company_id=? AND status IN ('exported','partially_settled','settled')").bind(batchId,actor.companyId).first<{file_key:string|null;file_checksum:string|null}>();if(!batch?.file_key)throw new ApiError(404,"PAYMENT_FILE_NOT_FOUND","Payment export is not available.");const object=await files.get(batch.file_key);if(!object)throw new ApiError(503,"PAYMENT_FILE_UNAVAILABLE","Payment export is temporarily unavailable.");return {object,checksum:batch.file_checksum};}

export async function payrollRunsPage(actor:WorkContext,cursor?:string) {
 actor=requirePermission(actor,"payroll.read");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
 const {write}=await holdingResources(actor.holdingId);
 return readListPage<Record<string,string|number|null> & {id:string;page_key:string}>(write,{
  sql:"SELECT *,pay_date||':'||created_at::text page_key FROM hr_payroll_runs WHERE company_id=?",args:[actor.companyId],cursor,descending:true,scope:[actor.companyId,'payroll'],
 });
}
