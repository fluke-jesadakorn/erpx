import { createHash } from "node:crypto";
import { z } from "zod";
import { samePerson } from "@/lib/iam";
import { requirePermission } from "./iam-permissions";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { holdingResources } from "./runtime";

export const accountingProfileSchema=z.object({
  statutoryChartId:z.string().min(1).max(128),
  standard:z.literal("TFRS for NPAEs"),
  fiscalStartMonth:z.number().int().min(1).max(12),
  effectiveDate:z.string().date(),
  eligibilityEvidence:z.string().trim().min(10).max(4000),
  policyEvidence:z.string().trim().min(10).max(4000),
  taxBasis:z.literal("current_tax"),
  inventoryPolicy:z.literal("moving_average"),
  depreciationPolicy:z.literal("straight_line"),
});
export const accountingProfileDecisionSchema=z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(10).max(4000)});
export type AccountingProfile=Record<string,string|number|null>;

export async function accountingProfiles(actor:WorkContext){
  actor=requirePermission(actor,"finance.read");
  const {write}=await holdingResources(actor.holdingId);
  return (await write.prepare("SELECT p.*,c.name chart_name,c.base_currency FROM finance_accounting_profiles p JOIN finance_charts c ON c.id=p.statutory_chart_id WHERE p.company_id=? ORDER BY p.created_at DESC,p.id").bind(actor.companyId).all<AccountingProfile>()).results;
}

export async function prepareAccountingProfile(actor:WorkContext,input:z.infer<typeof accountingProfileSchema>,key:string){
  actor=requirePermission(actor,"finance.write");input=accountingProfileSchema.parse(input);
  const {write}=await holdingResources(actor.holdingId),id=`accounting-profile-${key}`;
  const previous=await replayCommand(write,actor,key,"finance.profile.prepare",id,input);if(previous)return previous;
  const chart=await write.prepare("SELECT base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(input.statutoryChartId,actor.companyId).first<{base_currency:string}>();
  if(!chart)throw new ApiError(404,"CHART_NOT_FOUND","Select an active Company ledger.");
  if(chart.base_currency!=="THB")throw new ApiError(422,"THAI_STATUTORY_CURRENCY","A new Thai NPAEs profile requires a THB statutory ledger.");
  if(await write.prepare("SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved'").bind(actor.companyId).first())throw new ApiError(409,"STATUTORY_PROFILE_EXISTS","This Company already has an approved statutory ledger. A reviewed migration is required to replace it.");
  return executeCommand(write,actor,key,"finance.profile.prepare",input,id,1,{id,status:"draft",version:1},[
    assertion(write,key,"ledger","EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency='THB')",[input.statutoryChartId,actor.companyId]),
    assertion(write,key,"profile","NOT EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved')",[actor.companyId]),
    write.prepare("INSERT INTO finance_accounting_profiles(id,company_id,statutory_chart_id,standard,fiscal_start_month,effective_date,eligibility_evidence,policy_evidence,tax_basis,inventory_policy,depreciation_policy,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
      .bind(id,actor.companyId,input.statutoryChartId,input.standard,input.fiscalStartMonth,input.effectiveDate,input.eligibilityEvidence,input.policyEvidence,input.taxBasis,input.inventoryPolicy,input.depreciationPolicy,actor.userId,Date.now()),
  ]);
}

export async function decideAccountingProfile(actor:WorkContext,id:string,decision:"approve"|"reject",input:z.infer<typeof accountingProfileDecisionSchema>,key:string){
  actor=requirePermission(requirePermission(actor,"finance.write"),"finance.approve");input=accountingProfileDecisionSchema.parse(input);
  const {write}=await holdingResources(actor.holdingId),operation=`finance.profile.${decision}`;
  const previous=await replayCommand(write,actor,key,operation,id,input);if(previous)return previous;
  const profile=await write.prepare("SELECT * FROM finance_accounting_profiles WHERE id=? AND company_id=?").bind(id,actor.companyId).first<AccountingProfile>();
  if(!profile)throw new ApiError(404,"PROFILE_NOT_FOUND","Accounting profile not found in this Company.");
  if(samePerson(actor,profile.prepared_by))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","Another Finance approver must review NPAEs eligibility and Company accounting policies.");
  if(profile.status!=="draft"||profile.version!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Refresh the profile before reviewing it.");
  const now=Date.now(),status=decision==="approve"?"approved":"rejected";
  return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1},[
    assertion(write,key,"profile","EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE id=? AND company_id=? AND version=? AND status='draft')",[id,actor.companyId,input.expectedVersion]),
    ...(decision==="approve"?[
      assertion(write,key,"ledger","EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency='THB')",[profile.statutory_chart_id,actor.companyId]),
      assertion(write,key,"single-statutory-ledger","NOT EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved')",[actor.companyId]),
    ]:[]),
    write.prepare("UPDATE finance_accounting_profiles SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1 WHERE id=? AND company_id=?")
      .bind(status,actor.userId,input.reason,now,id,actor.companyId),
    write.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,id,operation,actor.userId,actor.assignmentId,input.expectedVersion,createHash("sha256").update(JSON.stringify(input)).digest("hex"),now),
  ]);
}

export async function financialEvents(actor:WorkContext,query:{chartId:string;startDate?:string;endDate?:string}){
  actor=requirePermission(actor,"finance.read");
  query=z.object({chartId:z.string().min(1),startDate:z.string().date().optional(),endDate:z.string().date().optional()})
    .refine(q=>!q.startDate||!q.endDate||q.startDate<=q.endDate,{message:"Report start must not follow its end date."}).parse(query);
  const {write}=await holdingResources(actor.holdingId);
  if(!await write.prepare("SELECT 1 FROM finance_charts WHERE id=? AND company_id=?").bind(query.chartId,actor.companyId).first())throw new ApiError(404,"CHART_NOT_FOUND","Ledger not found in this Company.");
  return (await write.prepare("SELECT * FROM finance_financial_events WHERE company_id=? AND chart_id=? AND (?::text IS NULL OR effective_date>=?) AND (?::text IS NULL OR effective_date<=?) ORDER BY effective_date,id LIMIT 1000")
    .bind(actor.companyId,query.chartId,query.startDate??null,query.startDate??null,query.endDate??null,query.endDate??null).all<AccountingProfile>()).results;
}
