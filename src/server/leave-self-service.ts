import "server-only";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { activeStaff, requestRoute, requireCompanyAuthority } from "./staff-policy";
import { ApiError } from "./http";
import type { PostgresDatabase } from "./runtime/postgres";
import { executeCommand, replayCommand } from "./commands";
import { openApproval } from "./approvals";
import {
  calculateLeaveDays,
  dateRange,
  addLeaveDays,
  leaveCreateSchema,
  leavePreviewSchema,
  type LeaveCalculationDay,
} from "@/lib/leave-self-service";
import { z } from "zod";
import { scheduledFeaturePredicate } from "./hospitality-rollout";

type Policy = {
  assignment_id: string;
  policy_version_id: string;
  policy_name: string;
  policy_version: number;
  timezone: string;
  workweek_json: number[] | string;
  allow_negative: boolean;
  holiday_calendar_id: string | null;
};
type LeaveType = {
  id: string;
  code: string;
  name: string;
  description: string;
  tracked: boolean;
  evidence_after_units: number | null;
  evidence_note: string;
  grant_kind: "annual" | "period" | "accrual" | "untracked";
  grant_units: number;
  accrual_units: number;
  accrual_period: string | null;
  carryover_limit: number | null;
  employee_eligible: boolean;
  contractor_eligible: boolean;
};

async function rows<T>(db: SqlDatabase, sql: string, ...args: unknown[]) {
  return (await db.prepare(sql).bind(...args).all<T>()).results;
}

async function policyFor(db: SqlDatabase, context: WorkContext, date: string) {
  const staff = await activeStaff(db, context.companyId, context.userId);
  const matches = await rows<Policy>(db, `SELECT a.id assignment_id,p.id policy_version_id,p.name policy_name,p.version policy_version,p.timezone,p.workweek_json,p.allow_negative,a.holiday_calendar_id
    FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id
    WHERE a.company_id=? AND a.engagement_id=? AND p.status='active' AND a.effective_from<=? AND (a.effective_to IS NULL OR a.effective_to>=?)
    AND p.effective_from<=? AND (p.effective_to IS NULL OR p.effective_to>=?) ORDER BY a.effective_from DESC,p.version DESC`,
    context.companyId, staff.engagement_id, date, date, date, date);
  if (matches.length > 1 && matches[0].assignment_id !== matches[1].assignment_id)
    throw new ApiError(409, "LEAVE_POLICY_AMBIGUOUS", "HR must resolve overlapping leave policy assignments.");
  return { staff, policy: matches[0] ?? null };
}

function workweek(value: number[] | string) {
  const parsed = typeof value === "string" ? JSON.parse(value) : value;
  return Array.isArray(parsed) && parsed.every((x) => Number.isInteger(x) && x >= 1 && x <= 7)
    ? parsed
    : [1, 2, 3, 4, 5];
}

async function balance(db: SqlDatabase, companyId: string, engagementId: string, leaveTypeId: string, year = new Date().getUTCFullYear()) {
  return await db.prepare(`SELECT
    coalesce(sum(units),0)::float available,
    coalesce(-sum(units) FILTER (WHERE kind IN ('reservation','reservation_release')),0)::float reserved,
    coalesce(-sum(units) FILTER (WHERE kind IN ('usage','usage_reversal')),0)::float used,
    coalesce(sum(units) FILTER (WHERE kind='accrual'),0)::float accrued
    FROM leave_balance_entries WHERE company_id=? AND engagement_id=? AND leave_type_id=? AND entitlement_year=?`)
    .bind(companyId, engagementId, leaveTypeId, year)
    .first<{ available: number; reserved: number; used: number; accrued: number }>();
}

