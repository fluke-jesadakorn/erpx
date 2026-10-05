import "server-only";
import {z} from "zod";
import type {WorkContext} from "./authorization";
import type {SqlDatabase} from "./runtime/types";
import type {HoldingId} from "./runtime";
import {holdingResources} from "./runtime";
import {requirePermission} from "./iam-permissions";
import {loadIamAuthority} from "./iam-store";
import {assertion,executeCommand,replayCommand} from "./commands";
import {financeTransaction} from "./finance-transactions";
import {businessBillingSchema,recognitionRunSchema,prepareBusinessBilling,prepareRecognitionRun} from "./finance-business-contracts";
import {assetPolicyRunSchema,prepareAssetPolicyRun} from "./finance-asset-policies";
import {instrumentRunSchema,prepareInstrumentRun} from "./finance-instruments";
import {sha256} from "./crypto";
import {ApiError} from "./http";
const kind=z.enum(["billing","recognition","asset_depreciation","instrument_payment"]);
export const financeDueDraftSchema=z.object({kind,sourceId:z.string().min(1),runOn:z.string().date(),input:z.record(z.string(),z.unknown()),reason:z.string().trim().min(3).max(2000)}).strict();
export const financeDueDecisionSchema=z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(2000)}).strict();
type Kind=z.infer<typeof kind>;
type Row=Record<string,string|number|null>;
function parsedInput(k:Kind,input:Record<string,unknown>){switch(k){case "billing":return businessBillingSchema.parse(input);case "recognition":return recognitionRunSchema.parse(input);case "asset_depreciation":return assetPolicyRunSchema.parse(input);case "instrument_payment":return instrumentRunSchema.parse(input);}}
const sources:Record<Kind,string>={billing:"finance_business_contracts",recognition:"finance_business_billing_runs",asset_depreciation:"finance_asset_policy_editions",instrument_payment:"finance_instrument_editions"};
function holding(actor:WorkContext,permission:string){actor=requirePermission(actor,permission);if(actor.scope.kind!=="holding"||actor.assignmentScope!=="holding")throw new ApiError(403,"FINANCE_SCHEDULE_HOLDING_REQUIRED","Schedule Company-wide Finance drafts with a Holding Finance assignment.");return actor;}
export async function financeDueWorkspace(actor:WorkContext){actor=holding(actor,"finance.read");const {write}=await holdingResources(actor.holdingId);return (await write.prepare("SELECT * FROM finance_due_drafts WHERE company_id=? ORDER BY run_on DESC,created_at DESC LIMIT 500").bind(actor.companyId).all<Row>()).results;}
export async function scheduleFinanceDraft(actor:WorkContext,raw:z.input<typeof financeDueDraftSchema>,key:string){
 actor=holding(actor,"finance.write");const input=financeDueDraftSchema.parse(raw),payload=parsedInput(input.kind,input.input),id=`finance-due-${key}`,op="finance.due-draft.schedule",{write}=await holdingResources(actor.holdingId);
 if(input.kind==="recognition"&&recognitionRunSchema.parse(payload).billingRunId!==input.sourceId)throw new ApiError(422,"FINANCE_SCHEDULE_SOURCE","Recognition must use the selected billing run.");
 return financeTransaction(write,async db=>{
  const prior=await replayCommand(db,actor,key,op,id,input);if(prior)return prior;
  const source=await db.prepare(`SELECT * FROM ${sources[input.kind]} WHERE id=? AND company_id=?`).bind(input.sourceId,actor.companyId).first<Row>(),assignment=await db.prepare("SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?").bind(actor.assignmentId,actor.userId,actor.companyId,actor.role,Date.now(),Date.now()).first<{version:number}>();
  const sourceVersion=Number(source?.version),expected="expectedVersion" in payload?payload.expectedVersion:sourceVersion;
  if(!source||!assignment||!Number.isSafeInteger(sourceVersion)||sourceVersion!==expected||input.kind!=="recognition"&&source.status!=="active")throw new ApiError(409,"FINANCE_SCHEDULE_SOURCE_CHANGED","Choose the current active source edition and a current Company Finance assignment.");
  const bytes=JSON.stringify(payload),now=Date.now();return executeCommand(db,actor,key,op,input,id,1,{id,status:"scheduled",version:1},[db.prepare("INSERT INTO finance_due_drafts(id,company_id,holding_id,kind,source_id,run_on,source_version,input_json,input_hash,delegated_by,assignment_id,assignment_version,role,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,actor.holdingId,input.kind,input.sourceId,input.runOn,sourceVersion,bytes,await sha256(bytes),actor.userId,actor.assignmentId,assignment.version,actor.role,input.reason,now,now)]);
 });
}
export async function decideFinanceDueDraft(actor:WorkContext,id:string,raw:z.input<typeof financeDueDecisionSchema>,action:"retry"|"cancel",key:string){
 actor=holding(actor,"finance.write");const input=financeDueDecisionSchema.parse(raw),{write}=await holdingResources(actor.holdingId),op=`finance.due-draft.${action}`;
 return financeTransaction(write,async db=>{const prior=await replayCommand(db,actor,key,op,id,input);if(prior)return prior;const row=await db.prepare("SELECT * FROM finance_due_drafts WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Row>();if(!row||row.version!==input.expectedVersion||!["scheduled","blocked"].includes(String(row.status)))throw new ApiError(409,"FINANCE_SCHEDULE_CHANGED","Refresh this pending Finance schedule before changing it.");return executeCommand(db,actor,key,op,input,id,input.expectedVersion+1,{id,status:action==="retry"?"scheduled":"cancelled",version:input.expectedVersion+1},[assertion(db,key,"schedule-version","EXISTS(SELECT 1 FROM finance_due_drafts WHERE id=? AND company_id=? AND version=? AND status IN ('scheduled','blocked'))",[id,actor.companyId,input.expectedVersion]),db.prepare("UPDATE finance_due_drafts SET status=?,version=version+1,next_attempt_at=0,last_code=NULL,last_message=NULL,updated_at=? WHERE id=?").bind(action==="retry"?"scheduled":"cancelled",Date.now(),id)]);});
}
/** Native durable-job execution prepares drafts only, retaining and rechecking the delegated assignment. */
export async function drainFinanceDueDrafts(write:SqlDatabase,holdingId:HoldingId,now=new Date()):Promise<number>{
 const candidates=(await write.prepare("SELECT id FROM finance_due_drafts WHERE holding_id=? AND status IN ('scheduled','blocked') AND run_on<=? AND next_attempt_at<=? ORDER BY run_on,id LIMIT 25").bind(holdingId,new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(now),now.getTime()).all<{id:string}>()).results;let completed=0;
 for(const candidate of candidates){try{const done=await financeTransaction(write,async db=>{
  const r=await db.prepare("SELECT * FROM finance_due_drafts WHERE id=? AND status IN ('scheduled','blocked') AND next_attempt_at<=? FOR UPDATE SKIP LOCKED").bind(candidate.id,now.getTime()).first<Row>();if(!r)return false;
  if(await sha256(String(r.input_json))!==r.input_hash)throw new ApiError(409,"FINANCE_SCHEDULE_EVIDENCE_CHANGED","The retained scheduled draft differs from its original checksum.");
  const a=await db.prepare("SELECT a.version FROM assignments a JOIN organization_nodes n ON n.id=a.company_id AND n.status='active' WHERE a.id=? AND a.user_id=? AND a.company_id=? AND a.role=? AND a.scope_kind='holding' AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?").bind(r.assignment_id,r.delegated_by,r.company_id,r.role,Date.now(),Date.now()).first<{version:number}>();
  if(!a||a.version!==r.assignment_version)throw new ApiError(403,"FINANCE_SCHEDULE_AUTHORITY_CHANGED","The delegated Finance assignment changed. Cancel and schedule with a current assignment.");
  const iam=await loadIamAuthority(db,String(r.company_id),String(r.delegated_by),String(r.role));iam.queuedAssignmentVersion=a.version;
  const actor=holding({userId:String(r.delegated_by),name:"Scheduled Finance preparer",email:"",assignmentId:String(r.assignment_id),role:String(r.role),companyId:String(r.company_id),branchId:null,departmentId:null,holdingId,holdingSlug:"",holdingName:"",scope:{kind:"holding",companyId:String(r.company_id),holdingId,holdingSlug:"",holdingName:""},assignmentScope:"holding",inherited:false,modules:["finance"],assignments:[],workspaces:[],iam},"finance.write"),p=JSON.parse(String(r.input_json)),key=`${r.id}:draft`;
  let result:Record<string,unknown>;switch(r.kind){case "billing":result=await prepareBusinessBilling(actor,String(r.source_id),p,key,db);break;case "recognition":result=await prepareRecognitionRun(actor,p,key,db);break;case "asset_depreciation":result=await prepareAssetPolicyRun(actor,String(r.source_id),p,key,db);break;case "instrument_payment":result=await prepareInstrumentRun(actor,String(r.source_id),p,key,db);break;default:throw new ApiError(422,"FINANCE_SCHEDULE_KIND","Choose a supported draft workflow.");}
  await db.prepare("UPDATE finance_due_drafts SET status='prepared',version=version+1,result_json=?,attempts=attempts+1,last_code=NULL,last_message=NULL,updated_at=? WHERE id=?").bind(JSON.stringify(result),now.getTime(),r.id).run();return true;
 });if(done)completed++;}catch(error){const code=error instanceof ApiError?error.code:"FINANCE_SCHEDULE_RETRY",message=error instanceof ApiError?error.message:"The draft could not be prepared. Check the source configuration and retry.";await write.prepare("UPDATE finance_due_drafts SET status='blocked',version=version+1,attempts=attempts+1,last_code=?,last_message=?,next_attempt_at=?,updated_at=? WHERE id=? AND status IN ('scheduled','blocked')").bind(code,message,now.getTime()+3600000,now.getTime(),candidate.id).run();}}
 return completed;
}
