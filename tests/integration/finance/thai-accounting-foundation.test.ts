import { packageGrantStore, readCompanyPackageGrants } from "../../../src/server/modules/commercial/public/package-grants";
import { confirmManualRate, configureCurrencyScale, moneyContext, saveCurrencyPreference } from "../../../src/server/modules/finance/public/company-money";
import { createFinanceDimension, financeBookWorkspace } from "../../../src/server/modules/finance/public/finance-books";
import type { FinanceRow } from "../../../src/server/modules/finance/public/finance";
import { beforeAll,beforeEach,afterEach,describe,expect,it,vi } from "vitest";
import { applyMigrations,env } from "../../support/runtime-env";
import { expenseDb as db,expenseFixture,key } from "../../support/helpers/expense-fixture";
import { accountingProfiles,financialEvents,decideAccountingProfile,prepareAccountingProfile } from "../../../src/server/modules/finance/public/accounting-profiles";
import { aging,beginClose,calculateCloseReconciliations,createAccount,createChart,createInvoice,createJournal,createJournalSchema,createPeriod,createParty,createTaxCode,createLoan,importAccounts,importBankStatement,invoiceSchema,postInvoice,postJournalBatch,reversePayment,statements,trialBalance } from "../../../src/server/modules/finance/public/finance";
import { settleInvoice } from "../../../src/server/modules/commerce/public/commerce";
import { assignFlowPrimary,resolveFlow } from "../../../src/server/modules/workflow/public/flow-routing";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
afterEach(()=>vi.unstubAllEnvs());
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

async function configurationGrant(packageId:"finance"|"finance-advanced"|"pos"|"pms",state:"active"|"draining"="active",endsAt:number|null=null){
 const prior=(await readCompanyPackageGrants(db,f.company)).find(grant=>grant.packageId===packageId);
 return packageGrantStore(db,{authorize:async()=>({userId:"isolated-configuration-review"}),obligations:async()=>{throw Error("No configuration withdrawal witness");}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(prior?.version??0)+1,reason:"Native purchased Company configuration",evidence:"test:finance-shared-configuration"}});
}
const configurationRows=()=>Promise.all(["finance_charts","finance_accounts","finance_periods","finance_parties","finance_tax_codes","finance_accounting_profiles","finance_dimension_definitions","finance_dimension_values","finance_book_rules","company_fx_rates","company_currency_scales","finance_loans","command_receipts","command_assertions","audit_events","outbox_events"].map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results));
const configurationProfile=(chartId=f.chart)=>({statutoryChartId:chartId,standard:"TFRS for NPAEs" as const,fiscalStartMonth:1,effectiveDate:"2026-01-01",eligibilityEvidence:"Independent Company review of current NPAEs eligibility",policyEvidence:"Current tax, moving average inventory and straight line depreciation reviewed",taxBasis:"current_tax" as const,inventoryPolicy:"moving_average" as const,depreciationPolicy:"straight_line" as const});
it("package configuration rejects unpurchased owner setup without blocking shared money display preferences",async()=>{
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const before=await configurationRows();
 const commands=[()=>createChart(f.reviewer,{name:"No purchase chart",baseCurrency:"THB",accountingBasis:"IFRS"},key()),()=>createAccount(f.reviewer,{chartId:f.chart,code:key(),name:"No purchase account",category:"asset",isControl:false,controlKind:"none"},key()),()=>createPeriod(f.reviewer,{name:"No purchase period",startDate:"2028-01-01",endDate:"2028-01-31"},key()),()=>createParty(f.reviewer,{kind:"supplier",code:key(),name:"No purchase supplier",currency:"THB",paymentTermsDays:30,creditLimit:"0"},key()),()=>createTaxCode(f.reviewer,{code:key(),name:"No purchase VAT",rate:"0.07",recoverable:"1",effectiveFrom:"2026-01-01"},key()),()=>importAccounts(f.reviewer,f.chart,[{code:key(),name:"No purchase import",category:"expense",currency:"THB",controlKind:"none"}],key()),()=>prepareAccountingProfile(f.reviewer,configurationProfile(),key()),()=>confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-01",currencyPerUsd:"35",note:"No purchase manual quote"},key()),()=>configureCurrencyScale(f.manager,{currency:"XAU",scale:6},key()),()=>createFinanceDimension(f.reviewer,{code:"configuration_scope",name:"No purchase dimension",required:false,values:[{code:"default",name:"Default"}]},key())];
 for(const command of commands)await expect(command()).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});for(const read of [()=>accountingProfiles(f.owner),()=>financialEvents(f.owner,{chartId:f.chart}),()=>financeBookWorkspace(f.owner)])await expect(read()).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await configurationRows()).toEqual(before);
 expect(await moneyContext(f.reviewer)).toBeTruthy();expect(await saveCurrencyPreference(f.reviewer,{currency:"USD"},key())).toEqual({currency:"USD"});expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);
});
it("package POS configures its included accounting foundation and independently reviews a draining policy without full Finance",async()=>{
 await configurationGrant("pos");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const chartKey=key(),input={name:"POS included Company ledger",baseCurrency:"THB",accountingBasis:"IFRS"},chart=await createChart(f.reviewer,input,chartKey),account=await createAccount(f.reviewer,{chartId:chart.id,code:key(),name:"Included bank",category:"asset",isControl:true,controlKind:"bank"},key());
 await importAccounts(f.reviewer,chart.id,[{code:key(),name:"Included expense",category:"expense",currency:"THB",controlKind:"none"}],key());await createTaxCode(f.reviewer,{code:key(),name:"Included VAT",rate:"0.07",recoverable:"1",effectiveFrom:"2026-01-01"},key());await createPeriod(f.reviewer,{name:key(),startDate:"2028-01-01",endDate:"2028-01-31"},key());await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-01",currencyPerUsd:"35",note:"Current Company manual quote"},key());await configureCurrencyScale(f.manager,{currency:"XAU",scale:6},key());
 await createFinanceDimension(f.reviewer,{code:"pos_reference",name:"Included reporting dimension",required:false,values:[{code:"shop",name:"Shop"}]},key());
 const prepared=await prepareAccountingProfile(f.reviewer,configurationProfile(chart.id),key());await configurationGrant("pos","draining");expect(await createChart(f.reviewer,input,chartKey)).toEqual(chart);
 await expect(createChart(f.reviewer,{...input,name:"New draining ledger"},key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});await decideAccountingProfile(f.manager,prepared.id,"approve",{expectedVersion:1,reason:"Independent eligibility and Company policy review"},key());expect(await accountingProfiles(f.manager)).toEqual(expect.arrayContaining([expect.objectContaining({id:prepared.id,status:"approved"})]));
 for(const read of [()=>financialEvents(f.owner,{chartId:chart.id}),()=>financeBookWorkspace(f.owner)])await expect(read()).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(account.id).toBeTruthy();expect((await readCompanyPackageGrants(db,f.company)).map(grant=>grant.packageId)).toEqual(["pos"]);
 await configurationGrant("pms");expect(await createChart(f.reviewer,{...input,name:"Another active foundation"},key())).toHaveProperty("id");
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.manager.assignmentId).run();await expect(accountingProfiles(f.manager)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});
});
it("package configuration shadow keeps normal writes but never bypasses current assignment, and Finance base cannot create advanced instruments",async()=>{
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","shadow");const log=vi.spyOn(console,"info").mockImplementation(()=>{});
 try{expect(await createChart(f.reviewer,{name:key(),baseCurrency:"THB",accountingBasis:"IFRS"},key())).toHaveProperty("id");await expect(createChart({...f.reviewer,role:"finance_approver"},{name:key(),baseCurrency:"THB",accountingBasis:"IFRS"},key())).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(log.mock.calls.some(args=>String(args[0]).includes("commercial.purchase.shadow"))).toBe(true);}finally{log.mockRestore();}
 await configurationGrant("finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const loan={loanNumber:key(),lender:"Reviewed lender",currency:"THB",principal:"100",annualRate:"0",commencementDate:"2026-09-01",maturityDate:"2027-09-01"},before=await configurationRows();await expect(createLoan(f.reviewer,loan,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await configurationRows()).toEqual(before);await configurationGrant("finance-advanced");const command=key(),result=await createLoan(f.reviewer,loan,command);expect(await createLoan(f.reviewer,loan,command)).toEqual(result);
});
it("package shared configuration expiry after deferred effects rolls back chart, audit and receipt",async()=>{
 await configurationGrant("pos","active",Date.now()+1500);vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const before=await configurationRows(),trigger=`configuration_${key().replaceAll('-','')}`,command=key();
 await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(1.6);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.action='finance.chart.create' AND NEW.authority_json::jsonb->>'companyId'='${f.company}') EXECUTE FUNCTION ${trigger}()`);
 try{await expect(createChart(f.reviewer,{name:key(),baseCurrency:"THB",accountingBasis:"IFRS"},command)).rejects.toMatchObject({code:"PACKAGE_CAPABILITY_CHANGED"});expect(await configurationRows()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${trigger} ON audit_events;DROP FUNCTION ${trigger}()`);}
});