export async function previewLeave(context: WorkContext, raw: z.infer<typeof leavePreviewSchema>, excludeId: string | null = null, database?: SqlDatabase) {
  const input = leavePreviewSchema.parse(raw);
  if (input.endDate < input.startDate) throw new ApiError(422, "LEAVE_DATE_RANGE", "End date must be on or after the start date.");
  if(Date.parse(input.endDate)-Date.parse(input.startDate)>365*86400000)throw new ApiError(422,"LEAVE_DATE_RANGE","A request can cover at most 366 calendar dates. Split longer periods into separate requests.");
  const write = database ?? (await holdingResources(context.holdingId)).write;
  const { staff, policy } = await policyFor(write, context, input.startDate);
  if (!policy) throw new ApiError(409, "LEAVE_POLICY_REQUIRED", "HR setup required: ask HR to assign a leave policy to your active engagement.");
  const type = await write.prepare(`SELECT t.id,t.code,t.name,t.description,t.tracked,t.evidence_after_units,t.evidence_note,r.grant_kind,r.grant_units,r.accrual_units,r.accrual_period,r.carryover_limit,r.employee_eligible,r.contractor_eligible
    FROM leave_types t JOIN leave_policy_rules r ON r.leave_type_id=t.id AND r.policy_version_id=?
    WHERE t.id=? AND t.company_id=? AND t.active=true AND t.effective_from<=? AND (t.effective_to IS NULL OR t.effective_to>=?)`)
    .bind(policy.policy_version_id, input.leaveTypeId, context.companyId, input.startDate, input.endDate).first<LeaveType>();
  if (!type) throw new ApiError(422, "LEAVE_TYPE_UNAVAILABLE", "This leave type is not available under the assigned policy for those dates.");
  if (!(staff.worker_type === "contractor" ? type.contractor_eligible : type.employee_eligible)) throw new ApiError(409,"LEAVE_INELIGIBLE","This leave type is not available to your engagement. Ask HR to review your policy eligibility.");
  if (input.startDate < staff.start_date || staff.end_date && input.endDate > staff.end_date) throw new ApiError(422,"LEAVE_ENGAGEMENT_DATES","The dates must fall within your engagement.");
  for (const date of dateRange(input.startDate,input.endDate)) {
    const applicable=await policyFor(write,context,date);
    if(applicable.policy?.policy_version_id!==policy.policy_version_id || applicable.policy.assignment_id!==policy.assignment_id)
      throw new ApiError(409,"LEAVE_POLICY_BOUNDARY","A policy changes during this range. Create a separate request for each policy period so each calculation can be reviewed independently.");
  }
  const holidays = new Set((policy.holiday_calendar_id ? await rows<{ holiday_date: string }>(write,
    "SELECT holiday_date FROM leave_holidays WHERE calendar_id=? AND holiday_date BETWEEN ? AND ?", policy.holiday_calendar_id, input.startDate, input.endDate) : []).map((x) => x.holiday_date));
  const published = await rows<{ week_start: string }>(write, `SELECT week_start FROM people_roster_weeks WHERE company_id=? AND state='published' AND department_id IS NOT DISTINCT FROM ? AND week_start::date<=?::date AND week_start::date+6>=?::date`,
    context.companyId, staff.department_id, input.endDate, input.startDate);
  const rosterPublishedDates = new Set<string>();
  for (const week of published) for (let i = 0; i < 7; i++) rosterPublishedDates.add(new Date(Date.parse(`${week.week_start}T12:00:00Z`) + i * 86_400_000).toISOString().slice(0, 10));
  const rosterDates = new Set((await rows<{ local_date: string }>(write, `SELECT DISTINCT s.local_date FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id
    WHERE w.company_id=? AND w.state='published' AND a.engagement_id=? AND a.state='assigned' AND s.state='active' AND s.local_date BETWEEN ? AND ?`,
    context.companyId, staff.engagement_id, input.startDate, input.endDate)).map((x) => x.local_date));
  let days: LeaveCalculationDay[];
  try {
    days = calculateLeaveDays({ ...input, workweek: workweek(policy.workweek_json), holidays, rosterDates, rosterPublishedDates });
  } catch (error) {
    throw new ApiError(422, "LEAVE_DATE_RANGE", error instanceof Error ? error.message.replaceAll("_", " ").toLowerCase() : "Invalid date range.");
  }
  const units = days.reduce((sum, day) => sum + day.units, 0);
  if (!units) throw new ApiError(422, "NO_WORKING_DAYS", "The selected range contains no scheduled working days.");
  const current = (await balance(write, context.companyId, staff.engagement_id, type.id, Number(input.startDate.slice(0,4)))) ?? { available: 0, reserved: 0, used: 0, accrued: 0 };
  const candidates = await rows<{id:string;number:string;state:string;leave_date:string;segment:string}>(write, `SELECT r.id,r.number,r.state,s.leave_date,s.segment FROM leave_requests r JOIN leave_day_segments s ON s.request_id=r.id
    WHERE r.company_id=? AND r.engagement_id=? AND r.id IS DISTINCT FROM ? AND r.state IN ('pending_approval','approved') AND s.leave_date BETWEEN ? AND ?`, context.companyId,staff.engagement_id,excludeId,input.startDate,input.endDate);
  const legacy = await rows<{id:string;number:string;state:string}>(write,`SELECT r.id,r.number,r.state FROM leave_requests r WHERE r.company_id=? AND r.engagement_id=? AND r.id IS DISTINCT FROM ? AND r.state IN ('pending_approval','approved') AND r.start_date<=? AND coalesce(r.end_date,r.start_date)>=? AND NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=r.id)`,context.companyId,staff.engagement_id,excludeId,input.endDate,input.startDate);
  const overlaps = [...new Map([...legacy,...candidates.filter(other=>days.some(day=>day.date===other.leave_date&&(day.segment==='full'||other.segment==='full'||day.segment===other.segment)))].map(row=>[row.id,{id:row.id,number:row.number,state:row.state}])).values()];
  const accounts=await Promise.all([...new Set(days.map(d=>Number(d.date.slice(0,4))))].map(async year=>({year,units:days.filter(d=>Number(d.date.slice(0,4))===year).reduce((n,d)=>n+d.units,0),...(await balance(write,context.companyId,staff.engagement_id,type.id,year))!})));
  const route = await requestRoute(write, context.companyId, context.userId, "leave");
  const approvers = route.steps.length ? await rows<{ user_id: string; legal_name: string }>(write,
    "SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND user_id=ANY(?::text[])", context.companyId, `{${route.steps.map((x) => x.userId).join(",")}}`) : [];
  const blockers = [
    ...overlaps.map((item) => `Overlaps ${item.number} (${item.state.replaceAll("_", " ")}).`),
    ...(route.blockedReason ? [route.blockedReason] : []),
    ...accounts.filter(a=>type.tracked&&!policy.allow_negative&&a.available<a.units).map(a=>`${a.year}: insufficient balance (${a.available} available, ${a.units} requested).`),
  ];
  return {
    accounts,
    leaveType: type,
    policy: { id: policy.policy_version_id, name: policy.policy_name, version: policy.policy_version, timezone: policy.timezone, workweek: workweek(policy.workweek_json), allowNegative: policy.allow_negative, holidayCalendarId: policy.holiday_calendar_id },
    days,
    units,
    balanceBefore: current.available,
    balanceAfter: type.tracked ? current.available - units : null,
    evidenceRequired: type.evidence_after_units !== null && units >= type.evidence_after_units,
    evidenceNote: type.evidence_note,
    overlaps,
    approverRoute: route.steps.map((step) => ({
      stage: step.authority === "hr_admin" ? "HR review" : "Manager review",
      userId: step.userId,
      name: approvers.find((person) => person.user_id === step.userId)?.legal_name ?? "Assigned reviewer",
    })),
    blockers,
    canSubmit: blockers.length === 0,
  };
}

