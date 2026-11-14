import { z } from "zod";
import { expenseDb } from "./expense-fixture";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { calculatePayrollItemForSystem } from "@erpx/server-people_payroll/public/people-payroll";
import { payrollAccountingLines, type PayrollPostingItem } from "@erpx/server-people_payroll/domain/payroll-accounting";
import { payrollPostSchema } from "@erpx/server-people_payroll/domain/people-payroll";
import { assertStatutoryRunReady } from "@erpx/server-people_payroll/public/payroll-statutory";
import { prepareJournal } from "@erpx/server-finance/public/journal-planning";
import { snapshotMoney } from "@erpx/server-finance/public/company-money";
import { preparePayslipIssue } from "@erpx/server-people_payroll/public/payroll-payslips";
import { preparePayrollDeductionConsumption } from "@erpx/server-people_payroll/public/payroll-deductions";
import { calculatePayrollAdjustmentRun } from "@erpx/server-people_payroll/adapters/payroll-adjustments";

/** Test-only payroll-run seed. The production create/calculate/post commands are
 * native; this preserves the original run, item and posting rows the retired
 * TypeScript writers produced so retained approval, correction, remittance,
 * export and payment-batch oracles can still exercise their own commands. */
export async function createPayrollRun(actor:WorkContext,input:{payGroupId:string;countryPackId?:string|null;periodStart:string;periodEnd:string;payDate:string;currency:string;mode?:"preview"|"production"|"final_pay"},key:string){
 const id=`payrun-${key}`,now=Date.now(),mode=input.mode??"preview";
 await expenseDb.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'calculating',?,?,?)").bind(id,actor.companyId,input.payGroupId,input.countryPackId??null,input.periodStart,input.periodEnd,input.payDate,input.currency,mode,actor.userId,now,now).run();
 return {id,status:"calculating",mode,version:1};
}

type RunRow = {
 id:string;company_id:string;pay_group_id:string;country_pack_id:string|null;period_start:string;period_end:string;pay_date:string;
 currency:string;mode:string;status:string;version:number;adjustment_of_run_id:string|null;scope_engagement_id?:string|null;
};
type EngagementRow = {id:string;version:number;person_id:string;worker_type:string;employment_type:string;start_date:string;end_date:string|null;job_title:string};

async function fingerprint(actor:WorkContext,operation:string,recordId:string,input:unknown){
 const payload={actor:actor.userId,assignment:actor.assignmentId,role:actor.role,authority:actor.authority,company:actor.companyId,branch:actor.branchId,department:actor.departmentId,operation,recordId,input};
 return sha256(JSON.stringify(payload));
}

async function receipt(db:SqlDatabase,key:string,actor:WorkContext,operation:string,recordId:string,input:unknown,result:unknown){
 const now=Date.now();
 await db.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?)")
  .bind(key,operation,actor.userId,await fingerprint(actor,operation,recordId,input),recordId,JSON.stringify(result),now,JSON.stringify({role:actor.role,assignmentId:actor.assignmentId,iam:actor.iam,...actor.authority,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId})).run();
}

/** Seeds the calculation receipt, run preview totals and payroll items the
 * native calculate command produces. */
