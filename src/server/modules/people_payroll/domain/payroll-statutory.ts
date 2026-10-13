import {z} from 'zod';import type {WorkContext} from '../../identity_access/public/authorization';import {thaiPayroll2026Pack,thaiPayrollAdapterId} from "@/lib/people_payroll/thai-payroll-pack";import {calculateThaiPayroll,ThaiPayrollRuleError,type ThaiPayrollCalculation} from "@/lib/people_payroll/thai-payroll-rules";import {minorToDecimal} from "@/lib/finance/finance-money";import {BusinessRuleError} from '../../shared';
export type PayrollStatutoryRun = { id:string;company_id:string;pay_group_id:string;country_pack_id?:string|null;period_start:string;period_end:string;pay_date?:string;currency:string;mode?:string;adjustment_of_run_id?:string|null };
export type PackRow = { id:string;country_code:string;version:string;status:string;rules_json:string;adapter_id:string|null;source_digest:string|null;record_version:number };
export const amount = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
export const thaiPayrollProfileSchema = z.object({
  engagementId: z.string().min(1), expectedEngagementVersion:z.number().int().positive(),
  taxYear:z.number().int().min(2000).max(2200), effectiveFrom:z.string().date(), effectiveTo:z.string().date(),
  verifiedAnnualDeductionsMinor:amount, scheduledPaysInYear:z.number().int().min(1).max(12),
  socialSecurityEligible:z.boolean(),socialSecurityExemptionEvidence:z.string().trim().min(3).max(2000).optional(),
  openingThroughDate:z.string().date(),openingTaxableMinor:amount,openingIrregularMinor:amount,openingWithheldMinor:amount,
  priorIncomeComplete:z.boolean(), evidenceRef:z.string().trim().min(8).max(2000),
}).strict();
export const thaiPayrollProfileDecisionSchema = z.object({ expectedVersion:z.number().int().positive(), action:z.enum(["approve","supersede"]), reason:z.string().trim().min(8).max(2000) }).strict();
export type Profile = {id:string;company_id:string;engagement_id:string;tax_year:number;effective_from:string;effective_to:string;annual_deductions_minor:number;scheduled_pays:number;social_security_eligible:boolean;social_security_exemption_evidence:string|null;opening_through_date:string;opening_taxable_minor:number;opening_irregular_minor:number;opening_withheld_minor:number;prior_income_complete:boolean;evidence_ref:string;status:string;created_by:string;approved_by:string|null;version:number};
export type PriorItem = {id:string;run_id:string;run_version:number;input_digest:string;result_json:string;pay_date:string};
export type ThaiPayrollEvidence = {pack:{id:string;recordVersion:number;sourceDigest:string;rulesJson:string};profile:Profile;priorItems:PriorItem[]};
export type Calculated = {gross:number;deductions:number;employer:number;net:number;applied:Array<{code:string;label:string;kind:string;amount:string;amountMinor:number;taxable?:boolean;incomeTiming?:string;socialSecurityWage?:boolean}>};
export function canonical(value: unknown): string {
  const normalize = (entry: unknown): unknown => Array.isArray(entry) ? entry.map(normalize)
    : entry && typeof entry === "object" ? Object.fromEntries(Object.entries(entry).sort(([a],[b]) => a.localeCompare(b)).map(([key,item]) => [key,normalize(item)])) : entry;
  return JSON.stringify(normalize(value));
}
export function companyScope(actor:WorkContext) {
  if(actor.branchId || actor.departmentId) throw new BusinessRuleError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a Company Payroll assignment for statutory evidence.");
}
export function applyThaiStatutory(calculated:Calculated,evidence:ThaiPayrollEvidence,run:PayrollStatutoryRun,overtimeMinor:number) {
  const earnings=calculated.applied.filter(item=>item.kind==="earning");
  if(earnings.some(item=>item.incomeTiming===undefined || item.socialSecurityWage===undefined))throw new BusinessRuleError(422,"PAYROLL_COMPONENT_CLASSIFICATION_REQUIRED","Classify each earning as regular, occasional or exempt and specify its Section 33 wage treatment.");
  if(calculated.applied.some(item=>item.kind!=="earning" && /(^|[:_])(TAX|PIT|PND1|SSO|SOCIAL)([:_]|$)/i.test(item.code)))
    throw new BusinessRuleError(422,"PAYROLL_STATUTORY_COMPONENT_DUPLICATE","Use calculated tax and social-security deductions instead of fixed statutory components.");
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
  try {calculation=calculateThaiPayroll(input,thaiPayroll2026Pack.rules);}catch(error){if(error instanceof ThaiPayrollRuleError)throw new BusinessRuleError(422,error.code,error.message);throw error;}
  const statutory={adapterId:thaiPayrollAdapterId,sourceDigest:evidence.pack.sourceDigest,certified:false,input,calculation};
  const applied=[...calculated.applied,
    {code:"STATUTORY:TAX",label:"Thai payroll withholding tax",kind:"deduction",amount:minorToDecimal(calculation.taxWithheldMinor),amountMinor:calculation.taxWithheldMinor},
    {code:"STATUTORY:SSO",label:"Section 33 employee contribution",kind:"deduction",amount:minorToDecimal(calculation.employeeSsoMinor),amountMinor:calculation.employeeSsoMinor},
    {code:"STATUTORY:SSO:EMPLOYER",label:"Section 33 employer contribution",kind:"employer",amount:minorToDecimal(calculation.employerSsoMinor),amountMinor:calculation.employerSsoMinor},
  ];
  return {gross:calculation.grossMinor,deductions:calculated.deductions+calculation.taxWithheldMinor+calculation.employeeSsoMinor,employer:calculated.employer+calculation.employerSsoMinor,net:calculation.netMinor,applied,statutory};
}
