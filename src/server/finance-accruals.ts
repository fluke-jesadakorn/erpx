import "server-only";
import { z } from "zod";
import { decimalToMinor, minorToDecimal } from "./finance";
import { assertion, executeCommand, replayCommand } from "./commands";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import type { SqlDatabase } from "./runtime/types";

const date = z.string().date();
export const accrualScheduleSchema = z.object({
  description: z.string().trim().min(3).max(240),
  debitAccountId: z.string().min(1),
  creditAccountId: z.string().min(1),
  currency: z.string().length(3).transform(value=>value.toUpperCase()),
  total: z.string().regex(/^\d+(\.\d{1,2})?$/),
  startDate: date,
  endDate: date,
  frequency: z.enum(["monthly","quarterly","annual"]).default("monthly"),
});
export const accrualScheduleDecisionSchema = z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)});
export const accrualRunPostSchema = z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)});

function addMonths(value:string,count:number){const dateValue=new Date(`${value}T00:00:00Z`);dateValue.setUTCMonth(dateValue.getUTCMonth()+count);return dateValue.toISOString().slice(0,10);}
function periodEnd(start:string,frequency:"monthly"|"quarterly"|"annual"){const dateValue=new Date(`${start}T00:00:00Z`);dateValue.setUTCMonth(dateValue.getUTCMonth()+({monthly:1,quarterly:3,annual:12}[frequency]));dateValue.setUTCDate(0);return dateValue.toISOString().slice(0,10);}
function frequencyMonths(frequency:string){return frequency==="annual"?12:frequency==="quarterly"?3:1;}
function periodCount(start:string,end:string,frequency:string){let count=0,cursor=start;const step=frequencyMonths(frequency);while(cursor<=end&&count<241){count++;cursor=addMonths(cursor,step);}if(!count||cursor<=end)throw new ApiError(422,"ACCRUAL_RANGE_TOO_LONG","Accrual schedules must contain between one and 240 periods.");return count;}
function periodIndex(start:string,value:string,frequency:string){let index=0,cursor=start;const step=frequencyMonths(frequency);while(cursor<value&&index<241){cursor=addMonths(cursor,step);index++;}return index;}
function allocate(total:number,count:number,index:number){const base=Math.floor(total/count),remainder=total%count;return base+(index<remainder?1:0);}
async function hash(value:unknown){const bytes=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(JSON.stringify(value)));return Array.from(new Uint8Array(bytes),byte=>byte.toString(16).padStart(2,"0")).join("");}
function accountSql(write:SqlDatabase,actor:WorkContext,input:z.infer<typeof accrualScheduleSchema>){return write.prepare(`SELECT a.id,a.chart_id,a.currency,a.status,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id IN (?,?) AND a.company_id=? AND a.status='active'`).bind(input.debitAccountId,input.creditAccountId,actor.companyId);}

