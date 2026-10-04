import "server-only";
import {z} from "zod";
import {createHash} from "node:crypto";
import {financialNoteTitles,financialPresentation,type FinancialPresentationCode} from "@/lib/finance-reporting";
import {samePerson} from "@/lib/iam";
import {minorToDecimal} from "@/lib/finance-money";
import type {WorkContext} from "./authorization";
import type {SqlDatabase,SqlStatement} from "./runtime/types";
import {holdingResources} from "./runtime";
import {requirePermission} from "./iam-permissions";
import {assertion,executeCommand,replayCommand} from "./commands";
import {ApiError} from "./http";
import {prepareJournal,type JournalLineInput} from "./finance";
import {snapshotMoney} from "./company-money";

const date=z.iso.date(),reason=z.string().trim().min(10).max(4000),version=z.number().int().positive();
const digest=(value:unknown)=>createHash("sha256").update(JSON.stringify(value)).digest("hex");
const codes=Object.keys(financialPresentation) as [FinancialPresentationCode,...FinancialPresentationCode[]];
export const statementPackageSchema=z.object({chartId:z.string().min(1),startDate:date,endDate:date,comparativeStartDate:date,comparativeEndDate:date,previousPackageId:z.string().min(1).optional(),
 presentation:z.array(z.object({accountId:z.string().min(1),line:z.enum(codes)})).min(1).max(1000),
 notes:z.object({basis:reason,policies:reason,going_concern:reason,judgments:reason,disclosures:reason}),reason}).strict();
export const reportingDecisionSchema=z.object({expectedVersion:version,reason}).strict();
export const yearClosingSchema=z.object({chartId:z.string().min(1),startDate:date,endDate:date,periodId:z.string().min(1),retainedEarningsAccountId:z.string().min(1),reason}).strict();
type Profile={id:string;version:number;statutory_chart_id:string;fiscal_start_month:number;effective_date:string;policy_evidence:string;eligibility_evidence:string;inventory_policy:string;depreciation_policy:string;tax_basis:string};
type Chart={id:string;name:string;base_currency:string;status:string;company_name:string};
type Account={id:string;code:string;name:string;category:string;currency:string;status:string;balance_minor:number;opening_minor:number;income_minor:number;owner_movements_minor:number;closing_transfer_minor:number};
type Package=Record<string,string|number|null>;
type Presentation=z.infer<typeof statementPackageSchema>["presentation"];
export type StatementSet={dates:{start:string;end:string};position:Array<{code:FinancialPresentationCode;label:string;category:string;amountMinor:number}>;income:Array<{code:FinancialPresentationCode;label:string;category:string;amountMinor:number}>;assetsMinor:number;liabilitiesMinor:number;equityMinor:number;profitMinor:number;equity:{openingMinor:number;ownerMovementsMinor:number;profitMinor:number;closingMinor:number;components:Array<{accountId:string;code:string;name:string;openingMinor:number;ownerMovementsMinor:number;transferMinor:number;closingMinor:number}>;unclosed:{openingMinor:number;profitMinor:number;transferMinor:number;closingMinor:number}}};
export type StatementSnapshot={standard:string;certified:false;company:string;chart:Chart;profile:Profile;current:StatementSet;comparative:StatementSet;notes:z.infer<typeof statementPackageSchema>["notes"];sourceFingerprint:string;sourceRevision:number;standardsSource:{url:string;effectiveFrom:string;verifiedOn:string}};

