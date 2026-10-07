import { seedLocalFinanceJournal } from "../scripts/fixtures/local-seed-journals";
import type { FinanceRow } from "../src/server/modules/finance/public/finance";
import {beforeAll,beforeEach,describe,expect,it,vi} from "vitest";
import {createHash} from "node:crypto";
import {readFileSync} from "node:fs";
import {mkdir,writeFile} from "node:fs/promises";
import {join} from "node:path";
import {applyMigrations,env} from "./runtime-env";
import {expenseDb as db,expenseFixture,key} from "./helpers/expense-fixture";
import {createAccount,createJournal,createJournalSchema,createPeriod,createParty,createInvoice,invoiceSchema,partySchema,postInvoice,beginClose,calculateCloseReconciliations,closePeriod,postJournalBatch,reopenPeriod,statements,trialBalance} from "../src/server/finance";
import {prepareAccountingProfile,decideAccountingProfile} from "../src/server/accounting-profiles";
import {assignFlowPrimary,resolveFlow} from "../src/server/flow-routing";
import {decideStatementPackage,decideYearClosing,prepareStatementPackage,prepareYearClosing,reportingWorkspace,statementPackageDetail,statementPackageExport,statementSourceAccounts} from "../src/server/finance-reporting";
import {confirmManualRate} from "../src/server/company-money";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,capital:string,retained:string,receivable:string,payable:string;
const notes={basis:"Prepared on the accrual basis under the approved Company TFRS for NPAEs policy.",policies:"Historical cost, moving-average inventory, straight-line depreciation and current income tax follow approved policy evidence.",going_concern:"Directors reviewed liquidity and expect operations to continue; no material uncertainty is identified in this synthetic Company.",judgments:"Management reviewed recoverability, useful lives and the estimates relevant to these synthetic transactions.",disclosures:"Synthetic Company has no material contingent liabilities or contractual commitments and no adjusting subsequent events."};
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
 f=await expenseFixture();for(const [actor,role] of [[f.reviewer,"finance_preparer"],[f.manager,"finance_approver"]] as const){actor.role=role;actor.branchId=null;actor.departmentId=null;actor.assignmentId=await f.grant(actor,role);}
 capital=String((await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:"Paid-in capital",category:"equity",isControl:false,controlKind:"none"},key())).id);
 retained=String((await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:"Retained earnings",category:"equity",isControl:false,controlKind:"none"},key())).id);
 receivable=String((await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:"Trade receivables",category:"asset",isControl:true,controlKind:"receivable"},key())).id);
 payable=String((await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:"Trade payables",category:"liability",isControl:true,controlKind:"payable"},key())).id);
 const profile=await prepareAccountingProfile(f.reviewer,{statutoryChartId:f.chart,standard:"TFRS for NPAEs",fiscalStartMonth:1,effectiveDate:"2025-01-01",eligibilityEvidence:"Independent review confirms this synthetic Company satisfies the NPAEs eligibility criteria.",policyEvidence:"Approved accrual, historical cost, moving-average inventory, straight-line depreciation and current tax policies.",taxBasis:"current_tax",inventoryPolicy:"moving_average",depreciationPolicy:"straight_line"},key());
 await decideAccountingProfile(f.manager,profile.id,"approve",{expectedVersion:1,reason:"Reviewed statutory ledger selection, eligibility and Company accounting policy evidence."},key());
});
async function post(periodId:string,date:string,debitAccount:string,creditAccount:string,value:string){const prepared=await createJournal(f.reviewer,createJournalSchema.parse({chartId:f.chart,periodId,entryDate:date,currency:"THB",memo:"Reviewed synthetic financial reporting source transaction",lines:[{accountId:debitAccount,debit:value,credit:"0"},{accountId:creditAccount,debit:"0",credit:value}]}),key()),flow=await resolveFlow(db,f.owner,"journal",prepared.id);
 await assignFlowPrimary(db,f.owner,"journal",prepared.id,{stepKey:"draft",userId:f.manager.userId,reason:"Independent Finance reviewer for source transaction",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 await postJournalBatch(f.manager,{journals:[{id:prepared.id,expectedVersion:1}],reason:"Checked actual source transaction and the balanced functional-currency journal."},key());return prepared.id;}
async function close(periodId:string){const started=await beginClose(f.reviewer,periodId,key());const command=key(),input={expectedVersion:Number(started.version),proof:"Independent review of ledger, subledger and bank readiness evidence for this accounting period."},closed=await closePeriod(f.manager,periodId,input,command);expect(await closePeriod(f.manager,periodId,input,command)).toEqual(closed);return closed;}
function input(previousPackageId?:string){return {chartId:f.chart,startDate:"2026-01-01",endDate:"2026-12-31",comparativeStartDate:"2025-01-01",comparativeEndDate:"2025-12-31",...(previousPackageId?{previousPackageId}:{}),presentation:[{accountId:f.accounts.cash,line:"cash_equivalents" as const},{accountId:receivable,line:"trade_receivables" as const},{accountId:payable,line:"trade_payables" as const},{accountId:f.accounts.revenue,line:"sales_revenue" as const},{accountId:f.accounts.expense,line:"administrative_expenses" as const},{accountId:capital,line:"paid_in_capital" as const},{accountId:retained,line:"retained_earnings" as const}],notes,reason:"Prepared complete statutory statements from dated posted source records and approved Company policies."};}
async function sourceInvoice(kind:"receivable"|"payable",periodId:string,date:string,value:string){const party=await createParty(f.reviewer,partySchema.parse({kind:kind==="receivable"?"customer":"supplier",code:key(),name:`Synthetic reporting ${kind}`,currency:"THB"}),key()),invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:String(party.id),kind,invoiceNumber:key(),invoiceDate:date,dueDate:date,currency:"THB",subtotal:value,tax:"0",debitAccountId:kind==="receivable"?receivable:f.accounts.expense,creditAccountId:kind==="receivable"?f.accounts.revenue:payable}),key()),flow=await resolveFlow(db,f.owner,"invoice",invoice.id);await assignFlowPrimary(db,f.owner,"invoice",invoice.id,{stepKey:"draft",userId:f.manager.userId,reason:"Independent source invoice reviewer",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,invoice.id,{expectedVersion:1,periodId,entryDate:date},key());return invoice.id;}
const review={expectedVersion:1,reason:"Independently reconciled the source ledger, current and comparative result, owner movements and note disclosures."};