export async function accrualSchedulePage(actor:WorkContext){
 actor=requirePermission(actor,"finance.read");const {write}=await holdingResources(actor.holdingId);
 const [schedules,runs,accounts]=await Promise.all([
  write.prepare(`SELECT s.*,da.code debit_code,da.name debit_name,ca.code credit_code,ca.name credit_name FROM finance_accrual_schedules s JOIN finance_accounts da ON da.id=s.debit_account_id JOIN finance_accounts ca ON ca.id=s.credit_account_id WHERE s.company_id=? ORDER BY s.created_at DESC LIMIT 100`).bind(actor.companyId).all(),
  write.prepare(`SELECT r.*,s.description FROM finance_accrual_schedule_runs r JOIN finance_accrual_schedules s ON s.id=r.schedule_id WHERE r.company_id=? ORDER BY r.period_start DESC,r.id DESC LIMIT 200`).bind(actor.companyId).all(),
  write.prepare("SELECT id,code,name,currency FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all(),
 ]);return {schedules:schedules.results,runs:runs.results,accounts:accounts.results};
}

export async function createAccrualSchedule(actor:WorkContext,input:z.infer<typeof accrualScheduleSchema>,key:string){
 actor=requirePermission(actor,"finance.write");const {write}=await holdingResources(actor.holdingId),id=`accrual-${key}`,previous=await replayCommand(write,actor,key,"finance.accrual.create",id,input);if(previous)return previous;
 if(input.endDate<input.startDate)throw new ApiError(422,"INVALID_ACCRUAL_DATES","Accrual end date must follow the start date.");
 const total=decimalToMinor(input.total);if(total<=0)throw new ApiError(422,"INVALID_ACCRUAL_TOTAL","Accrual total must be greater than zero.");const count=periodCount(input.startDate,input.endDate,input.frequency);
 const accounts=await accountSql(write,actor,input).all<{id:string;chart_id:string;currency:string;status:string;base_currency:string}>();if(accounts.results.length!==2||accounts.results.some(account=>account.currency!==input.currency||account.base_currency!==input.currency))throw new ApiError(422,"ACCRUAL_ACCOUNT_INVALID","Choose two active accounts in the chart's functional currency.");if(input.debitAccountId===input.creditAccountId)throw new ApiError(422,"ACCRUAL_ACCOUNT_INVALID","Debit and credit accounts must be different.");
 const now=Date.now(),result={id,description:input.description,status:"draft",version:1,periodCount:count,total:minorToDecimal(total)};
 return executeCommand(write,actor,key,"finance.accrual.create",input,id,1,result,[
  write.prepare("INSERT INTO finance_accrual_schedules(id,company_id,description,debit_account_id,credit_account_id,currency,total_minor,start_date,end_date,next_run_date,frequency,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'draft',1,?,?,?)").bind(id,actor.companyId,input.description,input.debitAccountId,input.creditAccountId,input.currency,total,input.startDate,input.endDate,input.startDate,input.frequency,actor.userId,now,now),
  write.prepare("INSERT INTO finance_accrual_schedule_events(id,schedule_id,company_id,action,actor_id,schedule_version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,1,?,?,?)").bind(`accrual-event-${key}`,id,actor.companyId,"created",actor.userId,"Schedule prepared for independent approval",JSON.stringify({totalMinor:total,periodCount:count,frequency:input.frequency}),now),
 ]);
}

export async function decideAccrualSchedule(actor:WorkContext,scheduleId:string,input:z.infer<typeof accrualScheduleDecisionSchema>,action:"approve"|"cancel",key:string){
 actor=action==="approve"?requirePermission(actor,"finance.approve"):requirePermission(actor,"finance.write");const {write}=await holdingResources(actor.holdingId),previous=await replayCommand(write,actor,key,`finance.accrual.${action}`,scheduleId,input);if(previous)return previous;
 const row=await write.prepare("SELECT * FROM finance_accrual_schedules WHERE id=? AND company_id=?").bind(scheduleId,actor.companyId).first<Record<string,string|number|null>>();if(!row)throw new ApiError(404,"ACCRUAL_NOT_FOUND","Accrual schedule not found.");if(Number(row.version)!==input.expectedVersion)throw new ApiError(409,"ACCRUAL_CHANGED","The accrual schedule changed. Reload before deciding.");if(action==="approve"&&(row.status!=="draft"||row.created_by===actor.userId))throw new ApiError(409,row.created_by===actor.userId?"SELF_REVIEW_FORBIDDEN":"INVALID_ACCRUAL_TRANSITION",row.created_by===actor.userId?"A different Finance approver must approve this schedule.":"Only a draft schedule can be approved.");if(action==="cancel"&&!['draft','active'].includes(String(row.status)))throw new ApiError(409,"INVALID_ACCRUAL_TRANSITION","Only a draft or active schedule can be cancelled.");
 const status=action==="approve"?"active":"cancelled",version=input.expectedVersion+1,now=Date.now();return executeCommand(write,actor,key,`finance.accrual.${action}`,input,scheduleId,version,{id:scheduleId,status,version},[
  assertion(write,key,"schedule-version","EXISTS(SELECT 1 FROM finance_accrual_schedules WHERE id=? AND company_id=? AND version=? AND status=?)",[scheduleId,actor.companyId,input.expectedVersion,row.status]),
  write.prepare("UPDATE finance_accrual_schedules SET status=?,approved_by=CASE WHEN ?='active' THEN ? ELSE approved_by END,version=version+1,updated_at=? WHERE id=?").bind(status,status,actor.userId,now,scheduleId),
  write.prepare("INSERT INTO finance_accrual_schedule_events(id,schedule_id,company_id,action,actor_id,schedule_version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(`accrual-event-${key}`,scheduleId,actor.companyId,action==="approve"?"approved":"cancelled",actor.userId,version,input.reason,JSON.stringify({status}),now),
 ]);
}

export async function prepareAccrualRun(actor:WorkContext,scheduleId:string,key:string){
 actor=requirePermission(actor,"finance.write");const {write}=await holdingResources(actor.holdingId),runId=`accrual-run-${key}`,previous=await replayCommand(write,actor,key,"finance.accrual.prepare",runId,{scheduleId});if(previous)return previous;
 const schedule=await write.prepare("SELECT * FROM finance_accrual_schedules WHERE id=? AND company_id=?").bind(scheduleId,actor.companyId).first<Record<string,string|number|null>>();if(!schedule)throw new ApiError(404,"ACCRUAL_NOT_FOUND","Accrual schedule not found.");if(schedule.status!=="active")throw new ApiError(409,"ACCRUAL_NOT_ACTIVE","Only an active accrual schedule can create a period.");
 const periodStart=String(schedule.next_run_date),frequency=String(schedule.frequency) as "monthly"|"quarterly"|"annual",periodEndValue=periodEnd(periodStart,frequency)>String(schedule.end_date)?String(schedule.end_date):periodEnd(periodStart,frequency);if(periodStart>String(schedule.end_date))throw new ApiError(409,"ACCRUAL_COMPLETE","This schedule has no remaining periods.");const period=await write.prepare("SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?").bind(actor.companyId,periodStart,periodEndValue).first<{id:string}>();if(!period)throw new ApiError(409,"ACCRUAL_PERIOD_UNAVAILABLE","Open an accounting period covering the accrual period before preparing it.");
 const count=periodCount(String(schedule.start_date),String(schedule.end_date),frequency),index=periodIndex(String(schedule.start_date),periodStart,frequency);const amount=allocate(Number(schedule.total_minor),count,Math.max(0,Math.min(count-1,index)));const sourceHash=await hash({scheduleId,periodStart,periodEnd:periodEndValue,amount,scheduleVersion:schedule.version});const existing=await write.prepare("SELECT id FROM finance_accrual_schedule_runs WHERE schedule_id=? AND period_start=?").bind(scheduleId,periodStart).first();if(existing)throw new ApiError(409,"ACCRUAL_PERIOD_ALREADY_PREPARED","This accrual period already has a prepared run.");
 const chart=await write.prepare("SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=? AND status='active'").bind(schedule.debit_account_id,actor.companyId).first<{chart_id:string}>();if(!chart)throw new ApiError(422,"ACCRUAL_ACCOUNT_INVALID","The schedule account is no longer active.");const journalId=`journal-${runId}`,number=`ACR-${key}`,now=Date.now();
 const result={id:runId,scheduleId,journalId,periodStart,periodEnd:periodEndValue,amountMinor:amount,status:"prepared",version:1};return executeCommand(write,actor,key,"finance.accrual.prepare",{scheduleId},runId,1,result,[
  assertion(write,key,"schedule-version","EXISTS(SELECT 1 FROM finance_accrual_schedules WHERE id=? AND company_id=? AND status='active' AND version=?)",[scheduleId,actor.companyId,schedule.version]),
  write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)").bind(journalId,actor.companyId,chart.chart_id,period.id,number,periodEndValue,schedule.currency,1000000,"accrual_schedule",runId,`Accrual · ${schedule.description} · ${periodStart} to ${periodEndValue}`,actor.userId,now,now),
  write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,0),(?,?,?,?,?,?,?,?,?,1)").bind(`${journalId}-0`,journalId,schedule.debit_account_id,schedule.description,amount,0,amount,0,"{}",`${journalId}-1`,journalId,schedule.credit_account_id,schedule.description,0,amount,0,amount,"{}"),
  write.prepare("INSERT INTO finance_accrual_schedule_runs(id,schedule_id,company_id,period_start,period_end,amount_minor,currency,journal_id,schedule_version,source_hash,status,version,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?, 'prepared',1,?,?,?)").bind(runId,scheduleId,actor.companyId,periodStart,periodEndValue,amount,schedule.currency,journalId,Number(schedule.version),sourceHash,actor.userId,now,now),
  write.prepare("INSERT INTO finance_accrual_schedule_events(id,schedule_id,run_id,company_id,action,actor_id,schedule_version,run_version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`accrual-event-${key}`,scheduleId,runId,actor.companyId,"prepared",actor.userId,Number(schedule.version),1,"Prepare period accrual",JSON.stringify({periodStart,periodEnd:periodEndValue,amountMinor:amount,sourceHash}),now),
 ]);
}

export async function postAccrualRun(actor:WorkContext,runId:string,input:z.infer<typeof accrualRunPostSchema>,key:string){
 actor=requirePermission(actor,"finance.approve");const {write}=await holdingResources(actor.holdingId),previous=await replayCommand(write,actor,key,"finance.accrual.post",runId,input);if(previous)return previous;
 const row=await write.prepare(`SELECT r.*,s.description,s.frequency,s.start_date,s.end_date,s.next_run_date,s.version schedule_current_version,s.status schedule_status,j.chart_id,j.period_id,j.entry_date,j.status journal_status,j.journal_number FROM finance_accrual_schedule_runs r JOIN finance_accrual_schedules s ON s.id=r.schedule_id JOIN finance_journals j ON j.id=r.journal_id WHERE r.id=? AND r.company_id=?`).bind(runId,actor.companyId).first<Record<string,string|number|null>>();if(!row)throw new ApiError(404,"ACCRUAL_RUN_NOT_FOUND","Accrual period run not found.");if(row.status!=="prepared"||Number(row.version)!==input.expectedVersion)throw new ApiError(409,"ACCRUAL_RUN_CHANGED","This accrual run changed. Reload before posting.");if(row.prepared_by===actor.userId)throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different Finance approver must post this accrual.");if(row.schedule_status!=="active"||Number(row.schedule_current_version)!==Number(row.schedule_version)||row.journal_status!=="draft")throw new ApiError(409,"ACCRUAL_SOURCE_CHANGED","The schedule or journal changed. Prepare the period again.");
 const frequency=String(row.frequency) as "monthly"|"quarterly"|"annual",next=addMonths(String(row.period_start),frequencyMonths(frequency)),complete=next>String(row.end_date),version=input.expectedVersion+1,now=Date.now();return executeCommand(write,actor,key,"finance.accrual.post",input,runId,version,{id:runId,status:"posted",version,journalId:row.journal_id},[
  assertion(write,key,"run-version","EXISTS(SELECT 1 FROM finance_accrual_schedule_runs WHERE id=? AND company_id=? AND status='prepared' AND version=? AND schedule_version=?)",[runId,actor.companyId,input.expectedVersion,row.schedule_version]),
  assertion(write,key,"period-open","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open')",[row.period_id,actor.companyId]),
  write.prepare("UPDATE finance_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=? AND status='draft'").bind(actor.userId,now,now,row.journal_id),
  write.prepare("UPDATE finance_accrual_schedule_runs SET status='posted',posted_by=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId,now,runId),
  write.prepare("UPDATE finance_accrual_schedules SET next_run_date=?,status=CASE WHEN ? THEN 'completed' ELSE status END,version=version+1,updated_at=? WHERE id=? AND status='active' AND version=?").bind(next,complete,now,row.schedule_id,row.schedule_version),
  write.prepare("INSERT INTO finance_accrual_schedule_events(id,schedule_id,run_id,company_id,action,actor_id,schedule_version,run_version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`accrual-event-${key}`,row.schedule_id,runId,actor.companyId,"posted",actor.userId,Number(row.schedule_version)+1,version,input.reason,JSON.stringify({journalId:row.journal_id,periodStart:row.period_start,periodEnd:row.period_end,amountMinor:row.amount_minor}),now),
 ]);
}

/** Read by ID independently of the register's page limits. */
export async function accrualRecordDetail(actor: WorkContext, kind: "schedule" | "run", id: string) {
 actor = requirePermission(actor, "finance.read");
 const { write } = await holdingResources(actor.holdingId);
 const row = kind === "schedule"
  ? await write.prepare("SELECT s.*,da.code debit_code,da.name debit_name,ca.code credit_code,ca.name credit_name FROM finance_accrual_schedules s JOIN finance_accounts da ON da.id=s.debit_account_id JOIN finance_accounts ca ON ca.id=s.credit_account_id WHERE s.id=? AND s.company_id=?").bind(id,actor.companyId).first()
  : await write.prepare("SELECT r.*,s.description FROM finance_accrual_schedule_runs r JOIN finance_accrual_schedules s ON s.id=r.schedule_id WHERE r.id=? AND r.company_id=?").bind(id,actor.companyId).first();
 if (!row) throw new ApiError(404,"ACCRUAL_NOT_FOUND","Accrual record not found.");
 const data = await accrualSchedulePage(actor);
 return { row, data: { ...data, schedules: kind === "schedule" ? [row] : [], runs: kind === "run" ? [row] : data.runs.filter(run => run.schedule_id === id) } };
}
