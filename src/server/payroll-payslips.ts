import "server-only";
import {z} from "zod";
import {hasPermission} from "@/lib/permissions";
import {minorToDecimal} from "@/lib/finance-money";
import type {WorkContext} from "./authorization";
import {assertion,executeCommand,replayCommand} from "./commands";
import {sha256} from "./crypto";
import {ApiError} from "./http";
import {requirePermission} from "./iam-permissions";
import {holdingResources} from "./runtime";
import type {SqlDatabase,SqlStatement} from "./runtime/types";

type Row=Record<string,unknown>;
type PayslipItem={id:string;engagement_id:string;person_id:string;recipient_user_id:string|null;input_snapshot_json:string;result_json:string;gross_minor:number;deductions_minor:number;employer_cost_minor:number;net_minor:number;currency:string};
export type PayslipSnapshot={schema:"payroll.payslip.v1";company:{id:string;name:string};employee:{personId:string;engagementId:string;name:string;jobTitle:string};run:{id:string;revision:number;version:number;mode:string;periodStart:string;periodEnd:string;payDate:string;currency:string;inputDigest:string;journalId:string;issuedAt:number};grossMinor:number;deductionsMinor:number;employerCostMinor:number;netMinor:number;input:Row;result:Row;statutory:boolean;sourceDigest:string};
type Slip=Row&{id:string;run_id:string;person_id:string;recipient_user_id:string|null;snapshot_json:string;export_html:string;export_sha256:string;issued_at:number};
export const issuePayslipSchema=z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(10).max(2000)}).strict();
function escaped(value:unknown){return String(value??"").replace(/[&<>"']/g,char=>({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"}[char]!));}
function render(snapshot:PayslipSnapshot){
 const explanation=(snapshot.result.explanation??{}) as Row,components=Array.isArray(explanation.components)?explanation.components as Row[]:[],money=(n:unknown)=>`${escaped(snapshot.run.currency)} ${minorToDecimal(Number(n??0))}`;
 const componentRows=components.filter(item=>item.includedInTotals!==false).map(item=>`<tr><td>${escaped(item.label)}</td><td>${escaped(item.kind)}</td><td class="amount">${money(item.amountMinor)}</td></tr>`).join("");
 const statutory=snapshot.result.statutory as {adapterId?:string;sourceDigest?:string;input?:Row;calculation?:Row}|undefined;
 const statutoryNote=snapshot.statutory?`<p>Dated Thai payroll calculation · ${escaped(statutory?.adapterId)} · source ${escaped(statutory?.sourceDigest)}. Independently reviewed employee evidence; engineering verification, external certification separately reported.</p>`:`<p>Company payroll calculation · ${escaped(snapshot.run.mode)}. This record does not assert Thai statutory adapter validation.</p>`;
 const ytd=statutory?.input?`<p>Taxable income before this pay: ${money(statutory.input.taxablePaidBeforeMinor)} · tax withheld before this pay: ${money(statutory.input.taxWithheldBeforeMinor)}</p>`:"";
 return `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Payslip ${escaped(snapshot.employee.name)} ${escaped(snapshot.run.payDate)}</title><style>@page{size:A4;margin:18mm}body{font:14px Arial,sans-serif;color:#14202d;max-width:760px;margin:24px auto}h1{font-size:26px}h2{font-size:18px}table{border-collapse:collapse;width:100%;margin:20px 0}th,td{padding:8px;border-bottom:1px solid #ccd2da;text-align:left}.amount{text-align:right}small{overflow-wrap:anywhere;color:#465566}footer{margin-top:24px}</style></head><body><h1>Employee payslip / สลิปเงินเดือน</h1><h2>${escaped(snapshot.company.name)}</h2><p>${escaped(snapshot.employee.name)} · ${escaped(snapshot.employee.jobTitle)}</p><p>Work period ${escaped(snapshot.run.periodStart)} to ${escaped(snapshot.run.periodEnd)} · pay date ${escaped(snapshot.run.payDate)} · revision ${snapshot.run.revision}</p><table><thead><tr><th>Calculation</th><th>Type</th><th class="amount">Amount</th></tr></thead><tbody><tr><td>Base wages and overtime</td><td>Earning</td><td class="amount">${money(snapshot.grossMinor-components.filter(item=>item.kind==="earning").reduce((sum,item)=>sum+Number(item.amountMinor??0),0))}</td></tr>${componentRows}<tr><th>Gross pay</th><td></td><th class="amount">${money(snapshot.grossMinor)}</th></tr><tr><th>Employee deductions</th><td></td><th class="amount">${money(snapshot.deductionsMinor)}</th></tr><tr><th>Net payable</th><td></td><th class="amount">${money(snapshot.netMinor)}</th></tr></tbody></table><p>Regular hours ${escaped(explanation.regularHours)} · overtime hours ${escaped(explanation.overtimeHours)} · overtime pay ${money(explanation.overtimeMinor)}</p>${statutoryNote}${ytd}<p>Issued from independently posted payroll. Bank delivery is shown separately in the current payment record.</p><footer><small>Run ${escaped(snapshot.run.id)} · employee ${escaped(snapshot.employee.personId)} · journal ${escaped(snapshot.run.journalId)}<br>Immutable source SHA256 ${escaped(snapshot.sourceDigest)}</small></footer></body></html>`;
}

