import {financeExpansionWork} from "../../../src/server/modules/finance/public/finance-expansion-work";
import {attachmentPolicy,authorizeAttachmentTarget} from "../../../src/server/modules/files/public/attachments/policy";
import {bindRecordAttachmentSet,listAttachmentSets,attachmentContent} from "../../../src/server/modules/files/public/attachments/service";
import {enableAttachmentV2ForTest,seedAttachmentSet,attachmentBindingState} from "../../support/helpers/attachment-v2";
import { reviewedFinanceGrant } from "../../support/helpers/finance-purchase-fixture";
import {financeExpansionRecord} from "../../../src/server/modules/finance/public/finance-expansion-records";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  instrumentSchedule,
  monthlyDate,
  depreciationAmount,
} from "../../../src/server/modules/finance/public/rules/finance-instrument-policy";
import {
  prepareInstrument,
  decideInstrument,
  prepareInstrumentRun,
  decideInstrumentRun,
  reverseInstrumentRun,
  instrumentEditionSchema,
  instrumentWorkspace,
} from "../../../src/server/modules/finance/public/finance-instruments";
import { setFinanceCompanyTool } from "../../../src/server/modules/finance/public/finance-company-tools";
import {scheduleFinanceDraft,drainFinanceDueDrafts,decideFinanceDueDraft} from "../../../src/server/modules/finance/public/finance-due-drafts";
import { trialBalance } from "../../../src/server/modules/finance/public/finance";
import { financePackageObligations } from "../../../src/server/modules/finance/public/package-obligations";
import { ownerPackageObligations, withOwnerPackageObligations } from "../../../src/server/modules/commercial/public/owner-package-obligations";
import { packageGrantStore, packageGrantRevision, readCompanyPackageGrants } from "../../../src/server/modules/commercial/public/package-grants";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
afterEach(()=>vi.unstubAllEnvs());
let f: Awaited<ReturnType<typeof expenseFixture>>,
  bank: string,
  liability: string,
  interest: string,
  rou: string,
  accumulated: string,
  depreciation: string,
  august: string;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  f.owner.role = "finance_preparer";
  f.owner.assignmentId = await f.grant(f.owner, "finance_preparer");
  f.manager.role = "finance_approver";
  f.manager.assignmentId = await f.grant(f.manager, "finance_approver");
  [bank, liability, interest, rou, accumulated, depreciation, august] =
    Array.from({ length: 7 }, key);
  for (const [id, code, category, control] of [
    [bank, "BANK", "asset", "bank"],
    [liability, "DEBT", "liability", "none"],
    [interest, "INT", "expense", "none"],
    [rou, "ROU", "asset", "none"],
    [accumulated, "ACC", "asset", "none"],
    [depreciation, "DEP", "expense", "none"],
  ])
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,1,1)",
      )
      .bind(
        id,
        f.chart,
        f.company,
        code,
        code,
        category,
        control === "bank" ? 1 : 0,
        control,
      )
      .run();
  await db
    .prepare(
      "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,'August','2026-08-01','2026-08-31',1)",
    )
    .bind(august, f.company)
    .run();
  await setFinanceCompanyTool(
    f.owner,
    {
      tool: "instruments",
      enabled: true,
      expectedVersion: 0,
      reason: "Disposable Finance fixture",
    },
    key(),
  );
});
function input(kind: "lease" | "loan" = "lease") {
  return instrumentEditionSchema.parse({
    kind,
    number: key(),
    name: "Fixed monthly financing",
    chartId: f.chart,
    currency: "THB",
    startDate: "2026-08-31",
    months: 3,
    annualRateBps: 1200,
    payment: kind === "lease" ? "100.00" : "101.00",
    principal: kind === "loan" ? "300.00" : undefined,
    bankAccountId: bank,
    liabilityAccountId: liability,
    interestAccountId: interest,
    rightOfUseAccountId: rou,
    accumulatedAccountId: accumulated,
    depreciationAccountId: depreciation,
    reason: "Reviewed signed terms",
    periodId: august,
  });
}
async function bankRow(date: string, amount: number) {
  const statement = key(),
    id = key();
  await db
    .prepare(
      "INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,?,'fixture',1)",
    )
    .bind(statement, f.company, bank, statement, date, key())
    .run();
  await db
    .prepare(
      "INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES(?,?,?,?,?,?,'THB')",
    )
    .bind(id, statement, f.company, date, id, amount)
    .run();
  return id;
}
async function approved(content = input()) {
  const prepared = await prepareInstrument(f.owner, content, key());
  await decideInstrument(
    f.manager,
    String(prepared.id),
    { expectedVersion: 1, reason: "Independent terms and schedule review" },
    "approve",
    key(),
  );
  return String(prepared.id);
}
it("rolls back installment posting and reversal after final outbox faults, then retries and replays each once", async () => {
  const id = await approved(), cash = await bankRow("2026-09-30", -10000);
  const prepared = await prepareInstrumentRun(f.owner, id, {
    expectedVersion: 2, periodId: f.period, entryDate: "2026-09-30",
    bankTransactionId: cash, bankVersion: 1, reason: "Retained installment and bank source",
  }, key()), runId = String(prepared.id);
  const state = async () => ({
    agreement: await db.prepare("SELECT * FROM finance_instrument_editions WHERE id=?").bind(id).first(),
    run: await db.prepare("SELECT * FROM finance_instrument_runs WHERE id=?").bind(runId).first(),
    bank: await db.prepare("SELECT * FROM finance_bank_transactions WHERE id=?").bind(cash).first(),
    journals: (await db.prepare("SELECT * FROM finance_journals WHERE source_type='instrument_payment' AND source_id=? ORDER BY id").bind(runId).all()).results,
    lines: (await db.prepare("SELECT l.* FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.source_type='instrument_payment' AND j.source_id=? ORDER BY l.id").bind(runId).all()).results,
  });
  const receipts = async (command: string, count: number) => {
    for (const table of ["command_receipts", "audit_events", "outbox_events"])
      expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(count);
  };
  const fail = async (operation: string, run: () => Promise<unknown>) => {
    const fault = `instrument_fault_${crypto.randomUUID().replaceAll("-", "")}`;
    await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'INSTRUMENT_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='${operation}') EXECUTE FUNCTION ${fault}();`);
    try { await run(); } finally { await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`); }
  };
  const postKey = key(), postInput = { expectedVersion: 1, reason: "Independent retained bank and schedule review" }, beforePost = await state();
  await fail("finance.instrument-run.post", async () => {
    await expect(decideInstrumentRun(f.manager, runId, postInput, "post", postKey)).rejects.toThrow();
    expect(await state()).toEqual(beforePost); await receipts(postKey, 0);
  });
  const posted = await decideInstrumentRun(f.manager, runId, postInput, "post", postKey);
  expect(await decideInstrumentRun(f.manager, runId, postInput, "post", postKey)).toEqual(posted);
  await receipts(postKey, 1);
  expect((await state()).bank).toMatchObject({ match_status: "matched", matched_id: posted.journalId, version: 2 });
  expect((await state()).agreement).toMatchObject({ status: "active", version: 3 });
  const reverseKey = key(), reverseInput = { expectedVersion: 2, periodId: f.period, entryDate: "2026-09-30", reason: "Independent correction of bank association" }, beforeReverse = await state();
  await fail("finance.instrument-run.reverse", async () => {
    await expect(reverseInstrumentRun(f.manager, runId, reverseInput, reverseKey)).rejects.toThrow();
    expect(await state()).toEqual(beforeReverse); await receipts(reverseKey, 0);
  });
  const reversed = await reverseInstrumentRun(f.manager, runId, reverseInput, reverseKey);
  expect(await reverseInstrumentRun(f.manager, runId, reverseInput, reverseKey)).toEqual(reversed);
  await receipts(reverseKey, 1);
  expect((await state()).bank).toMatchObject({ match_status: "unmatched", matched_id: null, version: 3 });
  expect((await state()).run).toMatchObject({ status: "reversed", version: 3, reversal_journal_id: reversed.reversalJournalId });
  expect((await state()).agreement).toMatchObject({ status: "active", version: 4 });
  expect(Number(await db.prepare("SELECT count(*) n FROM finance_journals WHERE id=?").bind(reversed.reversalJournalId).first("n"))).toBe(1);
});
it("prepares due financing only from the retained actual bank debit and leaves it unmatched until independent posting",async()=>{
 const id=await approved(),bankId=await bankRow("2026-09-30",-10000),j=await scheduleFinanceDraft(f.owner,{kind:"instrument_payment",sourceId:id,runOn:"2026-09-30",input:{expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",bankTransactionId:bankId,bankVersion:1,reason:"Prepare first retained actual lease installment"},reason:"Due draft with imported bank evidence"},key());await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-30T12:00:00Z"));const row=await db.prepare("SELECT status,result_json FROM finance_due_drafts WHERE id=?").bind(j.id).first<{status:string;result_json:string}>();expect(row?.status).toBe("prepared");const r=JSON.parse(row!.result_json);expect(r).toMatchObject({status:"draft",result:{interest:"2.94",principal:"97.06",closing:"197.04"}});expect(await db.prepare("SELECT match_status FROM finance_bank_transactions WHERE id=?").bind(bankId).first("match_status")).toBe("unmatched");await decideInstrumentRun(f.manager,String(r.id),{expectedVersion:1,reason:"Independent installment and bank source review"},"post",key());expect(await db.prepare("SELECT match_status FROM finance_bank_transactions WHERE id=?").bind(bankId).first("match_status")).toBe("matched");
});
it("retains independent exact PV, interest, repayment, date and depreciation golden amounts", () => {
  const lease = instrumentSchedule(input());
  // 100 / 1.01 + 100 / 1.01^2 + 100 / 1.01^3 = 294.098520..., rounded once to 294.10.
  expect(lease.principal).toBe("294.10");
  expect(
    lease.rows.map((r) => [r.interest, r.principal, r.closing, r.depreciation]),
  ).toEqual([
    ["2.94", "97.06", "197.04", "98.03"],
    ["1.97", "98.03", "99.01", "98.03"],
    ["0.99", "99.01", "0.00", "98.04"],
  ]);
  expect(
    instrumentSchedule(input("loan")).rows.map((r) => [
      r.interest,
      r.payment,
      r.closing,
    ]),
  ).toEqual([
    ["3.00", "101.00", "202.00"],
    ["2.02", "101.00", "103.02"],
    ["1.03", "104.05", "0.00"],
  ]);
  expect([1, 2, 3].map((i) => monthlyDate("2024-01-31", i))).toEqual([
    "2024-02-29",
    "2024-03-31",
    "2024-04-30",
  ]);
  expect(
    depreciationAmount({
      cost: BigInt(120001),
      residual: BigInt(0),
      life: 3,
      ordinal: 1,
      accumulated: BigInt(0),
      policy: { method: "straight_line" },
    }),
  ).toBe(BigInt(40000));
  expect(
    depreciationAmount({
      cost: BigInt(120000),
      residual: BigInt(20000),
      life: 12,
      ordinal: 1,
      accumulated: BigInt(0),
      policy: { method: "declining_balance", annualRateBps: 2000 },
    }),
  ).toBe(BigInt(2000));
  expect(
    depreciationAmount({
      cost: BigInt(10001),
      residual: BigInt(1),
      life: 12,
      ordinal: 2,
      accumulated: BigInt(3333),
      policy: { method: "units_of_production", totalUnits: 3 },
      priorUnits: 1,
      units: 2,
    }),
  ).toBe(BigInt(6667));
});
it("reviews a separate first payment boundary without changing legacy schedules or nominal monthly money", async () => {
  const content = { ...input("loan"), startDate: "2026-09-01", firstDueDate: "2026-09-30", periodId: f.period,
    bankTransactionId: await bankRow("2026-09-01", 30000), bankVersion: 1 };
  expect(instrumentSchedule(content).rows.map((r) => [r.dueDate,r.payment,r.interest,r.closing])).toEqual([
    ["2026-09-30","101.00","3.00","202.00"],
    ["2026-10-30","101.00","2.02","103.02"],
    ["2026-11-30","104.05","1.03","0.00"],
  ]);
  expect(instrumentSchedule({ ...content,firstDueDate:"2027-01-31" }).rows.map((r)=>r.dueDate)).toEqual(["2027-01-31","2027-02-28","2027-03-31"]);
  for(const date of ["2026-08-31","2026-09-01"])expect(()=>instrumentEditionSchema.parse({...content,firstDueDate:date})).toThrow("first payment must fall after");
  const id=await approved(content), debit=await bankRow("2026-09-30",-10100);
  await expect(prepareInstrumentRun(f.owner,id,{expectedVersion:2,periodId:f.period,entryDate:"2026-09-29",bankTransactionId:debit,bankVersion:1,reason:"Cannot settle before the reviewed first due date"},key())).rejects.toMatchObject({code:"INSTRUMENT_NOT_DUE"});
  const run=await prepareInstrumentRun(f.owner,id,{expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",bankTransactionId:debit,bankVersion:1,reason:"Exact first payment with reviewed separate due date"},key());
  await decideInstrumentRun(f.manager,String(run.id),{expectedVersion:1,reason:"Independent boundary, cash and interest review"},"post",key());
  expect(await db.prepare("SELECT match_status FROM finance_bank_transactions WHERE id=?").bind(debit).first("match_status")).toBe("matched");
  expect(run.result).toMatchObject({dueDate:"2026-09-30",interest:"3.00",closing:"202.00"});
});
it("independently approves a lease, posts exact imported instalment, reverses and retains amendment editions", async () => {
  const id = await approved(),
    cash = await bankRow("2026-09-30", -10000),
    run = await prepareInstrumentRun(
      f.owner,
      id,
      {
        expectedVersion: 2,
        periodId: f.period,
        entryDate: "2026-09-30",
        bankTransactionId: cash,
        bankVersion: 1,
        reason: "Actual statement debit",
      },
      key(),
    );
  await expect(
    decideInstrumentRun(
      f.owner,
      String(run.id),
      { expectedVersion: 1, reason: "Self review attempt" },
      "post",
      key(),
    ),
  ).rejects.toMatchObject({ code: "PERMISSION_REQUIRED" });
  const editionDetail=await financeExpansionRecord(f.owner,'finance_instrument',id);
  expect('instrumentData' in editionDetail&&editionDetail.instrumentData?.editions[0].id).toBe(id);
  const paymentDetail=await financeExpansionRecord(f.owner,'finance_instrument_run',String(run.id));
  expect('instrumentData' in paymentDetail&&paymentDetail.instrumentData?.runs.map(row=>row.id)).toEqual([run.id]);
  await expect(financeExpansionRecord({...f.owner,companyId:key()},'finance_instrument_run',String(run.id))).rejects.toMatchObject({status:404});
  const posted = await decideInstrumentRun(
    f.manager,
    String(run.id),
    { expectedVersion: 1, reason: "Independent bank and amortization review" },
    "post",
    key(),
  );
  expect(
    (await trialBalance(f.manager, undefined, f.chart)).find(
      (a) => a.id === liability,
    )?.balance,
  ).toBe("-197.04");
  expect(
    await db
      .prepare(
        "SELECT match_status,matched_id FROM finance_bank_transactions WHERE id=?",
      )
      .bind(cash)
      .first(),
  ).toMatchObject({ match_status: "matched", matched_id: posted.journalId });
  await reverseInstrumentRun(
    f.manager,
    String(run.id),
    {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "Wrong bank statement association",
    },
    key(),
  );
  expect(
    (await trialBalance(f.manager, undefined, f.chart)).find(
      (a) => a.id === liability,
    )?.balance,
  ).toBe("-294.10");
  const fresh = await prepareInstrumentRun(
    f.owner,
    id,
    {
      expectedVersion: 4,
      periodId: f.period,
      entryDate: "2026-09-30",
      bankTransactionId: cash,
      bankVersion: 3,
      reason: "Correct source reference",
    },
    key(),
  );
  await decideInstrumentRun(
    f.manager,
    String(fresh.id),
    { expectedVersion: 1, reason: "Independent corrected payment" },
    "post",
    key(),
  );
  const original = await db
      .prepare(
        "SELECT content_json FROM finance_instrument_editions WHERE id=?",
      )
      .bind(id)
      .first<{ content_json: string }>(),
    amendment = await prepareInstrument(
      f.owner,
      {
        ...JSON.parse(original!.content_json),
        parentId: id,
        parentVersion: 5,
        startDate: "2026-09-30",
        periodId: f.period,
        months: 2,
        payment: "110.00",
        reason: "Signed amendment after settled period",
      },
      key(),
    );
  await decideInstrument(
    f.manager,
    String(amendment.id),
    { expectedVersion: 1, reason: "Independent remeasurement review" },
    "approve",
    key(),
  );
  expect(
    (await trialBalance(f.manager, undefined, f.chart)).find(
      (a) => a.id === liability,
    )?.balance,
  ).toBe("-216.74");
  const revised = await db
    .prepare("SELECT schedule_json FROM finance_instrument_editions WHERE id=?")
    .bind(amendment.id)
    .first<{ schedule_json: string }>();
  expect(
    JSON.parse(revised!.schedule_json).rows.map(
      (r: { depreciation: string }) => r.depreciation,
    ),
  ).toEqual(["107.88", "107.89"]);
  expect(
    await db
      .prepare("SELECT status FROM finance_instrument_editions WHERE id=?")
      .bind(id)
      .first(),
  ).toMatchObject({ status: "superseded" });
  await expect(
    db
      .prepare(
        "UPDATE finance_instrument_editions SET schedule_json='{}' WHERE id=?",
      )
      .bind(id)
      .run(),
  ).rejects.toThrow("INSTRUMENT_EDITION_IMMUTABLE");
});
it("requires funded loan bank evidence, deduplicates payment and closes by actual/365 payoff", async () => {
  const funding = await bankRow("2026-08-31", 30000),
    content = { ...input("loan"), bankTransactionId: funding, bankVersion: 1 },
    id = await approved(content),
    cash = await bankRow("2026-09-30", -10100),
    cmd = key(),
    definition = {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      bankTransactionId: cash,
      bankVersion: 1,
      reason: "Exact loan instalment",
    };
  const run = await prepareInstrumentRun(f.owner, id, definition, cmd);
  expect(await prepareInstrumentRun(f.owner, id, definition, cmd)).toEqual(run);
  await decideInstrumentRun(
    f.manager,
    String(run.id),
    {
      expectedVersion: 1,
      reason: "Independent principal interest and cash review",
    },
    "post",
    key(),
  );
  const october = key();
  await db
    .prepare(
      "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,'October','2026-10-01','2026-10-31',1)",
    )
    .bind(october, f.company)
    .run();
  // 202.00 × 12% × 15/365 = 0.996164..., rounded to 1.00; final cash 203.00.
  const payoffCash = await bankRow("2026-10-15", -20300),
    payoff = await prepareInstrumentRun(
      f.owner,
      id,
      {
        kind: "payoff",
        expectedVersion: 3,
        periodId: october,
        entryDate: "2026-10-15",
        bankTransactionId: payoffCash,
        bankVersion: 1,
        reason: "Payoff between instalment dates",
      },
      key(),
    );
  expect(payoff.result).toMatchObject({
    principal: "202.00",
    interest: "1.00",
    payment: "203.00",
    closing: "0.00",
  });
  await decideInstrumentRun(
    f.manager,
    String(payoff.id),
    {
      expectedVersion: 1,
      reason: "Independent payoff and actual cash evidence",
    },
    "post",
    key(),
  );
  expect(
    await db
      .prepare("SELECT status FROM finance_instrument_editions WHERE id=?")
      .bind(id)
      .first(),
  ).toMatchObject({ status: "closed" });
  expect(
    (await trialBalance(f.manager, undefined, f.chart)).find(
      (a) => a.id === liability,
    )?.balance,
  ).toBe("0.00");
});
it("rejects self approval, stale mappings, reused cash and cross Company authority", async () => {
  const content = input(),
    r = await prepareInstrument(f.owner, content, key());
  f.owner.role = "finance_approver";
  f.owner.assignmentId = await f.grant(f.owner, "finance_approver");
  await expect(
    decideInstrument(
      f.owner,
      String(r.id),
      { expectedVersion: 1, reason: "Same person different role" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await db
    .prepare("UPDATE finance_accounts SET version=version+1 WHERE id=?")
    .bind(interest)
    .run();
  await expect(
    decideInstrument(
      f.manager,
      String(r.id),
      { expectedVersion: 1, reason: "Changed evidence" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "INSTRUMENT_SOURCE_CHANGED" });
  const other = await expenseFixture();
  other.owner.role = "finance_approver";
  other.owner.assignmentId = await other.grant(other.owner, "finance_approver");
  await expect(
    decideInstrument(
      other.owner,
      String(r.id),
      { expectedVersion: 1, reason: "Wrong Company" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "INSTRUMENT_NOT_FOUND" });
});

it("requires the Advanced add-on for direct records and queued legacy financing without consuming a candidate",async()=>{
 const id=await approved(),bankId=await bankRow("2026-09-30",-10000),schedule=await scheduleFinanceDraft(f.owner,{kind:"instrument_payment",sourceId:id,runOn:"2026-09-30",input:{expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",bankTransactionId:bankId,bankVersion:1,reason:"Retained actual lease installment"},reason:"Retained bank evidence"},key());
 await reviewedFinanceGrant(db,f.company,"finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 await expect(prepareInstrument(f.owner,input(),key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await expect(instrumentWorkspace(f.owner)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await expect(financeExpansionRecord(f.owner,"finance_instrument",id)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 expect(await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-30T12:00:00Z"))).toBe(0);
 expect(await db.prepare("SELECT status,attempts FROM finance_due_drafts WHERE id=?").bind(schedule.id).first()).toEqual({status:"scheduled",attempts:0});
 expect(await db.prepare("SELECT id FROM finance_instrument_runs WHERE instrument_id=?").bind(id).first()).toBeNull();
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","shadow");expect(await instrumentWorkspace(f.owner)).toBeDefined();
});
it("settles retained Advanced financing and replays exact schedule receipts while the add-on drains",async()=>{
 await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const command=key(),terms=input(),prepared=await prepareInstrument(f.owner,terms,command),id=String(prepared.id);
 await decideInstrument(f.manager,id,{expectedVersion:1,reason:"Independent retained terms review"},"approve",key());
 const bankId=await bankRow("2026-09-30",-10000),scheduleKey=key(),scheduleInput={kind:"instrument_payment" as const,sourceId:id,runOn:"2026-09-30",input:{expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",bankTransactionId:bankId,bankVersion:1,reason:"Retained actual lease installment"},reason:"Retained bank evidence"},schedule=await scheduleFinanceDraft(f.owner,scheduleInput,scheduleKey);
 await reviewedFinanceGrant(db,f.company,"finance-advanced","draining");
 expect(await prepareInstrument(f.owner,terms,command)).toEqual(prepared);expect(await scheduleFinanceDraft(f.owner,scheduleInput,scheduleKey)).toEqual(schedule);
 await expect(prepareInstrument(f.owner,input(),key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});
 expect(await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-30T12:00:00Z"))).toBe(1);
 const row=await db.prepare("SELECT result_json FROM finance_due_drafts WHERE id=?").bind(schedule.id).first<{result_json:string}>(),runId=String(JSON.parse(row!.result_json).id);
 expect(await financeExpansionRecord(f.owner,"finance_instrument_run",runId)).toBeDefined();
 await decideInstrumentRun(f.manager,runId,{expectedVersion:1,reason:"Independent original bank evidence review"},"post",key());
 expect(await db.prepare("SELECT match_status FROM finance_bank_transactions WHERE id=?").bind(bankId).first("match_status")).toBe("matched");
 const reversalKey=key(),reversalInput={expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",reason:"Reverse retained installment with bank evidence"},reversed=await reverseInstrumentRun(f.manager,runId,reversalInput,reversalKey);
 expect(await reverseInstrumentRun(f.manager,runId,reversalInput,reversalKey)).toEqual(reversed);
 expect(await db.prepare("SELECT match_status FROM finance_bank_transactions WHERE id=?").bind(bankId).first("match_status")).toBe("unmatched");
});
it("rolls back the owned instrument draft when authority changes after queue completion, then retries once",async()=>{
 await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const id=await approved(),bankId=await bankRow("2026-09-30",-10000),schedule=await scheduleFinanceDraft(f.owner,{kind:"instrument_payment",sourceId:id,runOn:"2026-09-30",input:{expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",bankTransactionId:bankId,bankVersion:1,reason:"Retained original installment"},reason:"Retained bank evidence"},key()),fault=`instrument_queue_${key().replaceAll('-','')}`;
 const tables=["finance_instrument_editions","finance_instrument_runs","finance_journals","finance_journal_lines","finance_financial_events","money_transaction_snapshots","assignments","command_receipts","audit_events","outbox_events"],image=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await image();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}';RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER UPDATE ON finance_due_drafts FOR EACH ROW WHEN (NEW.id='${schedule.id}' AND NEW.status='prepared') EXECUTE FUNCTION ${fault}()`);
 try{expect(await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-30T12:00:00Z"))).toBe(0);expect(await image()).toEqual(before);expect(await db.prepare("SELECT status,attempts,last_code FROM finance_due_drafts WHERE id=?").bind(schedule.id).first()).toEqual({status:"blocked",attempts:1,last_code:"FINANCE_ACCESS_CHANGED"});}finally{await db.exec(`DROP TRIGGER ${fault} ON finance_due_drafts;DROP FUNCTION ${fault}()`);}
 await decideFinanceDueDraft(f.owner,String(schedule.id),{expectedVersion:2,reason:"Retry with unchanged current authority"},"retry",key());
 expect(await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-30T12:00:00Z"))).toBe(1);
 expect(Number(await db.prepare("SELECT count(*) n FROM command_receipts WHERE command_id=?").bind(`${schedule.id}:draft`).first("n"))).toBe(1);
});

it("Finance source purchase hides legacy Advanced Work and file paths, then binds retained financing evidence while draining",async()=>{
 enableAttachmentV2ForTest();const draft=await prepareInstrument(f.owner,input(),key()),id=String(draft.id),files=await seedAttachmentSet(db,f.owner,{targetType:"finance_instrument",targetId:id,componentKind:"record",purposeKey:"finance_support",expectedTargetVersion:1});
 const objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(files.sourceId).first("object_key"));await env.H001_FILES.put(objectKey,"%PDF-1.7",{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(files.sourceId).run();
 const held=await db.prepare("SELECT checksum_sha256,legal_hold,revoked_at FROM attachments WHERE id=?").bind(files.sourceId).first();
 await reviewedFinanceGrant(db,f.company,"finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 expect((await financeExpansionWork(f.manager)).some(item=>item.id===id)).toBe(false);
 await expect(authorizeAttachmentTarget(db,f.owner,attachmentPolicy("finance_instrument","finance_support","record"),id,"read")).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await expect(listAttachmentSets(db,f.owner,{targetType:"finance_instrument",targetId:id})).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await expect(bindRecordAttachmentSet(db,f.owner,files.setId,1,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await expect(attachmentContent(db,env.H001_FILES,f.owner,files.linkId,"original")).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 expect(await attachmentBindingState(db,files.setId)).toMatchObject({set_state:"draft",link_state:"draft"});
 await reviewedFinanceGrant(db,f.company,"finance-advanced");await reviewedFinanceGrant(db,f.company,"finance-advanced","draining");
 expect((await financeExpansionWork(f.manager)).find(item=>item.id===id)).toMatchObject({canAct:true,href:"/finance/instruments"});
 await bindRecordAttachmentSet(db,f.owner,files.setId,1,key());expect(await attachmentBindingState(db,files.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_id:id});
 expect((await listAttachmentSets(db,f.owner,{targetType:"finance_instrument",targetId:id})).length).toBe(1);
 expect(await (await attachmentContent(db,env.H001_FILES,f.owner,files.linkId,"original")).object.text()).toBe("%PDF-1.7");
 expect(await db.prepare("SELECT checksum_sha256,legal_hold,revoked_at FROM attachments WHERE id=?").bind(files.sourceId).first()).toEqual(held);
});
it("Finance source purchase rolls file binding and its receipt back after deferred assignment revocation",async()=>{
 enableAttachmentV2ForTest();await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const draft=await prepareInstrument(f.owner,input(),key()),id=String(draft.id),files=await seedAttachmentSet(db,f.owner,{targetType:"finance_instrument",targetId:id,componentKind:"record",purposeKey:"finance_support",expectedTargetVersion:1}),command=key(),fault=`finance_file_${key().replaceAll('-','')}`;
 const tables=["finance_instrument_editions","attachment_sets","attachment_links","attachment_events","assignments","command_receipts","audit_events","outbox_events"],image=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await image();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(bindRecordAttachmentSet(db,f.owner,files.setId,1,command)).rejects.toThrow();expect(await image()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
 await bindRecordAttachmentSet(db,f.owner,files.setId,1,command);expect(await attachmentBindingState(db,files.setId)).toMatchObject({set_state:"bound",link_state:"bound"});expect(Number(await db.prepare("SELECT count(*) n FROM command_receipts WHERE command_id=?").bind(command).first("n"))).toBe(1);
});

it("Finance Advanced withdrawal needs independent retained-terms rejection and preserves its Legal-held files and history", async () => {
  enableAttachmentV2ForTest(); await reviewedFinanceGrant(db, f.company, "finance"); await reviewedFinanceGrant(db, f.company, "finance-advanced");
  vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT", "enforced");
  const draft = await prepareInstrument(f.owner, input(), key()), id = String(draft.id);
  const files = await seedAttachmentSet(db, f.owner, { targetType: "finance_instrument", targetId: id, componentKind: "record", purposeKey: "finance_support", expectedTargetVersion: 1 });
  const objectKey = String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(files.sourceId).first("object_key"));
  await env.H001_FILES.put(objectKey, "%PDF-1.7", { httpMetadata: { contentType: "application/pdf" } });
  await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(files.sourceId).run();
  await bindRecordAttachmentSet(db, f.owner, files.setId, 1, key());
  const file = await db.prepare("SELECT * FROM attachments WHERE id=?").bind(files.sourceId).first();
  await reviewedFinanceGrant(db, f.company, "finance-advanced", "draining");
  const scope = () => ({ companyId: f.company, packageId: "finance-advanced", packageVersion: 1, moduleId: "finance", now: Date.now() });
  const observation = () => withOwnerPackageObligations(db, scope(), tx => financePackageObligations(db, tx, scope()));
  expect((await observation()).outstanding).toBe(1);
  const before = await readCompanyPackageGrants(db, f.company), current = before.find(grant => grant.packageId === "finance-advanced")!;
  const request = { companyId: f.company, expectedRevision: packageGrantRevision(f.company, before), idempotencyKey: key(), changes: [{ expectedVersion: current.version,
    grant: { ...current, state: "inactive" as const, version: current.version + 1, reason: "Independent financing rejection retains held evidence", evidence: "test:finance-advanced-owner-withdrawal" } }] };
  const store = packageGrantStore(db, { authorize: async () => ({ userId: "isolated-finance-advanced-lifecycle" }), obligations: (tx, company, packageId) => ownerPackageObligations(db, tx, company, packageId) });
  const apply = () => withOwnerPackageObligations(db, scope(), tx => store.batch.applyIn(tx, request, async () => {}));
  await expect(apply()).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  const expectedVersion = Number(await db.prepare("SELECT version FROM finance_instrument_editions WHERE id=?").bind(id).first("version"));
  const self = { ...f.owner, role: "finance_approver", assignmentId: await f.grant(f.owner, "finance_approver") };
  await expect(decideInstrument(self, id, { expectedVersion, reason: "The preparer cannot close the retained terms alone" }, "reject", key())).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await decideInstrument(f.manager, id, { expectedVersion, reason: "Independent review rejected the unactivated agreement" }, "reject", key());
  expect((await observation()).outstanding).toBe(0);
  const retained = await db.prepare("SELECT * FROM finance_instrument_editions WHERE id=?").bind(id).first(), result = await apply();
  expect(await apply()).toEqual(result);
  expect(await db.prepare("SELECT * FROM finance_instrument_editions WHERE id=?").bind(id).first()).toEqual(retained);
  expect(await db.prepare("SELECT * FROM attachments WHERE id=?").bind(files.sourceId).first()).toEqual(file);
  expect(await (await attachmentContent(db, env.H001_FILES, f.owner, files.linkId, "original")).object.text()).toBe("%PDF-1.7");
  expect((await listAttachmentSets(db, f.owner, { targetType: "finance_instrument", targetId: id })).length).toBe(1);
});
