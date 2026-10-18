import {financePlanningWorkspace,financePlanRecord} from "../../../src/server/modules/finance/public/finance-planning";
import {afterEach} from "vitest";
import {reviewedFinanceGrant} from "../../support/helpers/finance-purchase-fixture";
import { readWorkSnapshot } from "../../../src/server/modules/work_projection/public/work-items";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { setFinanceCompanyTool } from "../../../src/server/modules/finance/public/finance-company-tools";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  prepareBudgetEdition,
  budgetEditionSchema,
  importBudgetCsv,
  decideFinancePlan,
  budgetActuals,
  budgetSourceRecords,
  financialReportSourceRecords,
  prepareForecastEdition,
  forecastEditionSchema,
  prepareFinancialReport,
  financialReportSchema,
  snapshotFinancialReport,
  exportFinancialReport,
} from "../../../src/server/modules/finance/public/finance-planning";
import {
  createJournal,
  createJournalSchema,
  postJournal,
} from "../../../src/server/modules/finance/public/finance";
import { financeCsvRows,financeCsv } from "../../../src/lib/finance/finance-csv";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
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
  await setFinanceCompanyTool(
    f.owner,
    {
      tool: "planning",
      enabled: true,
      expectedVersion: 0,
      reason: "Disposable engineering fixture",
    },
    key(),
  );
});
async function approve(id: string) {
  return decideFinancePlan(
    f.manager,
    id,
    { expectedVersion: 1, reason: "Accounts and source evidence reviewed" },
    "approve",
    key(),
  );
}
async function post() {
  const j = await createJournal(
    f.owner,
    createJournalSchema.parse({
      chartId: f.chart,
      periodId: f.period,
      entryDate: "2026-09-04",
      currency: "THB",
      lines: [
        { accountId: f.accounts.expense, debit: "80.01", credit: "0" },
        { accountId: f.accounts.cash, debit: "0", credit: "80.01" },
      ],
    }),
    key(),
  );
  await postJournal(f.manager, String(j.id), key(), { expectedVersion: 1 });
  return j;
}
it('rolls immutable report snapshot and issued CSV bytes back after final outbox failure, then retries and replays once',async()=>{
 await post();const definition=await prepareFinancialReport(f.owner,financialReportSchema.parse({chartId:f.chart,name:'Immutable export rollback',rows:[{key:'expense',label:'=1+1',kind:'accounts',accountIds:[f.accounts.expense]}]}),key());await approve(String(definition.id));
 const id=String(definition.id),command=key(),input={startDate:'2026-09-01',endDate:'2026-09-30'},fault=`planning_snapshot_fault_${crypto.randomUUID().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'REPORT_SNAPSHOT_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.report.snapshot') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(snapshotFinancialReport(f.owner,id,input,command)).rejects.toThrow();
  expect(Number(await db.prepare('SELECT count(*) n FROM finance_report_snapshots WHERE definition_id=?').bind(id).first('n'))).toBe(0);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const saved=await snapshotFinancialReport(f.owner,id,input,command);expect(await snapshotFinancialReport(f.owner,id,input,command)).toEqual(saved);
 const bytes=await exportFinancialReport(f.owner,String(saved.id));expect(bytes.content).toContain('"\'=1+1","80.01","money"');
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_report_snapshots WHERE definition_id=?').bind(id).first('n'))).toBe(1);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(1);
});
it('rolls independent plan approval back after final outbox failure, then retries and replays once',async()=>{
 const edition=await prepareBudgetEdition(f.owner,budgetEditionSchema.parse({chartId:f.chart,name:'Approved edition rollback',year:2026,lines:[{accountId:f.accounts.expense,month:'2026-09',amount:'100.00'}]}),key()),id=String(edition.id),command=key(),input={expectedVersion:1,reason:'Independent reviewed budget account evidence'},fault=`plan_approval_fault_${crypto.randomUUID().replaceAll('-','')}`,before=await db.prepare('SELECT * FROM finance_plan_editions WHERE id=?').bind(id).first();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'PLAN_APPROVAL_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.plan.approve') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(decideFinancePlan(f.manager,id,input,'approve',command)).rejects.toThrow();
  expect(await db.prepare('SELECT * FROM finance_plan_editions WHERE id=?').bind(id).first()).toEqual(before);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const approved=await decideFinancePlan(f.manager,id,input,'approve',command);expect(await decideFinancePlan(f.manager,id,input,'approve',command)).toEqual(approved);expect(approved).toMatchObject({status:'approved',version:2});
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(1);
});
it("keeps native Finance reviews in the combined Today queue with independent authority, and removes a reviewed plan", async () => {
  const budget=await prepareBudgetEdition(f.owner,budgetEditionSchema.parse({chartId:f.chart,name:"Today review fixture",year:2026,lines:[{accountId:f.accounts.expense,month:"2026-09",amount:"100.01"}]}),key());
  const reviewer=await readWorkSnapshot(f.manager),preparer=await readWorkSnapshot(f.owner);
  expect(reviewer.unavailableSources).not.toContain("Finance expansion reviews");
  expect(preparer.unavailableSources).not.toContain("Finance expansion reviews");
  expect(reviewer.items.find(item=>item.type==="finance_review"&&item.id===budget.id)).toMatchObject({queue:"todo",canAct:true,actionKind:"approval",href:`/finance/reports?chart=${f.chart}`});
  expect(preparer.items.find(item=>item.type==="finance_review"&&item.id===budget.id)).toMatchObject({queue:"waiting",canAct:false,nextAction:"Waiting for an independent Finance approver"});
  await approve(String(budget.id));
  expect((await readWorkSnapshot(f.manager)).items.some(item=>item.type==="finance_review"&&item.id===budget.id)).toBe(false);
});
it("revalidates approved planning sources and freezes spreadsheet-safe export bytes",async()=>{
 const budget=await prepareBudgetEdition(f.owner,budgetEditionSchema.parse({chartId:f.chart,name:"Stale account",year:2026,lines:[{accountId:f.accounts.expense,month:"2026-09",amount:"100.00"}]}),key());
 await db.prepare("UPDATE finance_accounts SET status='inactive',version=version+1 WHERE id=?").bind(f.accounts.expense).run();
 await expect(approve(String(budget.id))).rejects.toMatchObject({code:"ACCOUNT_INVALID"});
 expect(await db.prepare("SELECT status FROM finance_plan_editions WHERE id=?").bind(budget.id).first("status")).toBe("draft");
 await db.prepare("UPDATE finance_accounts SET status='active',version=version+1 WHERE id=?").bind(f.accounts.expense).run();
 const report=await prepareFinancialReport(f.owner,financialReportSchema.parse({chartId:f.chart,name:"Formula-safe report",rows:[{key:"expense",label:"=1+1",kind:"accounts",accountIds:[f.accounts.expense],basis:"period",sign:"debit"}]}),key());
 await approve(String(report.id));
 const snapshot=await snapshotFinancialReport(f.owner,String(report.id),{startDate:"2026-09-01",endDate:"2026-09-30"},key()),exported=await exportFinancialReport(f.owner,String(snapshot.id));
 expect(exported.content).toContain('"\'=1+1","0.00","money"');
 expect(await db.prepare("SELECT export_content FROM finance_report_snapshots WHERE id=?").bind(snapshot.id).first("export_content")).toBe(exported.content);
 expect(financeCsv(["Label","Amount"],[["-SUM(1,2)","-1.01"]])).toBe('"Label","Amount"\r\n"\'-SUM(1,2)","-1.01"\r\n');
 expect(()=>financialReportSchema.parse({chartId:f.chart,name:"Invalid units",rows:[{key:"a",label:"A",kind:"accounts",accountIds:[f.accounts.expense],basis:"period",sign:"debit"},{key:"p",label:"Percent",kind:"ratio",references:["a","a"]},{key:"invalid",label:"Wrong unit",kind:"sum",references:["a","p"]}]})).toThrow("money rows");
});
it("retains reviewed budget editions and derives actuals from posted journals", async () => {
  const input = budgetEditionSchema.parse({
      chartId: f.chart,
      name: "FY26",
      year: 2026,
      lines: [
        { accountId: f.accounts.expense, month: "2026-09", amount: "100.00" },
      ],
    }),
    k = key();
  const r = await prepareBudgetEdition(f.owner, input, k);
  expect(await prepareBudgetEdition(f.owner, input, k)).toEqual(r);
  const self = {
    ...f.owner,
    role: "finance_approver",
    assignmentId: await f.grant(f.owner, "finance_approver"),
  };
  await expect(
    decideFinancePlan(
      self,
      String(r.id),
      { expectedVersion: 1, reason: "Self review refused" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await approve(String(r.id));
  await post();
  expect((await budgetActuals(f.owner, String(r.id))).lines[0]).toMatchObject({
    budget: "100.00",
    actual: "80.01",
    variance: "-19.99",
  });
  const budgetSources=await budgetSourceRecords(f.owner,String(r.id),0);
  expect(budgetSources.row.reported_total).toBe('80.01');expect(budgetSources.rows).toHaveLength(1);
  await expect(budgetSourceRecords({...f.owner,companyId:key()},String(r.id),0)).rejects.toMatchObject({status:404});
  const amended = await prepareBudgetEdition(
    f.owner,
    budgetEditionSchema.parse({
      ...input,
      parentId: r.id,
      expectedVersion: 2,
      lines: [
        { accountId: f.accounts.expense, month: "2026-09", amount: "90.00" },
      ],
    }),
    key(),
  );
  expect(amended).toMatchObject({ edition: 2, status: "draft" });
  await approve(String(amended.id));
  expect((await budgetActuals(f.owner, String(r.id))).lines[0].budget).toBe(
    "100.00",
  );
  await expect(
    db
      .prepare("UPDATE finance_plan_editions SET content_json='{}' WHERE id=?")
      .bind(r.id)
      .run(),
  ).rejects.toThrow();
  await expect(
    budgetActuals({ ...f.owner, companyId: key() }, String(r.id)),
  ).rejects.toMatchObject({ code: "APPROVED_PLAN_NOT_FOUND" });
});
it("imports quoted CSV and generates exactly twelve months with explicit scenarios", async () => {
  expect(financeCsvRows('name,amount\r\n"A, \"\"B\"\"",10\r\n')).toEqual([
    { name: 'A, "B"', amount: "10" },
  ]);
  const r = await importBudgetCsv(
    f.owner,
    {
      chartId: f.chart,
      name: "CSV",
      year: 2026,
      csv: "account,month,amount\r\nexpense,2026-09,10.01\r\n",
    },
    key(),
  );
  await approve(String(r.id));
  expect((await budgetActuals(f.owner, String(r.id))).lines[0].budget).toBe(
    "10.01",
  );
  const forecast = await prepareForecastEdition(
    f.owner,
    forecastEditionSchema.parse({
      chartId: f.chart,
      name: "Next 12 months",
      startMonth: "2026-10",
      assumptions: "Stable service volumes",
      lines: [
        {
          accountId: f.accounts.revenue,
          monthlyAmount: "100.01",
          goodBps: 1000,
          badBps: -1000,
        },
      ],
    }),
    key(),
  );
  await approve(String(forecast.id));
  const row = await db
    .prepare("SELECT content_json FROM finance_plan_editions WHERE id=?")
    .bind(forecast.id)
    .first<{ content_json: string }>();
  const c = JSON.parse(row!.content_json);
  expect(c.months).toHaveLength(12);
  expect(c.months[0]).toMatchObject({
    month: "2026-10",
    baseMinor: 10001,
    goodMinor: 11001,
    badMinor: 9001,
  });
  expect(c.months[11].month).toBe("2027-09");
});
it("freezes report sources and evaluates only bounded earlier-row formulas", async () => {
  const r = await prepareFinancialReport(
    f.owner,
    financialReportSchema.parse({
      chartId: f.chart,
      name: "Cost report",
      rows: [
        {
          key: "cost",
          label: "Cost",
          kind: "accounts",
          accountIds: [f.accounts.expense],
        },
        { key: "same", label: "Equal cost", kind: "sum", references: ["cost"] },
        {
          key: "margin",
          label: "Ratio",
          kind: "ratio",
          references: ["cost", "same"],
        },
      ],
    }),
    key(),
  );
  await approve(String(r.id));
  const j = await post();
  const report = await snapshotFinancialReport(
    f.owner,
    String(r.id),
    { startDate: "2026-09-01", endDate: "2026-09-30" },
    key(),
  );
  expect(report.rows).toMatchObject([
    { value: "80.01", journalIds: [j.id] },
    { value: "80.01" },
    { value: "100.00", unit: "percent" },
  ]);
  const sourceRows=await financialReportSourceRecords(f.owner,String(report.id),{key:report.rows[0].key});
  expect(sourceRows.rows.map(row=>row.journal_id)).toEqual([j.id]);expect(sourceRows.row.reported_total).toBe('80.01');
  const csv = await exportFinancialReport(f.owner, String(report.id));
  await post();
  await expect(financialReportSourceRecords(f.owner,String(report.id),{key:report.rows[0].key})).rejects.toMatchObject({code:'REPORT_SOURCES_CHANGED'});
  expect(await exportFinancialReport(f.owner, String(report.id))).toEqual(csv);
  expect(csv.content).toContain('"Cost","80.01","money"');
  expect(
    financialReportSchema.safeParse({
      chartId: f.chart,
      name: "Cycle",
      rows: [{ key: "a", label: "A", kind: "sum", references: ["a"] }],
    }).success,
  ).toBe(false);
});
it("reconciles a reviewed cash-flow layout and retains cash-flow amounts in its export",async()=>{
 await db.prepare("UPDATE finance_accounts SET control_kind='bank',is_control=1 WHERE id=?").bind(f.accounts.cash).run();
 const p=await prepareFinancialReport(f.owner,financialReportSchema.parse({chartId:f.chart,name:"Cash-flow statement",rows:[{key:"cost",label:"Cost",kind:"accounts",accountIds:[f.accounts.expense]}],cashFlow:{bankAccountIds:[f.accounts.cash],classifications:[{accountId:f.accounts.expense,kind:"operating"}]}}),key());await approve(String(p.id));await post();
 const report=await snapshotFinancialReport(f.owner,String(p.id),{startDate:"2026-09-01",endDate:"2026-09-30"},key());
 expect(report).toMatchObject({cashFlow:{opening:"0.00",operating:"-80.01",investing:"0.00",financing:"0.00",change:"-80.01",closing:"-80.01"}});
 expect((await exportFinancialReport(f.owner,String(report.id))).content).toContain('"Cash flow: closing","-80.01","money"');
});

afterEach(()=>vi.unstubAllEnvs());

it("unpurchased planning refuses private records while draining Finance reviews and exports the retained definition",async()=>{
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const data=financialReportSchema.parse({chartId:f.chart,name:"Retained source-only report",rows:[{key:"expense",label:"Reviewed expense",kind:"accounts",accountIds:[f.accounts.expense],basis:"period",sign:"debit"}]}),command=key();
 await expect(prepareFinancialReport(f.owner,data,command)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(financePlanningWorkspace(f.owner,f.chart)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(financePlanRecord(f.owner,"absent")).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(exportFinancialReport(f.owner,"absent")).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await reviewedFinanceGrant(db,f.company,"finance");await post();const report=await prepareFinancialReport(f.owner,data,command);await reviewedFinanceGrant(db,f.company,"finance","draining");expect(await prepareFinancialReport(f.owner,data,command)).toEqual(report);await expect(prepareFinancialReport(f.owner,data,key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});await approve(String(report.id));
 const snapshotKey=key(),range={startDate:"2026-09-01",endDate:"2026-09-30"},snapshot=await snapshotFinancialReport(f.owner,String(report.id),range,snapshotKey);expect(await snapshotFinancialReport(f.owner,String(report.id),range,snapshotKey)).toEqual(snapshot);expect(snapshot.rows).toEqual([expect.objectContaining({key:"expense",minor:8001})]);const exported=await exportFinancialReport(f.owner,String(snapshot.id));expect(exported.content).toContain("80.01");expect(await exportFinancialReport(f.owner,String(snapshot.id))).toEqual(exported);
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();await expect(exportFinancialReport(f.owner,String(snapshot.id))).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});
});
it("report final preparer revocation rolls back immutable result and issued CSV bytes before the same-key retry",async()=>{
 await reviewedFinanceGrant(db,f.company,"finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const report=await prepareFinancialReport(f.owner,financialReportSchema.parse({chartId:f.chart,name:"Exact report source fence",rows:[{key:"expense",label:"Expense",kind:"accounts",accountIds:[f.accounts.expense],basis:"period",sign:"debit"}]}),key());await approve(String(report.id));const command=key(),range={startDate:"2026-09-01",endDate:"2026-09-30"},fault=`report_purchase_${key().replaceAll("-","")}`;
 const snapshot=async()=>Promise.all(["finance_plan_editions","finance_report_snapshots","command_receipts","audit_events","outbox_events"].map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(snapshotFinancialReport(f.owner,String(report.id),range,command)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(await snapshot()).toEqual(before);}
 finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}()`);}
 const result=await snapshotFinancialReport(f.owner,String(report.id),range,command);expect(await snapshotFinancialReport(f.owner,String(report.id),range,command)).toEqual(result);expect((await exportFinancialReport(f.owner,String(result.id))).content).toContain("Expense");
});