export async function leaveSummary(context: WorkContext) {
  const { write } = await holdingResources(context.holdingId);
  let staff;
  try {
    staff = await activeStaff(write, context.companyId, context.userId);
  } catch (error) {
    if (!(error instanceof ApiError) || error.code !== "STAFF_LINK_REQUIRED") throw error;
    return {
      asOf: Date.now(), setupRequired: true,
      setupMessage: "HR setup required: your account must be linked to one active employee or contractor engagement in this company.",
      policy: null, balances: [], upcoming: [], pending: [], history: [], holidays: [],
    };
  }
  const assigned = await policyFor(write, context, new Date().toISOString().slice(0, 10));
  const types = assigned.policy ? await rows<LeaveType>(write, `SELECT t.id,t.code,t.name,t.description,t.tracked,t.evidence_after_units,t.evidence_note,r.grant_kind,r.grant_units,r.accrual_units,r.accrual_period,r.carryover_limit,r.employee_eligible,r.contractor_eligible
    FROM leave_policy_rules r JOIN leave_types t ON t.id=r.leave_type_id WHERE r.policy_version_id=? AND t.active=true ORDER BY t.name`, assigned.policy.policy_version_id) : [];
  const balances = await Promise.all(types.map(async (type) => ({ type, ...((await balance(write, context.companyId, staff.engagement_id, type.id)) ?? { available: 0, reserved: 0, used: 0, accrued: 0 }) })));
  const requests = await rows<Record<string, unknown>>(write, `SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name
    FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id WHERE r.company_id=? AND r.engagement_id=? ORDER BY r.created_at DESC,r.id DESC LIMIT 100`, context.companyId, staff.engagement_id);
  const holidays = assigned.policy?.holiday_calendar_id ? await rows<{ holiday_date: string; name: string }>(write,
    "SELECT h.holiday_date,h.name FROM leave_holidays h WHERE h.calendar_id=? AND h.holiday_date>=to_char(current_date,'YYYY-MM-DD') ORDER BY h.holiday_date LIMIT 8", assigned.policy.holiday_calendar_id) : [];
  return {
    asOf: Date.now(),
    setupRequired: !assigned.policy,
    setupMessage: !assigned.policy ? "HR setup required: a leave policy has not been assigned to your active engagement." : null,
    policy: assigned.policy ? { id: assigned.policy.policy_version_id, name: assigned.policy.policy_name, version: assigned.policy.policy_version, timezone: assigned.policy.timezone } : null,
    balances,
    upcoming: requests.filter((r) => r.state === "approved" && String(r.end_date ?? r.start_date) >= new Date().toISOString().slice(0, 10)),
    pending: requests.filter((r) => r.state === "pending_approval"),
    history: requests,
    holidays,
  };
}