function companyScope(actor:WorkContext){if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_FINANCE_REQUIRED","Use a Company Finance assignment for statements and fiscal closing.");}
async function chartAndProfile(db:SqlDatabase,actor:WorkContext,chartId:string,startDate:string){
 const chart=await db.prepare("SELECT c.id,c.name,c.base_currency,c.status,n.display_name company_name FROM finance_charts c JOIN organization_nodes n ON n.id=c.company_id WHERE c.id=? AND c.company_id=?").bind(chartId,actor.companyId).first<Chart>();
 if(!chart)throw new ApiError(404,"CHART_NOT_FOUND","Company reporting ledger not found.");
 const profile=await db.prepare("SELECT * FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND standard='TFRS for NPAEs' AND effective_date<=?").bind(actor.companyId,chartId,startDate).first<Profile>();
 if(!profile)throw new ApiError(409,"NPAES_PROFILE_REQUIRED","Review the Company's statutory ledger and accounting policies before preparing NPAEs statements.");
 return {chart,profile};
}
async function revision(db:SqlDatabase,chartId:string){return Number(await db.prepare("SELECT revision FROM finance_reporting_revisions WHERE chart_id=?").bind(chartId).first("revision")??0);}
function sourceGuards(db:SqlDatabase,actor:WorkContext,key:string,chartId:string,profileId:string,profileVersion:number,sourceRevision:number):SqlStatement[]{return [
 db.prepare("SELECT chart_id FROM finance_reporting_revisions WHERE chart_id=? FOR UPDATE").bind(chartId),
 assertion(db,key,"reporting-revision","EXISTS(SELECT 1 FROM finance_reporting_revisions WHERE chart_id=? AND revision=?)",[chartId,sourceRevision]),
 assertion(db,key,"reporting-profile","EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE id=? AND company_id=? AND statutory_chart_id=? AND status='approved' AND version=?)",[profileId,actor.companyId,chartId,profileVersion]),
];}
async function totals(db:SqlDatabase,actor:WorkContext,chartId:string,startDate:string,endDate:string){
 return (await db.prepare(`WITH dates AS (SELECT ?::text starts,?::text ends)
  SELECT a.id,a.code,a.name,a.category,a.currency,a.status,
   COALESCE(SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE j.entry_date<=d.ends),0) balance_minor,
   COALESCE(SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE j.entry_date<d.starts),0) opening_minor,
   COALESCE(SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE j.entry_date BETWEEN d.starts AND d.ends AND j.source_type<>'fiscal_close'),0) income_minor,
   COALESCE(SUM(l.base_credit_minor-l.base_debit_minor) FILTER(WHERE j.entry_date BETWEEN d.starts AND d.ends AND j.source_type<>'fiscal_close' AND a.category='equity'),0) owner_movements_minor,
   COALESCE(SUM(l.base_credit_minor-l.base_debit_minor) FILTER(WHERE j.entry_date BETWEEN d.starts AND d.ends AND j.source_type='fiscal_close' AND a.category='equity'),0) closing_transfer_minor
  FROM finance_accounts a CROSS JOIN dates d LEFT JOIN finance_journals j ON j.company_id=a.company_id AND j.chart_id=a.chart_id AND j.status IN ('posted','reversed') AND j.entry_date<=d.ends
  LEFT JOIN finance_journal_lines l ON l.journal_id=j.id AND l.account_id=a.id
  WHERE a.company_id=? AND a.chart_id=? GROUP BY a.id,a.code,a.name,a.category,a.currency,a.status ORDER BY a.code,a.id`)
  .bind(startDate,endDate,actor.companyId,chartId).all<Account>()).results;
}
function amount(value:number){if(!Number.isSafeInteger(value))throw new ApiError(422,"MONEY_OVERFLOW","A financial statement exceeds supported integer precision.");return value;}
function statement(accounts:Account[],presentation:Presentation,start:string,end:string):StatementSet {
 const byAccount=new Map(presentation.map(row=>[row.accountId,row.line])),incomeCategories=new Set(["revenue","other_income","expense","other_expense"]);
 const balances=new Map<FinancialPresentationCode,number>(),income=new Map<FinancialPresentationCode,number>();
 const equityComponents:StatementSet["equity"]["components"]=[],profit=accounts.filter(account=>incomeCategories.has(account.category)).reduce((sum,account)=>sum-Number(account.income_minor),0);
 const unclosed=accounts.filter(account=>incomeCategories.has(account.category)).reduce((sum,account)=>sum-Number(account.balance_minor),0),openingUnclosed=accounts.filter(account=>incomeCategories.has(account.category)).reduce((sum,account)=>sum-Number(account.opening_minor),0);
 for(const account of accounts){const code=byAccount.get(account.id),nonzero=[account.balance_minor,account.opening_minor,account.income_minor].some(value=>Number(value)!==0);
  if(nonzero&&!code)throw new ApiError(422,"STATEMENT_PRESENTATION_REQUIRED",`Classify ${account.code} · ${account.name} for the current and comparative statements.`);
  if(!code)continue;const definition=financialPresentation[code];
  if(definition.category!==account.category)throw new ApiError(422,"STATEMENT_PRESENTATION_INVALID",`The reporting line for ${account.code} does not match its immutable account category.`);
  const positive=["asset","expense","other_expense"].includes(account.category)?1:-1;
  if(definition.statement==="position")balances.set(code,amount((balances.get(code)??0)+Number(account.balance_minor)*positive));
  else income.set(code,amount((income.get(code)??0)+Number(account.income_minor)*positive));
  if(account.category==="equity")equityComponents.push({accountId:account.id,code:account.code,name:account.name,openingMinor:-Number(account.opening_minor),ownerMovementsMinor:Number(account.owner_movements_minor),transferMinor:Number(account.closing_transfer_minor),closingMinor:-Number(account.balance_minor)});
 }
 balances.set("retained_earnings",amount((balances.get("retained_earnings")??0)+unclosed));
 const position=[...balances].map(([code,amountMinor])=>({code,label:financialPresentation[code].label,category:financialPresentation[code].category,amountMinor}));
 const incomeRows=[...income].map(([code,amountMinor])=>({code,label:financialPresentation[code].label,category:financialPresentation[code].category,amountMinor}));
 const category=(name:string)=>position.filter(row=>row.category===name).reduce((sum,row)=>sum+row.amountMinor,0),assets=amount(category("asset")),liabilities=amount(category("liability")),equity=amount(category("equity"));
 if(assets!==liabilities+equity)throw new ApiError(409,"STATEMENT_LEDGER_UNBALANCED","Financial position does not reconcile to the posted ledger.");
 const openingEquity=amount(equityComponents.reduce((sum,row)=>sum+row.openingMinor,openingUnclosed)),ownerMovements=amount(equityComponents.reduce((sum,row)=>sum+row.ownerMovementsMinor,0)),transfer=amount(equityComponents.reduce((sum,row)=>sum+row.transferMinor,0));
 if(openingEquity+ownerMovements+profit!==equity||openingUnclosed+profit-transfer!==unclosed)throw new ApiError(409,"STATEMENT_EQUITY_UNRECONCILED","Owner movements and current result do not reconcile to equity.");
 return {dates:{start,end},position,income:incomeRows,assetsMinor:assets,liabilitiesMinor:liabilities,equityMinor:equity,profitMinor:amount(profit),equity:{openingMinor:openingEquity,ownerMovementsMinor:ownerMovements,profitMinor:amount(profit),closingMinor:equity,components:equityComponents,unclosed:{openingMinor:openingUnclosed,profitMinor:amount(profit),transferMinor:-transfer,closingMinor:unclosed}}};
}
async function sourceEvidence(db:SqlDatabase,actor:WorkContext,chartId:string,endDate:string){return (await db.prepare("SELECT j.id,j.version,j.entry_date,j.source_type,j.source_id,j.currency,j.fx_rate_micros,l.id line_id,l.account_id,l.base_debit_minor,l.base_credit_minor,l.ordinal FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=? ORDER BY j.id,l.ordinal,l.id").bind(actor.companyId,chartId,endDate).all()).results;}
export async function reportingWorkspace(actor:WorkContext,chartId:string){
 actor=requirePermission(actor,"finance.read");companyScope(actor);const {write}=await holdingResources(actor.holdingId);
 if(!await write.prepare("SELECT id FROM finance_charts WHERE id=? AND company_id=?").bind(chartId,actor.companyId).first())throw new ApiError(404,"CHART_NOT_FOUND","Company ledger not found.");
 const [packages,closings,accounts]=await Promise.all([
  write.prepare("SELECT id,start_date,end_date,comparative_start_date,comparative_end_date,edition,previous_package_id,status,version,prepared_by,reviewed_by,source_fingerprint,created_at FROM finance_statement_packages WHERE company_id=? AND chart_id=? ORDER BY end_date DESC,edition DESC").bind(actor.companyId,chartId).all<Package>(),
  write.prepare("SELECT * FROM finance_year_closings WHERE company_id=? AND chart_id=? ORDER BY end_date DESC,created_at DESC").bind(actor.companyId,chartId).all<Package>(),
  write.prepare("SELECT id,code,name,category,status FROM finance_accounts WHERE company_id=? AND chart_id=? ORDER BY code,id").bind(actor.companyId,chartId).all<Package>(),
 ]);return {packages:packages.results,closings:closings.results,accounts:accounts.results};
}
export async function statementPackageDetail(actor:WorkContext,id:string){actor=requirePermission(actor,"finance.read");companyScope(actor);const {write}=await holdingResources(actor.holdingId),row=await write.prepare("SELECT * FROM finance_statement_packages WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Package>();if(!row)throw new ApiError(404,"STATEMENT_PACKAGE_NOT_FOUND","Statement package not found.");const history=(await write.prepare("SELECT * FROM finance_statement_events WHERE package_id=? ORDER BY package_version").bind(id).all()).results;return {row,snapshot:JSON.parse(String(row.snapshot_json)) as StatementSnapshot,history};}
export async function prepareStatementPackage(actor:WorkContext,input:z.infer<typeof statementPackageSchema>,key:string){
 actor=requirePermission(actor,"finance.write");companyScope(actor);input=statementPackageSchema.parse(input);const {write}=await holdingResources(actor.holdingId),id=`statement-package-${key}`,op="finance.statement.prepare",prior=await replayCommand(write,actor,key,op,id,input);if(prior)return prior;
 if(input.startDate>input.endDate||input.comparativeStartDate>input.comparativeEndDate||input.comparativeEndDate>=input.startDate)throw new ApiError(422,"STATEMENT_DATES_INVALID","Current and preceding comparative dates must be ordered and separate.");
 if(new Set(input.presentation.map(row=>row.accountId)).size!==input.presentation.length)throw new ApiError(422,"STATEMENT_PRESENTATION_DUPLICATE","Classify each ledger account once.");
 const {chart,profile}=await chartAndProfile(write,actor,input.chartId,input.startDate),sourceRevision=await revision(write,input.chartId);
 const [current,comparative,evidence]=await Promise.all([totals(write,actor,input.chartId,input.startDate,input.endDate),totals(write,actor,input.chartId,input.comparativeStartDate,input.comparativeEndDate),sourceEvidence(write,actor,input.chartId,input.endDate)]);
 const allowed=new Set(current.map(account=>account.id));if(input.presentation.some(row=>!allowed.has(row.accountId)))throw new ApiError(422,"STATEMENT_PRESENTATION_INVALID","Every classified account must belong to this Company ledger.");
 const sourceFingerprint=digest({chart,profile,accounts:current,evidence}),snapshot:StatementSnapshot={standard:"TFRS for NPAEs (revised 2022)",certified:false,company:chart.company_name,chart,profile,current:statement(current,input.presentation,input.startDate,input.endDate),comparative:statement(comparative,input.presentation,input.comparativeStartDate,input.comparativeEndDate),notes:input.notes,sourceFingerprint,sourceRevision,standardsSource:{url:"https://acpro-std.tfac.or.th/standard/2/-NPAEs",effectiveFrom:"2023-01-01",verifiedOn:"2026-10-03"}};
 if(await revision(write,input.chartId)!==sourceRevision)throw new ApiError(409,"REPORTING_SOURCE_CHANGED","Ledger data changed while preparing the statement. Refresh and prepare again.");
 const previous=input.previousPackageId?await write.prepare("SELECT id FROM finance_statement_packages WHERE id=? AND company_id=? AND chart_id=? AND start_date=? AND end_date=? AND status='issued'").bind(input.previousPackageId,actor.companyId,input.chartId,input.startDate,input.endDate).first():null;
 if(input.previousPackageId&&!previous)throw new ApiError(409,"STATEMENT_PREVIOUS_INVALID","An amended edition must reference the issued statement for the same Company ledger and dates.");
 const edition=Number(await write.prepare("SELECT COALESCE(MAX(edition),0)+1 edition FROM finance_statement_packages WHERE chart_id=? AND start_date=? AND end_date=?").bind(input.chartId,input.startDate,input.endDate).first("edition")),now=Date.now();
 return executeCommand(write,actor,key,op,input,id,1,{id,status:"draft",version:1,edition,sourceFingerprint},[
  ...sourceGuards(write,actor,key,input.chartId,profile.id,profile.version,sourceRevision),
  assertion(write,key,"statement-edition","NOT EXISTS(SELECT 1 FROM finance_statement_packages WHERE chart_id=? AND start_date=? AND end_date=? AND edition=?)",[input.chartId,input.startDate,input.endDate,edition]),
  write.prepare("INSERT INTO finance_statement_packages(id,company_id,chart_id,profile_id,profile_version,start_date,end_date,comparative_start_date,comparative_end_date,previous_package_id,edition,source_revision,source_fingerprint,presentation_json,notes_json,snapshot_json,reason,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,input.chartId,profile.id,profile.version,input.startDate,input.endDate,input.comparativeStartDate,input.comparativeEndDate,input.previousPackageId??null,edition,sourceRevision,sourceFingerprint,JSON.stringify(input.presentation),JSON.stringify(input.notes),JSON.stringify(snapshot),input.reason,actor.userId,now,now),
  write.prepare("INSERT INTO finance_statement_events VALUES(?,?,?,'prepared',?,1,?,?,?)").bind(`statement-event-${key}`,id,actor.companyId,actor.userId,input.reason,key,now),
 ]);
}
const escape=(value:unknown)=>String(value??"").replaceAll("&","&amp;").replaceAll("<","&lt;").replaceAll(">","&gt;").replaceAll('"',"&quot;").replaceAll("'","&#39;");
function statementHtml(snapshot:StatementSnapshot,edition:number){
 const current=snapshot.current,comparison=snapshot.comparative,number=(value:number)=>minorToDecimal(value),table=(kind:"position"|"income")=>{
  const a=new Map(current[kind].map(row=>[row.code,row])),b=new Map(comparison[kind].map(row=>[row.code,row]));
  const rows=codes.filter(code=>a.has(code)||b.has(code)).map(code=>`<tr><td>${escape(financialPresentation[code].label)}</td><td>${number(a.get(code)?.amountMinor??0)}</td><td>${number(b.get(code)?.amountMinor??0)}</td></tr>`).join("");
  return `<table><thead><tr><th>Account presentation</th><th>${escape(current.dates.end)}</th><th>${escape(comparison.dates.end)}</th></tr></thead><tbody>${rows}</tbody></table>`;
 };
 const equity=(set:StatementSet)=>`<tr><td>${escape(set.dates.start)} to ${escape(set.dates.end)}</td><td>${number(set.equity.openingMinor)}</td><td>${number(set.equity.ownerMovementsMinor)}</td><td>${number(set.profitMinor)}</td><td>${number(set.equityMinor)}</td></tr>`;
 return `<!doctype html><html lang="en"><meta charset="utf-8"><title>${escape(snapshot.company)} · Financial statements edition ${edition}</title><style>body{font:15px/1.6 system-ui,sans-serif;color:#17202a;max-width:1000px;margin:48px auto;padding:0 24px}h1,h2,h3{line-height:1.2}table{width:100%;border-collapse:collapse;margin:20px 0}th,td{padding:10px;border-bottom:1px solid #cbd5e1;text-align:right}th:first-child,td:first-child{text-align:left}small{color:#475569}p{white-space:pre-wrap}section{break-inside:avoid}a{color:inherit}@media print{body{margin:0;font-size:11pt}h2{break-after:avoid}}</style><body><h1>${escape(snapshot.company)}</h1><h2>Financial statements · Edition ${edition}</h2><p>${escape(current.dates.start)} to ${escape(current.dates.end)} · Comparative ${escape(comparison.dates.start)} to ${escape(comparison.dates.end)}<br>Ledger: ${escape(snapshot.chart.name)} · Functional currency: ${escape(snapshot.chart.base_currency)}</p><small>Prepared under the Company's approved ${escape(snapshot.standard)} policies. Independent internal approval; certification: not claimed.</small><section><h2>Statement of financial position</h2>${table("position")}<p>Assets ${number(current.assetsMinor)} · Liabilities ${number(current.liabilitiesMinor)} · Equity ${number(current.equityMinor)}</p></section><section><h2>Income statement</h2>${table("income")}<p>Current profit or loss ${number(current.profitMinor)} · Comparative ${number(comparison.profitMinor)}</p></section><section><h2>Statement of changes in equity</h2><table><thead><tr><th>Reporting period</th><th>Opening equity</th><th>Owner movements</th><th>Profit or loss</th><th>Closing equity</th></tr></thead><tbody>${equity(current)}${equity(comparison)}</tbody></table>${[current,comparison].map(set=>`<h3>${escape(set.dates.end)} · Equity components</h3><table><thead><tr><th>Component</th><th>Opening</th><th>Owner movements</th><th>Closing transfers</th><th>Closing</th></tr></thead><tbody>${set.equity.components.map(row=>`<tr><td>${escape(row.code)} · ${escape(row.name)}</td><td>${number(row.openingMinor)}</td><td>${number(row.ownerMovementsMinor)}</td><td>${number(row.transferMinor)}</td><td>${number(row.closingMinor)}</td></tr>`).join("")}<tr><td>Unclosed result</td><td>${number(set.equity.unclosed.openingMinor)}</td><td>${number(set.equity.unclosed.profitMinor)}</td><td>${number(set.equity.unclosed.transferMinor)}</td><td>${number(set.equity.unclosed.closingMinor)}</td></tr></tbody></table>`).join("")}</section><section><h2>Notes to the financial statements</h2>${Object.entries(financialNoteTitles).map(([key,title],index)=>`<h3>${index+1}. ${escape(title)}</h3><p>${escape(snapshot.notes[key as keyof typeof snapshot.notes])}</p>`).join("")}</section><small>Source fingerprint: ${escape(snapshot.sourceFingerprint)}<br>Approved policy revision: ${escape(snapshot.profile.id)} / ${snapshot.profile.version}<br>Presentation basis: <a href="${escape(snapshot.standardsSource.url)}">TFAC NPAEs</a> · effective ${snapshot.standardsSource.effectiveFrom}</small></body></html>`;
}
export async function decideStatementPackage(actor:WorkContext,id:string,action:"issue"|"reject"|"cancel",input:z.infer<typeof reportingDecisionSchema>,key:string){
 actor=requirePermission(actor,action==="cancel"?"finance.write":"finance.approve");companyScope(actor);input=reportingDecisionSchema.parse(input);const {write}=await holdingResources(actor.holdingId),op=`finance.statement.${action}`,prior=await replayCommand(write,actor,key,op,id,input);if(prior)return prior;
 const row=await write.prepare("SELECT * FROM finance_statement_packages WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Package>();if(!row)throw new ApiError(404,"STATEMENT_PACKAGE_NOT_FOUND","Statement package not found.");
 if(row.status!=="draft"||Number(row.version)!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Refresh the prepared statement before reviewing it.");
 if(action!=="cancel"&&samePerson(actor,row.prepared_by))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","A different Finance approver must issue the statement package.");
 if(action==="cancel"&&!samePerson(actor,row.prepared_by))requirePermission(actor,"finance.approve");
 if(action==="issue"&&await revision(write,String(row.chart_id))!==Number(row.source_revision))throw new ApiError(409,"REPORTING_SOURCE_CHANGED","Ledger data changed after preparation. Retain this draft and prepare a new edition.");
 const snapshot=JSON.parse(String(row.snapshot_json)) as StatementSnapshot,html=action==="issue"?statementHtml(snapshot,Number(row.edition)):null,sha256=html?createHash("sha256").update(html,"utf8").digest("hex"):null,now=Date.now(),status=action==="issue"?"issued":action==="reject"?"rejected":"cancelled",next=input.expectedVersion+1;
 return executeCommand(write,actor,key,op,input,id,next,{id,status,version:next,edition:Number(row.edition),sha256},[
  ...(action==="issue"?sourceGuards(write,actor,key,String(row.chart_id),String(row.profile_id),Number(row.profile_version),Number(row.source_revision)):[]),
  assertion(write,key,"statement-version","EXISTS(SELECT 1 FROM finance_statement_packages WHERE id=? AND company_id=? AND status='draft' AND version=?)",[id,actor.companyId,input.expectedVersion]),
  write.prepare("UPDATE finance_statement_packages SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,export_html=?,export_sha256=?,version=version+1,updated_at=? WHERE id=?").bind(status,actor.userId,input.reason,now,html,sha256,now,id),
  write.prepare("INSERT INTO finance_statement_events VALUES(?,?,?,?,?,?,?,?,?)").bind(`statement-event-${key}`,id,actor.companyId,status,actor.userId,next,input.reason,key,now),
 ]);
}
export async function statementPackageExport(actor:WorkContext,id:string){const detail=await statementPackageDetail(actor,id);if(detail.row.status!=="issued"||!detail.row.export_html||!detail.row.export_sha256)throw new ApiError(409,"STATEMENT_NOT_ISSUED","An independent Finance approver must issue this edition before export.");return {content:String(detail.row.export_html),sha256:String(detail.row.export_sha256),edition:Number(detail.row.edition)};}

function fiscalDates(start:string,fiscalStartMonth:number){const year=Number(start.slice(0,4)),expectedStart=`${year}-${String(fiscalStartMonth).padStart(2,"0")}-01`,end=new Date(Date.UTC(year+1,fiscalStartMonth-1,0)).toISOString().slice(0,10);return {start:expectedStart,end};}
export async function prepareYearClosing(actor:WorkContext,input:z.infer<typeof yearClosingSchema>,key:string){
 actor=requirePermission(actor,"finance.write");companyScope(actor);input=yearClosingSchema.parse(input);const {write}=await holdingResources(actor.holdingId),id=`year-closing-${key}`,op="finance.year-close.prepare",prior=await replayCommand(write,actor,key,op,id,input);if(prior)return prior;
 const {chart,profile}=await chartAndProfile(write,actor,input.chartId,input.startDate),dates=fiscalDates(input.startDate,profile.fiscal_start_month);
 if(dates.start!==input.startDate||dates.end!==input.endDate)throw new ApiError(422,"FISCAL_DATES_INVALID","Close one full fiscal year using the approved Company fiscal start month.");
 if(await write.prepare("SELECT id FROM finance_year_closings WHERE company_id=? AND chart_id=? AND start_date=? AND end_date=? AND status IN ('draft','posted')").bind(actor.companyId,input.chartId,input.startDate,input.endDate).first())throw new ApiError(409,"FISCAL_CLOSING_EXISTS","Review the current fiscal closing. A posted year retains its original transfer and subsequent adjustments use an open period.");
 const period=await write.prepare("SELECT id,start_date,end_date,state FROM finance_periods WHERE id=? AND company_id=?").bind(input.periodId,actor.companyId).first<{id:string;start_date:string;end_date:string;state:string}>();
 if(!period||period.state!=="open"||period.start_date>input.endDate||period.end_date!==input.endDate)throw new ApiError(409,"FISCAL_PERIOD_NOT_READY","Use the open final accounting period ending on the fiscal year-end date.");
 const equity=await write.prepare("SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND category='equity' AND status='active' AND currency=?").bind(input.retainedEarningsAccountId,actor.companyId,input.chartId,chart.base_currency).first();if(!equity)throw new ApiError(422,"RETAINED_EARNINGS_ACCOUNT_REQUIRED","Select an active retained earnings account in this ledger's functional currency.");
 const sourceRevision=await revision(write,input.chartId),accounts=await totals(write,actor,input.chartId,input.startDate,input.endDate),income=accounts.filter(account=>["revenue","other_income","expense","other_expense"].includes(account.category));
 if(income.some(account=>Number(account.opening_minor)!==0))throw new ApiError(409,"PREVIOUS_FISCAL_RESULT_OPEN","Close or reconcile the preceding fiscal result before transferring the current year.");
 const evidence=await sourceEvidence(write,actor,input.chartId,input.endDate),sourceFingerprint=digest({chart,profile,accounts,evidence}),profit=amount(income.reduce((sum,account)=>sum-Number(account.balance_minor),0)),now=Date.now(),snapshot={chart,profile,accounts:income,profitMinor:profit,sourceFingerprint};
 if(await revision(write,input.chartId)!==sourceRevision)throw new ApiError(409,"REPORTING_SOURCE_CHANGED","Ledger data changed while preparing fiscal closing.");
 return executeCommand(write,actor,key,op,input,id,1,{id,status:"draft",version:1,profitMinor:profit,sourceFingerprint},[
  ...sourceGuards(write,actor,key,input.chartId,profile.id,profile.version,sourceRevision),
  assertion(write,key,"earlier-periods","NOT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND id<>? AND start_date<=? AND end_date>=? AND state<>'closed')",[actor.companyId,input.periodId,input.endDate,input.startDate]),
  assertion(write,key,"year-drafts","NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND chart_id=? AND entry_date BETWEEN ? AND ? AND status IN ('draft','in_review'))",[actor.companyId,input.chartId,input.startDate,input.endDate]),
  write.prepare("INSERT INTO finance_year_closings(id,company_id,chart_id,profile_id,profile_version,start_date,end_date,period_id,retained_earnings_account_id,source_revision,source_fingerprint,snapshot_json,profit_minor,reason,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,input.chartId,profile.id,profile.version,input.startDate,input.endDate,input.periodId,input.retainedEarningsAccountId,sourceRevision,sourceFingerprint,JSON.stringify(snapshot),profit,input.reason,actor.userId,now,now),
  write.prepare("INSERT INTO finance_year_closing_events VALUES(?,?,?,'prepared',?,1,?,?,?)").bind(`year-closing-event-${key}`,id,actor.companyId,actor.userId,input.reason,key,now),
 ]);
}
export async function decideYearClosing(actor:WorkContext,id:string,action:"post"|"reject"|"cancel",input:z.infer<typeof reportingDecisionSchema>,key:string){
 actor=requirePermission(actor,action==="cancel"?"finance.write":"finance.approve");companyScope(actor);input=reportingDecisionSchema.parse(input);const {write}=await holdingResources(actor.holdingId),op=`finance.year-close.${action}`,prior=await replayCommand(write,actor,key,op,id,input);if(prior)return prior;
 const row=await write.prepare("SELECT * FROM finance_year_closings WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Package>();if(!row)throw new ApiError(404,"YEAR_CLOSING_NOT_FOUND","Fiscal closing not found.");
 if(row.status!=="draft"||Number(row.version)!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Refresh the fiscal closing before review.");
 if(action!=="cancel"&&samePerson(actor,row.prepared_by))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","A different Finance approver must post fiscal closing.");
 if(action==="cancel"&&!samePerson(actor,row.prepared_by))requirePermission(actor,"finance.approve");
 if(action==="post"&&await revision(write,String(row.chart_id))!==Number(row.source_revision))throw new ApiError(409,"REPORTING_SOURCE_CHANGED","Ledger data changed after fiscal closing preparation.");
 const snapshot=JSON.parse(String(row.snapshot_json)) as {chart:Chart;accounts:Account[];profitMinor:number},lines:JournalLineInput[]=snapshot.accounts.filter(account=>Number(account.balance_minor)!==0).map(account=>({accountId:account.id,debit:account.balance_minor<0?minorToDecimal(-Number(account.balance_minor)):"0",credit:account.balance_minor>0?minorToDecimal(Number(account.balance_minor)):"0",description:"Transfer fiscal result to retained earnings"}));
 if(snapshot.profitMinor!==0)lines.push({accountId:String(row.retained_earnings_account_id),debit:snapshot.profitMinor<0?minorToDecimal(-snapshot.profitMinor):"0",credit:snapshot.profitMinor>0?minorToDecimal(snapshot.profitMinor):"0"});
 const journal=action==="post"&&lines.length?await prepareJournal(write,actor,{chartId:String(row.chart_id),periodId:String(row.period_id),entryDate:String(row.end_date),currency:snapshot.chart.base_currency,memo:`Fiscal closing ${row.start_date} to ${row.end_date}`,sourceType:"fiscal_close",sourceId:id,isOpening:false,lines},key,"posted"):null;
 const money=journal?await snapshotMoney(write,actor,{sourceType:"finance_year_closing",sourceId:id,amountKind:"profit_transfer",transactionDate:String(row.end_date),currency:snapshot.chart.base_currency,currencyScale:2,amountUnits:String(snapshot.profitMinor)}):null;
 const now=Date.now(),status=action==="post"?"posted":action==="reject"?"rejected":"cancelled",next=input.expectedVersion+1;
 return executeCommand(write,actor,key,op,input,id,next,{id,status,version:next,profitMinor:Number(row.profit_minor),journalId:journal?.result.id??null},[
  ...(action==="post"?sourceGuards(write,actor,key,String(row.chart_id),String(row.profile_id),Number(row.profile_version),Number(row.source_revision)):[]),
  assertion(write,key,"closing-version","EXISTS(SELECT 1 FROM finance_year_closings WHERE id=? AND company_id=? AND status='draft' AND version=?)",[id,actor.companyId,input.expectedVersion]),
  ...(action==="post"?[assertion(write,key,"closing-period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND end_date=?)",[row.period_id,actor.companyId,row.end_date]),assertion(write,key,"earlier-periods","NOT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND id<>? AND start_date<=? AND end_date>=? AND state<>'closed')",[actor.companyId,row.period_id,row.end_date,row.start_date])]:[]),
  ...(journal?.steps??[]),...(money?[money.statement]:[]),
  write.prepare("UPDATE finance_year_closings SET status=?,journal_id=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?").bind(status,journal?.result.id??null,actor.userId,input.reason,now,now,id),
  write.prepare("INSERT INTO finance_year_closing_events VALUES(?,?,?,?,?,?,?,?,?)").bind(`year-closing-event-${key}`,id,actor.companyId,status,actor.userId,next,input.reason,key,now),
 ]);
}