describe("Thai accounting foundation",()=>{
  it("imports and revises an unused chart account with replay and independent Company ledger history",async()=>{
    const rows=[{code:`IMPORTED-${key()}`,name:"Imported current receivable",category:"asset" as const,currency:"THB",controlKind:"receivable" as const},{code:`IMPORTED-${key()}`,name:"Imported operating expense",category:"expense" as const,currency:"THB",controlKind:"none" as const}],command=key(),imported=await importAccounts(f.reviewer,f.chart,rows,command);
    expect(await importAccounts(f.reviewer,f.chart,rows,command)).toEqual(imported);
    let accounts=(await db.prepare("SELECT code,name,category,currency,control_kind,version FROM finance_accounts WHERE company_id=? AND chart_id=? AND code IN (?,?) ORDER BY code").bind(f.company,f.chart,...rows.map(row=>row.code)).all()).results;
    expect(accounts).toEqual(expect.arrayContaining(rows.map(row=>expect.objectContaining({code:row.code,name:row.name,category:row.category,currency:"THB",control_kind:row.controlKind,version:1}))));
    await importAccounts(f.reviewer,f.chart,[{...rows[1],name:"Reviewed unused expense name"}],key());
    accounts=(await db.prepare("SELECT name,version FROM finance_accounts WHERE chart_id=? AND code=?").bind(f.chart,rows[1].code).all()).results;expect(accounts).toEqual([{name:"Reviewed unused expense name",version:2}]);
    await expect(importAccounts({...f.reviewer,companyId:"another-company"},f.chart,rows,key())).rejects.toMatchObject({code:"CHART_NOT_FOUND"});
    expect(await db.prepare("SELECT COUNT(*) n FROM audit_events WHERE command_id=? AND action='finance.account.import'").bind(command).first("n")).toBe(1);
  });
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
    const period=await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first<FinanceRow>();
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