describe("NPAEs statement and fiscal closing lifecycle",()=>{
 it("closes consecutive months and a reviewed fiscal year while retaining complete comparative statement editions",async()=>{
  const comparative=await createPeriod(f.reviewer,{name:"Comparative fiscal 2025",startDate:"2025-01-01",endDate:"2025-12-31"},key());
  // Dated synthetic quote belongs to the shared Money fixture; no provider certification is implied.
  await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2025-01-01",currencyPerUsd:"36",note:"Synthetic comparative reporting fixture quote; not a market observation or provider result."},key());
  const sourceJournals=[await post(String(comparative.id),"2025-01-02",f.accounts.cash,capital,"500"),await post(String(comparative.id),"2025-08-31",f.accounts.cash,f.accounts.revenue,"100"),await post(String(comparative.id),"2025-09-30",f.accounts.expense,f.accounts.cash,"40")];
  const priorClosing=await prepareYearClosing(f.reviewer,{chartId:f.chart,startDate:"2025-01-01",endDate:"2025-12-31",periodId:String(comparative.id),retainedEarningsAccountId:retained,reason:"Reconciled comparative-year actual income, expenses and owner capital before transferring the result."},key());expect(priorClosing).toMatchObject({profitMinor:6000});
  await decideYearClosing(f.manager,String(priorClosing.id),"post",review,key());await close(String(comparative.id));
  const january=await createPeriod(f.reviewer,{name:"January 2026",startDate:"2026-01-01",endDate:"2026-01-31"},key()),february=await createPeriod(f.reviewer,{name:"February 2026",startDate:"2026-02-01",endDate:"2026-02-28"},key());
  sourceJournals.push(await post(String(january.id),"2026-01-20",f.accounts.cash,f.accounts.revenue,"150"),await post(String(january.id),"2026-01-25",f.accounts.expense,f.accounts.cash,"50"));
  const sourceInvoices=[await sourceInvoice("receivable",String(january.id),"2026-01-26","20"),await sourceInvoice("payable",String(january.id),"2026-01-27","15")];
  const closingInput={chartId:f.chart,startDate:"2026-01-01",endDate:"2026-12-31",periodId:f.period,retainedEarningsAccountId:retained,reason:"Reconciled fiscal 2026 source records, prior result and Company policy evidence before final transfer."};
  await expect(prepareYearClosing(f.reviewer,closingInput,key())).rejects.toMatchObject({code:"CONFLICT"});
  await close(String(january.id));sourceJournals.push(await post(String(february.id),"2026-02-20",retained,f.accounts.cash,"10"));await close(String(february.id));
  expect((await db.prepare("SELECT state FROM finance_periods WHERE id IN (?,?)").bind(january.id,february.id).all()).results).toEqual([{state:"closed"},{state:"closed"}]);
  const prepareKey=key(),prepared=await prepareStatementPackage(f.reviewer,input(),prepareKey);expect(await prepareStatementPackage(f.reviewer,input(),prepareKey)).toEqual(prepared);
  const snapshot=(await statementPackageDetail(f.manager,String(prepared.id))).snapshot;expect(snapshot.current).toMatchObject({assetsMinor:67000,liabilitiesMinor:1500,equityMinor:65500,profitMinor:10500,equity:{openingMinor:56000,ownerMovementsMinor:-1000,profitMinor:10500,closingMinor:65500}});expect(snapshot.comparative).toMatchObject({assetsMinor:56000,equityMinor:56000,profitMinor:6000});expect(snapshot.notes).toEqual(notes);expect(snapshot.certified).toBe(false);
  await expect(decideStatementPackage({...f.reviewer,role:"finance_approver"},String(prepared.id),"issue",review,key())).rejects.toMatchObject({code:"INDEPENDENT_APPROVER_REQUIRED"});
  await expect(statementPackageExport(f.manager,String(prepared.id))).rejects.toMatchObject({code:"STATEMENT_NOT_ISSUED"});
  const issueKey=key(),issued=await decideStatementPackage(f.manager,String(prepared.id),"issue",review,issueKey);expect(await decideStatementPackage(f.manager,String(prepared.id),"issue",review,issueKey)).toEqual(issued);
  const exported=await statementPackageExport(f.manager,String(prepared.id));expect(exported.sha256).toBe(createHash("sha256").update(exported.content).digest("hex"));expect(exported.content).toContain("Statement of changes in equity");expect(exported.content).toContain(notes.disclosures);expect(exported.content).toContain("2025-12-31");
  await expect(db.prepare("UPDATE finance_statement_packages SET export_html='replacement' WHERE id=?").bind(prepared.id).run()).rejects.toThrow("FINANCE_ISSUED_REPORT_IMMUTABLE");
  await expect(db.prepare("UPDATE finance_statement_events SET reason='replacement' WHERE package_id=?").bind(prepared.id).run()).rejects.toThrow("FINANCE_REPORTING_EVIDENCE_IMMUTABLE");
  const supersededDraft=await prepareStatementPackage(f.reviewer,input(String(prepared.id)),key());
  sourceJournals.push(await post(f.period,"2026-12-20",f.accounts.cash,f.accounts.revenue,"5"));
  await expect(decideStatementPackage(f.manager,String(supersededDraft.id),"issue",review,key())).rejects.toMatchObject({code:"REPORTING_SOURCE_CHANGED"});await decideStatementPackage(f.reviewer,String(supersededDraft.id),"cancel",review,key());
  const amended=await prepareStatementPackage(f.reviewer,input(String(prepared.id)),key());expect(amended).toMatchObject({edition:3});expect((await statementPackageDetail(f.manager,String(amended.id))).snapshot.current).toMatchObject({assetsMinor:67500,equityMinor:66000,profitMinor:11000});await decideStatementPackage(f.manager,String(amended.id),"issue",review,key());
  expect(await statementPackageExport(f.manager,String(prepared.id))).toEqual(exported);
  const beforeTransfer=await statements(f.manager,undefined,f.chart,{startDate:"2026-01-01",endDate:"2026-12-31"});expect(beforeTransfer.profitAndLoss.netProfit).toBe("110.00");
  const fiscal=await prepareYearClosing(f.reviewer,closingInput,key());expect(fiscal).toMatchObject({profitMinor:11000});await expect(decideYearClosing({...f.reviewer,role:"finance_approver"},String(fiscal.id),"post",review,key())).rejects.toMatchObject({code:"INDEPENDENT_APPROVER_REQUIRED"});
  const postKey=key(),posted=await decideYearClosing(f.manager,String(fiscal.id),"post",review,postKey);expect(await decideYearClosing(f.manager,String(fiscal.id),"post",review,postKey)).toEqual(posted);
  const journal=await db.prepare("SELECT status,source_type FROM finance_journals WHERE id=?").bind(posted.journalId).first();expect(journal).toEqual({status:"posted",source_type:"fiscal_close"});
  const ledger=await trialBalance(f.manager,undefined,f.chart,{endDate:"2026-12-31"});expect(ledger.filter(row=>[f.accounts.revenue,f.accounts.expense].includes(String(row.id))).map(row=>row.balance)).toEqual(["0.00","0.00"]);expect(ledger.find(row=>row.id===retained)?.balance).toBe("-160.00");expect(ledger.reduce((sum,row)=>sum+Number(row.balance),0)).toBe(0);
  const report=await statements(f.manager,undefined,f.chart,{startDate:"2026-01-01",endDate:"2026-12-31"});expect(report.profitAndLoss).toMatchObject({revenue:"175.00",expenses:"65.00",netProfit:"110.00"});expect(report.balanceSheet).toMatchObject({assets:"675.00",liabilities:"15.00",equity:"660.00",unclosedProfit:"0.00"});
  const finalPeriod=await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first<FinanceRow>(),reconciliation=await calculateCloseReconciliations(db,f.manager,finalPeriod!);expect(reconciliation.ready).toBe(true);expect(reconciliation.rows).toHaveLength(2);expect(reconciliation.rows.map(row=>[row.kind,row.ledgerMinor,row.sourceMinor,row.differenceMinor])).toEqual(expect.arrayContaining([["receivable",2000,2000,0],["payable",1500,1500,0]]));
  const closure=await close(f.period);await expect(prepareYearClosing(f.reviewer,closingInput,key())).rejects.toMatchObject({code:"FISCAL_CLOSING_EXISTS"});
  await expect(reopenPeriod(f.manager,f.period,{expectedVersion:Number(closure.version),proof:"Same closing approver cannot reopen own decision."},key())).rejects.toMatchObject({code:"CLOSE_APPROVAL_SEPARATION"});
  f.tie.role="finance_approver";f.tie.branchId=null;f.tie.departmentId=null;f.tie.assignmentId=await f.grant(f.tie,"finance_approver");const reopened=await reopenPeriod(f.tie,f.period,{expectedVersion:Number(closure.version),proof:"Another Finance approver authorizes reopening for a reviewed readiness inspection."},key());expect(reopened).toMatchObject({state:"open"});
  // Reopening a period never silently undoes the immutable fiscal transfer.
  await expect(post(f.period,"2026-12-25",f.accounts.cash,f.accounts.revenue,"1")).rejects.toBeDefined();expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE company_id=? AND status='draft'").bind(f.company).first("n")).toBe(0);await close(f.period);
  expect(await statementPackageExport(f.manager,String(prepared.id))).toEqual(exported);await expect(statementPackageExport({...f.manager,companyId:"another-company"},String(prepared.id))).rejects.toMatchObject({code:"STATEMENT_PACKAGE_NOT_FOUND"});
  const yearLines=(await db.prepare("SELECT SUM(base_debit_minor) debits,SUM(base_credit_minor) credits FROM finance_journal_lines WHERE journal_id=?").bind(posted.journalId).first())!;expect(yearLines).toEqual({debits:17500,credits:17500});expect((await reportingWorkspace(f.manager,f.chart)).closings.filter(row=>row.status==="posted")).toHaveLength(2);
  const amendmentExport=await statementPackageExport(f.manager,String(amended.id));expect(amendmentExport.sha256).toBe(createHash("sha256").update(amendmentExport.content).digest("hex"));expect(amendmentExport.sha256).not.toBe(exported.sha256);
  const evidenceDirectory=process.env.ERPX_ENGINEERING_EVIDENCE_DIR;if(evidenceDirectory){const candidate=process.env.ERPX_ENGINEERING_RELEASE_DIR,release=candidate?JSON.parse(readFileSync(join(candidate,"release.json"),"utf8")):null;if(release)expect(release.sourceFingerprint).toBe(process.env.ERPX_ENGINEERING_SOURCE_FINGERPRINT);await mkdir(evidenceDirectory,{recursive:true});await writeFile(join(evidenceDirectory,"financial-continuity.json"),JSON.stringify({passed:true,case:"NPAEs statement and fiscal closing lifecycle closes consecutive months and a reviewed fiscal year while retaining complete comparative statement editions",environment:"isolated-postgresql-fixtures",sourceFingerprint:process.env.ERPX_ENGINEERING_SOURCE_FINGERPRINT??null,compiledCandidate:Boolean(candidate),softwareVersion:release?.softwareVersion??null,fileManifestSha256:release?.fileManifestSha256??null,companyId:f.company,chartId:f.chart,sourceJournals,sourceInvoices,closedMonths:["2026-01","2026-02"],comparativeYear:2025,currentYear:2026,postedYearCloseJournalIds:[(await db.prepare("SELECT journal_id FROM finance_year_closings WHERE id=?").bind(priorClosing.id).first("journal_id")),posted.journalId],issuedStatementIds:[prepared.id,amended.id],originalSha256:exported.sha256,amendmentSha256:amendmentExport.sha256,notes:true,comparatives:true,equityMovementVerified:true,closingProfitMinor:11000,beforeTransferProfitMinor:11000,subledgerDifferenceMinor:"0",trialBalanceDifferenceMinor:"0",immutableIssuedCopies:true,staleSourceRejected:true,companyScopeVerified:true,currentAssetsMinor:67500,currentLiabilitiesMinor:1500,currentEquityMinor:66000,subledgerAssertions:reconciliation.rows.map(row=>({kind:row.kind,ledgerMinor:row.ledgerMinor,sourceMinor:row.sourceMinor,differenceMinor:row.differenceMinor}))},null,2));}
 });

 it("rejects stale reporting sources and preserves rejected or cancelled immutable drafts",async()=>{
  const forged=key();await expect(createJournal(f.reviewer,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-12-31",currency:"THB",memo:"Forged fiscal close owner source",sourceType:"fiscal_close",lines:[{accountId:f.accounts.revenue,debit:"20",credit:"0"},{accountId:retained,debit:"0",credit:"20"}]}),forged)).rejects.toMatchObject({code:"FISCAL_CLOSING_OWNER_REQUIRED"});expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE id=?").bind(`journal-${forged}`).first("n")).toBe(0);
  await post(f.period,"2026-05-01",f.accounts.cash,f.accounts.revenue,"20");
  const prepared=await prepareStatementPackage(f.reviewer,input(),key());await post(f.period,"2026-05-02",f.accounts.expense,f.accounts.cash,"3");await expect(decideStatementPackage(f.manager,String(prepared.id),"issue",review,key())).rejects.toMatchObject({code:"REPORTING_SOURCE_CHANGED"});
  await expect(decideStatementPackage(f.manager,String(prepared.id),"cancel",{...review,expectedVersion:99},key())).rejects.toMatchObject({code:"VERSION_CONFLICT"});
  const command=key(),cancelled=await decideStatementPackage(f.reviewer,String(prepared.id),"cancel",review,command);expect(await decideStatementPackage(f.reviewer,String(prepared.id),"cancel",review,command)).toEqual(cancelled);expect((await statementPackageDetail(f.manager,String(prepared.id))).history.map(row=>row.action)).toEqual(["prepared","cancelled"]);
  await expect(db.prepare("UPDATE finance_statement_packages SET notes_json='{}' WHERE id=?").bind(prepared.id).run()).rejects.toThrow("FINANCE_ISSUED_REPORT_IMMUTABLE");
  const second=await prepareStatementPackage(f.reviewer,input(),key());await decideStatementPackage(f.manager,String(second.id),"reject",review,key());expect((await statementPackageDetail(f.manager,String(second.id))).row).toMatchObject({status:"rejected",version:2});
  await expect(prepareStatementPackage(f.reviewer,{...input(),presentation:input().presentation.filter(row=>row.accountId!==f.accounts.revenue)},key())).rejects.toMatchObject({code:"STATEMENT_PRESENTATION_REQUIRED"});
  await expect(prepareStatementPackage(f.reviewer,{...input(),presentation:input().presentation.map(row=>row.accountId===f.accounts.revenue?{...row,line:"cash_equivalents" as const}:row)},key())).rejects.toMatchObject({code:"STATEMENT_PRESENTATION_INVALID"});
  await expect(prepareStatementPackage({...f.reviewer,branchId:f.branch},input(),key())).rejects.toMatchObject({code:"COMPANY_FINANCE_REQUIRED"});
  await expect(prepareStatementPackage({...f.reviewer,companyId:"another-company"},input(),key())).rejects.toMatchObject({code:"CHART_NOT_FOUND"});
  const yearInput={chartId:f.chart,startDate:"2026-01-01",endDate:"2026-12-31",periodId:f.period,retainedEarningsAccountId:retained,reason:"Reviewed all actual income and expense sources before closing preparation."},closing=await prepareYearClosing(f.reviewer,yearInput,key());await post(f.period,"2026-06-01",f.accounts.cash,f.accounts.revenue,"1");await expect(decideYearClosing(f.manager,String(closing.id),"post",review,key())).rejects.toMatchObject({code:"REPORTING_SOURCE_CHANGED"});
  await decideYearClosing(f.reviewer,String(closing.id),"cancel",review,key());expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE company_id=? AND source_type='fiscal_close'").bind(f.company).first("n")).toBe(0);
 });
});

