import { z } from "zod";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { snapshotMoney } from "./company-money";
import { sha256 } from "./crypto";
import { decimalToMinor, minorToDecimal, prepareJournal } from "./finance";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import type { SqlDatabase, SqlStatement } from "./runtime/types";

const moneySchema=z.string().regex(/^-?\d+(\.\d{1,6})?$/, "Use a decimal money amount.");
export const assetSchema = z.object({
  assetNumber:z.string().trim().min(1).max(64),name:z.string().trim().min(1).max(200),accountId:z.string().min(1),
  depreciationAccountId:z.string().min(1).optional(),depreciationExpenseAccountId:z.string().min(1).optional(),
  currency:z.string().length(3).transform(v=>v.toUpperCase()).default("USD"),cost:moneySchema,residual:moneySchema.default("0"),
  usefulLifeMonths:z.number().int().positive().max(1200),inServiceDate:z.string().date(),
});
export const assetEditSchema=assetSchema.extend({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(2000)});
export const assetActivationSchema=z.object({expectedVersion:z.number().int().positive(),periodId:z.string().min(1),entryDate:z.string().date(),offsetAccountId:z.string().min(1),evidenceRef:z.string().trim().min(3).max(2000)});
export const assetDepreciationSchema=z.object({expectedVersion:z.number().int().positive(),periodId:z.string().min(1),entryDate:z.string().date()});
export const assetDecisionSchema=z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(2000)});
export const assetDisposalSchema=z.object({expectedVersion:z.number().int().positive(),periodId:z.string().min(1),entryDate:z.string().date(),proceeds:moneySchema.default("0"),proceedsAccountId:z.string().min(1).optional(),gainAccountId:z.string().min(1),lossAccountId:z.string().min(1),evidenceRef:z.string().trim().min(3).max(2000)});

type Asset={id:string;company_id:string;asset_number:string;name:string;account_id:string;depreciation_account_id:string|null;depreciation_expense_account_id:string|null;currency:string;cost_minor:number;residual_minor:number;useful_life_months:number;in_service_date:string;accumulated_depreciation_minor:number;activation_journal_id:string|null;status:string;created_by:string;version:number};
type Disposal={id:string;asset_id:string;company_id:string;asset_version:number;period_id:string;entry_date:string;proceeds_minor:number;proceeds_account_id:string|null;gain_account_id:string;loss_account_id:string;status:string;created_by:string;version:number};