export const leaveEditSchema=leaveCreateSchema.extend({expectedVersion:z.number().int().positive()});
export async function createSelfServiceLeave(context:WorkContext,raw:z.infer<typeof leaveCreateSchema>,key:string) {
 return saveSelfServiceLeave(context,null,leaveCreateSchema.parse(raw),key);
}
export async function editSelfServiceLeave(context:WorkContext,id:string,raw:z.infer<typeof leaveEditSchema>,key:string) {
 return saveSelfServiceLeave(context,id,leaveEditSchema.parse(raw),key);
}
async function saveSelfServiceLeave(context:WorkContext,recordId:string|null,input:z.infer<typeof leaveCreateSchema>&{expectedVersion?:number},key:string) {
 const db=(await holdingResources(context.holdingId)).write as PostgresDatabase,id=recordId??`leave-${key}`,operation=recordId?(input.saveOnly?'leave.self.save':'leave.self.submit'):'leave.self.create';
 return db.transaction(async write=>{
  const prior=await replayCommand(write,context,key,operation,id,input);if(prior)return prior;
  const staff=await activeStaff(write,context.companyId,context.userId),now=Date.now();
  await write.prepare('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE').bind(staff.engagement_id).first();
  const row=recordId?await write.prepare('SELECT * FROM leave_requests WHERE id=? AND company_id=? AND created_by=? FOR UPDATE').bind(id,context.companyId,context.userId).first<{version:number;state:string;engagement_id:string}>():null;
  if(recordId&&!row)throw new ApiError(404,'NOT_FOUND','Leave request not found.');
  if(row&&(row.version!==input.expectedVersion||!['draft','rejected'].includes(row.state)))throw new ApiError(409,'STALE_VERSION','This request changed. Reload to review the current version; your entered values are retained.');
  if(row&&row.engagement_id!==staff.engagement_id)throw new ApiError(409,'ENGAGEMENT_CHANGED','Start a new request for your current engagement.');
  const preview=await previewLeave(context,input,id,write);
  if(!input.saveOnly&&!preview.canSubmit)throw new ApiError(409,'LEAVE_BLOCKED',preview.blockers.join(' '));
  if(!input.saveOnly&&preview.evidenceRequired&&!await write.prepare("SELECT 1 FROM attachments a JOIN leave_requests r ON r.id=a.owner_id WHERE r.company_id=? AND a.owner_type='leave_request' AND a.owner_id=? AND a.revoked_at IS NULL").bind(context.companyId,id).first())throw new ApiError(409,'LEAVE_EVIDENCE_REQUIRED','Save a draft and attach the required private evidence before submission.');
  const version=(row?.version??0)+1,state=input.saveOnly?'draft':'pending_approval',statements=[];
  if(!row) {
   const employee=await write.prepare('SELECT id FROM employees WHERE person_id=? AND company_id=?').bind(staff.person_id,context.companyId).first<{id:string}>();
   const employeeId=employee?.id??`employee-${staff.person_id}`;
   if(!employee)statements.push(write.prepare("INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'active',?)").bind(employeeId,staff.person_id,context.companyId,staff.branch_id??'',staff.department_id,staff.legal_name,now));
   statements.push(write.prepare('INSERT INTO leave_requests(id,number,employee_id,person_id,engagement_id,company_id,branch_id,department_id,start_date,days,handover_note,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(id,`LR-${key}`,employeeId,staff.person_id,staff.engagement_id,context.companyId,staff.branch_id??'',staff.department_id,input.startDate,Math.ceil(preview.units),input.handoverNote,context.userId,now,now));
  }
  statements.push(write.prepare('UPDATE leave_requests SET start_date=?,days=?,handover_note=?,leave_type_id=?,end_date=?,start_segment=?,end_segment=?,policy_version_id=?,calculated_units=?,calculation_snapshot=?::jsonb,state=?,version=?,prepared_by=?,decided_by=NULL,decision_note=NULL,last_command_id=?,updated_at=? WHERE id=?').bind(input.startDate,Math.ceil(preview.units),input.handoverNote,input.leaveTypeId,input.endDate,input.startSegment,input.endSegment,preview.policy.id,preview.units,JSON.stringify(preview),state,version,context.userId,key,now,id),write.prepare('DELETE FROM leave_day_segments WHERE request_id=?').bind(id),...preview.days.map(day=>write.prepare('INSERT INTO leave_day_segments(id,request_id,company_id,leave_date,segment,units,source) VALUES(?,?,?,?,?,?,?)').bind(crypto.randomUUID(),id,context.companyId,day.date,day.segment,day.units,day.source)));
  if(!input.saveOnly) {
   const route=await requestRoute(write,context.companyId,context.userId,'leave');
   statements.push(...(await openApproval(write,key,'leave',id,'request',context.userId,null,route,version)).statements);
   if(preview.leaveType.tracked)for(const account of preview.accounts)statements.push(write.prepare("INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,request_id,kind,units,effective_date,entitlement_year,command_key,note,created_by,created_at) VALUES(?,?,?,?,?,'reservation',?,?,?,?,?,?,?)").bind(crypto.randomUUID(),context.companyId,staff.engagement_id,input.leaveTypeId,id,-account.units,`${account.year}-01-01`,account.year,`${key}:${account.year}`,'Pending leave reservation',context.userId,now));
  }
  return executeCommand(write,context,key,operation,input,id,version,{id,state,version,preview},statements,{companyId:context.companyId,branchId:staff.branch_id,departmentId:staff.department_id});
 });
}
/** Deprecated payloads enter the same policy, calendar, evidence and entitlement checks. */
export async function normalizeLegacyLeave(context:WorkContext,input:{startDate:string;days:number;handoverNote:string},recordId?:string) {
 const summary=await leaveSummary(context),db=(await holdingResources(context.holdingId)).write;
 const existing=recordId?await db.prepare('SELECT leave_type_id,end_date,start_segment,end_segment FROM leave_requests WHERE id=? AND company_id=? AND created_by=?').bind(recordId,context.companyId,context.userId).first<{leave_type_id:string;end_date:string;start_segment:'full'|'first_half'|'second_half';end_segment:'full'|'first_half'|'second_half'}>():null;
 const type=summary.balances.find(b=>b.type.id===existing?.leave_type_id)??summary.balances.find(b=>b.type.code==='annual'||b.type.code==='legacy-annual')??summary.balances[0];
 if(!type)throw new ApiError(409,'LEAVE_POLICY_REQUIRED',summary.setupMessage??'Ask HR to assign a leave policy.');
 const command={leaveTypeId:type.type.id,startDate:input.startDate,endDate:addLeaveDays(input.startDate,input.days-1),startSegment:'full' as const,endSegment:'full' as const,handoverNote:input.handoverNote};
 // The old command specifies working days. Extend its range using the same
 // roster/policy calculation until that many whole days are accounted for.
 for(let attempt=0;attempt<366;attempt++) {
  try {const preview=await previewLeave(context,command,recordId??null);if(preview.units===input.days)return command;command.endDate=addLeaveDays(command.endDate,input.days-preview.units);}
  catch(error){if(!(error instanceof ApiError)||error.code!=='NO_WORKING_DAYS')throw error;command.endDate=addLeaveDays(command.endDate,1);}
 }
 throw new ApiError(422,'LEAVE_DATE_RANGE','Choose an explicit date range for this request.');
}

export async function listOwnLeave(context: WorkContext, state?: string) {
  const summary = await leaveSummary(context);
  return state ? summary.history.filter((row) => row.state === state) : summary.history;
}

export const leaveAdminSchema = z.discriminatedUnion("resource", [
  z.object({ resource: z.literal("type"), code: z.string().min(1).max(40), name: z.string().min(1).max(100), description: z.string().max(500).default(""), tracked: z.boolean().default(true), evidenceAfterUnits: z.number().positive().nullable().default(null), evidenceNote: z.string().max(500).default("") }),
  z.object({ resource: z.literal("calendar"), name: z.string().min(1).max(100), timezone: z.string().min(1).max(80) }),
  z.object({ resource: z.literal("policy"), policyKey: z.string().min(1).max(80), version: z.number().int().positive(), name: z.string().min(1).max(100), effectiveFrom: z.iso.date(), effectiveTo: z.iso.date().nullable().default(null), timezone: z.string().min(1).max(80), workweek: z.array(z.number().int().min(1).max(7)).min(1), allowNegative: z.boolean().default(false), rules: z.array(z.object({ leaveTypeId: z.string().min(1), grantKind: z.enum(["annual","period","accrual","untracked"]), grantUnits: z.number().min(0).default(0), accrualUnits: z.number().min(0).default(0), accrualPeriod: z.enum(["monthly","quarterly","annual"]).nullable().default(null), carryoverLimit: z.number().min(0).nullable().default(null), employeeEligible:z.boolean().default(true), contractorEligible:z.boolean().default(false) })).min(1) }),
  z.object({ resource: z.literal("holiday"), calendarId: z.string().min(1), date: z.iso.date(), name: z.string().min(1).max(100) }),
  z.object({ resource: z.literal("assignment"), engagementId: z.string().min(1), policyVersionId: z.string().min(1), calendarId: z.string().min(1).nullable().default(null), effectiveFrom: z.iso.date(), effectiveTo: z.iso.date().nullable().default(null) }),
  z.object({resource:z.literal("assignment_end"),id:z.string().min(1),expectedVersion:z.number().int().positive(),effectiveTo:z.iso.date()}),
  z.object({ resource: z.literal("adjustment"), engagementId: z.string().min(1), leaveTypeId: z.string().min(1), units: z.number().min(-365).max(365).refine((x) => x !== 0), effectiveDate: z.iso.date(), note: z.string().min(1).max(500) }),
]);

export async function administerLeave(context: WorkContext, raw: z.infer<typeof leaveAdminSchema>, key: string) {
  const input = leaveAdminSchema.parse(raw), database = (await holdingResources(context.holdingId)).write as PostgresDatabase;
  return database.transaction(async write=>{
  const prior=await replayCommand(write,context,key,"leave.policy.manage",context.companyId,input);if(prior)return prior;
  await requireCompanyAuthority(write, context, "leave.policy.manage");
  if('effectiveFrom' in input && input.effectiveTo && input.effectiveTo<input.effectiveFrom)throw new ApiError(422,'LEAVE_DATE_RANGE','Effective end must not precede the start.');
  if('timezone' in input){try{new Intl.DateTimeFormat('en',{timeZone:input.timezone});}catch{throw new ApiError(422,'LEAVE_TIMEZONE','Choose a valid timezone.');}}
  if(input.resource==='policy'&&new Set(input.rules.map(r=>r.leaveTypeId)).size!==input.rules.length)throw new ApiError(422,'LEAVE_RULE_DUPLICATE','Use one rule per leave type in this policy.');
  if(input.resource==='policy')for(const rule of input.rules){if(!await write.prepare('SELECT 1 FROM leave_types WHERE id=? AND company_id=?').bind(rule.leaveTypeId,context.companyId).first())throw new ApiError(404,'NOT_FOUND','Leave type not found in this Company.');if(rule.grantKind==='accrual'&&(!rule.accrualPeriod||rule.accrualUnits<=0))throw new ApiError(422,'LEAVE_ACCRUAL','Choose a positive accrual amount and period.');}
  if(input.resource==='adjustment' && (!await write.prepare('SELECT 1 FROM hr_engagements WHERE id=? AND company_id=?').bind(input.engagementId,context.companyId).first()||!await write.prepare('SELECT 1 FROM leave_types WHERE id=? AND company_id=?').bind(input.leaveTypeId,context.companyId).first()))throw new ApiError(404,'NOT_FOUND','Engagement or leave type not found in this Company.');
  if((input.resource==='holiday'||input.resource==='assignment')&&input.calendarId&&!await write.prepare('SELECT 1 FROM leave_holiday_calendars WHERE id=? AND company_id=? AND active=true').bind(input.calendarId,context.companyId).first())throw new ApiError(404,'NOT_FOUND','Holiday calendar not found in this Company.');
  if(input.resource==='assignment'&&!await write.prepare('SELECT 1 FROM hr_engagements WHERE id=? AND company_id=?').bind(input.engagementId,context.companyId).first())throw new ApiError(404,'NOT_FOUND','Engagement not found in this Company.');
  const id = `leave-${input.resource}-${crypto.randomUUID()}`, now = Date.now();
  if (input.resource === "type") await write.prepare("INSERT INTO leave_types(id,company_id,code,name,description,tracked,evidence_after_units,evidence_note,effective_from,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
    .bind(id, context.companyId, input.code, input.name, input.description, input.tracked, input.evidenceAfterUnits, input.evidenceNote, new Date().toISOString().slice(0,10), context.userId, now, now).run();
  else if (input.resource === "calendar") await write.prepare("INSERT INTO leave_holiday_calendars(id,company_id,name,timezone,created_by,created_at) VALUES(?,?,?,?,?,?)").bind(id, context.companyId, input.name, input.timezone, context.userId, now).run();
  else if (input.resource === "policy") await write.batch([
    write.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,effective_to,timezone,workweek_json,allow_negative,created_by,created_at) VALUES(?,?,?,?,?,'active',?,?,?,?::jsonb,?,?,?)").bind(id,context.companyId,input.policyKey,input.version,input.name,input.effectiveFrom,input.effectiveTo,input.timezone,JSON.stringify(input.workweek),input.allowNegative,context.userId,now),
    ...input.rules.map((rule)=>write.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units,accrual_units,accrual_period,carryover_limit,employee_eligible,contractor_eligible) SELECT ?,?,?,?,?,?,?,?,? WHERE EXISTS(SELECT 1 FROM leave_types WHERE id=? AND company_id=?)").bind(id,rule.leaveTypeId,rule.grantKind,rule.grantUnits,rule.accrualUnits,rule.accrualPeriod,rule.carryoverLimit,rule.employeeEligible,rule.contractorEligible,rule.leaveTypeId,context.companyId)),
  ]);
  else if (input.resource === "holiday") await write.prepare("INSERT INTO leave_holidays(id,calendar_id,holiday_date,name) SELECT ?,?,?,? WHERE EXISTS(SELECT 1 FROM leave_holiday_calendars WHERE id=? AND company_id=?)").bind(id,input.calendarId,input.date,input.name,input.calendarId,context.companyId).run();
  else if (input.resource === "assignment") {
    const overlaps=await rows<{id:string;policy_key:string;effective_from:string}>(write,`SELECT a.id,p.policy_key,a.effective_from FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id WHERE a.company_id=? AND a.engagement_id=? AND a.effective_from<=? AND coalesce(a.effective_to,'9999-12-31')>=?`,context.companyId,input.engagementId,input.effectiveTo??'9999-12-31',input.effectiveFrom);
    if(overlaps.some(a=>a.policy_key!=='legacy-untracked'||a.effective_from>=input.effectiveFrom))throw new ApiError(409,'LEAVE_POLICY_OVERLAP','End the existing assignment before assigning a new policy.');
    for(const row of overlaps)await write.prepare('UPDATE leave_policy_assignments SET effective_to=? WHERE id=?').bind(addLeaveDays(input.effectiveFrom,-1),row.id).run();
    await write.prepare('SELECT id FROM leave_policy_versions WHERE id=? AND company_id=?').bind(input.policyVersionId,context.companyId).first().then(row=>{if(!row)throw new ApiError(404,'NOT_FOUND','Policy not found in this Company.');});
    await write.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,holiday_calendar_id,effective_from,effective_to,created_by,created_at) SELECT ?,?,?,?,?,?,?,?,? WHERE EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=?)").bind(id,context.companyId,input.engagementId,input.policyVersionId,input.calendarId,input.effectiveFrom,input.effectiveTo,context.userId,now,input.engagementId,context.companyId).run();
  } else if(input.resource==='assignment_end') {
    const row=await write.prepare('SELECT effective_from,version FROM leave_policy_assignments WHERE id=? AND company_id=? FOR UPDATE').bind(input.id,context.companyId).first<{effective_from:string;version:number}>();
    if(!row)throw new ApiError(404,'NOT_FOUND','Policy assignment not found.');
    if(row.version!==input.expectedVersion)throw new ApiError(409,'STALE_VERSION','The assignment changed. Reload before editing.');
    if(input.effectiveTo<row.effective_from)throw new ApiError(422,'LEAVE_DATE_RANGE','End must not precede the assignment start.');
    await write.prepare('UPDATE leave_policy_assignments SET effective_to=?,version=version+1 WHERE id=? AND company_id=?').bind(input.effectiveTo,input.id,context.companyId).run();
  } else await write.prepare("INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,kind,units,effective_date,command_key,note,created_by,created_at) VALUES(?,?,?,?,'adjustment',?,?,?,?,?,?) ON CONFLICT(company_id,command_key,kind,leave_type_id) DO NOTHING").bind(id,context.companyId,input.engagementId,input.leaveTypeId,input.units,input.effectiveDate,key,input.note,context.userId,now).run();
  await runLeaveAccruals(write,new Date().toISOString().slice(0,10));
  return executeCommand(write,context,key,"leave.policy.manage",input,context.companyId,1,{id},[]);
  });
}

