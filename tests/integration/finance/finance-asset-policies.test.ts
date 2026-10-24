import {afterEach} from "vitest";
import {reviewedFinanceGrant} from "../../support/helpers/finance-purchase-fixture";
import { financeExpansionRecord } from "@erpx/server-finance/public/finance-expansion-records";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  createAsset,
  activateAsset,
  assetSchema,
  assetActivationSchema,
  assetDepreciationSchema,
  depreciateAsset,
  prepareAssetDisposal,
  decideAssetDisposal,
  assetOptions,
} from "@erpx/server-finance/public/finance-assets";
import {
  prepareAssetPolicy,
  decideAssetPolicy,
  prepareAssetPolicyRun,
  decideAssetPolicyRun,
  reverseAssetPolicyRun,
} from "@erpx/server-finance/public/finance-asset-policies";
import {
  prepareBookRule,
  decideBookRule,
  bookRuleSchema,
  createFinanceDimension,
} from "@erpx/server-finance/public/finance-books";
import { setFinanceCompanyTool } from "@erpx/server-finance/public/finance-company-tools";
import {
  scheduleFinanceDraft,
  drainFinanceDueDrafts,
} from "@erpx/server-finance/public/finance-due-drafts";
import {trialBalance} from '@erpx/server-finance/public/finance';
import { financeExpansionClosePending } from "@erpx/server-finance/public/finance-expansion-close";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  cost: string,
  accumulated: string,
  expense: string,
  offset: string;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  f.owner.role = "finance_preparer";
  f.owner.assignmentId = await f.grant(f.owner, "finance_preparer");
  f.manager.role = "finance_approver";
  f.manager.assignmentId = await f.grant(f.manager, "finance_approver");
  [cost, accumulated, expense, offset] = Array.from({ length: 4 }, key);
  for (const [id, code, category] of [
    [cost, "COST", "asset"],
    [accumulated, "ACC", "asset"],
    [expense, "DEP", "expense"],
    [offset, "FUNDED", "equity"],
  ])
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)",
      )
      .bind(id, f.chart, f.company, code, code, category)
      .run();
  await setFinanceCompanyTool(
    f.owner,
    {
      tool: "asset_policies",
      enabled: true,
      expectedVersion: 0,
      reason: "Disposable independent accounting fixture",
    },
    key(),
  );
});
async function recognized(life = 3) {
  const a = await createAsset(
    f.owner,
    assetSchema.parse({
      assetNumber: key(),
      name: "Machine",
      accountId: cost,
      depreciationAccountId: accumulated,
      depreciationExpenseAccountId: expense,
      currency: "THB",
      cost: "1200.01",
      usefulLifeMonths: life,
      inServiceDate: "2026-09-01",
    }),
    key(),
  );
  await activateAsset(
    f.manager,
    String(a.id),
    {
      expectedVersion: 1,
      periodId: f.period,
      entryDate: "2026-09-01",
      offsetAccountId: offset,
      evidenceRef: "Independent acquisition evidence",
    },
    key(),
  );
  return String(a.id);
}
it("retains required Project evidence for acquisition and legacy depreciation in every enabled book, refusing incomplete activation atomically", async () => {
  const b=await secondary();
  const options=await assetOptions(f.owner);
  expect(options.accounts.map(a=>a.id)).toContain(cost);
  expect(options.accounts.some(a=>a.chart_id===b.chart)).toBe(false);
  const mappedCost=b.maps.find(m=>m.primaryAccountId===cost)!.secondaryAccountId;
  await expect(createAsset(f.owner,assetSchema.parse({assetNumber:key(),name:"Wrong secondary source",accountId:mappedCost,currency:"THB",cost:"1200.01",usefulLifeMonths:3,inServiceDate:"2026-09-01"}),key())).rejects.toMatchObject({code:"ASSET_PRIMARY_BOOK_REQUIRED"});
  await createFinanceDimension(f.owner,{code:"project",name:"Project",required:true,values:[{code:"ALPHA",name:"Project Alpha"}]},key());
  const asset=await createAsset(f.owner,assetSchema.parse({assetNumber:key(),name:"Project machine",accountId:cost,depreciationAccountId:accumulated,depreciationExpenseAccountId:expense,currency:"THB",cost:"1200.01",usefulLifeMonths:3,inServiceDate:"2026-09-01"}),key());
  const activation={expectedVersion:1,periodId:f.period,entryDate:"2026-09-01",offsetAccountId:offset,evidenceRef:"Independent Project machine acquisition evidence"};
  for(const dimensions of [undefined,{project:"WRONG"}]) await expect(activateAsset(f.manager,String(asset.id),assetActivationSchema.parse({...activation,dimensions}),key())).rejects.toMatchObject({code:"DIMENSION_VALUE_REQUIRED"});
  expect(await db.prepare("SELECT status FROM finance_assets WHERE id=?").bind(asset.id).first("status")).toBe("draft");
  expect(await db.prepare("SELECT COUNT(*) count FROM finance_journals WHERE company_id=? AND source_type='asset_acquisition'").bind(f.company).first("count")).toBe(0);
  const input=assetActivationSchema.parse({...activation,dimensions:{project:"ALPHA"}}),activationKey=key();
  const activated=await activateAsset(f.manager,String(asset.id),input,activationKey);
  expect(await activateAsset(f.manager,String(asset.id),input,activationKey)).toEqual(activated);
  const depreciation={expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",dimensions:{project:"ALPHA"}};
  const posted=await depreciateAsset(f.owner,String(asset.id),assetDepreciationSchema.parse(depreciation),key());
  expect(posted.amount).toBe("400.00");
  const lines=(await db.prepare("SELECT l.dimension_json FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND (j.id IN (?,?) OR j.id IN (SELECT secondary_journal_id FROM finance_book_postings WHERE primary_journal_id IN (?,?)))").bind(f.company,activated.journalId,posted.journalId,activated.journalId,posted.journalId).all<{dimension_json:string}>()).results;
  expect(lines).toHaveLength(8);
  for(const line of lines) expect(JSON.parse(line.dimension_json)).toEqual({project:"ALPHA"});
  for(const chart of [f.chart,b.chart]) {
    const mapped=(id:string)=>chart===f.chart?id:b.maps.find(m=>m.primaryAccountId===id)!.secondaryAccountId;
    const balances=await trialBalance(f.owner,undefined,chart);
    expect(balances.find(a=>a.id===mapped(cost))?.balance).toBe("1200.01");
    expect(balances.find(a=>a.id===mapped(accumulated))?.balance).toBe("-400.00");
    expect(balances.find(a=>a.id===mapped(expense))?.balance).toBe("400.00");
  }
  const events=(await db.prepare("SELECT payload_json FROM finance_asset_events WHERE asset_id=? AND event_type IN ('activated','depreciated')").bind(asset.id).all<{payload_json:string}>()).results;
  expect(events).toHaveLength(2);
  for(const event of events) expect(JSON.parse(event.payload_json).dimensions).toEqual({project:"ALPHA"});
});
it("finds omitted monthly depreciation directly from approved asset policy and removes that close blocker only after owned posting", async () => {
  const assetId = await recognized(),
    p = await prepareAssetPolicy(
      f.owner,
      {
        assetId,
        expectedVersion: 2,
        books: [{ chartId: f.chart, policy: { method: "straight_line" } }],
        reason: "Reviewed depreciation required for each in-service month",
      },
      key(),
    );
  await decideAssetPolicy(
    f.manager,
    String(p.id),
    {
      expectedVersion: 1,
      reason: "Independent policy review before monthly close",
    },
    "approve",
    key(),
  );
  expect(
    await financeExpansionClosePending(
      db,
      f.company,
      "2026-09-01",
      "2026-09-30",
    ),
  ).toContainEqual({ kind: "due asset depreciation", id: assetId });
  const r = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "Prepare the omitted first asset month",
    },
    key(),
  );
  expect(
    await financeExpansionClosePending(
      db,
      f.company,
      "2026-09-01",
      "2026-09-30",
    ),
  ).toContainEqual({ kind: "asset depreciation", id: String(r.id) });
  await decideAssetPolicyRun(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Independent monthly carrying-value review" },
    "post",
    key(),
  );
  expect(
    await financeExpansionClosePending(
      db,
      f.company,
      "2026-09-01",
      "2026-09-30",
    ),
  ).toEqual([]);
});
it("prepares due depreciation from the reviewed book policy without posting the asset until independent review", async () => {
 await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
  const assetId = await recognized(),
    p = await prepareAssetPolicy(
      f.owner,
      {
        assetId,
        expectedVersion: 2,
        books: [{ chartId: f.chart, policy: { method: "straight_line" } }],
        reason: "Reviewed monthly straight-line allocation",
      },
      key(),
    );
  await decideAssetPolicy(
    f.manager,
    String(p.id),
    { expectedVersion: 1, reason: "Independent asset policy review" },
    "approve",
    key(),
  );
  const job = await scheduleFinanceDraft(
    f.owner,
    {
      kind: "asset_depreciation",
      sourceId: String(p.id),
      runOn: "2026-09-30",
      input: {
        expectedVersion: 2,
        periodId: f.period,
        entryDate: "2026-09-30",
        reason: "Prepare the first reviewed asset month",
      },
      reason: "Due depreciation draft with retained policy version",
    },
    key(),
  );
  await drainFinanceDueDrafts(
    db,
    f.owner.holdingId,
    new Date("2026-09-30T12:00:00Z"),
  );
  const row = await db
    .prepare("SELECT status,result_json FROM finance_due_drafts WHERE id=?")
    .bind(job.id)
    .first<{ status: string; result_json: string }>();
  expect(row?.status).toBe("prepared");
  const r = JSON.parse(row!.result_json);
  expect(r).toMatchObject({
    status: "draft",
    result: { books: [{ amount: "400.00", accumulated: "400.00" }] },
  });
  expect(
    await db
      .prepare(
        "SELECT accumulated_depreciation_minor FROM finance_assets WHERE id=?",
      )
      .bind(assetId)
      .first("accumulated_depreciation_minor"),
  ).toBe(0);
  await decideAssetPolicyRun(
    f.manager,
    String(r.id),
    {
      expectedVersion: 1,
      reason: "Independent first monthly depreciation review",
    },
    "post",
    key(),
  );
  expect(
    await db
      .prepare(
        "SELECT accumulated_depreciation_minor FROM finance_assets WHERE id=?",
      )
      .bind(assetId)
      .first("accumulated_depreciation_minor"),
  ).toBe(40000);
});
async function secondary(
  effectiveDate = "2026-09-01",
  openingPeriodId = f.period,
) {
  const chart = key(),
    maps = [];
  await db
    .prepare(
      "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,'Management','THB','fixture',1,1)",
    )
    .bind(chart, f.company)
    .run();
  for (const a of (
    await db
      .prepare("SELECT id FROM finance_accounts WHERE chart_id=? ORDER BY code")
      .bind(f.chart)
      .all<{ id: string }>()
  ).results) {
    const id = key();
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) SELECT ?,?,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at FROM finance_accounts WHERE id=?",
      )
      .bind(id, chart, a.id)
      .run();
    maps.push({ primaryAccountId: a.id, secondaryAccountId: id });
  }
  const r = await prepareBookRule(
    f.owner,
    bookRuleSchema.parse({
      primaryChartId: f.chart,
      secondaryChartId: chart,
      effectiveDate,
      openingPeriodId,
      reason: "Reviewed book opening",
      mappings: maps,
    }),
    key(),
  );
  await decideBookRule(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Independent opening review" },
    "approve",
    key(),
  );
  return { chart, maps };
}
it("posts separate reviewed book methods atomically, reverses and reaches exact lifetime carrying totals", async () => {
 await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
  const secondaryBook = await secondary(),
    assetId = await recognized(),
    p = await prepareAssetPolicy(
      f.owner,
      {
        assetId,
        expectedVersion: 2,
        books: [
          { chartId: f.chart, policy: { method: "straight_line" } },
          {
            chartId: secondaryBook.chart,
            policy: { method: "declining_balance", annualRateBps: 2000 },
          },
        ],
        reason: "Different reviewed Company book policies",
      },
      key(),
    );
  await decideAssetPolicy(
    f.manager,
    String(p.id),
    { expectedVersion: 1, reason: "Independent asset book policy review" },
    "approve",
    key(),
  );
  await expect(
    depreciateAsset(
      f.owner,
      assetId,
      { expectedVersion: 2, periodId: f.period, entryDate: "2026-09-30" },
      key(),
    ),
  ).rejects.toMatchObject({ code: "ASSET_POLICY_OWNER_REQUIRED" });
  const r = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "First full month reviewed",
    },
    key(),
  );
  const policyDetail = await financeExpansionRecord(
    f.owner,
    "finance_asset_policy",
    String(p.id),
  );
  expect(
    "assetPolicyData" in policyDetail &&
      policyDetail.assetPolicyData?.policies[0].id,
  ).toBe(p.id);
  const runDetail = await financeExpansionRecord(
    f.owner,
    "finance_asset_policy_run",
    String(r.id),
  );
  expect(
    "assetPolicyData" in runDetail &&
      runDetail.assetPolicyData?.runs.map((row) => row.id),
  ).toEqual([r.id]);
  // A current, purchased actor in another Company still receives the original
  // not-found response. A forged Company context is rejected at the new gate.
  const otherCompany=await expenseFixture();await reviewedFinanceGrant(db,otherCompany.company,"finance");
  await expect(financeExpansionRecord({...f.owner,companyId:key()},"finance_asset_policy",String(p.id))).rejects.toMatchObject({status:403});
  await expect(
    financeExpansionRecord(
      otherCompany.owner,
      "finance_asset_policy",
      String(p.id),
    ),
  ).rejects.toMatchObject({ status: 404 });
  expect(r.result.books.map((b) => [b.chartId, b.amount])).toEqual([
    [f.chart, "400.00"],
    [secondaryBook.chart, "20.00"],
  ]);
  await decideAssetPolicyRun(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Independent depreciation review" },
    "post",
    key(),
  );
  const secondaryExpense = secondaryBook.maps.find(
    (m) => m.primaryAccountId === expense,
  )!.secondaryAccountId;
  expect(
    (await trialBalance(f.manager, undefined, f.chart)).find(
      (a) => a.id === expense,
    )?.balance,
  ).toBe("400.00");
  expect(
    (await trialBalance(f.manager, undefined, secondaryBook.chart)).find(
      (a) => a.id === secondaryExpense,
    )?.balance,
  ).toBe("20.00");
  const october = key(),
    november = key();
  for (const [id, month, end] of [
    [october, "10", "31"],
    [november, "11", "30"],
  ])
    await db
      .prepare(
        "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,?, ?,?,1)",
      )
      .bind(id, f.company, month, `2026-${month}-01`, `2026-${month}-${end}`)
      .run();
  const second = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 3,
      periodId: october,
      entryDate: "2026-10-31",
      reason: "Second month review",
    },
    key(),
  );
  expect(second.result.books[1].amount).toBe("19.67");
  await decideAssetPolicyRun(
    f.manager,
    String(second.id),
    { expectedVersion: 1, reason: "Independent second month review" },
    "post",
    key(),
  );
  await reverseAssetPolicyRun(
    f.manager,
    String(second.id),
    {
      expectedVersion: 2,
      periodId: october,
      entryDate: "2026-10-31",
      reason: "Retain corrected depreciation source",
    },
    key(),
  );
  expect(
    (await trialBalance(f.manager, undefined, secondaryBook.chart)).find(
      (a) => a.id === secondaryExpense,
    )?.balance,
  ).toBe("20.00");
  const corrected = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 5,
      periodId: october,
      entryDate: "2026-10-31",
      reason: "Corrected October evidence",
    },
    key(),
  );
  await decideAssetPolicyRun(
    f.manager,
    String(corrected.id),
    { expectedVersion: 1, reason: "Independent corrected posting" },
    "post",
    key(),
  );
  const final = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 6,
      periodId: november,
      entryDate: "2026-11-30",
      reason: "Final month residual cap",
    },
    key(),
  );
  expect(final.result.books.map((b) => b.amount)).toEqual([
    "400.01",
    "1160.34",
  ]);
  await decideAssetPolicyRun(
    f.manager,
    String(final.id),
    {
      expectedVersion: 1,
      reason: "Independent final cent and carrying review",
    },
    "post",
    key(),
  );
  expect(
    (await trialBalance(f.manager, undefined, secondaryBook.chart)).find(
      (a) => a.id === secondaryExpense,
    )?.balance,
  ).toBe("1200.01");
  expect(
    (await trialBalance(f.manager, undefined, f.chart)).find(
      (a) => a.id === expense,
    )?.balance,
  ).toBe("1200.01");
});
it("retires the different carrying amounts and disposal losses in every book", async () => {
  const gain = key();
  await db
    .prepare(
      "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'GAIN','Disposal gain','revenue','THB',1,1)",
    )
    .bind(gain, f.chart, f.company)
    .run();
  const b = await secondary(),
    assetId = await recognized(),
    p = await prepareAssetPolicy(
      f.owner,
      {
        assetId,
        expectedVersion: 2,
        books: [
          { chartId: f.chart, policy: { method: "straight_line" } },
          {
            chartId: b.chart,
            policy: { method: "declining_balance", annualRateBps: 2000 },
          },
        ],
        reason: "Reviewed carrying policies",
      },
      key(),
    );
  await decideAssetPolicy(
    f.manager,
    String(p.id),
    { expectedVersion: 1, reason: "Independent policy review" },
    "approve",
    key(),
  );
  const r = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "Month before disposal",
    },
    key(),
  );
  await decideAssetPolicyRun(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Independent month review" },
    "post",
    key(),
  );
  const d = await prepareAssetDisposal(
    f.owner,
    assetId,
    {
      expectedVersion: 3,
      periodId: f.period,
      entryDate: "2026-09-30",
      proceeds: "500.00",
      proceedsAccountId: f.accounts.tax,
      gainAccountId: gain,
      lossAccountId: f.accounts.expense,
      evidenceRef: "Signed sale and receipt evidence",
    },
    key(),
  );
  await decideAssetDisposal(
    f.manager,
    String(d.id),
    "post",
    { expectedVersion: 1, reason: "Independent disposal review" },
    key(),
  );
  const primary = await trialBalance(f.manager, undefined, f.chart),
    secondaryBalance = await trialBalance(f.manager, undefined, b.chart),
    mapped = (id: string) =>
      b.maps.find((m) => m.primaryAccountId === id)!.secondaryAccountId;
  expect(primary.find((a) => a.id === f.accounts.expense)?.balance).toBe(
    "300.01",
  );
  expect(
    secondaryBalance.find((a) => a.id === mapped(f.accounts.expense))?.balance,
  ).toBe("680.01");
  expect(secondaryBalance.find((a) => a.id === mapped(cost))?.balance).toBe(
    "0.00",
  );
  expect(
    secondaryBalance.find((a) => a.id === mapped(accumulated))?.balance,
  ).toBe("0.00");
});
it("retains usage evidence including zero usage and blocks stale, self and cross Company review", async () => {
  const assetId = await recognized(12),
    p = await prepareAssetPolicy(
      f.owner,
      {
        assetId,
        expectedVersion: 2,
        books: [
          {
            chartId: f.chart,
            policy: { method: "units_of_production", totalUnits: 3 },
          },
        ],
        reason: "Meter evidence reviewed",
      },
      key(),
    );
  f.owner.role = "finance_approver";
  f.owner.assignmentId = await f.grant(f.owner, "finance_approver");
  await expect(
    decideAssetPolicy(
      f.owner,
      String(p.id),
      { expectedVersion: 1, reason: "Same person different role" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  f.owner.role = "finance_preparer";
  f.owner.assignmentId = await f.grant(f.owner, "finance_preparer");
  await decideAssetPolicy(
    f.manager,
    String(p.id),
    { expectedVersion: 1, reason: "Independent meter and lifetime review" },
    "approve",
    key(),
  );
  await expect(
    prepareAssetPolicyRun(
      f.owner,
      String(p.id),
      {
        expectedVersion: 2,
        periodId: f.period,
        entryDate: "2026-09-30",
        units: 0,
        reason: "No usage",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "ASSET_USAGE_EVIDENCE_REQUIRED" });
  const r = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      units: 0,
      usageEvidence: "Counter remained at zero",
      reason: "No usage reviewed",
    },
    key(),
  );
  expect(r.result.books[0].amount).toBe("0.00");
  await decideAssetPolicyRun(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Independent zero usage evidence" },
    "post",
    key(),
  );
  const other = await expenseFixture();
  other.owner.role = "finance_approver";
  other.owner.assignmentId = await other.grant(other.owner, "finance_approver");
  await expect(
    decideAssetPolicyRun(
      other.owner,
      String(r.id),
      { expectedVersion: 2, reason: "Wrong Company" },
      "post",
      key(),
    ),
  ).rejects.toMatchObject({ code: "ASSET_POLICY_RUN_NOT_FOUND" });
});

async function laterPeriod(month: string, end: string) {
  const id = key();
  await db
    .prepare(
      "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,?, ?,?,1)",
    )
    .bind(id, f.company, month, `2026-${month}-01`, `2026-${month}-${end}`)
    .run();
  return id;
}
it("adopts immutable legacy depreciation into a reviewed two-book carrying basis and continues the remaining months without restating history", async () => {
  const assetId = await recognized(),
    old = await depreciateAsset(
      f.owner,
      assetId,
      { expectedVersion: 2, periodId: f.period, entryDate: "2026-09-30" },
      key(),
    );
  expect(old.amount).toBe("400.00");
  const journal = String(old.journalId),
    before = JSON.stringify(
      (
        await db
          .prepare(
            "SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal",
          )
          .bind(journal)
          .all()
      ).results,
    ),
    legacy = JSON.stringify(
      (
        await db
          .prepare("SELECT * FROM finance_asset_depreciations WHERE asset_id=?")
          .bind(assetId)
          .all()
      ).results,
    );
  const october = await laterPeriod("10", "31"),
    november = await laterPeriod("11", "30"),
    b = await secondary("2026-10-01", october),
    books = [
      {
        chartId: f.chart,
        policy: { method: "declining_balance" as const, annualRateBps: 2000 },
      },
      { chartId: b.chart, policy: { method: "straight_line" as const } },
    ];
  await expect(
    prepareAssetPolicy(
      f.owner,
      {
        assetId,
        expectedVersion: 3,
        books,
        reason: "Historical adoption must be explicit",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "ASSET_POLICY_OPENING_REQUIRED" });
  const p = await prepareAssetPolicy(
    f.owner,
    {
      assetId,
      expectedVersion: 3,
      adoptLegacy: true,
      effectiveDate: "2026-10-01",
      books,
      reason:
        "Review original acquisition, September depreciation and secondary opening",
    },
    key(),
  );
  const basis = JSON.parse(
    String(
      await db
        .prepare(
          "SELECT basis_json FROM finance_asset_policy_editions WHERE id=?",
        )
        .bind(p.id)
        .first("basis_json"),
    ),
  );
  expect(basis).toMatchObject({
    ordinal: 1,
    throughDate: "2026-09-30",
    books: [
      { chartId: f.chart, accumulated: "400.00", units: 0 },
      { chartId: b.chart, accumulated: "400.00", units: 0 },
    ],
  });
  await decideAssetPolicy(
    f.manager,
    String(p.id),
    {
      expectedVersion: 1,
      reason: "Independent carrying-value adoption review",
    },
    "approve",
    key(),
  );
  expect(
    await financeExpansionClosePending(
      db,
      f.company,
      "2026-09-01",
      "2026-09-30",
    ),
  ).toEqual([]);
  const r = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 2,
      periodId: october,
      entryDate: "2026-10-31",
      reason: "First prospective month after reviewed adoption",
    },
    key(),
  );
  expect(r.result).toMatchObject({
    ordinal: 2,
    books: [
      { amount: "13.33", accumulated: "413.33" },
      { amount: "400.00", accumulated: "800.00" },
    ],
  });
  await decideAssetPolicyRun(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Independent prospective October review" },
    "post",
    key(),
  );
  await reverseAssetPolicyRun(
    f.manager,
    String(r.id),
    {
      expectedVersion: 2,
      periodId: october,
      entryDate: "2026-10-31",
      reason: "Retain corrected October usage evidence",
    },
    key(),
  );
  const corrected = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 4,
      periodId: october,
      entryDate: "2026-10-31",
      reason: "Reprepare October after exact two-book inverse",
    },
    key(),
  );
  expect(corrected.result.books.map((x) => x.amount)).toEqual([
    "13.33",
    "400.00",
  ]);
  await decideAssetPolicyRun(
    f.manager,
    String(corrected.id),
    { expectedVersion: 1, reason: "Independent replacement month review" },
    "post",
    key(),
  );
  const final = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 5,
      periodId: november,
      entryDate: "2026-11-30",
      reason: "Final month settles exact residual carrying values",
    },
    key(),
  );
  expect(final.result.books.map((x) => x.amount)).toEqual(["786.68", "400.01"]);
  await decideAssetPolicyRun(
    f.manager,
    String(final.id),
    {
      expectedVersion: 1,
      reason: "Independent exact lifetime carrying review",
    },
    "post",
    key(),
  );
  expect(
    JSON.stringify(
      (
        await db
          .prepare(
            "SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal",
          )
          .bind(journal)
          .all()
      ).results,
    ),
  ).toBe(before);
  expect(
    JSON.stringify(
      (
        await db
          .prepare("SELECT * FROM finance_asset_depreciations WHERE asset_id=?")
          .bind(assetId)
          .all()
      ).results,
    ),
  ).toBe(legacy);
  expect(
    (await trialBalance(f.manager, undefined, f.chart)).find(
      (x) => x.id === expense,
    )?.balance,
  ).toBe("1200.01");
  expect(
    (await trialBalance(f.manager, undefined, b.chart)).find(
      (x) =>
        x.id ===
        b.maps.find((m) => m.primaryAccountId === expense)!.secondaryAccountId,
    )?.balance,
  ).toBe("1200.01");
  await expect(
    db
      .prepare(
        "UPDATE finance_asset_policy_editions SET basis_json=NULL WHERE id=?",
      )
      .bind(p.id)
      .run(),
  ).rejects.toThrow();
});
it("amends a declining policy prospectively and adopts an added book from its approved opening without a catch-up charge", async () => {
  const assetId = await recognized(),
    p = await prepareAssetPolicy(
      f.owner,
      {
        assetId,
        expectedVersion: 2,
        books: [
          {
            chartId: f.chart,
            policy: { method: "declining_balance", annualRateBps: 2000 },
          },
        ],
        reason: "Initial reviewed declining policy",
      },
      key(),
    );
  await decideAssetPolicy(
    f.manager,
    String(p.id),
    { expectedVersion: 1, reason: "Independent declining policy review" },
    "approve",
    key(),
  );
  const r = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "Initial declining month",
    },
    key(),
  );
  expect(r.result.books[0].amount).toBe("20.00");
  await decideAssetPolicyRun(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Independent first month review" },
    "post",
    key(),
  );
  const october = await laterPeriod("10", "31"),
    b = await secondary("2026-10-01", october);
  const amend = await prepareAssetPolicy(
    f.owner,
    {
      assetId,
      expectedVersion: 3,
      parentId: String(p.id),
      parentVersion: 3,
      effectiveDate: "2026-10-01",
      books: [
        { chartId: f.chart, policy: { method: "straight_line" } },
        { chartId: b.chart, policy: { method: "straight_line" } },
      ],
      reason: "Review remaining carrying value and newly opened secondary book",
    },
    key(),
  );
  await decideAssetPolicy(
    f.manager,
    String(amend.id),
    {
      expectedVersion: 1,
      reason: "Independent remaining-term and book-opening review",
    },
    "approve",
    key(),
  );
  const next = await prepareAssetPolicyRun(
    f.owner,
    String(amend.id),
    {
      expectedVersion: 2,
      periodId: october,
      entryDate: "2026-10-31",
      reason: "Allocate remaining 1180.01 over the remaining two months",
    },
    key(),
  );
  expect(next.result.books.map((x) => [x.amount, x.accumulated])).toEqual([
    ["590.00", "610.00"],
    ["590.00", "610.00"],
  ]);
  await decideAssetPolicyRun(
    f.manager,
    String(next.id),
    { expectedVersion: 1, reason: "Independent prospective allocation review" },
    "post",
    key(),
  );
  expect(
    await db
      .prepare("SELECT status FROM finance_asset_policy_editions WHERE id=?")
      .bind(p.id)
      .first("status"),
  ).toBe("superseded");
  expect(
    await db
      .prepare("SELECT status FROM finance_asset_policy_runs WHERE id=?")
      .bind(r.id)
      .first("status"),
  ).toBe("posted");
});
it("refuses an adoption approval after original depreciation evidence changes and leaves the new owner inactive", async () => {
  const assetId = await recognized();
  await depreciateAsset(
    f.owner,
    assetId,
    { expectedVersion: 2, periodId: f.period, entryDate: "2026-09-30" },
    key(),
  );
  const october = await laterPeriod("10", "31");
  const p = await prepareAssetPolicy(
    f.owner,
    {
      assetId,
      expectedVersion: 3,
      adoptLegacy: true,
      books: [{ chartId: f.chart, policy: { method: "straight_line" } }],
      reason: "Prepared retained original carrying-value review",
    },
    key(),
  );
  await depreciateAsset(
    f.owner,
    assetId,
    { expectedVersion: 3, periodId: october, entryDate: "2026-10-31" },
    key(),
  );
  await expect(
    decideAssetPolicy(
      f.manager,
      String(p.id),
      {
        expectedVersion: 1,
        reason: "Review must detect changed original carrying",
      },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "ASSET_CHANGED" });
  expect(
    await db
      .prepare("SELECT status FROM finance_asset_policy_editions WHERE id=?")
      .bind(p.id)
      .first("status"),
  ).toBe("draft");
  expect(
    await db
      .prepare(
        "SELECT count(*) n FROM finance_asset_policy_runs WHERE asset_id=?",
      )
      .bind(assetId)
      .first("n"),
  ).toBe(0);
});

