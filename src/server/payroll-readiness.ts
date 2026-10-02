import "server-only";

import { z } from "zod";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import { holdingResources } from "./runtime";
import { isTransactionalSqlDatabase, type SqlDatabase, type SqlStatement } from "./runtime/types";
import { requirePermission } from "./iam-permissions";
import { sha256 } from "./crypto";
import { assertion, executeCommand, replayCommand } from "./commands";
import { payrollPeriodForDate, type PayrollFrequency } from "@/lib/payroll-schedule";

export const payrollScheduleSchema = z.object({
  enabled: z.boolean(),
  payDelayDays: z.number().int().min(1).max(45),
  anchorDate: z.string().date().nullable().optional(),
  expectedVersion: z.number().int().positive(),
}).strict();

export const payrollSubmitReviewSchema = z.object({
  expectedVersion: z.number().int().positive(),
  inputDigest: z.string().regex(/^[a-f0-9]{64}$/),
}).strict();

type Schedule = { id:string; company_id:string; currency:string; country_code:string; frequency:string; timezone:string; schedule_enabled:boolean; pay_delay_days:number; schedule_anchor_date:string|null; schedule_version:number };
type Run = { id:string; company_id:string; pay_group_id:string; country_pack_id:string|null; scope_engagement_id:string|null; period_start:string; period_end:string; pay_date:string; currency:string; mode:string; status:string; version:number; revision:number; input_digest:string|null; submitted_by:string|null; submitted_digest:string|null };
export type PayrollBlocker = {code:string; message:string; href?:string};

function localDate(now:Date, timezone:string) {
  try {
    const parts = new Intl.DateTimeFormat("en-CA",{timeZone:timezone,year:"numeric",month:"2-digit",day:"2-digit"}).formatToParts(now);
    const part=(type:string)=>parts.find(value=>value.type===type)?.value;
    return `${part("year")}-${part("month")}-${part("day")}`;
  } catch { throw new ApiError(422,"PAYROLL_TIMEZONE_INVALID","Set a valid pay group time zone."); }
}

function equal(a:unknown,b:unknown):boolean {
  const normalized=(value:unknown):unknown=>Array.isArray(value)?value.map(normalized):value&&typeof value==="object"?Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([key,entry])=>[key,normalized(entry)])):value;
  return JSON.stringify(normalized(a))===JSON.stringify(normalized(b));
}

async function costEvidenceBlockers(write:SqlDatabase,run:Pick<Run,"company_id"|"pay_group_id"|"currency"|"period_start"|"period_end"|"scope_engagement_id">):Promise<PayrollBlocker[]> {
  const engagements=(await write.prepare(`SELECT e.id,e.start_date,e.end_date,p.id person_id,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id
    WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended')
      AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?)
      AND (?::text IS NULL OR e.id=?) ORDER BY e.id`)
    .bind(run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id)
    .all<{id:string;start_date:string;end_date:string|null;person_id:string;legal_name:string}>()).results;
  const blockers:PayrollBlocker[]=[];
  for(const engagement of engagements){
    const assignments=(await write.prepare(`SELECT id,effective_from,effective_to,branch_id,department_id,evidence_status FROM hr_payroll_cost_assignments
      WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from,id`)
      .bind(engagement.id,run.company_id,run.period_end,run.period_start)
      .all<{id:string;effective_from:string;effective_to:string|null;branch_id:string|null;department_id:string|null;evidence_status:string}>()).results;
    const start=engagement.start_date>run.period_start?engagement.start_date:run.period_start;
    const end=engagement.end_date&&engagement.end_date<run.period_end?engagement.end_date:run.period_end;
    const assignmentOn=(date:string)=>assignments.filter(row=>row.effective_from<=date&&(row.effective_to===null||row.effective_to>=date));
    let complete=true;
    for(let time=Date.parse(start);time<=Date.parse(end);time+=86400000){
      const matches=assignmentOn(new Date(time).toISOString().slice(0,10));
      if(matches.length!==1||matches[0].evidence_status!=="verified"){complete=false;break;}
    }
    if(!complete){blockers.push({code:"COST_ASSIGNMENT_UNVERIFIED",message:`Verify dated branch and department assignment for ${engagement.legal_name}.`,href:`/people/${encodeURIComponent(engagement.person_id)}`});continue;}
    const timeEntries=(await write.prepare(`SELECT work_date,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries
      WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id`)
      .bind(engagement.id,run.period_start,run.period_end)
      .all<{work_date:string;cost_assignment_id:string|null;cost_branch_id:string|null;cost_department_id:string|null}>()).results;
    if(timeEntries.some(entry=>{const matches=assignmentOn(entry.work_date);return matches.length!==1||entry.cost_assignment_id!==matches[0].id||entry.cost_branch_id!==matches[0].branch_id||entry.cost_department_id!==matches[0].department_id;}))
      blockers.push({code:"TIME_COST_EVIDENCE_MISSING",message:`Approved time needs a dated cost unit for ${engagement.legal_name}.`,href:"/people/time"});
  }
  return blockers;
}