/** Catch up every due period, including grants and carryover, with stable entitlement keys. */
export async function runLeaveAccruals(db:SqlDatabase,effectiveDate:string) {
 z.iso.date().parse(effectiveDate);
 const due=await rows<{company_id:string;engagement_id:string;leave_type_id:string;assignment_id:string;policy_version_id:string;grant_kind:string;grant_units:number;accrual_units:number;accrual_period:string|null;carryover_limit:number|null;first_day:string;last_day:string}>(db,`SELECT a.company_id,a.engagement_id,r.leave_type_id,a.id assignment_id,p.id policy_version_id,r.grant_kind,r.grant_units,r.accrual_units,r.accrual_period,r.carryover_limit,
 greatest(a.effective_from,p.effective_from,e.start_date) first_day,least(coalesce(a.effective_to,?),coalesce(p.effective_to,?),coalesce(e.end_date,?),?) last_day
 FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id JOIN leave_policy_rules r ON r.policy_version_id=p.id
 JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id JOIN leave_types t ON t.id=r.leave_type_id AND t.company_id=a.company_id
 WHERE p.status IN ('active','retired') AND t.tracked=true AND a.effective_from<=? AND (CASE WHEN e.worker_type='contractor' THEN r.contractor_eligible ELSE r.employee_eligible END)
 AND ${scheduledFeaturePredicate("a.company_id","people_payroll")} ORDER BY first_day,a.id,r.leave_type_id`,effectiveDate,effectiveDate,effectiveDate,effectiveDate,effectiveDate);
 let inserted=0;
 for(const item of due) {
  if(item.first_day>item.last_day)continue;
  const insert=async(kind:string,units:number,date:string)=>{
   if(!units)return;
   const result=await db.prepare(`INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,kind,units,effective_date,command_key,note,created_by,created_at)
    SELECT ?,?,?,?,?,?,?,?,?,'system:leave-accrual',? WHERE ${scheduledFeaturePredicate("?","people_payroll")}
    ON CONFLICT(company_id,command_key,kind,leave_type_id) DO NOTHING`)
   .bind(crypto.randomUUID(),item.company_id,item.engagement_id,item.leave_type_id,kind,units,date,`leave:${item.grant_kind==='period'?item.assignment_id:item.engagement_id}:${kind}:${kind==='grant'||kind==='carryover'?date.slice(0,4):date}`,`Policy ${item.policy_version_id}: ${kind}`,Date.now(),...(process.env.NODE_ENV==='production'?[item.company_id]:[])).run();
   inserted+=result.meta.changes;
  };
  if(item.grant_kind==='period')await insert('grant',item.grant_units,item.first_day);
  for(let year=Number(item.first_day.slice(0,4));year<=Number(item.last_day.slice(0,4));year++) {
   const yearStart=`${year}-01-01`,first=yearStart<item.first_day?item.first_day:yearStart;
   if(item.grant_kind==='annual')await insert('grant',item.grant_units,first);
   if(yearStart>=item.first_day&&item.carryover_limit!==null) {
    const previous=await balance(db,item.company_id,item.engagement_id,item.leave_type_id,year-1);
    await insert('carryover',Math.min(item.carryover_limit,Math.max(0,previous?.available??0)),yearStart);
   }
   if(item.grant_kind==='accrual')for(let month=1;month<=12;month++) {
    const date=`${year}-${String(month).padStart(2,'0')}-01`;
    if(date<item.first_day||date>item.last_day)continue;
    if(item.accrual_period==='monthly'||item.accrual_period==='quarterly'&&[1,4,7,10].includes(month)||item.accrual_period==='annual'&&month===1)await insert('accrual',item.accrual_units,date);
   }
  }
 }
 return inserted;
}