async function resources(actor:WorkContext,permission:"finance.read"|"finance.write"|"finance.approve"){
  actor=requirePermission(actor,permission);const {write}=await holdingResources(actor.holdingId);return {actor,write};
}
async function getAsset(write:SqlDatabase,actor:WorkContext,id:string){
  const row=await write.prepare("SELECT * FROM finance_assets WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Asset>();
  if(!row)throw new ApiError(404,"ASSET_NOT_FOUND","Asset not found.");return row;
}
function version(asset:{version:number},expected:number){if(asset.version!==expected)throw new ApiError(409,"ASSET_CHANGED","Reload and review the current asset revision.");}
function assetGuards(write:SqlDatabase,actor:WorkContext,id:string,asset:Asset){return [
  write.prepare("SELECT id FROM finance_assets WHERE id=? AND company_id=? FOR UPDATE").bind(asset.id,actor.companyId),
  assertion(write,id,"asset-version","EXISTS(SELECT 1 FROM finance_assets WHERE id=? AND company_id=? AND version=? AND status=?)",[asset.id,actor.companyId,asset.version,asset.status]),
];}
function event(write:SqlDatabase,actor:WorkContext,key:string,assetId:string,assetVersion:number,type:string,payload:unknown){
  return write.prepare("INSERT INTO finance_asset_events(id,asset_id,company_id,event_type,asset_version,command_id,payload_json,actor_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(`asset-event-${key}`,assetId,actor.companyId,type,assetVersion,key,JSON.stringify(payload),actor.userId,Date.now());
}
async function accounts(write:SqlDatabase,actor:WorkContext,ids:{cost:string;accumulated?:string|null;expense?:string|null}){
  const rows=await write.prepare("SELECT id,chart_id,category FROM finance_accounts WHERE company_id=? AND status='active' AND id IN (?,?,?)").bind(actor.companyId,ids.cost,ids.accumulated??null,ids.expense??null).all<{id:string;chart_id:string;category:string}>();
  const byId=new Map(rows.results.map(row=>[row.id,row])),cost=byId.get(ids.cost);
  if(!cost||cost.category!=="asset"||[ids.accumulated,ids.expense].filter(Boolean).some(id=>!byId.has(id!)||byId.get(id!)!.chart_id!==cost.chart_id))throw new ApiError(422,"ACCOUNT_INVALID","Asset accounts must be active and belong to one company chart.");
  if(ids.accumulated&&(ids.accumulated===ids.cost||byId.get(ids.accumulated)?.category!=="asset"))throw new ApiError(422,"ASSET_ACCUMULATED_ACCOUNT_REQUIRED","Use a separate accumulated depreciation asset account.");
  if(ids.expense&&(ids.expense===ids.cost||ids.expense===ids.accumulated||byId.get(ids.expense)?.category!=="expense"))throw new ApiError(422,"ASSET_EXPENSE_ACCOUNT_REQUIRED","Use a separate depreciation expense account.");
  return cost.chart_id;
}
function values(input:z.infer<typeof assetSchema>){const cost=decimalToMinor(input.cost),residual=decimalToMinor(input.residual);if(cost<=0||residual<0||residual>cost)throw new ApiError(422,"ASSET_VALUE_INVALID","Cost must be positive and residual must be between zero and cost.");return {cost,residual};}

export async function assetDetail(actor:WorkContext,id:string){
  const r=await resources(actor,"finance.read"),asset=await getAsset(r.write,r.actor,id);
  const [events,depreciations,disposals]=await Promise.all([
    r.write.prepare("SELECT * FROM finance_asset_events WHERE asset_id=? ORDER BY created_at,id").bind(id).all(),
    r.write.prepare("SELECT * FROM finance_asset_depreciations WHERE asset_id=? ORDER BY ordinal").bind(id).all(),
    r.write.prepare("SELECT * FROM finance_asset_disposals WHERE asset_id=? ORDER BY created_at,id").bind(id).all(),
  ]);return {asset,events:events.results,depreciations:depreciations.results,disposals:disposals.results};
}
export async function assetPage(actor:WorkContext){const r=await resources(actor,"finance.read");return (await r.write.prepare("SELECT * FROM finance_assets WHERE company_id=? ORDER BY asset_number,id").bind(r.actor.companyId).all()).results;}
export async function assetOptions(actor:WorkContext){const r=await resources(actor,"finance.read"),[accounts,periods]=await Promise.all([r.write.prepare("SELECT id,chart_id,code,name,category,status,currency FROM finance_accounts WHERE company_id=? ORDER BY code,id").bind(r.actor.companyId).all(),r.write.prepare("SELECT id,name,start_date,end_date,state FROM finance_periods WHERE company_id=? ORDER BY end_date DESC,id").bind(r.actor.companyId).all()]);return {accounts:accounts.results,periods:periods.results};}

export async function createAsset(actor:WorkContext,input:z.infer<typeof assetSchema>,key:string){
  const r=await resources(actor,"finance.write");actor=r.actor;const {write}=r,id=`asset-${key}`,prior=await replayCommand(write,actor,key,"finance.asset.create",id,input);if(prior)return prior;
  const amount=values(input);await accounts(write,actor,{cost:input.accountId,accumulated:input.depreciationAccountId,expense:input.depreciationExpenseAccountId});
  const money=await snapshotMoney(write,actor,{sourceType:"finance_asset",sourceId:id,amountKind:"cost",transactionDate:input.inServiceDate,currency:input.currency,currencyScale:2,amountUnits:String(amount.cost)}),now=Date.now();
  const result={id,status:"draft",version:1,cost:minorToDecimal(amount.cost)};
  return executeCommand(write,actor,key,"finance.asset.create",input,id,1,result,[
    write.prepare("INSERT INTO finance_assets(id,company_id,asset_number,name,account_id,depreciation_account_id,depreciation_expense_account_id,currency,cost_minor,residual_minor,useful_life_months,in_service_date,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)").bind(id,actor.companyId,input.assetNumber,input.name,input.accountId,input.depreciationAccountId??null,input.depreciationExpenseAccountId??null,input.currency,amount.cost,amount.residual,input.usefulLifeMonths,input.inServiceDate,actor.userId,now,now),
    money.statement,event(write,actor,key,id,1,"created",input),
  ]);
}
export async function editAsset(actor:WorkContext,id:string,input:z.infer<typeof assetEditSchema>,key:string){
  const r=await resources(actor,"finance.write");actor=r.actor;const {write}=r,prior=await replayCommand(write,actor,key,"finance.asset.edit",id,input);if(prior)return prior;
  const asset=await getAsset(write,actor,id);version(asset,input.expectedVersion);if(asset.status!=="draft")throw new ApiError(409,"ASSET_NOT_DRAFT","Only a draft asset can be edited.");
  const amount=values(input);await accounts(write,actor,{cost:input.accountId,accumulated:input.depreciationAccountId,expense:input.depreciationExpenseAccountId});
  const next=asset.version+1,money=await snapshotMoney(write,actor,{sourceType:"finance_asset_revision",sourceId:`${id}:${next}`,amountKind:"cost",transactionDate:input.inServiceDate,currency:input.currency,currencyScale:2,amountUnits:String(amount.cost)}),result={id,status:"draft",version:next,cost:minorToDecimal(amount.cost)};
  return executeCommand(write,actor,key,"finance.asset.edit",input,id,next,result,[...assetGuards(write,actor,key,asset),
    write.prepare("UPDATE finance_assets SET asset_number=?,name=?,account_id=?,depreciation_account_id=?,depreciation_expense_account_id=?,currency=?,cost_minor=?,residual_minor=?,useful_life_months=?,in_service_date=?,version=?,updated_at=? WHERE id=?").bind(input.assetNumber,input.name,input.accountId,input.depreciationAccountId??null,input.depreciationExpenseAccountId??null,input.currency,amount.cost,amount.residual,input.usefulLifeMonths,input.inServiceDate,next,Date.now(),id),
    money.statement,event(write,actor,key,id,next,"edited",input),
  ]);
}
export async function cancelDraftAsset(actor:WorkContext,id:string,input:z.infer<typeof assetDecisionSchema>,key:string){
  const r=await resources(actor,"finance.write");actor=r.actor;const {write}=r,prior=await replayCommand(write,actor,key,"finance.asset.cancel",id,input);if(prior)return prior;
  const asset=await getAsset(write,actor,id);version(asset,input.expectedVersion);if(asset.status!=="draft")throw new ApiError(409,"ASSET_NOT_DRAFT","Only an unrecognized draft asset can be cancelled.");
  const result={id,status:"retired",version:asset.version+1,cancelled:true};return executeCommand(write,actor,key,"finance.asset.cancel",input,id,result.version,result,[...assetGuards(write,actor,key,asset),write.prepare("UPDATE finance_assets SET status='retired',version=?,updated_at=? WHERE id=?").bind(result.version,Date.now(),id),event(write,actor,key,id,result.version,"cancelled",input)]);
}
export async function activateAsset(actor:WorkContext,id:string,input:z.infer<typeof assetActivationSchema>,key:string){
  const r=await resources(actor,"finance.approve");actor=r.actor;const {write}=r,prior=await replayCommand(write,actor,key,"finance.asset.activate",id,input);if(prior)return prior;
  const asset=await getAsset(write,actor,id);version(asset,input.expectedVersion);
  if(asset.status!=="draft"||asset.activation_journal_id)throw new ApiError(409,"ASSET_NOT_DRAFT","Only an unrecognized draft asset can be activated.");
  if(samePerson(actor,asset.created_by))throw new ApiError(403,"SELF_APPROVAL","Another finance reviewer must recognize the asset.");
  if(!asset.depreciation_account_id||!asset.depreciation_expense_account_id)throw new ApiError(422,"ASSET_DEPRECIATION_MAPPING_REQUIRED","Configure accumulated depreciation and expense accounts before recognition.");
  if(input.entryDate>asset.in_service_date)throw new ApiError(422,"ASSET_RECOGNITION_DATE","Recognize acquisition on or before its in-service date.");
  const chart=await accounts(write,actor,{cost:asset.account_id,accumulated:asset.depreciation_account_id,expense:asset.depreciation_expense_account_id});
  if([asset.account_id,asset.depreciation_account_id,asset.depreciation_expense_account_id].includes(input.offsetAccountId))throw new ApiError(422,"ASSET_OFFSET_INVALID","Use a separate acquisition offset account.");
  const journal=await prepareJournal(write,actor,{chartId:chart,periodId:input.periodId,entryDate:input.entryDate,currency:asset.currency,memo:`Acquisition ${asset.asset_number}: ${input.evidenceRef}`,sourceType:"asset_acquisition",sourceId:id,isOpening:false,lines:[{accountId:asset.account_id,debit:minorToDecimal(asset.cost_minor),credit:"0"},{accountId:input.offsetAccountId,debit:"0",credit:minorToDecimal(asset.cost_minor)}]},key,"posted"),next=asset.version+1,result={id,status:"in_service",version:next,journalId:journal.result.id};
  return executeCommand(write,actor,key,"finance.asset.activate",input,id,next,result,[...assetGuards(write,actor,key,asset),...journal.steps,write.prepare("UPDATE finance_assets SET status='in_service',activation_journal_id=?,activated_by=?,version=?,updated_at=? WHERE id=?").bind(journal.result.id,actor.userId,next,Date.now(),id),event(write,actor,key,id,next,"activated",{...input,journalId:journal.result.id}),write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,id,"activate",actor.userId,actor.assignmentId,asset.version,await sha256(JSON.stringify({asset,input})),Date.now())]);
}
function monthOrdinal(date:string){return Number(date.slice(0,4))*12+Number(date.slice(5,7))-1;}
export async function depreciateAsset(actor:WorkContext,id:string,input:z.infer<typeof assetDepreciationSchema>,key:string){
  const r=await resources(actor,"finance.write");actor=r.actor;const {write}=r,prior=await replayCommand(write,actor,key,"finance.asset.depreciate",id,input);if(prior)return prior;
  const asset=await getAsset(write,actor,id);version(asset,input.expectedVersion);
  if(asset.status!=="in_service"||!asset.activation_journal_id)throw new ApiError(409,"ASSET_NOT_RECOGNIZED","Recognize the asset before depreciation.");
  if(!asset.depreciation_account_id||!asset.depreciation_expense_account_id)throw new ApiError(422,"ASSET_DEPRECIATION_MAPPING_REQUIRED","Configure accumulated depreciation and expense accounts.");
  if(await write.prepare("SELECT 1 FROM finance_asset_disposals WHERE asset_id=? AND status='draft'").bind(id).first())throw new ApiError(409,"ASSET_DISPOSAL_PENDING","Decide the prepared disposal before changing depreciation.");
  const priorRows=await write.prepare("SELECT ordinal,amount_minor FROM finance_asset_depreciations WHERE asset_id=? ORDER BY ordinal").bind(id).all<{ordinal:number;amount_minor:number}>(),index=priorRows.results.length;
  if(index>=asset.useful_life_months)throw new ApiError(409,"ASSET_FULLY_DEPRECIATED","The scheduled depreciation is complete.");
  if(monthOrdinal(input.entryDate)!==monthOrdinal(asset.in_service_date)+index||input.entryDate<asset.in_service_date)throw new ApiError(422,"ASSET_DEPRECIATION_SEQUENCE","Post each in-service calendar month once, in sequence.");
  const depreciable=BigInt(asset.cost_minor-asset.residual_minor),life=BigInt(asset.useful_life_months),previous=depreciable*BigInt(index)/life,target=depreciable*BigInt(index+1)/life,amount=Number(target-previous);
  if(asset.accumulated_depreciation_minor!==Number(previous)||priorRows.results.reduce((sum,row)=>sum+row.amount_minor,0)!==Number(previous))throw new ApiError(409,"ASSET_HISTORY_RECONCILIATION_REQUIRED","The retained asset history needs reconciliation before another depreciation.");
  if(amount<=0)throw new ApiError(422,"ASSET_DEPRECIATION_PRECISION","Cost is too small for a positive monthly posting under this convention.");
  const chart=await accounts(write,actor,{cost:asset.account_id,accumulated:asset.depreciation_account_id,expense:asset.depreciation_expense_account_id}),journal=await prepareJournal(write,actor,{chartId:chart,periodId:input.periodId,entryDate:input.entryDate,currency:asset.currency,memo:`Full-month straight-line depreciation ${asset.asset_number}`,sourceType:"asset_depreciation",sourceId:`${id}:${input.entryDate.slice(0,7)}`,isOpening:false,lines:[{accountId:asset.depreciation_expense_account_id,debit:minorToDecimal(amount),credit:"0"},{accountId:asset.depreciation_account_id,debit:"0",credit:minorToDecimal(amount)}]},key,"posted"),next=asset.version+1,result={id,status:"posted",version:next,journalId:journal.result.id,amount:minorToDecimal(amount)};
  const money=await snapshotMoney(write,actor,{sourceType:"asset_depreciation",sourceId:`${id}:${input.entryDate.slice(0,7)}`,amountKind:"depreciation",transactionDate:input.entryDate,currency:asset.currency,currencyScale:2,amountUnits:String(amount)});
  return executeCommand(write,actor,key,"finance.asset.depreciate",input,id,next,result,[...assetGuards(write,actor,key,asset),assertion(write,key,"asset-disposal","NOT EXISTS(SELECT 1 FROM finance_asset_disposals WHERE asset_id=? AND status='draft')",[id]),...journal.steps,money.statement,write.prepare("INSERT INTO finance_asset_depreciations(id,asset_id,company_id,depreciation_month,ordinal,period_id,entry_date,amount_minor,journal_id,asset_version,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(`asset-depreciation-${key}`,id,actor.companyId,input.entryDate.slice(0,7),index+1,input.periodId,input.entryDate,amount,journal.result.id,next,key,Date.now()),write.prepare("UPDATE finance_assets SET accumulated_depreciation_minor=?,version=?,updated_at=? WHERE id=?").bind(Number(target),next,Date.now(),id),event(write,actor,key,id,next,"depreciated",{...input,amountMinor:amount,journalId:journal.result.id})]);
}
export async function prepareAssetDisposal(actor:WorkContext,id:string,input:z.infer<typeof assetDisposalSchema>,key:string){
  const r=await resources(actor,"finance.write");actor=r.actor;const {write}=r,disposalId=`asset-disposal-${key}`,prior=await replayCommand(write,actor,key,"finance.asset.disposal.prepare",disposalId,input);if(prior)return prior;
  const asset=await getAsset(write,actor,id);version(asset,input.expectedVersion);if(asset.status!=="in_service"||!asset.activation_journal_id)throw new ApiError(409,"ASSET_NOT_RECOGNIZED","Only a recognized in-service asset can be disposed.");
  const amount=decimalToMinor(input.proceeds);if(amount<0||amount>0&&!input.proceedsAccountId)throw new ApiError(422,"ASSET_PROCEEDS_INVALID","Positive disposal proceeds require a destination account.");
  if(input.entryDate<asset.in_service_date)throw new ApiError(422,"ASSET_DISPOSAL_DATE","Disposal cannot precede the in-service date.");
  const latest=await write.prepare("SELECT MAX(entry_date) date FROM finance_asset_depreciations WHERE asset_id=?").bind(id).first<{date:string|null}>();if(latest?.date&&input.entryDate<latest.date)throw new ApiError(422,"ASSET_DISPOSAL_DATE","Disposal cannot precede posted depreciation.");
  const chart=await accounts(write,actor,{cost:asset.account_id,accumulated:asset.depreciation_account_id,expense:asset.depreciation_expense_account_id}),mapped=await write.prepare("SELECT id,category FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (?,?,?)").bind(actor.companyId,chart,input.proceedsAccountId??null,input.gainAccountId,input.lossAccountId).all<{id:string;category:string}>(),byId=new Map(mapped.results.map(a=>[a.id,a.category]));
  if(!["revenue","other_income"].includes(byId.get(input.gainAccountId)??"")||!["expense","other_expense"].includes(byId.get(input.lossAccountId)??"")||amount>0&&byId.get(input.proceedsAccountId!)!=="asset")throw new ApiError(422,"ASSET_DISPOSAL_ACCOUNTS_INVALID","Use same-chart asset proceeds, income gain and expense loss accounts.");
  const result={id:disposalId,assetId:id,status:"draft",version:1},now=Date.now();return executeCommand(write,actor,key,"finance.asset.disposal.prepare",input,disposalId,1,result,[...assetGuards(write,actor,key,asset),write.prepare("INSERT INTO finance_asset_disposals(id,asset_id,company_id,asset_version,period_id,entry_date,proceeds_minor,proceeds_account_id,gain_account_id,loss_account_id,evidence_ref,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(disposalId,id,actor.companyId,asset.version,input.periodId,input.entryDate,amount,input.proceedsAccountId??null,input.gainAccountId,input.lossAccountId,input.evidenceRef,actor.userId,now,now),event(write,actor,key,id,asset.version,"disposal_prepared",{...input,disposalId})]);
}
export async function decideAssetDisposal(actor:WorkContext,id:string,action:"post"|"cancel",input:z.infer<typeof assetDecisionSchema>,key:string){
  const r=await resources(actor,action==="post"?"finance.approve":"finance.write");actor=r.actor;const {write}=r,operation=`finance.asset.disposal.${action}`,prior=await replayCommand(write,actor,key,operation,id,input);if(prior)return prior;
  const disposal=await write.prepare("SELECT * FROM finance_asset_disposals WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Disposal>();if(!disposal)throw new ApiError(404,"ASSET_DISPOSAL_NOT_FOUND","Disposal not found.");version(disposal,input.expectedVersion);if(disposal.status!=="draft")throw new ApiError(409,"ASSET_DISPOSAL_FINAL","This disposal was already decided.");
  const asset=await getAsset(write,actor,disposal.asset_id);version(asset,disposal.asset_version);
  if(action==="post"&&samePerson(actor,disposal.created_by))throw new ApiError(403,"SELF_APPROVAL","Another finance reviewer must post the disposal.");
  const steps:SqlStatement[]=[...assetGuards(write,actor,key,asset),write.prepare("SELECT id FROM finance_asset_disposals WHERE id=? FOR UPDATE").bind(id),assertion(write,key,"disposal-version","EXISTS(SELECT 1 FROM finance_asset_disposals WHERE id=? AND company_id=? AND status='draft' AND version=?)",[id,actor.companyId,disposal.version])];
  let journalId:string|null=null;
  if(action==="post"){
    if(asset.status!=="in_service"||!asset.activation_journal_id||!asset.depreciation_account_id)throw new ApiError(409,"ASSET_NOT_RECOGNIZED","The current asset is unavailable for disposal.");
    const book=asset.cost_minor-asset.accumulated_depreciation_minor,gain=disposal.proceeds_minor-book,chart=await accounts(write,actor,{cost:asset.account_id,accumulated:asset.depreciation_account_id,expense:asset.depreciation_expense_account_id});
    const lines=[{accountId:asset.account_id,debit:"0",credit:minorToDecimal(asset.cost_minor)},...(asset.accumulated_depreciation_minor>0?[{accountId:asset.depreciation_account_id,debit:minorToDecimal(asset.accumulated_depreciation_minor),credit:"0"}]:[]),...(disposal.proceeds_minor>0?[{accountId:disposal.proceeds_account_id!,debit:minorToDecimal(disposal.proceeds_minor),credit:"0"}]:[]),...(gain>0?[{accountId:disposal.gain_account_id,debit:"0",credit:minorToDecimal(gain)}]:gain<0?[{accountId:disposal.loss_account_id,debit:minorToDecimal(-gain),credit:"0"}]:[])];
    const journal=await prepareJournal(write,actor,{chartId:chart,periodId:disposal.period_id,entryDate:disposal.entry_date,currency:asset.currency,memo:`Disposal ${asset.asset_number}: ${input.reason}`,sourceType:"asset_disposal",sourceId:id,isOpening:false,lines},key,"posted");journalId=journal.result.id;
    const money=await snapshotMoney(write,actor,{sourceType:"asset_disposal",sourceId:id,amountKind:"proceeds",transactionDate:disposal.entry_date,currency:asset.currency,currencyScale:2,amountUnits:String(disposal.proceeds_minor)});
    steps.push(...journal.steps,money.statement,write.prepare("UPDATE finance_assets SET status='retired',version=version+1,updated_at=? WHERE id=?").bind(Date.now(),asset.id));
  }
  const result={id,assetId:asset.id,status:action==="post"?"posted":"cancelled",version:disposal.version+1,journalId,assetVersion:asset.version+(action==="post"?1:0)};
  steps.push(write.prepare("UPDATE finance_asset_disposals SET status=?,journal_id=?,decided_by=?,version=?,updated_at=? WHERE id=?").bind(result.status,journalId,actor.userId,result.version,Date.now(),id),event(write,actor,key,asset.id,result.assetVersion,action==="post"?"disposed":"disposal_cancelled",{...input,disposalId:id,journalId}),write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,id,action,actor.userId,actor.assignmentId,disposal.version,await sha256(JSON.stringify({disposal,asset,input})),Date.now()));
  return executeCommand(write,actor,key,operation,input,id,result.version,result,steps);
}
