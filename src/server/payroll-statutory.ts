import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { ApiError } from "./http";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { samePerson } from "@/lib/iam";
import { assertion, executeCommand, replayCommand } from "./commands";
import { sha256 } from "./crypto";
import { thaiPayroll2026Pack, thaiPayrollAdapterId } from "@/lib/thai-payroll-pack";
import { calculateThaiPayroll, ThaiPayrollRuleError, type ThaiPayrollCalculation } from "@/lib/thai-payroll-rules";
import { calculateThaiPayrollAdjustment, type ThaiPayrollAdjustmentInput } from "@/lib/thai-payroll-adjustment";
import { minorToDecimal } from "@/lib/finance-money";

function canonical(value: unknown): string {
  const normalize = (entry: unknown): unknown => Array.isArray(entry) ? entry.map(normalize)
    : entry && typeof entry === "object" ? Object.fromEntries(Object.entries(entry).sort(([a],[b]) => a.localeCompare(b)).map(([key,item]) => [key,normalize(item)])) : entry;
  return JSON.stringify(normalize(value));
}

export async function recognizedThaiPack(rules: unknown, countryCode: string, version: string) {
  if (countryCode !== "TH" || version !== thaiPayroll2026Pack.rules.version || canonical(rules) !== canonical(thaiPayroll2026Pack))
    throw new ApiError(409,"PAYROLL_ADAPTER_RULES_UNSUPPORTED","Install the source-dated Thai rule revision supported by the calculation adapter before validating this pack.");
  return { adapterId: thaiPayrollAdapterId, sourceDigest: await sha256(canonical(thaiPayroll2026Pack)), rules: thaiPayroll2026Pack.rules };
}

export type PayrollStatutoryRun = { id:string;company_id:string;pay_group_id:string;country_pack_id?:string|null;period_start:string;period_end:string;pay_date?:string;currency:string;mode?:string;adjustment_of_run_id?:string|null };
type PackRow = { id:string;country_code:string;version:string;status:string;rules_json:string;adapter_id:string|null;source_digest:string|null;record_version:number };

