import "server-only";
import { z } from "zod";
import { calculateFinance, type CalculationKind } from "@/lib/finance-calculations";
import { executeCommand, assertion, replayCommand } from "./commands";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { ApiError } from "./http";

type Row = Record<string, string | number | null>;
const decimal=z.string().regex(/^-?\d+(?:\.\d+)?$/), nonnegative=decimal.refine(value=>!value.startsWith("-"));
const base=z.object({periodId:z.string().min(1).nullable().optional(),currency:z.string().length(3).transform(value=>value.toUpperCase()),currencyScale:z.number().int().min(0).max(12).default(2),policyId:z.string().min(1).nullable().optional(),policyVersion:z.number().int().positive().default(1)});
const calculationInput=z.discriminatedUnion("kind",[
  base.extend({kind:z.literal("allocation"),input:z.object({total:decimal,weights:z.array(z.object({key:z.string().min(1),weight:nonnegative})).min(1)})}),
  base.extend({kind:z.literal("cash_flow"),input:z.object({openingCash:decimal,closingCash:decimal,profit:decimal,nonCashAdjustments:decimal,workingCapitalChange:decimal,investing:decimal,financing:decimal,fxEffect:decimal})}),
  base.extend({kind:z.literal("consolidation"),input:z.object({consideration:decimal,previousInterest:decimal.default("0"),fairValueNetAssets:decimal,nciFairValue:decimal.default("0"),parentBps:z.number().int().min(0).max(10000),openingNci:decimal.default("0"),subsidiaryProfit:decimal.default("0"),subsidiaryOci:decimal.default("0"),subsidiaryDividends:decimal.default("0"),nciOwnershipChange:decimal.default("0"),associateOpening:decimal.default("0"),associateBps:z.number().int().min(0).max(10000).default(0),associateProfit:decimal.default("0"),associateOci:decimal.default("0"),associateDividends:decimal.default("0"),associateImpairment:decimal.default("0")})}),
  base.extend({kind:z.literal("thai_statutory"),input:z.object({outputVat:nonnegative,inputVat:nonnegative,disallowedInputVat:nonnegative.default("0"),priorCredit:nonnegative.default("0"),pnd3Withholding:nonnegative.default("0"),pnd53Withholding:nonnegative.default("0"),pp36Vat:nonnegative.default("0"),pnd54Withholding:nonnegative.default("0")})}),
  base.extend({kind:z.literal("manufacturing_cost"),input:z.object({openingQuantityMilli:z.string().regex(/^\d+$/),openingValue:nonnegative,receipts:z.array(z.object({quantityMilli:z.string().regex(/^\d+$/),value:nonnegative})).default([]),issues:z.array(z.object({quantityMilli:z.string().regex(/^\d+$/)})).default([]),labor:nonnegative,overhead:nonnegative,abnormalScrapCost:nonnegative.default("0"),outputQuantityMilli:z.string().regex(/^\d+$/)})}),
  base.extend({kind:z.literal("reconciliation"),input:z.object({bankAmount:decimal,allocations:z.array(z.object({reference:z.string().min(1),amount:decimal})).min(1)})}),
]);
export const calculationRunSchema=calculationInput;
export const calculationDecisionSchema=z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)});

async function digest(value:unknown){return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(JSON.stringify(value))))).map(byte=>byte.toString(16).padStart(2,"0")).join("");}
const labels:Record<CalculationKind,string>={allocation:"Allocation",cash_flow:"Cash flow",consolidation:"NCI / Equity / Goodwill",thai_statutory:"Thai statutory",manufacturing_cost:"Manufacturing cost",reconciliation:"Reconciliation"};

export async function financeEngineWorkspace(actor:WorkContext){
  actor=requirePermission(actor,"finance.read");const {write}=await holdingResources(actor.holdingId);
  const [runs,counts,periods]=await Promise.all([
    write.prepare("SELECT * FROM finance_calculation_runs WHERE company_id=? ORDER BY created_at DESC LIMIT 50").bind(actor.companyId).all<Row>(),
    write.prepare("SELECT kind,status,count(*) count FROM finance_calculation_runs WHERE company_id=? GROUP BY kind,status ORDER BY kind,status").bind(actor.companyId).all<Row>(),
    write.prepare("SELECT id,name,start_date,end_date,state FROM finance_periods WHERE company_id=? ORDER BY start_date DESC LIMIT 24").bind(actor.companyId).all<Row>(),
  ]);
  return {runs:runs.results,counts:counts.results,periods:periods.results,labels};
}

