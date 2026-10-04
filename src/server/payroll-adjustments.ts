import "server-only";
import { z } from "zod";
import { minorToDecimal } from "@/lib/finance-money";
import { calculateThaiPayrollAdjustment, type ThaiPayrollAdjustmentInput } from "@/lib/thai-payroll-adjustment";
import { thaiPayroll2026Pack, thaiPayrollAdapterId } from "@/lib/thai-payroll-pack";
import { ThaiPayrollRuleError, type ThaiPayrollCalculation } from "@/lib/thai-payroll-rules";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { snapshotMoney } from "./company-money";
import { assertProductionModuleEnabled,productionFeatureGuard } from "./hospitality-rollout";
import { thaiEvidenceForEmployee, thaiEvidenceGuards, thaiPackForRun, type PayrollStatutoryRun } from "./payroll-statutory";

type Row=Record<string,unknown>;
export const payrollAdjustmentValuesSchema=z.object({
  taxableArrearsMinor:z.number().int().nonnegative().safe(), nonTaxableArrearsMinor:z.number().int().nonnegative().safe().default(0),
  socialSecurityWageMinor:z.number().int().nonnegative().safe().default(0),
  timeEntryIds:z.array(z.string().min(1)).max(100).default([]),
  reason:z.string().trim().min(10).max(2000), evidenceRef:z.string().trim().min(8).max(2000),
}).strict().refine(input=>input.socialSecurityWageMinor<=input.taxableArrearsMinor,{message:"Contribution wages cannot exceed taxable arrears."});
export const payrollAdjustmentCreateSchema=z.object({expectedVersion:z.number().int().positive(),originalItemId:z.string().min(1),countryPackId:z.string().min(1),payDate:z.string().date(),sourceRef:z.string().trim().min(8).max(200),values:payrollAdjustmentValuesSchema}).strict();
export const payrollAdjustmentEditSchema=z.object({expectedVersion:z.number().int().positive(),values:payrollAdjustmentValuesSchema}).strict();
type Values=z.infer<typeof payrollAdjustmentValuesSchema>;
type Origin={run:Row;item:Row;slips:Row[]};
const normalize=(value:unknown):unknown=>Array.isArray(value)?value.map(normalize):value&&typeof value==="object"?Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([key,entry])=>[key,normalize(entry)])):value;
const equal=(a:unknown,b:unknown)=>JSON.stringify(normalize(a))===JSON.stringify(normalize(b));
function companyScope(actor:WorkContext){if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company Payroll assignment for a posted-payroll adjustment.");}

export async function originalSource(db:SqlDatabase,companyId:string,runId:string,itemId:string):Promise<Origin>{
 const run=await db.prepare("SELECT to_jsonb(r)::text source FROM hr_payroll_runs r WHERE id=? AND company_id=? AND status='posted' AND mode='production' AND adjustment_of_run_id IS NULL").bind(runId,companyId).first<{source:string}>();
 const item=await db.prepare("SELECT to_jsonb(i)::text source FROM hr_payroll_items i WHERE id=? AND run_id=? AND status='posted'").bind(itemId,runId).first<{source:string}>();
 if(!run||!item)throw new ApiError(404,"PAYROLL_ADJUSTMENT_SOURCE_NOT_FOUND","Select a posted statutory employee item in this Company.");
 const slips=(await db.prepare("SELECT id,source_digest,export_sha256 FROM hr_issued_payslips WHERE run_id=? AND item_id=? ORDER BY id").bind(runId,itemId).all<Row>()).results;
 if(slips.length!==1)throw new ApiError(409,"PAYROLL_ADJUSTMENT_SLIP_REQUIRED","The original posted employee payslip must be retained before correction.");
 return {run:JSON.parse(run.source),item:JSON.parse(item.source),slips};
}
async function timeSources(db:SqlDatabase,origin:Origin,values:Values){
 const ids=[...new Set(values.timeEntryIds)].sort();if(ids.length!==values.timeEntryIds.length)throw new ApiError(422,"PAYROLL_ADJUSTMENT_TIME_DUPLICATE","Select each late time source once.");
 const original=JSON.parse(String(origin.item.input_snapshot_json)) as {hours:{entries:Array<{id:string}>};costAssignments:Array<{id:string;effective_from:string;effective_to:string|null;branch_id:string|null;department_id:string|null;evidence_status:string}>};
 const entries=(await db.prepare(`SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries WHERE engagement_id=? AND company_id=? AND status='approved' ${ids.length?`AND id IN (${ids.map(()=>"?").join(",")})`:"AND FALSE"} ORDER BY id`).bind(origin.item.engagement_id,origin.run.company_id,...ids).all<Row>()).results;
 if(entries.length!==ids.length||entries.some(entry=>String(entry.work_date)<String(origin.run.period_start)||String(entry.work_date)>String(origin.run.period_end)||Number(entry.regular_hours)!==0||Number(entry.overtime_hours)<=0||original.hours.entries.some(old=>old.id===entry.id)))throw new ApiError(422,"PAYROLL_ADJUSTMENT_TIME_INVALID","Use independently approved late overtime, within the original work period, with zero regular hours and absent from its posted inputs.");
 for(const entry of entries){const costs=original.costAssignments.filter(cost=>cost.effective_from<=String(entry.work_date)&&(cost.effective_to===null||cost.effective_to>=String(entry.work_date)));if(costs.length!==1||costs[0].evidence_status!=="verified"||entry.cost_assignment_id!==costs[0].id||entry.cost_branch_id!==costs[0].branch_id||entry.cost_department_id!==costs[0].department_id)throw new ApiError(422,"PAYROLL_TIME_COST_EVIDENCE_MISSING","Late time must retain the verified original work placement.");}
 if(values.taxableArrearsMinor+values.nonTaxableArrearsMinor===0&&!entries.length)throw new ApiError(422,"PAYROLL_ADJUSTMENT_EMPTY","Record an evidenced arrear or an approved late overtime source.");
 return entries;
}
async function revisionStatements(db:SqlDatabase,actor:WorkContext,runId:string,revision:number,payDate:string,values:Values,key:string){
 const id=`payroll-adjustment-revision-${key}`,now=Date.now(),money=await Promise.all([
  snapshotMoney(db,actor,{sourceType:"payroll_adjustment_revision",sourceId:id,amountKind:"taxable_arrears",transactionDate:payDate,currency:"THB",currencyScale:2,amountUnits:String(values.taxableArrearsMinor)}),
  snapshotMoney(db,actor,{sourceType:"payroll_adjustment_revision",sourceId:id,amountKind:"non_taxable_arrears",transactionDate:payDate,currency:"THB",currencyScale:2,amountUnits:String(values.nonTaxableArrearsMinor)}),
 ]);
 return [db.prepare("INSERT INTO hr_payroll_adjustment_revisions(id,run_id,revision,taxable_arrears_minor,non_taxable_arrears_minor,social_security_wage_minor,time_entries_json,reason,evidence_ref,created_by,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,runId,revision,values.taxableArrearsMinor,values.nonTaxableArrearsMinor,values.socialSecurityWageMinor,JSON.stringify([...values.timeEntryIds].sort()),values.reason,values.evidenceRef,actor.userId,key,now),...values.timeEntryIds.map(timeId=>db.prepare("INSERT INTO hr_payroll_adjustment_time_claims(run_id,source_revision,time_entry_id) VALUES(?,?,?)").bind(runId,revision,timeId)),...money.map(item=>item.statement)];
}
export async function createPayrollAdjustment(actor:WorkContext,originalRunId:string,input:z.infer<typeof payrollAdjustmentCreateSchema>,key:string){
 actor=requirePermission(actor,"payroll.prepare");companyScope(actor);const {write}=await holdingResources(actor.holdingId),id=`payrun-adjustment-${key}`,previous=await replayCommand(write,actor,key,"people.payroll.adjustment.create",id,{originalRunId,...input});if(previous)return previous;
 await assertProductionModuleEnabled(write,actor.companyId,'people_payroll');
 const origin=await originalSource(write,actor.companyId,originalRunId,input.originalItemId);
 if(Number(origin.run.version)!==input.expectedVersion)throw new ApiError(409,"PAYROLL_ADJUSTMENT_SOURCE_CHANGED","Reload the original posted payroll before preparing its correction.");
 if(input.payDate<=String(origin.run.pay_date)||input.payDate.slice(0,4)!==String(origin.run.pay_date).slice(0,4))throw new ApiError(422,"PAYROLL_ADJUSTMENT_PAY_DATE","Use a later payment in the same verified tax year; cross-year income requires a new reviewed source adapter.");
 const run={id,company_id:actor.companyId,pay_group_id:String(origin.run.pay_group_id),country_pack_id:input.countryPackId,period_start:String(origin.run.period_start),period_end:String(origin.run.period_end),pay_date:input.payDate,currency:"THB",mode:"production",adjustment_of_run_id:originalRunId};
 const pack=await thaiPackForRun(write,run),originResult=JSON.parse(String(origin.item.result_json)) as {statutory:{sourceDigest:string;input:ThaiPayrollCalculation}};
 if(originResult.statutory.sourceDigest!==pack.sourceDigest||originResult.statutory.input.regularTaxableMinor<=0)throw new ApiError(409,"PAYROLL_ADJUSTMENT_BASIS_REQUIRED","The current reviewed pack must match the original statutory salary basis.");
 const entries=await timeSources(write,origin,input.values),now=Date.now(),source=JSON.stringify(origin);
 return executeCommand(write,actor,key,"people.payroll.adjustment.create",{originalRunId,...input},id,1,{id,status:"calculating",version:1,originalRunId},[
  ...productionFeatureGuard(write,key,actor.companyId,'people_payroll'),
  write.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`payroll-adjustment:${actor.companyId}:${origin.item.engagement_id}`),
  assertion(write,key,"adjustment-original","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)",[originalRunId,actor.companyId,input.expectedVersion]),
  ...entries.map(entry=>assertion(write,key,`adjustment-time-${entry.id}`,"EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND company_id=? AND engagement_id=? AND status='approved' AND version=?)",[String(entry.id),actor.companyId,String(origin.item.engagement_id),Number(entry.version)])),
  write.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,adjustment_of_run_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'THB','production','calculating',?,?,?,?)").bind(id,actor.companyId,run.pay_group_id,input.countryPackId,origin.item.engagement_id,run.period_start,run.period_end,input.payDate,originalRunId,actor.userId,now,now),
  write.prepare("INSERT INTO hr_payroll_adjustments(run_id,company_id,original_run_id,original_item_id,engagement_id,source_ref,source_snapshot_json,source_digest,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,originalRunId,input.originalItemId,origin.item.engagement_id,input.sourceRef,source,await sha256(source),actor.userId,now),
  ...await revisionStatements(write,actor,id,1,input.payDate,input.values,key),
 ]);
}
export async function editPayrollAdjustment(actor:WorkContext,runId:string,input:z.infer<typeof payrollAdjustmentEditSchema>,key:string){
 actor=requirePermission(actor,"payroll.prepare");companyScope(actor);const {write}=await holdingResources(actor.holdingId),previous=await replayCommand(write,actor,key,"people.payroll.adjustment.edit",runId,input);if(previous)return previous;
 const row=await write.prepare("SELECT a.*,r.status,r.submitted_by,r.created_by preparer,r.version,r.pay_date FROM hr_payroll_adjustments a JOIN hr_payroll_runs r ON r.id=a.run_id WHERE a.run_id=? AND a.company_id=?").bind(runId,actor.companyId).first<Row>();
 if(!row)throw new ApiError(404,"PAYROLL_ADJUSTMENT_NOT_FOUND","Payroll adjustment not found.");
 await assertProductionModuleEnabled(write,actor.companyId,'people_payroll');
 if(Number(row.version)!==input.expectedVersion||!['draft','calculating','failed','preview'].includes(String(row.status))||row.submitted_by)throw new ApiError(409,"PAYROLL_ADJUSTMENT_LOCKED","Only the current unsubmitted correction can be edited.");
 if(!samePerson(actor,String(row.preparer)))throw new ApiError(403,"PAYROLL_ADJUSTMENT_OWNER_REQUIRED","The correction preparer must edit its source.");
 await timeSources(write,JSON.parse(String(row.source_snapshot_json)),input.values);const revision=Number(row.source_revision)+1,now=Date.now();
 return executeCommand(write,actor,key,"people.payroll.adjustment.edit",input,runId,input.expectedVersion+1,{id:runId,status:"calculating",version:input.expectedVersion+1,sourceRevision:revision},[
  ...productionFeatureGuard(write,key,actor.companyId,'people_payroll'),
  write.prepare("SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE").bind(runId),
  assertion(write,key,"adjustment-edit-version","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status IN ('draft','calculating','failed','preview') AND submitted_by IS NULL)",[runId,actor.companyId,input.expectedVersion]),
  write.prepare("UPDATE hr_payroll_adjustment_time_claims SET active=false WHERE run_id=? AND active").bind(runId),
  write.prepare("UPDATE hr_payroll_adjustments SET source_revision=source_revision+1 WHERE run_id=?").bind(runId),
  ...await revisionStatements(write,actor,runId,revision,String(row.pay_date),input.values,key),
  write.prepare("UPDATE hr_payroll_runs SET status='calculating',input_digest=NULL,totals_json='{}',version=version+1,updated_at=? WHERE id=?").bind(now,runId),
 ]);
}

/** Current sources are re-read before calculation, submission, approval and posting. */
export async function payrollAdjustmentInputs(db:SqlDatabase,run:PayrollStatutoryRun){
 const row=await db.prepare("SELECT a.*,v.id revision_id,v.taxable_arrears_minor,v.non_taxable_arrears_minor,v.social_security_wage_minor,v.time_entries_json,v.reason,v.evidence_ref FROM hr_payroll_adjustments a JOIN hr_payroll_adjustment_revisions v ON v.run_id=a.run_id AND v.revision=a.source_revision WHERE a.run_id=? AND a.company_id=? AND a.active").bind(run.id,run.company_id).first<Row>();
 if(!row)throw new ApiError(409,"PAYROLL_ADJUSTMENT_SOURCE_CHANGED","The active correction source is missing.");
 const origin=await originalSource(db,run.company_id,String(row.original_run_id),String(row.original_item_id));if(!equal(origin,JSON.parse(String(row.source_snapshot_json))))throw new ApiError(409,"PAYROLL_ADJUSTMENT_SOURCE_CHANGED","The original posted source does not match its retained correction evidence.");
 const values={taxableArrearsMinor:Number(row.taxable_arrears_minor),nonTaxableArrearsMinor:Number(row.non_taxable_arrears_minor),socialSecurityWageMinor:Number(row.social_security_wage_minor),timeEntryIds:JSON.parse(String(row.time_entries_json)) as string[],reason:String(row.reason),evidenceRef:String(row.evidence_ref)},entries=await timeSources(db,origin,values);
 const original=JSON.parse(String(origin.item.input_snapshot_json)) as Row,engagement=await db.prepare("SELECT id,version,person_id,worker_type,employment_type,start_date,end_date,job_title,branch_id,department_id FROM hr_engagements WHERE id=? AND company_id=? AND status IN ('active','ended')").bind(row.engagement_id,run.company_id).first<Row>();
 const payee=await db.prepare("SELECT id,version,legal_name,preferred_name FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL").bind(engagement?.person_id??"",run.company_id).first<Row>();
 if(!engagement||!payee)throw new ApiError(422,"PAYROLL_PAYEE_INVALID","Verify the original employee identity before correcting payroll.");
 const priorAdjustments=(await db.prepare("SELECT a.run_id,i.result_json,r.input_digest FROM hr_payroll_adjustments a JOIN hr_payroll_runs r ON r.id=a.run_id JOIN hr_payroll_items i ON i.run_id=r.id WHERE a.original_item_id=? AND r.status='posted' AND r.id<>? ORDER BY r.pay_date,r.id").bind(row.original_item_id,run.id).all<Row>()).results;
 return {...original,compensation:original.compensation,policy:original.policy,costAssignments:original.costAssignments,engagement,payee,hours:{regular:0,overtime:entries.reduce((sum,entry)=>sum+Number(entry.overtime_hours),0),entries,handoffs:[]},benefitInputs:[],advanceRepayments:[],period:{start:run.period_start,end:run.period_end},thaiStatutory:await thaiEvidenceForEmployee(db,run,String(row.engagement_id)),adjustment:{runId:run.id,originalRunId:row.original_run_id,originalItemId:row.original_item_id,sourceRef:row.source_ref,sourceDigest:row.source_digest,sourceRevision:Number(row.source_revision),revisionId:row.revision_id,values,originalStatutory:JSON.parse(String(origin.item.result_json)).statutory,priorAdjustments}};
}
export async function calculatePayrollAdjustmentItem(db:SqlDatabase,run:PayrollStatutoryRun){
 const inputSnapshot=await payrollAdjustmentInputs(db,run),source=inputSnapshot.adjustment,original=source.originalStatutory as {input:ThaiPayrollCalculation;calculation:{employeeSsoMinor:number;employerSsoMinor:number}},compensation=inputSnapshot.compensation as Row,policy=inputSnapshot.policy as {overtimeMultiplierBps:number};
 const overtime=Math.round(Number(compensation.hourly_rate_minor??0)*inputSnapshot.hours.overtime*Number(policy.overtimeMultiplierBps)/10000);
 if(inputSnapshot.hours.overtime>0&&Number(compensation.hourly_rate_minor??0)<=0)throw new ApiError(422,"OVERTIME_RATE_MISSING","The original approved overtime rate is required.");
 const evidence=inputSnapshot.thaiStatutory,profile=evidence.profile,prior=evidence.priorItems.map(item=>JSON.parse(item.result_json).statutory),previous=source.priorAdjustments.map(item=>JSON.parse(String(item.result_json)).statutory);
 const input:ThaiPayrollAdjustmentInput={payDate:run.pay_date!,regularTaxableMinor:0,irregularTaxableMinor:source.values.taxableArrearsMinor+overtime,nonTaxableMinor:source.values.nonTaxableArrearsMinor,ordinaryTaxableBasisMinor:original.input.regularTaxableMinor,
  irregularPaidBeforeMinor:profile.opening_irregular_minor+prior.reduce((sum,item)=>sum+item.input.irregularTaxableMinor,0),socialSecurityWageMinor:source.values.socialSecurityWageMinor,socialSecurityEligible:profile.social_security_eligible,socialSecurityExemptionEvidence:profile.social_security_exemption_evidence??undefined,
  originalSocialSecurityWageMinor:original.input.socialSecurityWageMinor,originalEmployeeSsoMinor:original.calculation.employeeSsoMinor,originalEmployerSsoMinor:original.calculation.employerSsoMinor,
  priorAdjustmentSocialSecurityWageMinor:previous.reduce((sum,item)=>sum+item.input.socialSecurityWageMinor,0),priorAdjustmentEmployeeSsoMinor:previous.reduce((sum,item)=>sum+item.calculation.employeeSsoMinor,0),priorAdjustmentEmployerSsoMinor:previous.reduce((sum,item)=>sum+item.calculation.employerSsoMinor,0),
  otherEmployeeDeductionsMinor:0,verifiedAnnualDeductionsMinor:profile.annual_deductions_minor,taxablePaidBeforeMinor:profile.opening_taxable_minor+prior.reduce((sum,item)=>sum+item.input.regularTaxableMinor+item.input.irregularTaxableMinor,0),taxWithheldBeforeMinor:profile.opening_withheld_minor+prior.reduce((sum,item)=>sum+item.calculation.taxWithheldMinor,0),scheduledPaysInYear:profile.scheduled_pays,lastPayInYear:run.pay_date!.slice(5,7)==='12',priorIncomeComplete:profile.prior_income_complete};
 let calculation:ReturnType<typeof calculateThaiPayrollAdjustment>;try{calculation=calculateThaiPayrollAdjustment(input,thaiPayroll2026Pack.rules);}catch(error){if(error instanceof ThaiPayrollRuleError)throw new ApiError(422,error.code,error.message);throw error;}
 const components=[{code:"ARREARS",label:"Reviewed arrears",kind:"earning",amountMinor:source.values.taxableArrearsMinor+source.values.nonTaxableArrearsMinor},{code:"STATUTORY:TAX",label:"Thai correction withholding tax",kind:"deduction",amountMinor:calculation.taxWithheldMinor},{code:"STATUTORY:SSO",label:"Section 33 employee contribution difference",kind:"deduction",amountMinor:calculation.employeeSsoMinor},{code:"STATUTORY:SSO:EMPLOYER",label:"Section 33 employer contribution difference",kind:"employer",amountMinor:calculation.employerSsoMinor}].map(item=>({...item,amount:minorToDecimal(item.amountMinor)}));
 const result={grossMinor:calculation.grossMinor,deductionsMinor:calculation.grossMinor-calculation.netMinor,employerCostMinor:calculation.employerSsoMinor,netMinor:calculation.netMinor,statutory:{adapterId:thaiPayrollAdapterId,calculationKind:"adjustment",sourceDigest:evidence.pack.sourceDigest,certified:false,input,calculation},explanation:{baseMinor:0,hourlyRateMinor:Number(compensation.hourly_rate_minor??0),regularHours:0,overtimeHours:inputSnapshot.hours.overtime,overtimeMinor:overtime,overtimeMultiplierBps:Number(policy.overtimeMultiplierBps),components,adjustmentOfRunId:source.originalRunId,originalItemId:source.originalItemId,sourceRef:source.sourceRef}};
 return {engagementId:String(inputSnapshot.engagement.id),personId:String(inputSnapshot.engagement.person_id),workerType:"employee",inputSnapshot,result,gross:calculation.grossMinor,deductions:result.deductionsMinor,employer:calculation.employerSsoMinor,net:calculation.netMinor};
}
export async function calculatePayrollAdjustmentRun(db:SqlDatabase,actor:WorkContext,run:PayrollStatutoryRun & {version:number},key:string){
 const item=await calculatePayrollAdjustmentItem(db,run),totals={gross:item.gross,deductions:item.deductions,employer:item.employer,net:item.net,headcount:1},digest=await sha256(JSON.stringify([item.inputSnapshot])),now=Date.now();
 return executeCommand(db,actor,key,"people.payroll.calculate",{},run.id,run.version+1,{id:run.id,status:"preview",version:run.version+1,totals},[
  db.prepare("SELECT id FROM hr_payroll_runs WHERE id=? FOR UPDATE").bind(run.id),
  assertion(db,key,"adjustment-run-version","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status IN ('draft','calculating','preview','failed') AND (submitted_by IS NULL OR status='calculating' AND approved_by IS NULL))",[run.id,actor.companyId,run.version]),
  ...await payrollAdjustmentGuards(db,key,run,item.inputSnapshot),
  db.prepare("DELETE FROM hr_payroll_items WHERE run_id=?").bind(run.id),
  db.prepare("UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=?,input_digest=?,version=version+1,error_message=NULL,updated_at=? WHERE id=?").bind(JSON.stringify(totals),digest,now,run.id),
  db.prepare("INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES(?,?,?,'employee',?,?,?,?,?,?,'THB',?)").bind(`payitem-${key}-adjustment`,run.id,item.engagementId,JSON.stringify(item.inputSnapshot),JSON.stringify(item.result),item.gross,item.deductions,item.employer,item.net,now),
 ]);
}
export async function payrollAdjustmentSourceFresh(db:SqlDatabase,run:PayrollStatutoryRun){
 const row=await db.prepare("SELECT input_snapshot_json FROM hr_payroll_items WHERE run_id=?").bind(run.id).first<{input_snapshot_json:string}>();if(!row)throw new ApiError(409,"PREVIEW_REQUIRED","Calculate the correction before reviewing it.");
 if(!equal(JSON.parse(row.input_snapshot_json),await payrollAdjustmentInputs(db,run)))throw new ApiError(409,"PAYROLL_ADJUSTMENT_SOURCE_CHANGED","Correction inputs or actual prior payroll changed; recalculate before review.");
}
export async function payrollAdjustmentPanel(actor:WorkContext,runId:string){
 requirePermission(actor,'payroll.read');companyScope(actor);const {write}=await holdingResources(actor.holdingId),run=await write.prepare('SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?').bind(runId,actor.companyId).first<Row>();if(!run)throw new ApiError(404,'PAYROLL_RUN_NOT_FOUND','Payroll run not found.');
 const adjustments=(await write.prepare('SELECT r.id,r.status,r.pay_date,r.version,a.source_ref FROM hr_payroll_runs r JOIN hr_payroll_adjustments a ON a.run_id=r.id WHERE a.original_run_id=? AND a.company_id=? ORDER BY r.created_at,r.id').bind(runId,actor.companyId).all<Row>()).results;
 const source=await write.prepare('SELECT a.*,v.taxable_arrears_minor,v.non_taxable_arrears_minor,v.social_security_wage_minor,v.time_entries_json,v.reason,v.evidence_ref FROM hr_payroll_adjustments a JOIN hr_payroll_adjustment_revisions v ON v.run_id=a.run_id AND v.revision=a.source_revision WHERE a.run_id=? AND a.company_id=?').bind(runId,actor.companyId).first<Row>();
 const revisions=source?(await write.prepare('SELECT revision,taxable_arrears_minor,non_taxable_arrears_minor,time_entries_json,reason,created_at FROM hr_payroll_adjustment_revisions WHERE run_id=? ORDER BY revision').bind(runId).all<Row>()).results:[];
 const timeOptions=run.status==='posted'&&run.mode==='production'&&!run.adjustment_of_run_id?(await write.prepare("SELECT t.id,t.engagement_id,t.work_date,t.overtime_hours,p.legal_name FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id JOIN hr_people p ON p.id=e.person_id JOIN hr_payroll_items i ON i.engagement_id=e.id AND i.run_id=? WHERE t.company_id=? AND t.status='approved' AND t.regular_hours=0 AND t.overtime_hours>0 AND t.work_date BETWEEN ? AND ? AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(i.input_snapshot_json::jsonb->'hours'->'entries') old WHERE old->>'id'=t.id) AND NOT EXISTS(SELECT 1 FROM hr_payroll_adjustment_time_claims c WHERE c.time_entry_id=t.id AND c.active) ORDER BY t.work_date,t.id").bind(runId,actor.companyId,run.period_start,run.period_end).all<Row>()).results:[];
 return {adjustments,source,revisions,timeOptions};
}
export async function payrollAdjustmentGuards(db:SqlDatabase,key:string,run:PayrollStatutoryRun,input:Awaited<ReturnType<typeof payrollAdjustmentInputs>>):Promise<SqlStatement[]>{
 const source=input.adjustment,guards=thaiEvidenceGuards(db,key,input.thaiStatutory,run,String(input.engagement.id));
 guards.push(db.prepare("SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE").bind(run.id),assertion(db,key,"adjustment-revision","EXISTS(SELECT 1 FROM hr_payroll_adjustments WHERE run_id=? AND company_id=? AND source_revision=? AND source_digest=? AND active)",[run.id,run.company_id,source.sourceRevision,String(source.sourceDigest)]),
  assertion(db,key,"adjustment-current-payee","EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND version=? AND status='active' AND merged_into_id IS NULL)",[String(input.payee.id),run.company_id,Number(input.payee.version)]),
  assertion(db,key,"adjustment-current-engagement","EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status IN ('active','ended'))",[String(input.engagement.id),run.company_id,Number(input.engagement.version)]),
  assertion(db,key,"adjustment-prior-count","(SELECT COUNT(*) FROM hr_payroll_adjustments a JOIN hr_payroll_runs r ON r.id=a.run_id WHERE a.original_item_id=? AND r.status='posted' AND r.id<>?)=?",[String(source.originalItemId),run.id,source.priorAdjustments.length]));
 for(const entry of input.hours.entries)guards.push(db.prepare("SELECT id FROM hr_time_entries WHERE id=? FOR SHARE").bind(entry.id),assertion(db,key,`adjustment-current-time-${entry.id}`,"EXISTS(SELECT 1 FROM hr_time_entries t JOIN hr_payroll_adjustment_time_claims c ON c.time_entry_id=t.id WHERE t.id=? AND t.version=? AND t.status='approved' AND c.run_id=? AND c.source_revision=? AND c.active)",[String(entry.id),Number(entry.version),run.id,source.sourceRevision]));
 return guards;
}