export async function thaiPackForRun(db: SqlDatabase, run: PayrollStatutoryRun) {
  if (!run.country_pack_id || !run.pay_date || run.currency !== "THB")
    throw new ApiError(409,"PAYROLL_COUNTRY_PACK_REQUIRED","Select an active Thai rule pack and THB payroll currency.");
  const row = await db.prepare("SELECT id,country_code,version,status,rules_json,adapter_id,source_digest,record_version FROM hr_country_packs WHERE id=?").bind(run.country_pack_id).first<PackRow>();
  if (!row || row.status !== "active") throw new ApiError(409,"PAYROLL_COUNTRY_PACK_INACTIVE","The selected payroll rule pack is not active.");
  const recognized = await recognizedThaiPack(JSON.parse(row.rules_json),row.country_code,row.version);
  if (row.adapter_id !== recognized.adapterId || row.source_digest !== recognized.sourceDigest || run.pay_date < recognized.rules.effectiveFrom || run.pay_date > recognized.rules.effectiveTo)
    throw new ApiError(409,"PAYROLL_COUNTRY_PACK_NOT_EFFECTIVE","The adapter and immutable rule evidence must be effective on the pay date.");
  const group = await db.prepare("SELECT country_code,currency,frequency FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(run.pay_group_id,run.company_id).first<{country_code:string;currency:string;frequency:string}>();
  if (!group || group.country_code !== "TH" || group.currency !== "THB" || group.frequency !== "monthly")
    throw new ApiError(409,"PAYROLL_FREQUENCY_ADAPTER_UNSUPPORTED","This Thai adapter aggregates salaried, daily and hourly wages in a monthly Section 33 pay group.");
  return { ...recognized, row };
}

const amount = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
export const thaiPayrollProfileSchema = z.object({
  engagementId: z.string().min(1), expectedEngagementVersion:z.number().int().positive(),
  taxYear:z.number().int().min(2000).max(2200), effectiveFrom:z.string().date(), effectiveTo:z.string().date(),
  verifiedAnnualDeductionsMinor:amount, scheduledPaysInYear:z.number().int().min(1).max(12),
  socialSecurityEligible:z.boolean(),socialSecurityExemptionEvidence:z.string().trim().min(3).max(2000).optional(),
  openingThroughDate:z.string().date(),openingTaxableMinor:amount,openingIrregularMinor:amount,openingWithheldMinor:amount,
  priorIncomeComplete:z.boolean(), evidenceRef:z.string().trim().min(8).max(2000),
}).strict();
export const thaiPayrollProfileDecisionSchema = z.object({ expectedVersion:z.number().int().positive(), action:z.enum(["approve","supersede"]), reason:z.string().trim().min(8).max(2000) }).strict();
type Profile = {id:string;company_id:string;engagement_id:string;tax_year:number;effective_from:string;effective_to:string;annual_deductions_minor:number;scheduled_pays:number;social_security_eligible:boolean;social_security_exemption_evidence:string|null;opening_through_date:string;opening_taxable_minor:number;opening_irregular_minor:number;opening_withheld_minor:number;prior_income_complete:boolean;evidence_ref:string;status:string;created_by:string;approved_by:string|null;version:number};

function companyScope(actor:WorkContext) {
  if(actor.branchId || actor.departmentId) throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company Payroll assignment for statutory evidence.");
}

export async function createThaiPayrollProfile(actor:WorkContext,input:z.infer<typeof thaiPayrollProfileSchema>,key:string) {
  actor=requirePermission(actor,"payroll.prepare");companyScope(actor);
  const {write}=await holdingResources(actor.holdingId),id=`thai-payroll-profile-${key}`,prior=await replayCommand(write,actor,key,"people.thai-profile.create",id,input);if(prior)return prior;
  const year=String(input.taxYear);
  if(input.effectiveFrom>input.effectiveTo || !input.effectiveFrom.startsWith(year) || !input.effectiveTo.startsWith(year) || input.openingThroughDate>=input.effectiveFrom || input.openingIrregularMinor>input.openingTaxableMinor)
    throw new ApiError(422,"PAYROLL_STATUTORY_DATES_INVALID","Use an evidence interval within one tax year and opening YTD ending before it.");
  if(!input.socialSecurityEligible && !input.socialSecurityExemptionEvidence)throw new ApiError(422,"SSO_ELIGIBILITY_UNVERIFIED","Record the verified Section 33 exemption evidence.");
  const now=Date.now();
  return executeCommand(write,actor,key,"people.thai-profile.create",input,id,1,{id,status:"draft",version:1},[
    assertion(write,key,"engagement","EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND currency='THB' AND worker_type='employee')",[input.engagementId,actor.companyId,input.expectedEngagementVersion]),
    write.prepare(`INSERT INTO hr_thai_payroll_profiles(id,company_id,engagement_id,tax_year,effective_from,effective_to,annual_deductions_minor,scheduled_pays,social_security_eligible,social_security_exemption_evidence,opening_through_date,opening_taxable_minor,opening_irregular_minor,opening_withheld_minor,prior_income_complete,evidence_ref,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`)
      .bind(id,actor.companyId,input.engagementId,input.taxYear,input.effectiveFrom,input.effectiveTo,input.verifiedAnnualDeductionsMinor,input.scheduledPaysInYear,input.socialSecurityEligible,input.socialSecurityExemptionEvidence??null,input.openingThroughDate,input.openingTaxableMinor,input.openingIrregularMinor,input.openingWithheldMinor,input.priorIncomeComplete,input.evidenceRef,actor.userId,now,now),
  ]);
}

export async function decideThaiPayrollProfile(actor:WorkContext,id:string,input:z.infer<typeof thaiPayrollProfileDecisionSchema>,key:string) {
  actor=requirePermission(actor,"people.compensation.approve");companyScope(actor);
  const {write}=await holdingResources(actor.holdingId),prior=await replayCommand(write,actor,key,`people.thai-profile.${input.action}`,id,input);if(prior)return prior;
  const row=await write.prepare("SELECT * FROM hr_thai_payroll_profiles WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Profile>();
  if(!row)throw new ApiError(404,"PAYROLL_PROFILE_NOT_FOUND","Statutory payroll evidence not found.");
  if(row.version!==input.expectedVersion || row.status!==(input.action==="approve"?"draft":"approved"))throw new ApiError(409,"PAYROLL_PROFILE_CHANGED","The payroll evidence changed. Reload before deciding.");
  if(samePerson(actor,row.created_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different person must review statutory payroll evidence.");
  const status=input.action==="approve"?"approved":"superseded",now=Date.now();
  return executeCommand(write,actor,key,`people.thai-profile.${input.action}`,input,id,row.version+1,{id,status,version:row.version+1},[
    write.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`thai-payroll-profile:${actor.companyId}:${row.engagement_id}:${row.tax_year}`),
    assertion(write,key,"profile","EXISTS(SELECT 1 FROM hr_thai_payroll_profiles WHERE id=? AND company_id=? AND status=? AND version=?)",[id,actor.companyId,row.status,row.version]),
    ...(input.action==="approve"?[assertion(write,key,"overlap","NOT EXISTS(SELECT 1 FROM hr_thai_payroll_profiles WHERE engagement_id=? AND company_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?)",[row.engagement_id,actor.companyId,row.tax_year,row.effective_to,row.effective_from])]:[]),
    write.prepare("UPDATE hr_thai_payroll_profiles SET status=?,approved_by=CASE WHEN ?='approved' THEN ? ELSE approved_by END,version=version+1,updated_at=? WHERE id=?").bind(status,status,actor.userId,now,id),
    write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,id,`people.thai-profile.${input.action}`,actor.userId,actor.assignmentId,row.version,await sha256(canonical({row,reason:input.reason})),now),
  ]);
}

export async function thaiPayrollConfiguration(actor:WorkContext,engagementId?:string) {
  actor=requirePermission(actor,"payroll.configuration.read");companyScope(actor);const {write}=await holdingResources(actor.holdingId);
  const profiles=(await write.prepare("SELECT * FROM hr_thai_payroll_profiles WHERE company_id=? AND (?::text IS NULL OR engagement_id=?) ORDER BY created_at DESC,id").bind(actor.companyId,engagementId??null,engagementId??null).all()).results;
  const employees=(await write.prepare("SELECT e.id,e.version,e.status,e.worker_type,e.employment_type,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.currency='THB' AND e.worker_type='employee' ORDER BY p.legal_name,e.id").bind(actor.companyId).all()).results;
  return {supportedPack:thaiPayroll2026Pack,profiles,employees};
}

type PriorItem = {id:string;run_id:string;run_version:number;input_digest:string;result_json:string;pay_date:string};
export type ThaiPayrollEvidence = {pack:{id:string;recordVersion:number;sourceDigest:string;rulesJson:string};profile:Profile;priorItems:PriorItem[]};

export async function thaiEvidenceForEmployee(db:SqlDatabase,run:PayrollStatutoryRun,engagementId:string):Promise<ThaiPayrollEvidence> {
  const pack=await thaiPackForRun(db,run),year=Number(run.pay_date!.slice(0,4));
  const profiles=(await db.prepare("SELECT * FROM hr_thai_payroll_profiles WHERE company_id=? AND engagement_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?").bind(run.company_id,engagementId,year,run.pay_date,run.pay_date).all<Profile>()).results;
  if(profiles.length!==1 || !profiles[0].prior_income_complete)throw new ApiError(422,"PAYROLL_STATUTORY_EVIDENCE_INCOMPLETE","Verify annual deductions, eligibility, payment count and opening YTD before statutory calculation.");
  const profile=profiles[0];
  const priorItems=(await db.prepare(`SELECT i.id,i.run_id,r.version run_version,r.input_digest,i.result_json,r.pay_date FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id WHERE r.company_id=? AND i.engagement_id=? AND r.status='posted' AND r.currency='THB' AND r.pay_date>? AND r.pay_date<=? AND r.id<>? AND r.pay_date>=? ORDER BY r.pay_date,i.id`)
    .bind(run.company_id,engagementId,profile.opening_through_date,run.pay_date,run.id,`${year}-01-01`).all<PriorItem>()).results;
  if(priorItems.some(item=>!(JSON.parse(item.result_json) as {statutory?:{adapterId?:string}}).statutory?.adapterId))
    throw new ApiError(422,"PAYROLL_YTD_UNVERIFIED","Import reviewed opening YTD through the last gross-pay preview before calculating statutory payroll.");
  return {pack:{id:pack.row.id,recordVersion:pack.row.record_version,sourceDigest:pack.sourceDigest,rulesJson:pack.row.rules_json},profile,priorItems};
}

type Calculated = {gross:number;deductions:number;employer:number;net:number;applied:Array<{code:string;label:string;kind:string;amount:string;amountMinor:number;taxable?:boolean;incomeTiming?:string;socialSecurityWage?:boolean}>};
export function applyThaiStatutory(calculated:Calculated,evidence:ThaiPayrollEvidence,run:PayrollStatutoryRun,overtimeMinor:number) {
  const earnings=calculated.applied.filter(item=>item.kind==="earning");
  if(earnings.some(item=>item.incomeTiming===undefined || item.socialSecurityWage===undefined))throw new ApiError(422,"PAYROLL_COMPONENT_CLASSIFICATION_REQUIRED","Classify each earning as regular, occasional or exempt and specify its Section 33 wage treatment.");
  if(calculated.applied.some(item=>item.kind!=="earning" && /(^|[:_])(TAX|PIT|PND1|SSO|SOCIAL)([:_]|$)/i.test(item.code)))
    throw new ApiError(422,"PAYROLL_STATUTORY_COMPONENT_DUPLICATE","Use calculated tax and social-security deductions instead of fixed statutory components.");
  const baseWages=calculated.gross-earnings.reduce((sum,item)=>sum+item.amountMinor,0)-overtimeMinor;
  const regular=baseWages+earnings.filter(item=>item.incomeTiming==="regular").reduce((sum,item)=>sum+item.amountMinor,0);
  const irregular=overtimeMinor+earnings.filter(item=>item.incomeTiming==="irregular").reduce((sum,item)=>sum+item.amountMinor,0);
  const nonTaxable=earnings.filter(item=>item.incomeTiming==="non_taxable").reduce((sum,item)=>sum+item.amountMinor,0);
  const prior=evidence.priorItems.map(item=>JSON.parse(item.result_json) as {statutory:{input:ThaiPayrollCalculation}});
  const profile=evidence.profile;
  const input:ThaiPayrollCalculation={
    payDate:run.pay_date!,regularTaxableMinor:regular,irregularTaxableMinor:irregular,nonTaxableMinor:nonTaxable,
    irregularPaidBeforeMinor:profile.opening_irregular_minor+prior.reduce((sum,item)=>sum+item.statutory.input.irregularTaxableMinor,0),
    socialSecurityWageMinor:baseWages+earnings.filter(item=>item.socialSecurityWage).reduce((sum,item)=>sum+item.amountMinor,0),
    socialSecurityEligible:profile.social_security_eligible,socialSecurityExemptionEvidence:profile.social_security_exemption_evidence??undefined,
    otherEmployeeDeductionsMinor:calculated.deductions,verifiedAnnualDeductionsMinor:profile.annual_deductions_minor,
    taxablePaidBeforeMinor:profile.opening_taxable_minor+prior.reduce((sum,item)=>sum+item.statutory.input.regularTaxableMinor+item.statutory.input.irregularTaxableMinor,0),
    taxWithheldBeforeMinor:profile.opening_withheld_minor+evidence.priorItems.reduce((sum,item)=>sum+(JSON.parse(item.result_json) as {statutory:{calculation:{taxWithheldMinor:number}}}).statutory.calculation.taxWithheldMinor,0),
    scheduledPaysInYear:profile.scheduled_pays,lastPayInYear:run.mode==="final_pay" || run.pay_date!.slice(5,7)==="12",priorIncomeComplete:profile.prior_income_complete,
  };
  let calculation:ReturnType<typeof calculateThaiPayroll>;
  try {calculation=calculateThaiPayroll(input,thaiPayroll2026Pack.rules);}catch(error){if(error instanceof ThaiPayrollRuleError)throw new ApiError(422,error.code,error.message);throw error;}
  const statutory={adapterId:thaiPayrollAdapterId,sourceDigest:evidence.pack.sourceDigest,certified:false,input,calculation};
  const applied=[...calculated.applied,
    {code:"STATUTORY:TAX",label:"Thai payroll withholding tax",kind:"deduction",amount:minorToDecimal(calculation.taxWithheldMinor),amountMinor:calculation.taxWithheldMinor},
    {code:"STATUTORY:SSO",label:"Section 33 employee contribution",kind:"deduction",amount:minorToDecimal(calculation.employeeSsoMinor),amountMinor:calculation.employeeSsoMinor},
    {code:"STATUTORY:SSO:EMPLOYER",label:"Section 33 employer contribution",kind:"employer",amount:minorToDecimal(calculation.employerSsoMinor),amountMinor:calculation.employerSsoMinor},
  ];
  return {gross:calculation.grossMinor,deductions:calculated.deductions+calculation.taxWithheldMinor+calculation.employeeSsoMinor,employer:calculated.employer+calculation.employerSsoMinor,net:calculation.netMinor,applied,statutory};
}

export function thaiEvidenceGuards(db:SqlDatabase,key:string,evidence:ThaiPayrollEvidence,run:PayrollStatutoryRun,engagementId:string):SqlStatement[] {
  return [
    db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`thai-payroll-ytd:${run.company_id}:${engagementId}:${run.pay_date!.slice(0,4)}`),
    assertion(db,key,`thai-pack-${engagementId}`,"EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status='active' AND record_version=? AND source_digest=? AND rules_json=?)",[evidence.pack.id,evidence.pack.recordVersion,evidence.pack.sourceDigest,evidence.pack.rulesJson]),
    assertion(db,key,`thai-profile-${engagementId}`,"EXISTS(SELECT 1 FROM hr_thai_payroll_profiles WHERE id=? AND company_id=? AND engagement_id=? AND status='approved' AND version=?)",[evidence.profile.id,run.company_id,engagementId,evidence.profile.version]),
    assertion(db,key,`thai-ytd-count-${engagementId}`,"(SELECT COUNT(*) FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id WHERE r.company_id=? AND i.engagement_id=? AND r.status='posted' AND r.currency='THB' AND r.pay_date>? AND r.pay_date<=? AND r.id<>? AND r.pay_date>=?)=?",[run.company_id,engagementId,evidence.profile.opening_through_date,run.pay_date!,run.id,`${run.pay_date!.slice(0,4)}-01-01`,evidence.priorItems.length]),
    ...evidence.priorItems.map(item=>assertion(db,key,`thai-ytd-${item.id}`,"EXISTS(SELECT 1 FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id WHERE i.id=? AND i.run_id=? AND r.version=? AND r.status='posted' AND r.input_digest=? AND i.result_json=?)",[item.id,item.run_id,item.run_version,item.input_digest,item.result_json])),
  ];
}

/** Bank/GL readiness reads immutable posted facts; it does not require historical rule packs to remain active forever. */
export async function assertStatutoryRunReady(db:SqlDatabase,runId:string) {
  const run=await db.prepare("SELECT mode,country_pack_id,currency FROM hr_payroll_runs WHERE id=?").bind(runId).first<{mode:string;country_pack_id:string|null;currency:string}>();
  const items=(await db.prepare("SELECT gross_minor,deductions_minor,net_minor,result_json FROM hr_payroll_items WHERE run_id=?").bind(runId).all<{gross_minor:number;deductions_minor:number;net_minor:number;result_json:string}>()).results;
  if(!run?.country_pack_id || run.currency!=="THB" || !items.length)throw new ApiError(409,"PAYROLL_STATUTORY_ADAPTER_REQUIRED","A calculated, source-dated Thai statutory run is required for this payroll action.");
  const pack=await db.prepare("SELECT * FROM hr_country_packs WHERE id=? AND status IN ('active','superseded')").bind(run.country_pack_id).first<PackRow>();
  if(!pack)throw new ApiError(409,"PAYROLL_STATUTORY_ADAPTER_REQUIRED","Reviewed statutory rule evidence is unavailable.");
  const recognized=await recognizedThaiPack(JSON.parse(pack.rules_json),pack.country_code,pack.version);
  for(const item of items){
    const statutory=(JSON.parse(item.result_json) as {statutory?:{adapterId:string;calculationKind?:string;sourceDigest:string;input:ThaiPayrollCalculation|ThaiPayrollAdjustmentInput;calculation:ReturnType<typeof calculateThaiPayroll>}}).statutory;
    const calculation=statutory?.calculationKind==='adjustment'?calculateThaiPayrollAdjustment(statutory.input as ThaiPayrollAdjustmentInput,recognized.rules):statutory?calculateThaiPayroll(statutory.input,recognized.rules):null;
    if(!statutory || statutory.adapterId!==thaiPayrollAdapterId || statutory.sourceDigest!==recognized.sourceDigest || canonical(calculation)!==canonical(statutory.calculation) || item.gross_minor!==statutory.calculation.grossMinor || item.net_minor!==statutory.calculation.netMinor || item.gross_minor-item.deductions_minor!==item.net_minor)
      throw new ApiError(409,"PAYROLL_STATUTORY_RESULT_INVALID","Payroll amounts must reconcile with the retained statutory adapter calculation.");
  }
}