export async function createCalculationRun(actor:WorkContext,raw:z.input<typeof calculationRunSchema>,key:string){
  actor=requirePermission(actor,"finance.write");const parsed=calculationRunSchema.parse(raw),{write}=await holdingResources(actor.holdingId);
  const runId=`calc-${key}`;const existing=await replayCommand(write,actor,key,"finance.calculation.prepare",runId,parsed);if(existing)return existing;
  if(parsed.periodId&&!await write.prepare("SELECT 1 FROM finance_periods WHERE id=? AND company_id=?").bind(parsed.periodId,actor.companyId).first())throw new ApiError(404,"PERIOD_NOT_FOUND","Accounting period not found.");
  if(parsed.policyId&&!await write.prepare("SELECT 1 FROM finance_calculation_policies WHERE id=? AND company_id=? AND version=? AND status='approved'").bind(parsed.policyId,actor.companyId,parsed.policyVersion).first())throw new ApiError(409,"POLICY_NOT_APPROVED","The selected calculation policy and version must be approved.");
  let result:Record<string,unknown>;try{result=calculateFinance(parsed.kind,parsed.input,parsed.currencyScale);}catch(error){throw new ApiError(422,"CALCULATION_INVALID",error instanceof Error?error.message:"Calculation input is invalid.");}
  const sourceHash=await digest({kind:parsed.kind,input:parsed.input,policyId:parsed.policyId??null,policyVersion:parsed.policyVersion,currency:parsed.currency,scale:parsed.currencyScale}),now=Date.now();
  const explanation={engine:"erpx-deterministic-v1",label:labels[parsed.kind],sourceHash,inputs:Object.keys(parsed.input),humanApprovalRequired:true};
  const output={id:runId,kind:parsed.kind,status:"prepared",version:1,sourceHash,result};
  return executeCommand(write,actor,key,"finance.calculation.prepare",parsed,runId,1,output,[
    write.prepare("INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_id,policy_version,period_id,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,version,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,'prepared',1,?,?,?)").bind(runId,actor.companyId,actor.holdingId,parsed.kind,parsed.policyId??null,parsed.policyVersion,parsed.periodId??null,parsed.currency,parsed.currencyScale,sourceHash,JSON.stringify(parsed.input),JSON.stringify(result),JSON.stringify(explanation),actor.userId,now,now),
    write.prepare("INSERT INTO finance_calculation_events VALUES(?,?,1,'prepare',?,'Prepared deterministic calculation',?,?)").bind(crypto.randomUUID(),runId,actor.userId,await digest(result),now),
  ]);
}