export async function calculatePayrollRun(actor:WorkContext,runId:string,key=crypto.randomUUID()){
 const previous=await expenseDb.prepare("SELECT result_json FROM command_receipts WHERE command_id=? AND operation='people.payroll.calculate'").bind(key).first<{result_json:string}>();
 if(previous)return JSON.parse(previous.result_json) as {id:string;status:string;version:number;totals:Record<string,number>};
 return expenseDb.transaction(async db=>{
  const run=await db.prepare("SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<RunRow>();
  if(!run)throw Object.assign(new Error("Payroll run not found."),{status:404,code:"PAYROLL_RUN_NOT_FOUND"});
  if(!["draft","calculating","preview","failed"].includes(run.status)||!["preview","final_pay","production"].includes(run.mode))
   throw Object.assign(new Error("Only an unapproved payroll preview can be recalculated."),{status:409,code:"PAYROLL_LOCKED"});
  if(run.adjustment_of_run_id)return calculatePayrollAdjustmentRun(db,actor,run,key);
  const scope=run.scope_engagement_id??null;
  const engagements=await db.prepare("SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,e.end_date,e.job_title,e.branch_id,e.department_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id").bind(actor.companyId,run.pay_group_id,run.currency,run.period_end,run.period_start,scope,scope).all<EngagementRow>();
  if(!engagements.results.length)throw Object.assign(new Error("Assign active employees to this pay group before calculating."),{status:422,code:"PAYROLL_EMPTY"});
  const group=await db.prepare("SELECT overtime_multiplier_bps,proration_method FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(run.pay_group_id,actor.companyId).first<{overtime_multiplier_bps:number;proration_method:"calendar_days"|"none"}>();
  if(!group)throw Object.assign(new Error("The pay group is no longer active."),{status:409,code:"PAY_GROUP_CHANGED"});
  const policy={overtimeMultiplierBps:Number(group.overtime_multiplier_bps),prorationMethod:group.proration_method};
  const items=await Promise.all(engagements.results.map(e=>calculatePayrollItemForSystem(db,run,e,policy,false)));
  const total=items.reduce((a,i)=>({gross:a.gross+i.gross,deductions:a.deductions+i.deductions,employer:a.employer+i.employer,net:a.net+i.net}),{gross:0,deductions:0,employer:0,net:0});
  const digest=await sha256(JSON.stringify(items.map(i=>i.inputSnapshot))),now=Date.now();
  const result={id:runId,status:"preview",version:run.version+1,totals:{...total,headcount:items.length}};
  await receipt(db,key,actor,"people.payroll.calculate",runId,{},result);
  await db.batch([
   db.prepare("DELETE FROM hr_payroll_items WHERE run_id=?").bind(runId),
   db.prepare("UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=?,input_digest=?,version=version+1,updated_at=?,error_message=NULL WHERE id=?").bind(JSON.stringify({...total,headcount:items.length}),digest,now,runId),
   ...items.map((i,index)=>db.prepare("INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(`payitem-${key}-${index}`,runId,i.engagementId,i.workerType,JSON.stringify(i.inputSnapshot),JSON.stringify(i.result),i.gross,i.deductions,i.employer,i.net,run.currency,now)),
  ]);
  return result;
 });
}

/** Seeds the posting receipt, posted run/items, journal, posting, payslips,
 * deduction consumption and money snapshot the native post command produces. */
export async function postPayroll(actor:WorkContext,runId:string,input:z.infer<typeof payrollPostSchema>,key:string){
 const previous=await expenseDb.prepare("SELECT result_json FROM command_receipts WHERE command_id=? AND operation='people.payroll.post'").bind(key).first<{result_json:string}>();
 if(previous)return JSON.parse(previous.result_json) as {id:string;status:string;version:number;journalId:string;netPayableMinor:number;payslipIds:string[]};
 return expenseDb.transaction(async db=>{
  const run=await db.prepare("SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=? FOR UPDATE").bind(runId,actor.companyId).first<RunRow>();
  if(!run)throw Object.assign(new Error("Payroll run not found."),{status:404,code:"PAYROLL_RUN_NOT_FOUND"});
  if(run.status!=="approved")throw Object.assign(new Error("An independently approved payroll run is required before GL posting."),{status:409,code:"PAYROLL_NOT_APPROVED"});
  if(Number(run.version)!==input.expectedVersion)throw Object.assign(new Error("The payroll run changed. Refresh before posting."),{status:409,code:"VERSION_CONFLICT"});
  const mapping=input.accountMappingId
   ?await db.prepare("SELECT id,chart_id,accounts_json,version FROM hr_payroll_account_mappings WHERE id=? AND company_id=?").bind(input.accountMappingId,actor.companyId).first<{id:string;chart_id:string;accounts_json:string;version:number}>()
   :await db.prepare("SELECT id,chart_id,accounts_json,version FROM hr_payroll_account_mappings WHERE company_id=? AND pay_group_id=? AND status='approved' ORDER BY created_at DESC,id DESC LIMIT 1").bind(actor.companyId,run.pay_group_id).first<{id:string;chart_id:string;accounts_json:string;version:number}>();
  if(!mapping)throw Object.assign(new Error("An independently approved pay-group account mapping is required before payroll posting."),{status:409,code:"PAYROLL_ACCOUNT_MAPPING_REQUIRED"});
  const merged={...JSON.parse(mapping.accounts_json) as Record<string,string|undefined>,...input,chartId:mapping.chart_id};
  const entryDate=input.entryDate??run.period_end;
  if(!merged.expenseAccountId||!merged.netPayableAccountId)throw Object.assign(new Error("The approved mapping needs wage expense and net payable accounts."),{status:422,code:"PAYROLL_ACCOUNT_MAPPING_REQUIRED"});
  if(run.mode==="production"||run.country_pack_id)await assertStatutoryRunReady(db,runId);
  const items=(await db.prepare("SELECT id,gross_minor,deductions_minor,employer_cost_minor,net_minor,input_snapshot_json,result_json,worker_type FROM hr_payroll_items WHERE run_id=? ORDER BY id").bind(runId).all<PayrollPostingItem&{worker_type:string}>()).results;
  if(!items.length||items.some(item=>item.worker_type!=="employee"))throw Object.assign(new Error("Every payroll item must be a calculated employee item before posting."),{status:409,code:"PAYROLL_ITEMS_INVALID"});
  const accrual=payrollAccountingLines(items,merged as Parameters<typeof payrollAccountingLines>[1]);
  const journal=await prepareJournal(db,actor,{chartId:mapping.chart_id,periodId:input.periodId!,entryDate,currency:run.currency,memo:`Payroll ${run.period_start} to ${run.period_end}`,sourceType:"payroll",sourceId:runId,isOpening:false,lines:accrual.lines},`${key}-journal`,"posted");
  const money=await snapshotMoney(db,actor,{sourceType:"payroll_run",sourceId:runId,amountKind:"net_payable",transactionDate:run.pay_date,currency:run.currency,currencyScale:2,amountUnits:String(accrual.totals.netMinor)});
  const payslips=await preparePayslipIssue(db,actor,runId,String(journal.result.id),key,input.expectedVersion+1);
  const deductions=await preparePayrollDeductionConsumption(db,actor,runId,String(journal.result.id),merged.advanceReceivableAccountId,key);
  const now=Date.now(),result={id:runId,status:"posted",version:input.expectedVersion+1,journalId:String(journal.result.id),netPayableMinor:accrual.totals.netMinor,payslipIds:payslips.ids};
  await receipt(db,key,actor,"people.payroll.post",runId,input,result);
  await db.batch([
   ...journal.steps,
   money.statement,
   db.prepare("UPDATE hr_payroll_runs SET status='posted',posted_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(actor.userId,now,runId,actor.companyId),
   db.prepare("UPDATE hr_payroll_items SET status='posted' WHERE run_id=? AND status IN ('calculated','approved')").bind(runId),
   db.prepare("INSERT INTO hr_payroll_postings(id,run_id,journal_id,account_mapping_id,account_mapping_version,created_at) VALUES(?,?,?,?,?,?)").bind(`payroll-posting-${key}`,runId,journal.result.id,mapping.id,mapping.version,now),
   ...deductions.steps,
   ...payslips.steps,
  ]);
  return result;
 });
}