async function sourceBlockers(write:SqlDatabase,run:Run):Promise<PayrollBlocker[]> {
  if(!run.input_digest)return [{code:"PREVIEW_REQUIRED",message:"Calculate a payroll preview first."}];
  const rows=(await write.prepare("SELECT engagement_id,input_snapshot_json FROM hr_payroll_items WHERE run_id=? ORDER BY engagement_id").bind(run.id).all<{engagement_id:string;input_snapshot_json:string}>()).results;
  if(!rows.length)return [{code:"PAYROLL_EMPTY",message:"Add employees to this pay group before review."}];
  const snapshots=rows.map(row=>JSON.parse(row.input_snapshot_json) as Record<string,unknown>);
  if(await sha256(JSON.stringify(snapshots))!==run.input_digest)return [{code:"PREVIEW_DIGEST_CHANGED",message:"Payroll review evidence changed. Recalculate the preview."}];
  const current=(await write.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id").bind(run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id).all<{id:string}>()).results;
  if(!equal(current.map(row=>row.id),rows.map(row=>row.engagement_id)))return [{code:"PAYROLL_EMPLOYEES_CHANGED",message:"Employees in this pay group changed. Recalculate the preview."}];
  const group=await write.prepare("SELECT overtime_multiplier_bps,proration_method FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(run.pay_group_id,run.company_id).first<{overtime_multiplier_bps:number;proration_method:string}>();
  if(!group)return [{code:"PAY_GROUP_CHANGED",message:"The pay group is no longer active."}];
  const blockers:PayrollBlocker[]=[];
  for(let index=0;index<rows.length;index++){
    const snapshot=snapshots[index],engagementId=rows[index].engagement_id;
    const engagement=await write.prepare("SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,e.end_date,e.job_title,e.branch_id,e.department_id FROM hr_engagements e WHERE e.id=? AND e.company_id=?").bind(engagementId,run.company_id).first();
    const payee=await write.prepare("SELECT id,version,legal_name,preferred_name FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL").bind((engagement as {person_id?:string}|null)?.person_id??"",run.company_id).first();
    const compensation=await write.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1").bind(engagementId,run.period_end,run.period_start).first();
    const entries=(await write.prepare("SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id").bind(engagementId,run.period_start,run.period_end).all()).results;
    const activeStart=String((engagement as {start_date:string}|null)?.start_date??run.period_start)>run.period_start?String((engagement as {start_date:string}).start_date):run.period_start;
    const endDate=(engagement as {end_date?:string|null}|null)?.end_date;
    const activeEnd=endDate&&endDate<run.period_end?endDate:run.period_end;
    const costAssignments=(await write.prepare("SELECT id,effective_from,effective_to,branch_id,department_id,evidence_status,version FROM hr_payroll_cost_assignments WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from,id").bind(engagementId,run.company_id,activeEnd,activeStart).all()).results;
    const handoffs=(await write.prepare("SELECT id,request_id,request_version,minutes,work_date FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ? ORDER BY id").bind(engagementId,run.period_start,run.period_end).all()).results;
    const benefits=(await write.prepare("SELECT b.*,p.name plan_name FROM benefit_payroll_inputs b JOIN benefit_elections e ON e.id=b.election_id JOIN benefit_plans p ON p.id=e.plan_id WHERE b.engagement_id=? AND b.currency=? AND b.status='released' AND b.effective_from<=? AND (b.effective_to IS NULL OR b.effective_to>=?) ORDER BY b.id").bind(engagementId,run.currency,run.period_end,run.period_start).all()).results;
    const advances=(await write.prepare("SELECT r.id,r.version,r.amount_minor,r.due_period_start,r.due_period_end,a.reason FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id WHERE r.company_id=? AND a.engagement_id=? AND a.status='approved' AND r.status IN ('open','reserved') AND r.due_period_start<=? AND r.due_period_end>=? AND (r.payroll_run_id IS NULL OR r.payroll_run_id=?) ORDER BY r.sequence").bind(run.company_id,engagementId,run.period_end,run.period_start,run.id).all()).results;
    const hours=snapshot.hours as {entries:unknown;handoffs:unknown}|undefined;
    const policy=snapshot.policy as {overtimeMultiplierBps:number;prorationMethod:string}|undefined;
    if(!equal(snapshot.engagement,engagement)||!equal(snapshot.payee,payee)||!equal(snapshot.compensation,compensation)||!equal(hours?.entries,entries)||!equal(hours?.handoffs,handoffs)||!equal(snapshot.costAssignments,costAssignments)||!equal(snapshot.benefitInputs,benefits)||!equal(snapshot.advanceRepayments,advances)||!policy||Number(policy.overtimeMultiplierBps)!==Number(group.overtime_multiplier_bps)||policy.prorationMethod!==group.proration_method){
      blockers.push({code:"PAYROLL_SOURCE_CHANGED",message:"An employee's pay source changed. Recalculate the preview.",href:`/people/${encodeURIComponent(String((engagement as {person_id?:string}|null)?.person_id??""))}`});
      break;
    }
  }
  return blockers;
}

export async function assertPayrollSourceFresh(write:SqlDatabase,runId:string):Promise<void> {
  const run=await write.prepare("SELECT id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,version,revision,input_digest,submitted_by,submitted_digest FROM hr_payroll_runs WHERE id=?").bind(runId).first<Run>();
  if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if((await sourceBlockers(write,run)).length)throw new ApiError(409,"PAYROLL_SOURCE_CHANGED","Payroll inputs changed. Recalculate and submit this preview again.");
}

/** Source version checks are evaluated inside the same transaction as review. */
export async function payrollSourceGuards(write:SqlDatabase,key:string,runId:string):Promise<SqlStatement[]> {
  const run=await write.prepare("SELECT company_id,pay_group_id,period_start,period_end,currency,scope_engagement_id FROM hr_payroll_runs WHERE id=?").bind(runId).first<{company_id:string;pay_group_id:string;period_start:string;period_end:string;currency:string;scope_engagement_id:string|null}>();
  if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  const rows=(await write.prepare("SELECT engagement_id,input_snapshot_json FROM hr_payroll_items WHERE run_id=? ORDER BY engagement_id").bind(runId).all<{engagement_id:string;input_snapshot_json:string}>()).results;
  const guards:SqlStatement[]=[assertion(write,key,"source-employee-count",`(SELECT COUNT(*) FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?))=?`,[run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id,rows.length])];
  for(let index=0;index<rows.length;index++){
    const row=rows[index],snapshot=JSON.parse(row.input_snapshot_json) as {
      engagement:{version:number};compensation:{id:string;version:number}|null;
      payee:{id:string;version:number;legal_name:string;preferred_name:string|null};
      policy:{overtimeMultiplierBps:number;prorationMethod:string};
      hours:{entries:Array<{id:string;version:number;work_date:string;regular_hours:number;overtime_hours:number;cost_assignment_id:string|null;cost_branch_id:string|null;cost_department_id:string|null}>;handoffs:Array<{id:string;request_version:number;minutes:number;work_date:string}>};
      costAssignments:Array<{id:string;effective_from:string;effective_to:string|null;branch_id:string|null;department_id:string|null;evidence_status:string;version:number}>;
      benefitInputs:Array<{id:string;employee_deduction_minor:number;employer_cost_minor:number}>;
      advanceRepayments:Array<{id:string;version:number}>;
    };
    guards.push(assertion(write,key,`source-policy-${index}`,"EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active' AND overtime_multiplier_bps=? AND proration_method=?)",[run.pay_group_id,run.company_id,snapshot.policy.overtimeMultiplierBps,snapshot.policy.prorationMethod]));
    guards.push(assertion(write,key,`source-engagement-${index}`,"EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND pay_group_id=? AND currency=? AND worker_type='employee' AND version=? AND status IN ('active','ended'))",[row.engagement_id,run.company_id,run.pay_group_id,run.currency,snapshot.engagement.version]));
    if(!snapshot.payee?.id)throw new ApiError(409,"PAYROLL_PAYEE_REVIEW_REQUIRED","Recalculate and review employee payment identities.");
    guards.push(assertion(write,key,`source-payee-${index}`,"EXISTS(SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id WHERE e.id=? AND p.id=? AND p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND p.version=? AND p.legal_name=? AND p.preferred_name IS NOT DISTINCT FROM ?)",[row.engagement_id,snapshot.payee.id,run.company_id,snapshot.payee.version,snapshot.payee.legal_name,snapshot.payee.preferred_name]));
    if(snapshot.compensation)guards.push(assertion(write,key,`source-compensation-${index}`,"EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND engagement_id=? AND version=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?))",[snapshot.compensation.id,row.engagement_id,snapshot.compensation.version,run.period_end,run.period_start]));
    guards.push(assertion(write,key,`source-time-count-${index}`,"(SELECT COUNT(*) FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved')=?",[row.engagement_id,run.period_start,run.period_end,snapshot.hours.entries.length]));
    for(const entry of snapshot.hours.entries)guards.push(assertion(write,key,`source-time-${index}-${entry.id}`,"EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND engagement_id=? AND version=? AND status='approved' AND work_date=? AND regular_hours=? AND overtime_hours=? AND cost_assignment_id IS NOT DISTINCT FROM ? AND cost_branch_id IS NOT DISTINCT FROM ? AND cost_department_id IS NOT DISTINCT FROM ?)",[entry.id,row.engagement_id,entry.version,entry.work_date,entry.regular_hours,entry.overtime_hours,entry.cost_assignment_id,entry.cost_branch_id,entry.cost_department_id]));
    if(!Array.isArray(snapshot.costAssignments))throw new ApiError(409,"PAYROLL_COST_ALLOCATION_INCOMPLETE","Recalculate payroll with dated cost assignments.");
    guards.push(assertion(write,key,`source-cost-count-${index}`,"(SELECT COUNT(*) FROM hr_payroll_cost_assignments WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?))=?",[row.engagement_id,run.company_id,run.period_end,run.period_start,snapshot.costAssignments.length]));
    for(const assignment of snapshot.costAssignments)guards.push(assertion(write,key,`source-cost-${index}-${assignment.id}`,"EXISTS(SELECT 1 FROM hr_payroll_cost_assignments WHERE id=? AND engagement_id=? AND company_id=? AND status='active' AND evidence_status='verified' AND version=? AND effective_from=? AND effective_to IS NOT DISTINCT FROM ? AND branch_id IS NOT DISTINCT FROM ? AND department_id IS NOT DISTINCT FROM ?)",[assignment.id,row.engagement_id,run.company_id,assignment.version,assignment.effective_from,assignment.effective_to,assignment.branch_id,assignment.department_id]));
    guards.push(assertion(write,key,`source-handoff-count-${index}`,"(SELECT COUNT(*) FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ?)=?",[row.engagement_id,run.period_start,run.period_end,snapshot.hours.handoffs.length]));
    for(const handoff of snapshot.hours.handoffs)guards.push(assertion(write,key,`source-handoff-${index}-${handoff.id}`,"EXISTS(SELECT 1 FROM overtime_payroll_handoffs WHERE id=? AND engagement_id=? AND request_version=? AND minutes=? AND work_date=?)",[handoff.id,row.engagement_id,handoff.request_version,handoff.minutes,handoff.work_date]));
    guards.push(assertion(write,key,`source-benefits-count-${index}`,"(SELECT COUNT(*) FROM benefit_payroll_inputs WHERE engagement_id=? AND currency=? AND status='released' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?))=?",[row.engagement_id,run.currency,run.period_end,run.period_start,snapshot.benefitInputs.length]));
    for(const benefit of snapshot.benefitInputs)guards.push(assertion(write,key,`source-benefit-${index}-${benefit.id}`,"EXISTS(SELECT 1 FROM benefit_payroll_inputs WHERE id=? AND engagement_id=? AND status='released' AND employee_deduction_minor=? AND employer_cost_minor=?)",[benefit.id,row.engagement_id,benefit.employee_deduction_minor,benefit.employer_cost_minor]));
    guards.push(assertion(write,key,`source-advances-count-${index}`,"(SELECT COUNT(*) FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id WHERE r.company_id=? AND a.engagement_id=? AND a.status='approved' AND r.status IN ('open','reserved') AND r.due_period_start<=? AND r.due_period_end>=? AND (r.payroll_run_id IS NULL OR r.payroll_run_id=?))=?",[run.company_id,row.engagement_id,run.period_end,run.period_start,runId,snapshot.advanceRepayments.length]));
    for(const advance of snapshot.advanceRepayments)guards.push(assertion(write,key,`source-advance-${index}-${advance.id}`,"EXISTS(SELECT 1 FROM hr_payroll_advance_repayments WHERE id=? AND version=? AND status IN ('open','reserved'))",[advance.id,advance.version]));
  }
  return guards;
}

export async function payrollReadiness(actor:WorkContext,runId:string) {
  actor=requirePermission(actor,"payroll.read");
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write}=await holdingResources(actor.holdingId);
  const run=await write.prepare("SELECT id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,version,revision,input_digest,submitted_by,submitted_digest FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<Run>();
  if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  const today=localDate(new Date(),(await write.prepare("SELECT timezone FROM hr_pay_groups WHERE id=? AND company_id=?").bind(run.pay_group_id,actor.companyId).first<{timezone:string}>())?.timezone??"UTC");
  const blockers:PayrollBlocker[]=[];
  if(today<=run.period_end)blockers.push({code:"PERIOD_OPEN",message:"Payroll preview is available after the work period closes."});
  if(run.pay_date<=run.period_end)blockers.push({code:"PAY_DATE_INVALID",message:"Pay date must follow the work period."});
  const employees=await write.prepare("SELECT COUNT(*) AS count FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?)").bind(actor.companyId,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id).first<{count:number}>();
  if(Number(employees?.count??0)===0)blockers.push({code:"PAYROLL_EMPTY",message:"Assign employees to this pay group before calculating."});
  if(run.mode!=="final_pay"){
    const contractors=await write.prepare("SELECT COUNT(*) AS count FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.worker_type='contractor' AND e.status IN ('active','ended') AND p.status='active' AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?)").bind(actor.companyId,run.pay_group_id,run.period_end,run.period_start).first<{count:number}>();
    if(Number(contractors?.count??0)>0)blockers.push({code:"CONTRACTOR_PAYABLE_ROUTE",message:"Move contractors to the Finance payable flow before submitting this payroll."});
  }
  const missing=(await write.prepare("SELECT e.id,p.legal_name,p.id person_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) AND NOT EXISTS(SELECT 1 FROM hr_compensation_revisions c WHERE c.engagement_id=e.id AND c.status IN ('active','approved') AND c.currency=? AND c.effective_from<=? AND (c.effective_to IS NULL OR c.effective_to>=?)) ORDER BY e.id LIMIT 20").bind(actor.companyId,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id,run.currency,run.period_start,run.period_end).all<{id:string;legal_name:string;person_id:string}>()).results;
  for(const row of missing)blockers.push({code:"COMPENSATION_MISSING",message:`Approved compensation is missing for ${row.legal_name}.`,href:`/people/${encodeURIComponent(row.person_id)}`});
  const missingOvertimeRates=(await write.prepare(`SELECT p.id person_id,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id
    JOIN LATERAL (SELECT c.hourly_rate_minor FROM hr_compensation_revisions c WHERE c.engagement_id=e.id
      AND c.status IN ('active','approved') AND c.currency=? AND c.effective_from<=?
      AND (c.effective_to IS NULL OR c.effective_to>=?) ORDER BY c.effective_from DESC LIMIT 1) c ON TRUE
    WHERE e.company_id=? AND e.pay_group_id=? AND e.employment_type='salaried' AND e.worker_type='employee'
      AND e.status IN ('active','ended') AND p.status='active' AND e.start_date<=?
      AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?)
      AND COALESCE(c.hourly_rate_minor,0)<=0 AND (EXISTS(SELECT 1 FROM hr_time_entries t WHERE t.engagement_id=e.id
        AND t.work_date BETWEEN ? AND ? AND t.status='approved' AND t.overtime_hours>0)
        OR EXISTS(SELECT 1 FROM overtime_payroll_handoffs h WHERE h.engagement_id=e.id
        AND h.work_date BETWEEN ? AND ? AND h.minutes>0)) ORDER BY e.id LIMIT 20`)
    .bind(run.currency,run.period_start,run.period_end,actor.companyId,run.pay_group_id,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id,run.period_start,run.period_end,run.period_start,run.period_end).all<{person_id:string;legal_name:string}>()).results;
  for(const row of missingOvertimeRates)blockers.push({code:"OVERTIME_RATE_MISSING",message:`Approved hourly rate is missing for ${row.legal_name}'s overtime.`,href:`/people/${encodeURIComponent(row.person_id)}`});
  const pendingTime=await write.prepare("SELECT COUNT(*) AS count FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE e.company_id=? AND e.pay_group_id=? AND t.work_date BETWEEN ? AND ? AND t.status IN ('draft','submitted')").bind(actor.companyId,run.pay_group_id,run.period_start,run.period_end).first<{count:number}>();
  if(Number(pendingTime?.count??0)>0)blockers.push({code:"TIME_PENDING",message:"Approve or reject time entries in this period before payroll review.",href:"/people/time"});
  blockers.push(...await costEvidenceBlockers(write,run));
  const source=await sourceBlockers(write,run);
  const sourceFresh=source.length===0;
  if(run.input_digest)blockers.push(...source);
  const readyToCalculate=!blockers.some(item=>["PERIOD_OPEN","PAY_DATE_INVALID","PAYROLL_EMPTY","COMPENSATION_MISSING","OVERTIME_RATE_MISSING","TIME_PENDING","CONTRACTOR_PAYABLE_ROUTE","COST_ASSIGNMENT_UNVERIFIED","TIME_COST_EVIDENCE_MISSING"].includes(item.code));
  const readyToSubmit=readyToCalculate&&sourceFresh&&run.status==="preview";
  const readyToApprove=readyToCalculate&&sourceFresh&&run.status==="in_review"&&!!run.submitted_by&&run.submitted_digest===run.input_digest;
  return {runId,status:run.status,version:run.version,inputDigest:run.input_digest,readyToCalculate,readyToSubmit,readyToApprove,sourceFresh,blockers};
}

export async function submitPayrollReview(actor:WorkContext,runId:string,input:z.infer<typeof payrollSubmitReviewSchema>,key:string) {
  actor=requirePermission(actor,"payroll.prepare");
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write}=await holdingResources(actor.holdingId);
  const prior=await replayCommand(write,actor,key,"people.payroll.submit-review",runId,input);if(prior)return prior;
  const run=await write.prepare("SELECT id,status,version,input_digest FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<{id:string;status:string;version:number;input_digest:string|null}>();
  if(!run)throw new ApiError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if(run.status!=="preview"||run.version!==input.expectedVersion||run.input_digest!==input.inputDigest)throw new ApiError(409,"PAYROLL_REVIEW_CHANGED","Refresh the payroll preview before submitting review.");
  const readiness=await payrollReadiness(actor,runId);
  if(!readiness.readyToSubmit)throw new ApiError(409,"PAYROLL_NOT_READY",readiness.blockers[0]?.message??"Resolve payroll readiness issues first.");
  const now=Date.now(),result={id:runId,status:"in_review",version:run.version+1,submittedBy:actor.userId,inputDigest:input.inputDigest};
  return executeCommand(write,actor,key,"people.payroll.submit-review",input,runId,run.version+1,result,[
    assertion(write,key,"preview","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='preview' AND version=? AND input_digest=?)",[runId,actor.companyId,input.expectedVersion,input.inputDigest]),
    ...await payrollSourceGuards(write,key,runId),
    write.prepare("UPDATE hr_payroll_runs SET status='in_review',submitted_by=?,submitted_digest=?,submitted_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='preview' AND version=?").bind(actor.userId,input.inputDigest,now,now,runId,actor.companyId,input.expectedVersion),
  ]);
}

export async function getPayrollSchedule(actor:WorkContext,groupId:string){
  actor=requirePermission(actor,"payroll.read");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write}=await holdingResources(actor.holdingId);
  const group=await write.prepare("SELECT id,frequency,timezone,currency,schedule_enabled,pay_delay_days,schedule_anchor_date,schedule_version FROM hr_pay_groups WHERE id=? AND company_id=?").bind(groupId,actor.companyId).first<Schedule>();
  if(!group)throw new ApiError(404,"PAY_GROUP_NOT_FOUND","Pay group not found.");
  return {id:group.id,frequency:group.frequency,timezone:group.timezone,currency:group.currency,enabled:group.schedule_enabled,payDelayDays:group.pay_delay_days,anchorDate:group.schedule_anchor_date,version:group.schedule_version};
}

export async function savePayrollSchedule(actor:WorkContext,groupId:string,input:z.infer<typeof payrollScheduleSchema>,key:string){
  actor=requirePermission(actor,"payroll.prepare");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  const {write}=await holdingResources(actor.holdingId),prior=await replayCommand(write,actor,key,"people.payroll.schedule",groupId,input);if(prior)return prior;
  const group=await write.prepare("SELECT id,frequency,schedule_version FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(groupId,actor.companyId).first<{id:string;frequency:string;schedule_version:number}>();
  if(!group)throw new ApiError(404,"PAY_GROUP_NOT_FOUND","Active pay group not found.");
  if(group.schedule_version!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Pay group schedule changed. Refresh before saving.");
  if(input.enabled&&["weekly","fortnightly"].includes(group.frequency)&&!input.anchorDate)throw new ApiError(422,"PAYROLL_ANCHOR_REQUIRED","Set an anchor date for weekly or fortnightly payroll.");
  const result={id:groupId,enabled:input.enabled,payDelayDays:input.payDelayDays,anchorDate:input.anchorDate??null,version:group.schedule_version+1};
  return executeCommand(write,actor,key,"people.payroll.schedule",input,groupId,result.version,result,[
    assertion(write,key,"pay-group","EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active' AND schedule_version=?)",[groupId,actor.companyId,input.expectedVersion]),
    write.prepare("UPDATE hr_pay_groups SET schedule_enabled=?,pay_delay_days=?,schedule_anchor_date=?,schedule_version=schedule_version+1,updated_at=? WHERE id=? AND company_id=? AND schedule_version=?").bind(input.enabled,input.payDelayDays,input.anchorDate??null,Date.now(),groupId,actor.companyId,input.expectedVersion),
  ]);
}

/** Called by the native scheduled worker; never approves, posts, or pays. */
export async function drainPayrollSchedules(write:SqlDatabase,now=new Date()):Promise<number>{
  const groups=(await write.prepare("SELECT id,company_id,currency,country_code,frequency,timezone,schedule_enabled,pay_delay_days,schedule_anchor_date,schedule_version FROM hr_pay_groups WHERE status='active' AND schedule_enabled=TRUE ORDER BY id").all<Schedule>()).results;
  let created=0;
  for(const group of groups){
    const today=localDate(now,group.timezone),period=payrollPeriodForDate({...group,frequency:group.frequency as PayrollFrequency},today);
    const existing=await write.prepare("SELECT id FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND period_start=? AND period_end=? AND mode!='final_pay' LIMIT 1").bind(group.company_id,group.id,period.periodStart,period.periodEnd).first();
    if(existing)continue;
    const timestamp=now.getTime();
    const id=`payrun-scheduled-${crypto.randomUUID()}`;
    const result=await write.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) SELECT ?,?,?,NULL,?,?,?,?,'preview','draft','system:payroll-schedule',?,? WHERE NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND period_start=? AND period_end=? AND mode!='final_pay') ON CONFLICT DO NOTHING").bind(id,group.company_id,group.id,period.periodStart,period.periodEnd,period.payDate,group.currency,timestamp,timestamp,group.company_id,group.id,period.periodStart,period.periodEnd).run();
    if(result.meta.changes>0)created++;
  }
  return created;
}

/** Preview due system drafts in a serializable transaction under a system audit identity. */
export async function drainPayrollScheduledPreviews(write:SqlDatabase,now=new Date()):Promise<number>{
  if(!isTransactionalSqlDatabase(write))throw new Error("PAYROLL_TRANSACTION_REQUIRED");
  const due=(await write.prepare(`SELECT r.id,g.timezone FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id
    WHERE r.created_by='system:payroll-schedule' AND r.mode='preview' AND r.status IN ('draft','failed')
      AND g.status='active' AND g.schedule_enabled=TRUE AND (r.status='draft' OR r.updated_at<?)
    ORDER BY r.period_end,r.id LIMIT 25`).bind(now.getTime()-600_000).all<{id:string;timezone:string}>()).results;
  let calculated=0;
  for(const candidate of due){
    try{
      const completed=await write.transaction(async tx=>{
        const run=await tx.prepare(`SELECT r.id,r.company_id,r.pay_group_id,r.period_start,r.period_end,r.pay_date,r.currency,r.status,r.version,r.revision,g.timezone,
          g.overtime_multiplier_bps,g.proration_method FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id
          WHERE r.id=? AND r.mode='preview' AND r.created_by='system:payroll-schedule' AND r.status IN ('draft','failed')
            AND g.status='active' AND g.schedule_enabled=TRUE FOR UPDATE OF r SKIP LOCKED`).bind(candidate.id).first<{
          id:string;company_id:string;pay_group_id:string;period_start:string;period_end:string;pay_date:string;currency:string;
          status:string;version:number;revision:number;timezone:string;overtime_multiplier_bps:number;proration_method:"calendar_days"|"none";
        }>();
        if(!run||localDate(now,run.timezone)<=run.period_end||run.pay_date<=run.period_end)return false;
        const pending=await tx.prepare(`SELECT COUNT(*) AS count FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id
          WHERE e.company_id=? AND e.pay_group_id=? AND t.work_date BETWEEN ? AND ? AND t.status IN ('draft','submitted')`).bind(run.company_id,run.pay_group_id,run.period_start,run.period_end).first<{count:number}>();
        if(Number(pending?.count??0)>0)return false;
        const contractors=await tx.prepare(`SELECT COUNT(*) AS count FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id
          WHERE e.company_id=? AND e.pay_group_id=? AND e.worker_type='contractor' AND e.status IN ('active','ended')
            AND p.status='active' AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?)`).bind(run.company_id,run.pay_group_id,run.period_end,run.period_start).first<{count:number}>();
        if(Number(contractors?.count??0)>0)return false;
        const engagements=(await tx.prepare(`SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,e.end_date,e.job_title,e.branch_id,e.department_id
          FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=?
            AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL
            AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) ORDER BY e.id`).bind(run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start).all<{
          id:string;version:number;person_id:string;worker_type:string;employment_type:string;start_date:string;end_date:string|null;
          job_title:string;branch_id:string|null;department_id:string|null;
        }>()).results;
        if(!engagements.length)return false;
        const incomplete=await tx.prepare(`SELECT COUNT(*) AS count FROM hr_engagements e WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=?
          AND e.worker_type='employee' AND e.status IN ('active','ended') AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?)
          AND NOT EXISTS(SELECT 1 FROM hr_compensation_revisions c WHERE c.engagement_id=e.id AND c.currency=?
            AND c.status IN ('active','approved') AND c.effective_from<=? AND (c.effective_to IS NULL OR c.effective_to>=?))`).bind(run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start,run.currency,run.period_start,run.period_end).first<{count:number}>();
        if(Number(incomplete?.count??0)>0)return false;
        if((await costEvidenceBlockers(tx,{...run,scope_engagement_id:null})).length)return false;
        const {calculatePayrollItemForSystem}=await import("./people-payroll");
        const policy={overtimeMultiplierBps:Number(run.overtime_multiplier_bps),prorationMethod:run.proration_method};
        const items=await Promise.all(engagements.map(engagement=>calculatePayrollItemForSystem(tx,run,engagement,policy,false)));
        if(items.some(item=>!item.inputSnapshot.compensation||item.inputSnapshot.compensation.currency!==run.currency||item.net<0))return false;
        const totals=items.reduce((sum,item)=>({gross:sum.gross+item.gross,deductions:sum.deductions+item.deductions,employer:sum.employer+item.employer,net:sum.net+item.net}),{gross:0,deductions:0,employer:0,net:0});
        if(Object.values(totals).some(value=>!Number.isSafeInteger(value)))throw new Error("PAYROLL_MONEY_OVERFLOW");
        const digest=await sha256(JSON.stringify(items.map(item=>item.inputSnapshot))),timestamp=now.getTime();
        const commandId=`system-payroll-preview-${run.id}-${run.revision+1}`;
        await tx.prepare(`UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=?,input_digest=?,
          submitted_by=NULL,submitted_digest=NULL,submitted_at=NULL,version=version+1,updated_at=?,error_message=NULL WHERE id=?`)
          .bind(JSON.stringify({...totals,headcount:items.length}),digest,timestamp,run.id).run();
        for(let index=0;index<items.length;index++){
          const item=items[index];
          await tx.prepare(`INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,
            gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`)
            .bind(`payitem-system-${run.id}-${run.revision+1}-${index}`,run.id,item.engagementId,item.workerType,
              JSON.stringify(item.inputSnapshot),JSON.stringify(item.result),item.gross,item.deductions,item.employer,item.net,run.currency,timestamp).run();
        }
        await tx.prepare(`INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at)
          VALUES(?,?,'system:payroll-schedule','system:payroll-schedule','people.payroll.calculate','people_payroll',?,?,'Scheduled gross-pay preview',?)`)
          .bind(crypto.randomUUID(),commandId,run.id,run.version+1,timestamp).run();
        await tx.prepare(`INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)
          VALUES(?,?,'people.payroll.calculate','people_payroll',?,?,?,?)`)
          .bind(crypto.randomUUID(),commandId,run.id,run.version+1,JSON.stringify({id:run.id,companyId:run.company_id,status:"preview",system:true}),timestamp).run();
        return true;
      });
      if(completed)calculated++;
    }catch(error){
      const code=error instanceof ApiError?error.code:error instanceof Error?error.name:"PayrollPreviewError";
      console.warn(JSON.stringify({event:"payroll_scheduled_preview_deferred",runId:candidate.id,code}));
      await write.prepare(`UPDATE hr_payroll_runs SET status='failed',error_message=?,updated_at=?
        WHERE id=? AND status IN ('draft','failed') AND created_by='system:payroll-schedule'`).bind(String(code).slice(0,80),now.getTime(),candidate.id).run();
    }
  }
  return calculated;
}