export async function decideCalculationRun(actor:WorkContext,runId:string,input:z.infer<typeof calculationDecisionSchema>,action:"review"|"approve"|"reject",key:string){
  actor=requirePermission(actor,action==="review"?"finance.write":"finance.approve");const {write}=await holdingResources(actor.holdingId),run=await write.prepare("SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<Row>();
  if(!run)throw new ApiError(404,"CALCULATION_NOT_FOUND","Calculation run not found.");
  if(JSON.parse(String(run.explanation_json)).engine==="source-vat-register-v1")throw new ApiError(409,"VAT_PACKAGE_WORKFLOW_REQUIRED","Review this source-derived calculation through its VAT filing package.");
  if(JSON.parse(String(run.explanation_json)).engine==="source-acquisition-v1")throw new ApiError(409,"ACQUISITION_WORKFLOW_REQUIRED","Review this source-derived calculation through its acquisition evidence.");
  const expected=action==="review"?"prepared":"reviewed",next=action==="reject"?"rejected":action==="review"?"reviewed":"approved";
  if(run.status!==expected&&!(action==="reject"&&["prepared","reviewed"].includes(String(run.status))))throw new ApiError(409,"INVALID_CALCULATION_TRANSITION",`Calculation must be ${expected} before ${action}.`);
  if(Number(run.version)!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","The calculation changed. Refresh and review again.");
  if(String(run.prepared_by)===actor.userId)throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different person must review or approve this calculation.");
  const operation=`finance.calculation.${action}`,version=Number(run.version)+1,now=Date.now(),output={id:runId,status:next,version};
  return executeCommand(write,actor,key,operation,input,runId,version,output,[
    assertion(write,key,"run-version","EXISTS(SELECT 1 FROM finance_calculation_runs WHERE id=? AND company_id=? AND status=? AND version=?)",[runId,actor.companyId,run.status,run.version]),
    write.prepare(`UPDATE finance_calculation_runs SET status=?,version=version+1,updated_at=?,${action==="review"?"reviewed_by":"approved_by"}=? WHERE id=?`).bind(next,now,actor.userId,runId),
    write.prepare("INSERT INTO finance_calculation_events VALUES(?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),runId,version,action,actor.userId,input.reason,await digest(JSON.parse(String(run.result_json))),now),
  ]);
}

export async function calculationRunDetail(actor:WorkContext,runId:string){
  actor=requirePermission(actor,"finance.read");const {write}=await holdingResources(actor.holdingId);
  const run=await write.prepare("SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=?").bind(runId,actor.companyId).first<Row>();if(!run)throw new ApiError(404,"CALCULATION_NOT_FOUND","Calculation run not found.");
  const events=await write.prepare("SELECT * FROM finance_calculation_events WHERE run_id=? ORDER BY version").bind(runId).all<Row>();
  return {...run,input:JSON.parse(String(run.input_json)),result:JSON.parse(String(run.result_json)),explanation:JSON.parse(String(run.explanation_json)),events:events.results};
}

export async function snapshotCloseChecks(actor:WorkContext,periodId:string,key:string){
  actor=requirePermission(actor,"finance.write");const {write}=await holdingResources(actor.holdingId),period=await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId,actor.companyId).first<Row>();if(!period)throw new ApiError(404,"PERIOD_NOT_FOUND","Accounting period not found.");
  const checks=[
    {key:"draft-journals",query:"SELECT count(*) n FROM finance_journals WHERE company_id=? AND period_id=? AND status IN ('draft','in_review')",args:[actor.companyId,periodId]},
    {key:"draft-invoices",query:"SELECT count(*) n FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?",args:[actor.companyId,period.start_date,period.end_date]},
    {key:"unmatched-bank",query:"SELECT count(*) n FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?",args:[actor.companyId,period.start_date,period.end_date]},
    {key:"open-manufacturing",query:"SELECT count(*) n FROM manufacturing_orders WHERE company_id=? AND status IN ('reserved','in_progress','awaiting_qc')",args:[actor.companyId]},
    {key:"unapproved-calculations",query:"SELECT count(*) n FROM finance_calculation_runs WHERE company_id=? AND period_id=? AND status IN ('prepared','reviewed','stale')",args:[actor.companyId,periodId]},
  ];
  const values=[] as Array<{key:string;count:number;status:"pass"|"block"}>;for(const check of checks){const count=Number(await write.prepare(check.query).bind(...check.args).first("n")??0);values.push({key:check.key,count,status:count?"block":"pass"});}
  const sourceVersion=await digest({periodId,version:period.close_version,values}),now=Date.now(),output={periodId,sourceVersion,checks:values,ready:values.every(item=>item.status==="pass")};
  return executeCommand(write,actor,key,"finance.close.check",{periodId},periodId,Number(period.close_version),output,values.map(check=>write.prepare("INSERT INTO finance_close_checks(id,company_id,period_id,check_key,source_version,status,count_value,detail_json,checked_at,checked_by) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(period_id,check_key,source_version) DO NOTHING").bind(crypto.randomUUID(),actor.companyId,periodId,check.key,sourceVersion,check.status,check.count,JSON.stringify(check),now,actor.userId)));
}
