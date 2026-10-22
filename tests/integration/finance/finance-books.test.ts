import { reviewedFinanceGrant } from "../../support/helpers/finance-purchase-fixture";
import { packageGrantStore, readCompanyPackageGrants } from "@erpx/server-commercial/public/package-grants";
import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {prepareBookAdjustment,decideBookAdjustment,reverseBookAdjustment} from "@erpx/server-finance/public/finance-book-adjustments";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  bookRuleSchema,
  prepareBookRule,
  decideBookRule,
  createFinanceDimension,
} from "@erpx/server-finance/public/finance-books";
import {
  createJournal,
  createJournalSchema,
  postJournal,
  reverseJournal,
  trialBalance,
  calculateCloseReconciliations,
  createPeriod,
  beginClose,
  closePeriod,
  bankWorkspace,
  importBankStatement,
} from "@erpx/server-finance/public/finance";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
afterEach(()=>vi.unstubAllEnvs());
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  f.owner.role = "finance_preparer";
  f.owner.assignmentId = await f.grant(f.owner, "finance_preparer");
  f.manager.role = "finance_approver";
  f.manager.assignmentId = await f.grant(f.manager, "finance_approver");
});
async function setup(effectiveDate = "2026-09-03") {
  const chart = key();
  await db
    .prepare(
      "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,?,'THB',?,?,?)",
    )
    .bind(chart, f.company, chart, f.owner.userId, Date.now(), Date.now())
    .run();
  const src = (
    await db
      .prepare("SELECT * FROM finance_accounts WHERE chart_id=? ORDER BY code")
      .bind(f.chart)
      .all<{ id: string }>()
  ).results;
  const mappings = [];
  for (const a of src) {
    const target = key();
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) SELECT ?,?,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at FROM finance_accounts WHERE id=?",
      )
      .bind(target, chart, a.id)
      .run();
    mappings.push({ primaryAccountId: a.id, secondaryAccountId: target });
  }
  const rule = await prepareBookRule(
    f.owner,
    bookRuleSchema.parse({
      primaryChartId: f.chart,
      secondaryChartId: chart,
      effectiveDate,
      openingPeriodId: f.period,
      reason: "Management book same functional currency",
      mappings,
    }),
    key(),
  );
  return { chart, mappings, rule };
}
async function journal(
  date = "2026-09-04",
  dimensions?: Record<string, string>,
) {
  return createJournal(
    f.owner,
    createJournalSchema.parse({
      chartId: f.chart,
      periodId: f.period,
      entryDate: date,
      currency: "THB",
      lines: [
        {
          accountId: f.accounts.expense,
          debit: "100.01",
          credit: "0",
          dimensions,
        },
        {
          accountId: f.accounts.payable,
          debit: "0",
          credit: "100.01",
          dimensions,
        },
      ],
    }),
    key(),
  );
}
it('rolls back the reviewed secondary opening and rule approval after final outbox failure, then retries and replays once',async()=>{
 const original=await journal('2026-09-02');await postJournal(f.manager,String(original.id),key(),{expectedVersion:1});
 const s=await setup(),id=String(s.rule.id),command=key(),input={expectedVersion:1,reason:'Reviewed exact retained opening independently'},fault=`book_opening_fault_${crypto.randomUUID().replaceAll('-','')}`,before=await db.prepare('SELECT * FROM finance_book_rules WHERE id=?').bind(id).first();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'BOOK_OPENING_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.book.approve') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(decideBookRule(f.manager,id,input,'approve',command)).rejects.toThrow();
  expect(await db.prepare('SELECT * FROM finance_book_rules WHERE id=?').bind(id).first()).toEqual(before);
  expect(await db.prepare('SELECT id FROM finance_journals WHERE id=?').bind(`book-opening-${id}`).first()).toBeNull();
  expect(Number(await db.prepare('SELECT count(*) n FROM finance_journal_lines WHERE journal_id=?').bind(`book-opening-${id}`).first('n'))).toBe(0);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const approved=await decideBookRule(f.manager,id,input,'approve',command);expect(await decideBookRule(f.manager,id,input,'approve',command)).toEqual(approved);
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_journals WHERE id=?').bind(`book-opening-${id}`).first('n'))).toBe(1);
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_journal_lines WHERE journal_id=?').bind(`book-opening-${id}`).first('n'))).toBe(2);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(1);
});
it('rolls back secondary adjustment journal and Workflow decision proof after final outbox failure, then retries and replays once',async()=>{
 await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const s=await setup();await decideBookRule(f.manager,String(s.rule.id),{expectedVersion:1,reason:'Opening and mapping reviewed'},'approve',key());
 const source=await journal();await postJournal(f.manager,String(source.id),key(),{expectedVersion:1});
 const expense=s.mappings.find(m=>m.primaryAccountId===f.accounts.expense)!.secondaryAccountId,noncontrol=s.mappings.find(m=>m.primaryAccountId===f.accounts.tax)!.secondaryAccountId,adjustment=await prepareBookAdjustment(f.owner,{chartId:s.chart,primaryJournalId:String(source.id),periodId:f.period,entryDate:'2026-09-04',reason:'Reviewed secondary policy difference',lines:[{accountId:expense,debit:'10.01',credit:'0'},{accountId:noncontrol,debit:'0',credit:'10.01'}]},key()),id=String(adjustment.id),command=key(),input={expectedVersion:1,reason:'Independent secondary correction approval'},fault=`book_adjustment_fault_${crypto.randomUUID().replaceAll('-','')}`,before=await db.prepare('SELECT * FROM finance_book_adjustments WHERE id=?').bind(id).first(),originalSource=await db.prepare('SELECT * FROM finance_journals WHERE id=?').bind(source.id).first();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'BOOK_ADJUSTMENT_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.book-adjustment.post') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(decideBookAdjustment(f.manager,id,input,'post',command)).rejects.toThrow();
  expect(await db.prepare('SELECT * FROM finance_book_adjustments WHERE id=?').bind(id).first()).toEqual(before);
  expect(await db.prepare('SELECT * FROM finance_journals WHERE id=?').bind(source.id).first()).toEqual(originalSource);
  expect(Number(await db.prepare("SELECT count(*) n FROM finance_journals WHERE source_type='book_adjustment' AND source_id=?").bind(id).first('n'))).toBe(0);
  for(const table of ['command_receipts','audit_events','outbox_events','decision_evidence'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const posted=await decideBookAdjustment(f.manager,id,input,'post',command);expect(await decideBookAdjustment(f.manager,id,input,'post',command)).toEqual(posted);
 expect(Number(await db.prepare("SELECT count(*) n FROM finance_journals WHERE source_type='book_adjustment' AND source_id=?").bind(id).first('n'))).toBe(1);
 expect(await db.prepare('SELECT * FROM finance_journals WHERE id=?').bind(source.id).first()).toEqual(originalSource);
 for(const table of ['command_receipts','audit_events','outbox_events','decision_evidence'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(1);
});
it("retains independent secondary-only adjustments and their reversals without changing the business source",async()=>{
 const s=await setup();await decideBookRule(f.manager,String(s.rule.id),{expectedVersion:1,reason:"Opening and mapping reviewed"},"approve",key());
 const source=await journal();await postJournal(f.manager,String(source.id),key(),{expectedVersion:1});
 const expense=s.mappings.find(m=>m.primaryAccountId===f.accounts.expense)!.secondaryAccountId;
 const noncontrol=s.mappings.find(m=>m.primaryAccountId===f.accounts.tax)!.secondaryAccountId;
 const a=await prepareBookAdjustment(f.owner,{chartId:s.chart,primaryJournalId:String(source.id),periodId:f.period,entryDate:"2026-09-04",reason:"Reviewed secondary policy difference",lines:[{accountId:expense,debit:"10.01",credit:"0"},{accountId:noncontrol,debit:"0",credit:"10.01"}]},key());
 await decideBookAdjustment(f.manager,String(a.id),{expectedVersion:1,reason:"Independent policy adjustment approval"},"post",key());
 expect((await trialBalance(f.owner,undefined,f.chart)).find(r=>r.id===f.accounts.expense)?.balance).toBe("100.01");
 expect((await trialBalance(f.owner,undefined,s.chart)).find(r=>r.id===expense)?.balance).toBe("110.02");
 await reverseBookAdjustment(f.manager,String(a.id),{expectedVersion:2,periodId:f.period,entryDate:"2026-09-05",reason:"Correcting the approved policy adjustment"},key());
 expect((await trialBalance(f.owner,undefined,s.chart)).find(r=>r.id===expense)?.balance).toBe("100.01");
});
it("can reverse retained postings after a new required reporting dimension is introduced",async()=>{
 const source=await journal();await postJournal(f.manager,String(source.id),key(),{expectedVersion:1});
 await createFinanceDimension(f.owner,{code:"new_project",name:"New project reporting",required:true,values:[{code:"P1",name:"Project one"}]},key());
 await reverseJournal(f.manager,String(source.id),key(),{periodId:f.period,entryDate:"2026-09-05"});
 expect((await trialBalance(f.owner,undefined,f.chart)).find(r=>r.id===f.accounts.expense)?.balance).toBe("0.00");
 const newDraft=await journal();await expect(postJournal(f.manager,String(newDraft.id),key(),{expectedVersion:1})).rejects.toMatchObject({code:"DIMENSION_VALUE_REQUIRED"});
});
it("closes Company dates across retained annual and monthly periods, then reverses in the next open month in both books", async () => {
  const secondary=await setup("2026-09-01"),month=await createPeriod(f.owner,{name:"September monthly close",startDate:"2026-09-01",endDate:"2026-09-30"},key());
  await decideBookRule(f.manager,String(secondary.rule.id),{expectedVersion:1,reason:"Independently reviewed zero opening and identical secondary mappings"},"approve",key());
  const draft=await journal("2026-09-04");
  await expect(beginClose(f.owner,String(month.id),key())).rejects.toMatchObject({code:"CLOSE_NOT_READY"});
  await postJournal(f.manager,String(draft.id),key(),{expectedVersion:1});
  const started=await beginClose(f.owner,String(month.id),key());
  await expect(journal("2026-09-05")).rejects.toMatchObject({code:"PERIOD_CLOSED"});
  await closePeriod(f.manager,String(month.id),{expectedVersion:Number(started.version),proof:"Independent monthly close includes annual-period postings and both books"},key());
  await expect(journal("2026-09-05")).rejects.toMatchObject({code:"PERIOD_CLOSED"});
  const bypass=await journal("2026-10-02");
  await expect(db.prepare("UPDATE finance_journals SET status='posted',entry_date='2026-09-05' WHERE id=?").bind(bypass.id).run()).rejects.toThrow("PERIOD_CLOSED");
  expect(await db.prepare("SELECT status FROM finance_journals WHERE id=?").bind(bypass.id).first("status")).toBe("draft");
  await reverseJournal(f.manager,String(draft.id),key(),{periodId:f.period,entryDate:"2026-10-01",memo:"Keep the September close; correct in the next open month"});
  for(const chart of [f.chart,secondary.chart]) {
    expect((await trialBalance(f.owner,undefined,chart)).filter(a=>a.category==='expense' && a.balance!=="0.00")).toEqual([]);
    expect((await trialBalance(f.owner,undefined,chart,{endDate:"2026-09-30"})).filter(a=>a.category==='expense').map(a=>a.balance)).toEqual(["100.01"]);
  }
  expect(await db.prepare("SELECT state FROM finance_periods WHERE id=?").bind(month.id).first("state")).toBe("closed");
});
it("retains one business bank statement in the primary book and excludes secondary accounts from native imports", async () => {
  const s=await setup("2026-09-01"),secondaryBank=s.mappings.find(m=>m.primaryAccountId===f.accounts.cash)!.secondaryAccountId;
  await db.prepare("UPDATE finance_accounts SET control_kind='bank',is_control=1 WHERE id IN (?,?)").bind(f.accounts.cash,secondaryBank).run();
  await decideBookRule(f.manager,String(s.rule.id),{expectedVersion:1,reason:"Independent primary and secondary bank control mapping"},"approve",key());
  const options=await bankWorkspace(f.owner,{new:"1"});
  expect(options.accounts.map(a=>a.id)).toContain(f.accounts.cash);
  expect(options.accounts.some(a=>a.chart_id===s.chart)).toBe(false);
  const input={bankAccount:secondaryBank,statementNumber:key(),statementDate:"2026-09-30",opening:"0.00",closing:"100.01",sourceHash:key(),rows:[{externalId:key(),transactionDate:"2026-09-04",amount:"100.01",currency:"THB"}]};
  await expect(importBankStatement(f.owner,input,key())).rejects.toMatchObject({code:"BANK_ACCOUNT_INVALID"});
  expect(Number(await db.prepare("SELECT COUNT(*) n FROM finance_bank_statements WHERE company_id=?").bind(f.company).first("n"))).toBe(0);
  await importBankStatement(f.owner,{...input,bankAccount:f.accounts.cash},key());
  expect(Number(await db.prepare("SELECT COUNT(*) n FROM finance_bank_statements WHERE company_id=?").bind(f.company).first("n"))).toBe(1);
  expect(await db.prepare("SELECT bank_account FROM finance_bank_statements WHERE company_id=?").bind(f.company).first("bank_account")).toBe(f.accounts.cash);
});
it("serializes a monthly close racing an annual-period draft without leaving a draft inside closed Company dates", async () => {
  for (const day of ["2026-09-10","2026-09-11","2026-09-12"]) {
    const month=await createPeriod(f.owner,{name:`Race boundary ${day}`,startDate:day,endDate:day},key());
    const results=await Promise.allSettled([journal(day),beginClose(f.manager,String(month.id),key())]);
    expect(results.filter(r=>r.status==="fulfilled")).toHaveLength(1);
    const state=await db.prepare("SELECT state FROM finance_periods WHERE id=?").bind(month.id).first("state");
    const drafts=Number(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE company_id=? AND entry_date=? AND status IN ('draft','in_review')").bind(f.company,day).first("n"));
    expect({state,drafts}).toEqual(results[0].status==="fulfilled"?{state:"open",drafts:1}:{state:"closing",drafts:0});
  }
});
async function reviewedFinanceBookPurchase(state:"active"|"draining") {
 const prior=(await readCompanyPackageGrants(db,f.company)).find(grant=>grant.packageId==="finance");
 await packageGrantStore(db,{authorize:async()=>({userId:"isolated-book-purchase-review"}),obligations:async()=>{throw Error("No book withdrawal witness");}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId:"finance",packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:"Reviewed Finance book lifecycle",evidence:"test:finance-book-purchase"}});
}
it("reviews an opening independently and atomically mirrors postings, evidence and reversals", async () => {
  await reviewedFinanceBookPurchase("active");
  vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
  const first = await journal("2026-09-02");
  await postJournal(f.manager, String(first.id), key(), { expectedVersion: 1 });
  const s = await setup(),
    self = {
      ...f.owner,
      role: "finance_approver",
      assignmentId: await f.grant(f.owner, "finance_approver"),
    };
  await expect(
    decideBookRule(
      self,
      String(s.rule.id),
      { expectedVersion: 1, reason: "Self review refused" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await reviewedFinanceBookPurchase("draining");
  const k = key();
  await decideBookRule(
    f.manager,
    String(s.rule.id),
    { expectedVersion: 1, reason: "Opening and every account checked" },
    "approve",
    k,
  );
  await decideBookRule(
    f.manager,
    String(s.rule.id),
    { expectedVersion: 1, reason: "Opening and every account checked" },
    "approve",
    k,
  );
  await reviewedFinanceBookPurchase("active");
  const second = await journal();
  await postJournal(f.manager, String(second.id), key(), {
    expectedVersion: 1,
  });
  const mirrors = await db
    .prepare(
      "SELECT p.*,j.status FROM finance_book_postings p JOIN finance_journals j ON j.id=p.secondary_journal_id WHERE primary_journal_id=?",
    )
    .bind(second.id)
    .all();
  expect(mirrors.results).toHaveLength(1);
  expect(
    await db
      .prepare("SELECT 1 FROM finance_financial_events WHERE journal_id=?")
      .bind(mirrors.results[0].secondary_journal_id)
      .first(),
  ).toBeTruthy();
  const tb = await trialBalance(f.owner, undefined, s.chart);
  expect(tb.find((a) => a.category === "expense")?.balance).toBe("200.02");
  await reverseJournal(f.manager, String(second.id), key(), {
    periodId: f.period,
    entryDate: "2026-09-05",
    memo: "Correct reviewed source",
  });
  const after = await trialBalance(f.owner, undefined, s.chart);
  expect(after.find((a) => a.category === "expense")?.balance).toBe("100.01");
  expect(
    await db
      .prepare("SELECT status FROM finance_journals WHERE id=?")
      .bind(mirrors.results[0].secondary_journal_id)
      .first("status"),
  ).toBe("reversed");
});
it("rolls back the source command when a newly added account has no secondary mapping", async () => {
  const s = await setup();
  await decideBookRule(
    f.manager,
    String(s.rule.id),
    { expectedVersion: 1, reason: "All accounts checked" },
    "approve",
    key(),
  );
  const account = key();
  await db
    .prepare(
      "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'9999','New cost','expense','THB',?,?)",
    )
    .bind(account, f.chart, f.company, Date.now(), Date.now())
    .run();
  const j = await createJournal(
    f.owner,
    createJournalSchema.parse({
      chartId: f.chart,
      periodId: f.period,
      entryDate: "2026-09-04",
      currency: "THB",
      lines: [
        { accountId: account, debit: "1", credit: "0" },
        { accountId: f.accounts.payable, debit: "0", credit: "1" },
      ],
    }),
    key(),
  );
  const k = key();
  await expect(
    postJournal(f.manager, String(j.id), k, { expectedVersion: 1 }),
  ).rejects.toThrow();
  expect(
    await db
      .prepare("SELECT status FROM finance_journals WHERE id=?")
      .bind(j.id)
      .first("status"),
  ).toBe("draft");
  expect(
    await db
      .prepare("SELECT 1 FROM command_receipts WHERE command_id=?")
      .bind(k)
      .first(),
  ).toBeNull();
});
it("blocks stale openings, cross Company rules and unregistered required dimension values", async () => {
  const s = await setup(),
    j = await journal("2026-09-02");
  await postJournal(f.manager, String(j.id), key(), { expectedVersion: 1 });
  await expect(
    decideBookRule(
      f.manager,
      String(s.rule.id),
      { expectedVersion: 1, reason: "Balances changed" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "STALE_BOOK_OPENING" });
  await expect(
    decideBookRule(
      { ...f.manager, companyId: key() },
      String(s.rule.id),
      { expectedVersion: 1, reason: "Wrong company" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "BOOK_RULE_NOT_FOUND" });
  await createFinanceDimension(
    f.owner,
    {
      code: "project",
      name: "Project",
      required: true,
      values: [{ code: "alpha", name: "Alpha" }],
    },
    key(),
  );
  const bad = await journal();
  await expect(
    postJournal(f.manager, String(bad.id), key(), { expectedVersion: 1 }),
  ).rejects.toThrow();
  const good = await journal("2026-09-04", { project: "alpha" });
  await postJournal(f.manager, String(good.id), key(), { expectedVersion: 1 });
});
it("reconciles mapped secondary control accounts to the same business subledger", async () => {
  await db
    .prepare(
      "UPDATE finance_accounts SET control_kind='payable',is_control=1 WHERE id=?",
    )
    .bind(f.accounts.payable)
    .run();
  const s = await setup();
  await decideBookRule(
    f.manager,
    String(s.rule.id),
    { expectedVersion: 1, reason: "Opening checked" },
    "approve",
    key(),
  );
  const period = await db
    .prepare("SELECT * FROM finance_periods WHERE id=?")
    .bind(f.period)
    .first<Record<string, string | number | null>>();
  const result = await calculateCloseReconciliations(db, f.owner, period!);
  const mapped = s.mappings.find(
    (m) => m.primaryAccountId === f.accounts.payable,
  )!;
  expect(
    result.rows.find((r) => r.accountId === mapped.secondaryAccountId),
  ).toMatchObject({
    ledgerMinor: 0,
    sourceMinor: 0,
    differenceMinor: 0,
    source: { sourceAccountId: f.accounts.payable },
  });
});
it("preserves every separately approved book opening when a later secondary book is enabled",async()=>{
 const a=await setup();await decideBookRule(f.manager,String(a.rule.id),{expectedVersion:1,reason:"First book opening reviewed"},"approve",key());
 const j=await journal("2026-09-04");await postJournal(f.manager,String(j.id),key(),{expectedVersion:1});
 const b=await setup("2026-09-06");await decideBookRule(f.manager,String(b.rule.id),{expectedVersion:1,reason:"Second book opening reviewed"},"approve",key());
 const backdated=await journal("2026-09-05");await expect(postJournal(f.manager,String(backdated.id),key(),{expectedVersion:1})).rejects.toMatchObject({code:"BOOK_POSTING_PRECEDES_REVIEWED_OPENING"});
 expect((await trialBalance(f.owner,undefined,b.chart)).find(a=>a.category==="expense")?.balance).toBe("100.01");
 const current=await journal("2026-09-07");await postJournal(f.manager,String(current.id),key(),{expectedVersion:1});
 expect(await db.prepare("SELECT COUNT(*) value FROM finance_book_postings WHERE primary_journal_id=?").bind(current.id).first("value")).toBe(2);
});
it("rejects stale account identity at activation",async()=>{
 const s=await setup();await db.prepare("UPDATE finance_accounts SET control_kind='payable',is_control=1 WHERE id=?").bind(f.accounts.payable).run();
 await expect(decideBookRule(f.manager,String(s.rule.id),{expectedVersion:1,reason:"Changed control identity"},"approve",key())).rejects.toMatchObject({code:"BOOK_ACCOUNT_MAPPING_REQUIRED"});
});
it("preserves the original foreign transaction and functional-currency rounding in each book",async()=>{
 const s=await setup();await decideBookRule(f.manager,String(s.rule.id),{expectedVersion:1,reason:"Foreign transactions keep original carrying values"},"approve",key());
 const j=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-04",currency:"USD",fxRate:"35.5",lines:[{accountId:f.accounts.expense,debit:"1.01",credit:"0"},{accountId:f.accounts.payable,debit:"0",credit:"1.01"}]}),key());await postJournal(f.manager,String(j.id),key(),{expectedVersion:1});
 const mirror=await db.prepare("SELECT secondary_journal_id FROM finance_book_postings WHERE primary_journal_id=?").bind(j.id).first<{secondary_journal_id:string}>();expect(await db.prepare("SELECT debit_minor,credit_minor,base_debit_minor,base_credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal").bind(mirror!.secondary_journal_id).all().then(r=>r.results)).toEqual([{debit_minor:101,credit_minor:0,base_debit_minor:3586,base_credit_minor:0},{debit_minor:0,credit_minor:101,base_debit_minor:0,base_credit_minor:3586}]);
});

it("package book projection final authority revocation rolls back source, secondary book, evidence and command together",async()=>{
 const secondary=await setup("2026-09-01");await decideBookRule(f.manager,String(secondary.rule.id),{expectedVersion:1,reason:"Independent review of all retained opening accounts"},"approve",key());const draft=await journal(),prior=(await readCompanyPackageGrants(db,f.company)).find(grant=>grant.packageId==="finance");expect(prior).toBeUndefined();
 await packageGrantStore(db,{authorize:async()=>({userId:"isolated-book-expiry-review"}),obligations:async()=>{throw Error("No book expiry withdrawal witness");}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.company,packageId:"finance",packageVersion:1,state:"active",startsAt:0,endsAt:null,version:1,reason:"Native reviewed book posting purchase",evidence:"test:finance-book-final-authority"}});vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const tables=["finance_journals","finance_journal_lines","finance_book_postings","finance_financial_events","money_transaction_snapshots","assignments","command_receipts","audit_events","outbox_events"],image=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await image(),trigger=`book_expiry_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.action='finance.journal.post' AND NEW.authority_json::jsonb->>'companyId'='${f.company}') EXECUTE FUNCTION ${trigger}()`);
 try{await expect(postJournal(f.manager,String(draft.id),key(),{expectedVersion:1})).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(await image()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${trigger} ON audit_events;DROP FUNCTION ${trigger}()`);}
});