/** Called before posting, with its statements committed after the real source journal and items. */
export async function preparePayslipIssue(db:SqlDatabase,actor:WorkContext,runId:string,journalId:string,commandId:string,postedVersion?:number):Promise<{ids:string[];guards:SqlStatement[];steps:SqlStatement[]}>{
 const run=await db.prepare("SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=? AND status IN ('approved','posted')").bind(runId,actor.companyId).first<Row>();
 if(!run||!run.input_digest)throw new ApiError(409,"PAYSLIP_SOURCE_NOT_POSTABLE","A retained payroll calculation and independent approval are required before issuing employee slips.");
 const items=(await db.prepare("SELECT i.*,e.person_id,p.user_id recipient_user_id FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id AND e.company_id=? JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE i.run_id=? AND i.worker_type='employee' AND i.status IN ('calculated','approved','posted') ORDER BY i.id").bind(actor.companyId,runId).all<PayslipItem>()).results;
 const company=await db.prepare("SELECT display_name FROM organization_nodes WHERE id=? AND kind='company'").bind(actor.companyId).first<{display_name:string}>(),now=Date.now(),guards:SqlStatement[]=[],steps:SqlStatement[]=[],ids:string[]=[];
 if(!items.length)throw new ApiError(409,"PAYSLIP_SOURCE_EMPTY","The posted run needs calculated employee items.");
 for(const item of items){
  const input=JSON.parse(item.input_snapshot_json) as Row,result=JSON.parse(item.result_json) as Row,payee=input.payee as {id?:string;legal_name?:string}|undefined,engagement=input.engagement as {job_title?:string}|undefined;
  if(payee?.id!==item.person_id||!payee.legal_name||Number(item.gross_minor)-Number(item.deductions_minor)!==Number(item.net_minor))throw new ApiError(409,"PAYSLIP_SOURCE_CHANGED","The employee identity and exact totals must match the retained payroll input.");
  const sourceDigest=await sha256(JSON.stringify({runInputDigest:run.input_digest,itemId:item.id,input: item.input_snapshot_json,result:item.result_json})),id=`payslip-${runId}-${item.id}`,snapshot:PayslipSnapshot={schema:"payroll.payslip.v1",company:{id:actor.companyId,name:company?.display_name??actor.companyId},employee:{personId:item.person_id,engagementId:item.engagement_id,name:payee.legal_name,jobTitle:engagement?.job_title??""},run:{id:runId,revision:Number(run.revision),version:postedVersion??Number(run.version),mode:String(run.mode),periodStart:String(run.period_start),periodEnd:String(run.period_end),payDate:String(run.pay_date),currency:String(run.currency),inputDigest:String(run.input_digest),journalId,issuedAt:now},grossMinor:Number(item.gross_minor),deductionsMinor:Number(item.deductions_minor),employerCostMinor:Number(item.employer_cost_minor),netMinor:Number(item.net_minor),input,result,statutory:Boolean(result.statutory),sourceDigest},html=render(snapshot),checksum=await sha256(html);
  ids.push(id);
  guards.push(assertion(db,commandId,`payslip-item-${item.id}`,"EXISTS(SELECT 1 FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE i.id=? AND i.run_id=? AND e.company_id=? AND e.person_id=? AND p.user_id IS NOT DISTINCT FROM ? AND i.input_snapshot_json=? AND i.result_json=? AND i.net_minor=?)",[item.id,runId,actor.companyId,item.person_id,item.recipient_user_id,item.input_snapshot_json,item.result_json,item.net_minor]));
  steps.push(db.prepare("INSERT INTO hr_issued_payslips(id,company_id,run_id,run_revision,run_version,item_id,engagement_id,person_id,recipient_user_id,pay_date,currency,gross_minor,deductions_minor,net_minor,source_digest,snapshot_json,export_html,export_sha256,issued_by,command_id,issued_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,runId,run.revision,snapshot.run.version,item.id,item.engagement_id,item.person_id,item.recipient_user_id,run.pay_date,run.currency,item.gross_minor,item.deductions_minor,item.net_minor,sourceDigest,JSON.stringify(snapshot),html,checksum,actor.userId,commandId,now));
 }
 return {ids,guards,steps};
}

async function currentAccess(db:SqlDatabase,actor:WorkContext,ownOnly=false){
 const permission=!ownOnly&&hasPermission(actor,"payroll.read")&&!actor.branchId&&!actor.departmentId?"payroll.read":"self.requests";
 if(!hasPermission(actor,permission))throw new ApiError(404,"PAYSLIP_NOT_FOUND","The retained employee payslip is unavailable in this work role.");
 const now=Date.now(),grant=await db.prepare("SELECT 1 FROM assignments a WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.role=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?) AND iam_role_allows(a.role,a.company_id,?) AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?)").bind(actor.assignmentId,actor.companyId,actor.userId,actor.role,now,now,permission,actor.branchId,actor.departmentId).first();
 if(!grant)throw new ApiError(404,"PAYSLIP_NOT_FOUND","The retained employee payslip is unavailable under your current grant.");
 return permission==="payroll.read";
}
export async function listPayslips(actor:WorkContext,{runId,ownOnly=true}:{runId?:string;ownOnly?:boolean}={}){
 const {write}=await holdingResources(actor.holdingId),broad=await currentAccess(write,actor,ownOnly);
 return (await write.prepare(`SELECT s.id,s.run_id,s.run_revision,s.pay_date,s.currency,s.gross_minor,s.deductions_minor,s.net_minor,s.source_digest,s.export_sha256,s.issued_at,s.person_id,s.snapshot_json FROM hr_issued_payslips s JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id WHERE s.company_id=? ${runId?"AND s.run_id=?":""} ${broad?"":"AND s.recipient_user_id=? AND p.user_id=? AND p.merged_into_id IS NULL"} ORDER BY s.pay_date DESC,s.issued_at DESC,s.id`).bind(actor.companyId,...(runId?[runId]:[]),...(broad?[]:[actor.userId,actor.userId])).all<Row>()).results.map(row=>({id:String(row.id),runId:String(row.run_id),runRevision:Number(row.run_revision),payDate:String(row.pay_date),currency:String(row.currency),grossMinor:Number(row.gross_minor),deductionsMinor:Number(row.deductions_minor),netMinor:Number(row.net_minor),sourceDigest:String(row.source_digest),checksum:String(row.export_sha256),issuedAt:Number(row.issued_at),employeeName:(JSON.parse(String(row.snapshot_json)) as PayslipSnapshot).employee.name}));
}
export async function getPayslip(actor:WorkContext,id:string){
 const {write}=await holdingResources(actor.holdingId),broad=await currentAccess(write,actor),slip=await write.prepare(`SELECT s.* FROM hr_issued_payslips s JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id WHERE s.id=? AND s.company_id=? ${broad?"":"AND s.recipient_user_id=? AND p.user_id=? AND p.merged_into_id IS NULL"}`).bind(id,actor.companyId,...(broad?[]:[actor.userId,actor.userId])).first<Slip>();
 if(!slip)throw new ApiError(404,"PAYSLIP_NOT_FOUND","The retained employee payslip was not found.");
 const payment=await write.prepare("SELECT i.status,i.settled_at,b.status batch_status FROM hr_payment_batch_items i JOIN hr_payment_batches b ON b.id=i.batch_id WHERE b.company_id=? AND b.source_type='payroll' AND b.source_id=? AND i.person_id=? AND b.status!='cancelled' ORDER BY b.created_at DESC LIMIT 1").bind(actor.companyId,slip.run_id,slip.person_id).first<Row>();
 return {id:slip.id,checksum:slip.export_sha256,snapshot:JSON.parse(slip.snapshot_json) as PayslipSnapshot,payment};
}
export async function getPayslipExport(actor:WorkContext,id:string){
 const detail=await getPayslip(actor,id),{write}=await holdingResources(actor.holdingId),row=await write.prepare("SELECT export_html,export_sha256 FROM hr_issued_payslips WHERE id=? AND company_id=?").bind(id,actor.companyId).first<{export_html:string;export_sha256:string}>();
 if(!row||await sha256(row.export_html)!==detail.checksum)throw new ApiError(409,"PAYSLIP_EXPORT_CHECKSUM_INVALID","The retained payslip bytes failed their issued checksum.");
 return {html:row.export_html,checksum:row.export_sha256};
}
export async function issuePostedPayslips(actor:WorkContext,runId:string,input:z.infer<typeof issuePayslipSchema>,commandId:string){
 actor=requirePermission(actor,"finance.approve");input=issuePayslipSchema.parse(input);if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Issue retained employee slips with the Company Payroll owner.");
 const {write}=await holdingResources(actor.holdingId),previous=await replayCommand(write,actor,commandId,"people.payroll.payslips.issue",runId,input);if(previous)return previous;
 await currentAccess(write,actor);
 const posting=await write.prepare("SELECT p.journal_id,r.version FROM hr_payroll_postings p JOIN hr_payroll_runs r ON r.id=p.run_id JOIN finance_journals j ON j.id=p.journal_id WHERE r.id=? AND r.company_id=? AND r.status='posted' AND r.version=? AND j.status='posted'").bind(runId,actor.companyId,input.expectedVersion).first<{journal_id:string;version:number}>();
 if(!posting)throw new ApiError(409,"PAYSLIP_SOURCE_CHANGED","Reload the independently posted payroll revision.");
 const prepared=await preparePayslipIssue(write,actor,runId,posting.journal_id,commandId);
 return executeCommand(write,actor,commandId,"people.payroll.payslips.issue",input,runId,posting.version,{runId,payslipIds:prepared.ids},[assertion(write,commandId,"payslip-source","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)",[runId,actor.companyId,input.expectedVersion]),assertion(write,commandId,"payslips-not-issued","NOT EXISTS(SELECT 1 FROM hr_issued_payslips WHERE run_id=?)",[runId]),...prepared.guards,...prepared.steps]);
}