it("drills retained statement lines into their exact current and comparative account sources and rejects changed evidence",async()=>{
 const prepared=await prepareStatementPackage(f.reviewer,input(),key()),id=String(prepared.id);
 const sources=await statementSourceAccounts(f.reviewer,id,{period:'current',line:'cash_equivalents'});
 expect(sources.rows.every(row=>row.id===f.accounts.cash)).toBe(true);
 const retained=await statementPackageDetail(f.reviewer,id);
 expect(Number(sources.row.total_minor)).toBe(retained.snapshot.current.position.find(row=>row.code==='cash_equivalents')?.amountMinor);
 await expect(statementSourceAccounts({...f.reviewer,companyId:key()},id,{period:'current'})).rejects.toMatchObject({status:404});
 await post(f.period,'2026-05-02',f.accounts.expense,f.accounts.cash,'3');
 await expect(statementSourceAccounts(f.reviewer,id,{period:'current',line:'cash_equivalents'})).rejects.toMatchObject({code:'STATEMENT_SOURCES_CHANGED'});
});
it("rolls back fiscal transfer journal money source revision and retained closing after final outbox failure, then retries once",async()=>{
 await post(f.period,'2026-10-31',f.accounts.cash,f.accounts.revenue,'100');
 const statement=await prepareStatementPackage(f.reviewer,input(),key());await decideStatementPackage(f.manager,String(statement.id),'issue',review,key());const retainedExport=await statementPackageExport(f.manager,String(statement.id));
 const prepared=await prepareYearClosing(f.reviewer,{chartId:f.chart,startDate:'2026-01-01',endDate:'2026-12-31',periodId:f.period,retainedEarningsAccountId:retained,reason:'Reviewed fiscal transfer from dated posted transactions'},key()),id=String(prepared.id),command=key(),fault=`fiscal_close_fault_${crypto.randomUUID().replaceAll('-','')}`;
 const sourceRevision=await db.prepare('SELECT revision FROM finance_reporting_revisions WHERE chart_id=?').bind(f.chart).first('revision'),old=await db.prepare('SELECT * FROM finance_year_closings WHERE id=?').bind(id).first(),events=(await db.prepare('SELECT * FROM finance_year_closing_events WHERE closing_id=? ORDER BY closing_version').bind(id).all()).results;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'FISCAL_CLOSE_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.year-close.post') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(decideYearClosing(f.manager,id,'post',review,command)).rejects.toThrow();
  expect(await db.prepare('SELECT * FROM finance_year_closings WHERE id=?').bind(id).first()).toEqual(old);
  expect((await db.prepare('SELECT * FROM finance_year_closing_events WHERE closing_id=? ORDER BY closing_version').bind(id).all()).results).toEqual(events);
  expect(await db.prepare('SELECT revision FROM finance_reporting_revisions WHERE chart_id=?').bind(f.chart).first('revision')).toEqual(sourceRevision);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
  expect(Number(await db.prepare("SELECT count(*) n FROM finance_journals WHERE source_type='fiscal_close' AND source_id=?").bind(id).first('n'))).toBe(0);
  expect(Number(await db.prepare("SELECT count(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_year_closing' AND source_id=?").bind(f.company,id).first('n'))).toBe(0);
  expect((await trialBalance(f.manager,undefined,f.chart)).find(a=>a.id===f.accounts.revenue)?.balance).toBe('-100.00');
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const posted=await decideYearClosing(f.manager,id,'post',review,command);expect(await decideYearClosing(f.manager,id,'post',review,command)).toEqual(posted);
 expect(posted).toMatchObject({status:'posted',version:2,profitMinor:10000});
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(1);
 expect(Number(await db.prepare("SELECT count(*) n FROM finance_journals WHERE source_type='fiscal_close' AND source_id=?").bind(id).first('n'))).toBe(1);
 expect(Number(await db.prepare("SELECT count(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_year_closing' AND source_id=?").bind(f.company,id).first('n'))).toBe(1);
 expect((await trialBalance(f.manager,undefined,f.chart)).find(a=>a.id===f.accounts.revenue)?.balance).toBe('0.00');expect(await statementPackageExport(f.manager,String(statement.id))).toEqual(retainedExport);
});


