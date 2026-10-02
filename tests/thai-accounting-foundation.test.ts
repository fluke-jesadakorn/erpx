import { beforeAll,beforeEach,describe,expect,it,vi } from "vitest";
import { applyMigrations,env } from "./runtime-env";
import { expenseDb as db,expenseFixture,key } from "./helpers/expense-fixture";
import { accountingProfiles,decideAccountingProfile,prepareAccountingProfile } from "../src/server/accounting-profiles";
import { aging,beginClose,calculateCloseReconciliations,createAccount,createChart,createInvoice,createJournal,createJournalSchema,createPeriod,importAccounts,importBankStatement,invoiceSchema,postInvoice,postJournalBatch,reversePayment,statements,trialBalance } from "../src/server/finance";
import { settleInvoice } from "../src/server/commerce";
import { assignFlowPrimary,resolveFlow } from "../src/server/flow-routing";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
  f=await expenseFixture();
  for(const [actor,role] of [[f.reviewer,"finance_preparer"],[f.manager,"finance_approver"],[f.payer,"treasury"]] as const){actor.role=role;actor.branchId=null;actor.departmentId=null;actor.assignmentId=await f.grant(actor,role);}
});
async function assign(kind:"journal"|"invoice",id:string,stepKey:string,userId=f.manager.userId){
  const flow=await resolveFlow(db,f.owner,kind,id);
  await assignFlowPrimary(db,f.owner,kind,id,{stepKey,userId,reason:"Independent Thai accounting fixture review",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
}
async function draft(lines=2){
  const journal=await createJournal(f.reviewer,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-02",currency:"THB",memo:"Accountant checked fixture",lines:Array.from({length:lines},(_,index)=>({accountId:index%2?f.accounts.revenue:f.accounts.cash,debit:index%2?"0":"1",credit:index%2?"1":"0"}))}),key());
  await assign("journal",journal.id,"draft");return {id:journal.id,expectedVersion:1};
}

describe("Thai accounting foundation",()=>{
  it("selects one THB statutory ledger only after independent NPAEs and policy review",async()=>{
    const input={statutoryChartId:f.chart,standard:"TFRS for NPAEs" as const,fiscalStartMonth:1,effectiveDate:"2026-01-01",eligibilityEvidence:"Accountant reviewed the entity's public accountability and NPAEs criteria",policyEvidence:"Company confirmed current tax, moving average stock and straight-line depreciation",taxBasis:"current_tax" as const,inventoryPolicy:"moving_average" as const,depreciationPolicy:"straight_line" as const};
    const command=key(),prepared=await prepareAccountingProfile(f.reviewer,input,command);
    expect(await prepareAccountingProfile(f.reviewer,input,command)).toEqual(prepared);
    await expect(decideAccountingProfile({...f.reviewer,role:"finance_approver"},prepared.id,"approve",{expectedVersion:1,reason:"Cannot approve own evidence"},key())).rejects.toMatchObject({code:"INDEPENDENT_APPROVER_REQUIRED"});
    await expect(decideAccountingProfile({...f.manager,companyId:"another-company"},prepared.id,"approve",{expectedVersion:1,reason:"Cross Company profile review"},key())).rejects.toMatchObject({code:"PROFILE_NOT_FOUND"});
    await decideAccountingProfile(f.manager,prepared.id,"approve",{expectedVersion:1,reason:"Reviewed NPAEs eligibility, THB ledger and the Company's approved policies"},key());
    expect(await accountingProfiles(f.manager)).toEqual(expect.arrayContaining([expect.objectContaining({status:"approved",statutory_chart_id:f.chart,version:2})]));
    await expect(prepareAccountingProfile(f.reviewer,input,key())).rejects.toMatchObject({code:"STATUTORY_PROFILE_EXISTS"});
    const foreign=await createChart(f.reviewer,{name:"Foreign ledger",baseCurrency:"USD",accountingBasis:"IFRS"},key());
    await expect(prepareAccountingProfile(f.reviewer,{...input,statutoryChartId:String(foreign.id)},key())).rejects.toMatchObject({code:"THAI_STATUTORY_CURRENCY"});
  });

  it("posts a reviewed batch atomically, replays concurrent retries and captures immutable line evidence",async()=>{
    const journals=[await draft(),await draft()],command=key(),input={journals,reason:"Reviewed both source documents and balanced debit/credit lines"};
    const [first,retry]=await Promise.all([postJournalBatch(f.manager,input,command),postJournalBatch(f.manager,input,command)]);
    expect(first).toEqual(retry);expect(first).toMatchObject({status:"posted",lineCount:4});
    const events=(await db.prepare("SELECT evidence_json,legacy_snapshot FROM finance_financial_events WHERE company_id=? AND journal_id IN (?,?)").bind(f.company,...journals.map(j=>j.id)).all<{evidence_json:string;legacy_snapshot:number}>()).results;
    expect(events).toHaveLength(2);for(const event of events){expect(JSON.parse(event.evidence_json).lines).toHaveLength(2);expect(event.legacy_snapshot).toBe(0);}
    await expect(db.prepare("UPDATE finance_financial_events SET effective_date='2026-01-01' WHERE journal_id=?").bind(journals[0].id).run()).rejects.toThrow("FINANCIAL_EVENT_IMMUTABLE");
    await expect(db.prepare("UPDATE finance_journals SET entry_date='2026-01-01' WHERE id=?").bind(journals[0].id).run()).rejects.toThrow("POSTED_JOURNAL_EVIDENCE_IMMUTABLE");
    await expect(db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,1,0,1,0,9)").bind(key(),journals[0].id,f.accounts.cash).run()).rejects.toThrow("POSTED_JOURNAL_LINES_IMMUTABLE");
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_batch_review_items WHERE journal_id IN (?,?)").bind(...journals.map(j=>j.id)).first("n")).toBe(2);
  });

  it("rolls back every draft and event if the later draft has an inactive account",async()=>{
    const first=await draft(),second=await draft();
    const account=await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:"Later batch account",category:"asset",isControl:false,controlKind:"none"},key());
    await db.prepare("UPDATE finance_journal_lines SET account_id=? WHERE journal_id=? AND ordinal=0").bind(account.id,second.id).run();
    await db.prepare("UPDATE finance_accounts SET status='inactive' WHERE id=?").bind(account.id).run();
    await expect(postJournalBatch(f.manager,{journals:[first,second],reason:"Reviewed batch before the later account became inactive"},key())).rejects.toMatchObject({code:"CONFLICT"});
    expect((await db.prepare("SELECT status FROM finance_journals WHERE id IN (?,?)").bind(first.id,second.id).all()).results).toEqual([{status:"draft"},{status:"draft"}]);
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_financial_events WHERE journal_id IN (?,?)").bind(first.id,second.id).first("n")).toBe(0);
  });

  it("rejects stale source versions, duplicate selection, self review and more than 1,000 batch lines",async()=>{
    const small=await draft();
    await expect(postJournalBatch(f.manager,{journals:[{...small,expectedVersion:2}],reason:"Reviewed stale draft snapshot"},key())).rejects.toMatchObject({code:"VERSION_CONFLICT"});
    await expect(postJournalBatch(f.manager,{journals:[small,small],reason:"Duplicate selected journal"},key())).rejects.toMatchObject({code:"DUPLICATE_JOURNAL"});
    await expect(postJournalBatch({...f.reviewer,role:"finance_approver"},{journals:[small],reason:"Cannot independently approve own draft"},key())).rejects.toBeDefined();
    const big=[await draft(500),await draft(500),small];
    await expect(postJournalBatch(f.manager,{journals:big,reason:"Reviewed oversized accounting batch"},key())).rejects.toMatchObject({code:"BATCH_LINE_LIMIT"});
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE company_id=? AND status='posted'").bind(f.company).first("n")).toBe(0);
    expect(await postJournalBatch(f.manager,{journals:big.slice(0,2),reason:"Reviewed the permitted boundary of exactly 1,000 balanced lines"},key())).toMatchObject({lineCount:1000,status:"posted"});
  });

  it("preserves historical paid and reversed receipts using recognition and correction dates",async()=>{
    const party=`customer-${key()}`;
    await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Historical customer','THB',1,1)").bind(party,f.company,party).run();
    const invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party,kind:"receivable",invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-15",currency:"THB",subtotal:"100",tax:"0",debitAccountId:f.accounts.cash,creditAccountId:f.accounts.revenue}),key());
    await assign("invoice",invoice.id,"draft");await postInvoice(f.manager,invoice.id,{expectedVersion:1,periodId:f.period,entryDate:"2026-09-10"},key());
    await expect(db.prepare("UPDATE finance_invoices SET total_minor=1 WHERE id=?").bind(invoice.id).run()).rejects.toThrow("POSTED_INVOICE_EVIDENCE_IMMUTABLE");
    expect(await aging(f.manager,"receivable","2026-09-09",f.chart)).toEqual([]);
    const original=await aging(f.manager,"receivable","2026-09-30",f.chart);expect(original).toEqual(expect.arrayContaining([expect.objectContaining({id:invoice.id,outstanding:"100.00"})]));
    const next=await createPeriod(f.reviewer,{name:"Next year correction",startDate:"2027-01-01",endDate:"2027-01-31"},key());
    await assign("invoice",invoice.id,"posted",f.payer.userId);
    const receipt=await settleInvoice(f.payer,invoice.id,{expectedVersion:2,date:"2027-01-02",amount:"100",reference:key()},key());
    expect((await aging(f.manager,"receivable","2026-09-30",f.chart)).map(r=>r.outstanding)).toEqual(["100.00"]);
    expect(await aging(f.manager,"receivable","2027-01-02",f.chart)).toEqual([]);
    await reversePayment(f.manager,String(receipt.paymentId),{expectedVersion:1,periodId:String(next.id),entryDate:"2027-01-03",reason:"Independent bank evidence confirms incorrect customer allocation"},key());
    expect((await aging(f.manager,"receivable","2026-09-30",f.chart)).map(r=>r.outstanding)).toEqual(["100.00"]);
    expect(await aging(f.manager,"receivable","2027-01-02",f.chart)).toEqual([]);
    expect((await aging(f.manager,"receivable","2027-01-03",f.chart)).map(r=>r.outstanding)).toEqual(["100.00"]);
    await expect(aging(f.manager,"receivable","bad-date")).rejects.toMatchObject({code:"INVALID_AS_OF"});
  });

  it("blocks account imports that change referenced history and refuses implicit same-currency ledger aggregation",async()=>{
    await draft();
    const account=await db.prepare("SELECT code,name,category FROM finance_accounts WHERE id=?").bind(f.accounts.cash).first<{code:string;name:string;category:"asset"}>();
    await expect(importAccounts(f.reviewer,f.chart,[{...account!,category:"expense"}],key())).rejects.toMatchObject({code:"ACCOUNT_HISTORY_IMMUTABLE"});
    expect(await db.prepare("SELECT category FROM finance_accounts WHERE id=?").bind(f.accounts.cash).first("category")).toBe("asset");
    const management=await createChart(f.reviewer,{name:"Same THB management book",baseCurrency:"THB",accountingBasis:"IFRS"},key());
    await expect(trialBalance(f.manager)).rejects.toMatchObject({code:"CHART_REQUIRED"});
    await db.prepare("UPDATE finance_charts SET status='archived' WHERE id=?").bind(management.id).run();
    await expect(trialBalance(f.manager)).rejects.toMatchObject({code:"CHART_REQUIRED"});
    await expect(trialBalance(f.manager,"another-company-period",f.chart)).rejects.toMatchObject({code:"PERIOD_NOT_FOUND"});
    expect(await trialBalance(f.manager,undefined,f.chart)).toEqual([]);
  });

  it("blocks close when a legacy receivable matches GL but has no dated recognition evidence",async()=>{
    await db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='receivable' WHERE id=?").bind(f.accounts.cash).run();
    await postJournalBatch(f.manager,{journals:[await draft()],reason:"Reviewed dated GL balances independently of the legacy migration"},key());
    const party=`customer-${key()}`,invoice=`invoice-${key()}`;
    await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Legacy customer','THB',1,1)").bind(party,f.company,party).run();
    await db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,status,created_by,created_at,updated_at) VALUES(?,?,?,'receivable',?,'2026-09-02','2026-09-15','THB',100,0,100,100,100,100,?,?,'posted',?,1,1)").bind(invoice,f.company,party,invoice,f.accounts.cash,f.accounts.revenue,f.reviewer.userId).run();
    expect(await aging(f.manager,"receivable","2026-09-30",f.chart)).toEqual(expect.arrayContaining([expect.objectContaining({id:invoice,outstanding:"1.00",legacy_date_evidence:true})]));
    const period=await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first();
    const reconciliation=await calculateCloseReconciliations(db,f.manager,period!);
    expect(reconciliation).toMatchObject({ready:false,rows:[expect.objectContaining({differenceMinor:0,source:expect.objectContaining({legacyDateCount:1})})]});
    await expect(beginClose(f.reviewer,f.period,key())).rejects.toMatchObject({code:"CLOSE_RECONCILIATION_FAILED"});
  });

  it("imports a quiet-month bank statement and keeps balance-sheet carry-forward separate from period profit",async()=>{
    const quiet=await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:"2026-09-30",opening:"100",closing:"100",sourceHash:key(),rows:[]},key());
    expect(quiet).toMatchObject({imported:0});
    await expect(importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:"2026-10-31",opening:"100",closing:"101",sourceHash:key(),rows:[]},key())).rejects.toMatchObject({code:"STATEMENT_UNBALANCED"});
    await postJournalBatch(f.manager,{journals:[await draft()],reason:"Reviewed the earlier period's journal"},key());
    const report=await statements(f.manager,undefined,f.chart,{startDate:"2026-10-01",endDate:"2026-10-31"});
    expect(report.balanceSheet.assets).toBe("1.00");expect(report.balanceSheet.equity).toBe("1.00");expect(report.balanceSheet.unclosedProfit).toBe("1.00");expect(report.profitAndLoss.revenue).toBe("0.00");
  });
});