export async function leaveAdministration(context:WorkContext) {
 const {write}=await holdingResources(context.holdingId);await requireCompanyAuthority(write,context,'leave.policy.manage');
 const [types,policies,calendars,engagements,assignments,eligibilities]=await Promise.all([
  rows<{id:string;name:string;tracked:boolean}>(write,'SELECT id,name,tracked FROM leave_types WHERE company_id=? ORDER BY name',context.companyId),
  rows<{id:string;name:string;version:number;effective_from:string;effective_to:string|null}>(write,'SELECT id,name,version,effective_from,effective_to FROM leave_policy_versions WHERE company_id=? ORDER BY name,version DESC',context.companyId),
  rows<{id:string;name:string}>(write,'SELECT id,name FROM leave_holiday_calendars WHERE company_id=? ORDER BY name',context.companyId),
  rows<{id:string;name:string;worker_type:string}>(write,"SELECT e.id,p.legal_name name,e.worker_type FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=? AND e.status='active' ORDER BY p.legal_name",context.companyId),
  rows<{id:string;name:string;policy:string;effective_from:string;effective_to:string|null;version:number}>(write,'SELECT a.id,p.legal_name name,v.name policy,a.effective_from,a.effective_to,a.version FROM leave_policy_assignments a JOIN hr_engagements e ON e.id=a.engagement_id JOIN hr_people p ON p.id=e.person_id JOIN leave_policy_versions v ON v.id=a.policy_version_id WHERE a.company_id=? ORDER BY p.legal_name,a.effective_from DESC',context.companyId),
  rows<{engagement_id:string;eligible:boolean;version:number;top_level:boolean}>(write,'SELECT o.engagement_id,o.eligible,o.version,e.self_service_top_level top_level FROM overtime_eligibility o JOIN hr_engagements e ON e.id=o.engagement_id AND e.company_id=o.company_id WHERE o.company_id=?',context.companyId),
 ]);
 return {types,policies,calendars,engagements,assignments,eligibilities};
}