it("retains the earlier close cutoff when correcting a closed service month in the next open posting period", async () => {
  const assetId = await recognized(),
    p = await prepareAssetPolicy(
      f.owner,
      {
        assetId,
        expectedVersion: 2,
        books: [{ chartId: f.chart, policy: { method: "straight_line" } }],
        reason: "Reviewed monthly source and carrying policy",
      },
      key(),
    );
  await decideAssetPolicy(
    f.manager,
    String(p.id),
    {
      expectedVersion: 1,
      reason: "Independent initial depreciation policy review",
    },
    "approve",
    key(),
  );
  const first = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "Original September reviewed depreciation",
    },
    key(),
  );
  await decideAssetPolicyRun(
    f.manager,
    String(first.id),
    { expectedVersion: 1, reason: "Independent original month posting" },
    "post",
    key(),
  );
  const october = await laterPeriod("10", "31");
  await reverseAssetPolicyRun(
    f.manager,
    String(first.id),
    {
      expectedVersion: 2,
      periodId: october,
      entryDate: "2026-10-31",
      reason: "Correct retained September evidence in the next open period",
    },
    key(),
  );
  expect(
    await financeExpansionClosePending(
      db,
      f.company,
      "2026-09-01",
      "2026-09-30",
    ),
  ).toEqual([]);
  const corrected = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 4,
      periodId: october,
      entryDate: "2026-10-31",
      depreciationMonth: "2026-09",
      reason:
        "Retain September service and current-period replacement evidence",
    },
    key(),
  );
  expect(corrected.result).toMatchObject({
    ordinal: 1,
    books: [{ amount: "400.00" }],
  });
  await decideAssetPolicyRun(
    f.manager,
    String(corrected.id),
    {
      expectedVersion: 1,
      reason: "Independent closed-service-month replacement review",
    },
    "post",
    key(),
  );
  expect(
    await financeExpansionClosePending(
      db,
      f.company,
      "2026-09-01",
      "2026-09-30",
    ),
  ).toEqual([]);
  expect(
    await financeExpansionClosePending(
      db,
      f.company,
      "2026-10-01",
      "2026-10-31",
    ),
  ).toContainEqual({ kind: "due asset depreciation", id: assetId });
  const current = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 5,
      periodId: october,
      entryDate: "2026-10-31",
      depreciationMonth: "2026-10",
      reason: "Prepare October after the dated September correction",
    },
    key(),
  );
  expect(current.result).toMatchObject({
    ordinal: 2,
    books: [{ amount: "400.00" }],
  });
  await decideAssetPolicyRun(
    f.manager,
    String(current.id),
    {
      expectedVersion: 1,
      reason: "Independent current service month after correction",
    },
    "post",
    key(),
  );
  expect(
    await financeExpansionClosePending(
      db,
      f.company,
      "2026-10-01",
      "2026-10-31",
    ),
  ).toEqual([]);
  const prior = await db
    .prepare(
      "SELECT SUM(l.base_credit_minor-l.base_debit_minor) amount FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND l.account_id=? AND j.status IN ('posted','reversed') AND j.entry_date<='2026-09-30'",
    )
    .bind(f.company, accumulated)
    .first("amount");
  expect(prior).toBe(40000);
});
it("dates a zero-usage inverse without inventing a journal and retains the earlier completed service month", async () => {
  const assetId = await recognized(12),
    p = await prepareAssetPolicy(
      f.owner,
      {
        assetId,
        expectedVersion: 2,
        books: [
          {
            chartId: f.chart,
            policy: { method: "units_of_production", totalUnits: 3 },
          },
        ],
        reason: "Reviewed physical usage allocation",
      },
      key(),
    );
  await decideAssetPolicy(
    f.manager,
    String(p.id),
    {
      expectedVersion: 1,
      reason: "Independent meter and lifetime unit review",
    },
    "approve",
    key(),
  );
  const first = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      units: 0,
      usageEvidence: "Retained counter reads zero throughout September",
      reason: "Reviewed zero-use September service month",
    },
    key(),
  );
  await decideAssetPolicyRun(
    f.manager,
    String(first.id),
    { expectedVersion: 1, reason: "Independent retained zero-usage reading" },
    "post",
    key(),
  );
  const october = await laterPeriod("10", "31");
  await reverseAssetPolicyRun(
    f.manager,
    String(first.id),
    {
      expectedVersion: 2,
      periodId: october,
      entryDate: "2026-10-31",
      reason: "Retain corrected September physical-use evidence",
    },
    key(),
  );
  expect(
    await db
      .prepare(
        "SELECT reversal_entry_date,reversal_ids_json FROM finance_asset_policy_runs WHERE id=?",
      )
      .bind(first.id)
      .first(),
  ).toEqual({ reversal_entry_date: "2026-10-31", reversal_ids_json: "[]" });
  expect(
    await financeExpansionClosePending(
      db,
      f.company,
      "2026-09-01",
      "2026-09-30",
    ),
  ).toEqual([]);
  const corrected = await prepareAssetPolicyRun(
    f.owner,
    String(p.id),
    {
      expectedVersion: 4,
      periodId: october,
      entryDate: "2026-10-31",
      depreciationMonth: "2026-09",
      units: 1,
      usageEvidence: "Corrected retained meter shows one production unit",
      reason: "Current-period correction of retained September usage",
    },
    key(),
  );
  expect(corrected.result.books[0]).toMatchObject({
    amount: "400.00",
    units: 1,
  });
});
it("restores both books and asset carrying on final depreciation outbox failure, then retries and replays once",async()=>{
 await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const b=await secondary(),assetId=await recognized(),policy=await prepareAssetPolicy(f.owner,{assetId,expectedVersion:2,books:[{chartId:f.chart,policy:{method:'straight_line'}},{chartId:b.chart,policy:{method:'declining_balance',annualRateBps:2000}}],reason:'Independent primary and secondary depreciation policies'},key());
 await decideAssetPolicy(f.manager,String(policy.id),{expectedVersion:1,reason:'Review both original book policies independently'},'approve',key());
 const draft=await prepareAssetPolicyRun(f.owner,String(policy.id),{expectedVersion:2,periodId:f.period,entryDate:'2026-09-30',reason:'Retained September asset and two-book carrying evidence'},key()),input={expectedVersion:1,reason:'Independent current source and carrying review'},command=key(),fault=`asset_policy_post_fault_${crypto.randomUUID().replaceAll('-','')}`;
 const snapshots=async()=>Promise.all([
  db.prepare('SELECT * FROM finance_assets WHERE id=?').bind(assetId).first(),
  db.prepare('SELECT * FROM finance_asset_policy_editions WHERE asset_id=? ORDER BY id').bind(assetId).all().then(rows=>rows.results),
  db.prepare('SELECT * FROM finance_asset_policy_runs WHERE asset_id=? ORDER BY id').bind(assetId).all().then(rows=>rows.results),
  db.prepare('SELECT * FROM finance_asset_events WHERE asset_id=? ORDER BY id').bind(assetId).all().then(rows=>rows.results),
  db.prepare('SELECT * FROM finance_journals WHERE company_id=? ORDER BY id').bind(f.company).all().then(rows=>rows.results),
  db.prepare('SELECT * FROM finance_journal_lines WHERE journal_id IN(SELECT id FROM finance_journals WHERE company_id=?) ORDER BY id').bind(f.company).all().then(rows=>rows.results),
  db.prepare('SELECT * FROM finance_book_postings WHERE primary_journal_id IN(SELECT id FROM finance_journals WHERE company_id=?) ORDER BY primary_journal_id,rule_id').bind(f.company).all().then(rows=>rows.results),
  db.prepare('SELECT * FROM money_transaction_snapshots WHERE company_id=? ORDER BY source_type,source_id,amount_kind').bind(f.company).all().then(rows=>rows.results),
 ]),before=await snapshots();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'ASSET_POLICY_FINAL_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.event_type='finance.asset-policy-run.post') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(decideAssetPolicyRun(f.manager,String(draft.id),input,'post',command)).rejects.toThrow('ASSET_POLICY_FINAL_OUTBOX');
  expect(await snapshots()).toEqual(before);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const posted=await decideAssetPolicyRun(f.manager,String(draft.id),input,'post',command);
 expect(await decideAssetPolicyRun(f.manager,String(draft.id),input,'post',command)).toEqual(posted);
 expect(posted.journalIds).toHaveLength(2);
 expect(await db.prepare('SELECT version,accumulated_depreciation_minor FROM finance_assets WHERE id=?').bind(assetId).first()).toEqual({version:3,accumulated_depreciation_minor:40000});
 expect(await db.prepare('SELECT status,version FROM finance_asset_policy_runs WHERE id=?').bind(draft.id).first()).toEqual({status:'posted',version:2});
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(1);
});

afterEach(()=>vi.unstubAllEnvs());