it("seeds complete immutable journal evidence atomically and preserves prepared posted records on repeat",async()=>{
 const id=`seed-journal-${key()}`,input={id,companyId:f.company,chartId:f.chart,periodId:f.period,number:key(),date:"2026-09-15",currency:"THB",fxRateMicros:1_000_000,memo:"Declared local browser journal",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.cash,amountMinor:12500,branchId:f.branch,status:"posted" as const,actorId:f.reviewer.userId,reviewerId:f.manager.userId,now:Date.now()};
 expect(await seedLocalFinanceJournal(db,input)).toBe(true);
 const evidence=await db.prepare("SELECT evidence_json FROM finance_financial_events WHERE journal_id=?").bind(id).first<{evidence_json:string}>();
 expect(JSON.parse(evidence!.evidence_json).lines).toHaveLength(2);
 const snapshot=async()=>Promise.all([db.prepare("SELECT * FROM finance_journals WHERE id=?").bind(id).first(),db.prepare("SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal").bind(id).all().then(rows=>rows.results),db.prepare("SELECT * FROM finance_financial_events WHERE journal_id=?").bind(id).first()]),before=await snapshot();
 expect(await seedLocalFinanceJournal(db,{...input,memo:"Never overwrite a prepared journal",amountMinor:1,now:input.now+1})).toBe(false);
 expect(await snapshot()).toEqual(before);
 await expect(db.prepare("UPDATE finance_journal_lines SET debit_minor=1 WHERE journal_id=? AND ordinal=0").bind(id).run()).rejects.toThrow("POSTED_JOURNAL_LINES_IMMUTABLE");
 const failed=`seed-journal-${key()}`;
 await expect(seedLocalFinanceJournal(db,{...input,id:failed,number:key(),creditAccountId:`missing-${key()}`})).rejects.toThrow();
 expect(await db.prepare("SELECT 1 FROM finance_journals WHERE id=?").bind(failed).first()).toBeNull();
 expect(await db.prepare("SELECT 1 FROM finance_journal_lines WHERE journal_id=?").bind(failed).first()).toBeNull();
 expect(await db.prepare("SELECT 1 FROM audit_events WHERE record_id=?").bind(failed).first()).toBeNull();
});
