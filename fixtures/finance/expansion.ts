import { seedExpenseCompany } from "../expenses/company";
import type { SqlDatabase, FileStore } from "../../src/server/runtime/types";
import { setFinanceCompanyTool } from "../../src/server/finance-company-tools";
import {
  prepareBookRule,
  decideBookRule,
  bookRuleSchema,
} from "../../src/server/finance-books";
import {
  prepareBudgetEdition,
  budgetEditionSchema,
  prepareForecastEdition,
  forecastEditionSchema,
  prepareFinancialReport,
  financialReportSchema,
  decideFinancePlan,
  snapshotFinancialReport,
  exportFinancialReport,
} from "../../src/server/finance-planning";
import {
  createJournal,
  createJournalSchema,
  postJournal,
  postInvoice,
  importBankStatement,
  createPeriod,
  beginClose,
  closePeriod,
  calculateCloseReconciliations,
  trialBalance,
} from "../../src/server/finance";
import {
  createBankCsvMapping,
  bankCsvMappingSchema,
  importMappedBankCsv,
  proposeBankMatchGroup,
  decideBankMatchGroup,
} from "../../src/server/finance-bank-automation";
import {
  prepareCollectionCase,
  decideCollectionCase,
  reviseCollectionCase,
} from "../../src/server/finance-collections";
import {
  prepareBusinessContract,
  businessContractSchema,
  prepareRecognitionRun,
  decideBusinessContract,
  decideRecognitionRun,
} from "../../src/server/finance-business-contracts";
import {
  prepareBillingCorrection,
  decideBillingCorrection,
  billingCorrectionSchema,
} from "../../src/server/finance-billing-corrections";
import {
  prepareInstrument,
  decideInstrument,
  prepareInstrumentRun,
  decideInstrumentRun,
  instrumentEditionSchema,
} from "../../src/server/finance-instruments";
import {
  createAsset,
  activateAsset,
  assetSchema,
} from "../../src/server/finance-assets";
import {
  prepareAssetPolicy,
  decideAssetPolicy,
  prepareAssetPolicyRun,
  decideAssetPolicyRun,
} from "../../src/server/finance-asset-policies";
import {
  prepareAccountingProfile,
  decideAccountingProfile,
} from "../../src/server/accounting-profiles";
import {
  prepareYearClosing,
  decideYearClosing,
} from "../../src/server/finance-reporting";
import {
  prepareGroupReportEdition,
  decideGroupReportEdition,
  groupReportEditionExport,
} from "../../src/server/finance-group-editions";
import {
  scheduleFinanceDraft,
  drainFinanceDueDrafts,
} from "../../src/server/finance-due-drafts";
const key = () => crypto.randomUUID();
type Row = Record<string, string | number | null>;
/** Synthetic source-owned rehearsal. All amounts are stated in the companion test.
 * Caller must select an authorized disposable or development database first. */
export async function seedFinanceExpansionRehearsal(
  db: SqlDatabase,
  files: FileStore,
) {
  const f = await seedExpenseCompany(db, files),
    g = await seedExpenseCompany(db, files);
  for (const x of [f, g]) {
    await db
      .prepare("UPDATE organization_nodes SET display_name=? WHERE id=?")
      .bind(
        `Finance engineering rehearsal · ${x === f ? "Operating Company" : "Companion Company"}`,
        x.company,
      )
      .run();
    await db
      .prepare(
        "INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,created_at) VALUES(?,'h001',?,?,'THB',1)",
      )
      .bind(
        `${x.company}-entity`,
        x.company,
        x === f ? "Operating Company" : "Companion Company",
      )
      .run();
  }
  f.owner.role = "finance_preparer";
  f.owner.assignmentId = await f.grant(f.owner, "finance_preparer");
  const preparer = f.owner,
    approver = f.manager;
  for (const tool of [
    "planning",
    "bank_automation",
    "collections",
    "business_contracts",
    "instruments",
    "asset_policies",
    "group_reporting",
  ] as const)
    await setFinanceCompanyTool(
      preparer,
      {
        tool,
        enabled: true,
        expectedVersion: 0,
        reason: "Fictional isolated Finance engineering rehearsal only",
      },
      key(),
    );
  const account: Record<string, string> = {};
  for (const [code, category, control] of [
    ["AR", "asset", "receivable"],
    ["DEFERRED", "liability", "none"],
    ["COST", "asset", "none"],
    ["ACC", "asset", "none"],
    ["DEP", "expense", "none"],
    ["CAPITAL", "equity", "none"],
    ["DEBT", "liability", "none"],
    ["INTEREST", "expense", "none"],
    ["ROU", "asset", "none"],
    ["ROU_ACC", "asset", "none"],
    ["ROU_DEP", "expense", "none"],
    ["RETAINED", "equity", "none"],
  ]) {
    const id = key();
    account[code] = id;
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
        control === "none" ? 0 : 1,
        control,
      )
      .run();
  }
  await db
    .prepare(
      "UPDATE finance_accounts SET control_kind='bank',is_control=1 WHERE id=?",
    )
    .bind(f.accounts.cash)
    .run();
  const profile = async (x: typeof f) => {
    const p = await prepareAccountingProfile(
      x.reviewer,
      {
        statutoryChartId: x.chart,
        standard: "TFRS for NPAEs",
        fiscalStartMonth: 1,
        effectiveDate: "2026-01-01",
        eligibilityEvidence:
          "Fictional Company eligibility for the source-owned financial rehearsal",
        policyEvidence:
          "Independently reviewed fictional THB accrual, asset and income policies",
        taxBasis: "current_tax",
        inventoryPolicy: "moving_average",
        depreciationPolicy: "straight_line",
      },
      key(),
    );
    await decideAccountingProfile(
      x.manager,
      String(p.id),
      "approve",
      {
        expectedVersion: 1,
        reason: "Independent fictional eligibility and policy review",
      },
      key(),
    );
  };
  await profile(f);
  await profile(g);
  await db
    .prepare("UPDATE finance_periods SET end_date='2026-08-31' WHERE id=?")
    .bind(f.period)
    .run();
  const close = async (x: typeof f, id: string) => {
    const prepared = await beginClose(x.reviewer, id, key());
    await closePeriod(
      x.manager,
      id,
      {
        expectedVersion: Number(prepared.version),
        attachmentSetIds: [],
        proof:
          "Independent retained GL, bank and source-owner reconciliation for synthetic engineering close",
      },
      key(),
    );
  };
  const statement = async (
    date: string,
    opening: string,
    closing: string,
    rows: Array<{
      externalId: string;
      transactionDate: string;
      amount: string;
      currency: string;
    }>,
  ) =>
    importBankStatement(
      preparer,
      {
        bankAccount: f.accounts.cash,
        statementNumber: key(),
        statementDate: date,
        opening,
        closing,
        sourceHash: key(),
        rows,
      },
      key(),
    );
  await statement("2026-08-31", "0", "0", []);
  await close(f, f.period);
  const september = String(
      (
        await createPeriod(
          preparer,
          {
            name: "September rehearsal",
            startDate: "2026-09-01",
            endDate: "2026-09-30",
          },
          key(),
        )
      ).id,
    ),
    october = String(
      (
        await createPeriod(
          preparer,
          {
            name: "October rehearsal",
            startDate: "2026-10-01",
            endDate: "2026-10-31",
          },
          key(),
        )
      ).id,
    ),
    remainder = String(
      (
        await createPeriod(
          preparer,
          {
            name: "Fiscal remainder rehearsal",
            startDate: "2026-11-01",
            endDate: "2026-12-31",
          },
          key(),
        )
      ).id,
    );
  const secondary = key();
  await db
    .prepare(
      "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,'Reviewed secondary rehearsal','THB',?,1,1)",
    )
    .bind(secondary, f.company, preparer.userId)
    .run();
  const mappings = [];
  for (const a of (
    await db
      .prepare("SELECT id FROM finance_accounts WHERE chart_id=? ORDER BY id")
      .bind(f.chart)
      .all<{ id: string }>()
  ).results) {
    const to = key();
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) SELECT ?,?,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at FROM finance_accounts WHERE id=?",
      )
      .bind(to, secondary, a.id)
      .run();
    mappings.push({ primaryAccountId: a.id, secondaryAccountId: to });
  }
  const book = await prepareBookRule(
    preparer,
    bookRuleSchema.parse({
      primaryChartId: f.chart,
      secondaryChartId: secondary,
      effectiveDate: "2026-09-01",
      openingPeriodId: september,
      reason:
        "Independently review every source-account mapping before posting",
      mappings,
    }),
    key(),
  );
  await decideBookRule(
    approver,
    String(book.id),
    { expectedVersion: 1, reason: "Independent empty carrying-opening review" },
    "approve",
    key(),
  );
  const party = key();
  await db
    .prepare(
      "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Fictional rehearsal customer','THB',1,1)",
    )
    .bind(party, f.company, party)
    .run();
  const contract = await prepareBusinessContract(
    preparer,
    businessContractSchema.parse({
      chartId: f.chart,
      partyId: party,
      number: key(),
      name: "Fictional one-time service",
      currency: "THB",
      startDate: "2026-09-01",
      endDate: "2026-09-30",
      cycle: "once",
      pricing: { kind: "flat", amount: "100.01" },
      receivableAccountId: account.AR,
      deferredAccountId: account.DEFERRED,
      obligations: [
        {
          key: "service",
          name: "Delivered service",
          standalonePrice: "1.00",
          revenueAccountId: f.accounts.revenue,
          mode: "point",
        },
      ],
      reason: "Synthetic signed service and delivery evidence",
    }),
    key(),
  );
  await decideBusinessContract(
    approver,
    String(contract.id),
    {
      expectedVersion: 1,
      reason: "Independent price, source and allocation review",
    },
    "approve",
    key(),
  );
  const scheduled = await scheduleFinanceDraft(
    preparer,
    {
      kind: "billing",
      sourceId: String(contract.id),
      runOn: "2026-09-01",
      input: {
        expectedVersion: 2,
        invoiceDate: "2026-09-01",
        dueDate: "2026-09-30",
      },
      reason:
        "Source-versioned dated preparation through the existing jobs owner",
    },
    key(),
  );
  await drainFinanceDueDrafts(
    db,
    preparer.holdingId,
    new Date("2026-09-01T12:00:00Z"),
  );
  const bill = JSON.parse(
    String(
      await db
        .prepare("SELECT result_json FROM finance_due_drafts WHERE id=?")
        .bind(scheduled.id)
        .first("result_json"),
    ),
  ) as { id: string; invoiceId: string };
  await postInvoice(
    approver,
    bill.invoiceId,
    { expectedVersion: 1, periodId: september, entryDate: "2026-09-01" },
    key(),
  );
  const recognition = await prepareRecognitionRun(
    preparer,
    {
      billingRunId: bill.id,
      itemKey: "service:2026-09-30",
      periodId: september,
      entryDate: "2026-09-30",
      reason: "Retained original delivery evidence",
    },
    key(),
  );
  await decideRecognitionRun(
    approver,
    String(recognition.id),
    { expectedVersion: 1, reason: "Independent completed service review" },
    "post",
    key(),
  );
  const asset = await createAsset(
    preparer,
    assetSchema.parse({
      assetNumber: key(),
      name: "Synthetic one-month asset",
      accountId: account.COST,
      depreciationAccountId: account.ACC,
      depreciationExpenseAccountId: account.DEP,
      currency: "THB",
      cost: "1200.01",
      usefulLifeMonths: 1,
      inServiceDate: "2026-09-01",
    }),
    key(),
  );
  await activateAsset(
    approver,
    String(asset.id),
    {
      expectedVersion: 1,
      periodId: september,
      entryDate: "2026-09-01",
      offsetAccountId: account.CAPITAL,
      evidenceRef: "Fictional acquisition funding and asset source",
    },
    key(),
  );
  const policy = await prepareAssetPolicy(
    preparer,
    {
      assetId: String(asset.id),
      expectedVersion: 2,
      books: [
        { chartId: f.chart, policy: { method: "straight_line" } },
        {
          chartId: secondary,
          policy: { method: "declining_balance", annualRateBps: 2000 },
        },
      ],
      reason: "Reviewed per-book asset methods and final residual convention",
    },
    key(),
  );
  await decideAssetPolicy(
    approver,
    String(policy.id),
    {
      expectedVersion: 1,
      reason: "Independent book-policy and acquisition review",
    },
    "approve",
    key(),
  );
  const depreciation = await prepareAssetPolicyRun(
    preparer,
    String(policy.id),
    {
      expectedVersion: 2,
      periodId: september,
      entryDate: "2026-09-30",
      reason: "Final reviewed asset month",
    },
    key(),
  );
  await decideAssetPolicyRun(
    approver,
    String(depreciation.id),
    {
      expectedVersion: 1,
      reason: "Independent residual cent and carrying review",
    },
    "post",
    key(),
  );
  const funded = await statement("2026-09-01", "0", "300.00", [
      {
        externalId: "loan-funding",
        transactionDate: "2026-09-01",
        amount: "300.00",
        currency: "THB",
      },
    ]),
    fundingBank = String(
      await db
        .prepare(
          "SELECT id FROM finance_bank_transactions WHERE statement_id=?",
        )
        .bind(funded.id)
        .first("id"),
    );
  const debtTerms = {
    number: key(),
    name: "Fictional fixed financing",
    chartId: f.chart,
    currency: "THB",
    startDate: "2026-09-01",
    months: 1,
    bankAccountId: f.accounts.cash,
    liabilityAccountId: account.DEBT,
    interestAccountId: account.INTEREST,
    periodId: september,
    reason:
      "Synthetic fixed-payment agreement with actual imported fixture evidence",
  };
  const loan = await prepareInstrument(
    preparer,
    instrumentEditionSchema.parse({
      ...debtTerms,
      kind: "loan",
      annualRateBps: 1200,
      payment: "303.00",
      principal: "300.00",
      bankTransactionId: fundingBank,
      bankVersion: 1,
    }),
    key(),
  );
  await decideInstrument(
    approver,
    String(loan.id),
    {
      expectedVersion: 1,
      reason: "Independent funding and fixed-interest review",
    },
    "approve",
    key(),
  );
  const lease = await prepareInstrument(
    preparer,
    instrumentEditionSchema.parse({
      ...debtTerms,
      number: key(),
      kind: "lease",
      annualRateBps: 0,
      payment: "100.00",
      rightOfUseAccountId: account.ROU,
      accumulatedAccountId: account.ROU_ACC,
      depreciationAccountId: account.ROU_DEP,
    }),
    key(),
  );
  await decideInstrument(
    approver,
    String(lease.id),
    {
      expectedVersion: 1,
      reason: "Independent fixed lease carrying and term review",
    },
    "approve",
    key(),
  );
  const cashLines = [];
  for (const amount of ["60.01", "39.99"]) {
    const j = await createJournal(
      preparer,
      createJournalSchema.parse({
        chartId: f.chart,
        periodId: september,
        entryDate: "2026-09-04",
        currency: "THB",
        lines: [
          { accountId: f.accounts.cash, debit: amount, credit: "0" },
          { accountId: f.accounts.revenue, debit: "0", credit: amount },
        ],
      }),
      key(),
    );
    await postJournal(approver, String(j.id), key(), { expectedVersion: 1 });
    cashLines.push(
      String(
        await db
          .prepare(
            "SELECT id FROM finance_journal_lines WHERE journal_id=? AND account_id=?",
          )
          .bind(j.id, f.accounts.cash)
          .first("id"),
      ),
    );
  }
  const csv = await createBankCsvMapping(
      preparer,
      bankCsvMappingSchema.parse({
        name: "Retained rehearsal CSV",
        columns: {
          externalId: "Ref",
          transactionDate: "Date",
          amount: "Amount",
        },
      }),
      key(),
    ),
    imported = await importMappedBankCsv(
      preparer,
      {
        mappingId: String(csv.id),
        bankAccountId: f.accounts.cash,
        statementNumber: key(),
        statementDate: "2026-09-30",
        opening: "300.00",
        closing: "400.00",
        csv: "Ref,Date,Amount\ncombined,2026-09-04,100.00\n",
      },
      key(),
    ),
    bankRow = String(
      await db
        .prepare(
          "SELECT id FROM finance_bank_transactions WHERE statement_id=?",
        )
        .bind("id" in imported ? imported.id : undefined)
        .first("id"),
    );
  const match = await proposeBankMatchGroup(
    preparer,
    {
      bankTransactionIds: [bankRow],
      journalLineIds: cashLines,
      reason: "Review the exact combined synthetic bank receipt",
    },
    key(),
  );
  await decideBankMatchGroup(
    approver,
    String(match.id),
    { expectedVersion: 1, reason: "Independent many-item bank amount review" },
    "confirm",
    key(),
  );
  await close(f, september);
  const originalCollection = await prepareCollectionCase(
    preparer,
    { invoiceId: bill.invoiceId, expectedVersion: 2, asOf: "2026-10-15" },
    key(),
  );
  await decideCollectionCase(
    approver,
    String(originalCollection.id),
    {
      expectedVersion: 1,
      reason: "Independent overdue source and retained message review",
    },
    "approve",
    key(),
  );
  const collection = await reviseCollectionCase(
    preparer,
    String(originalCollection.id),
    {
      expectedVersion: 2,
      expectedInvoiceVersion: 2,
      asOf: "2026-10-15",
      message:
        "Fictional customer: please confirm the retained THB 100.01 service obligation before settlement.",
      reason: "Clarify the independently reviewed but unsent collection text",
    },
    key(),
  );
  await decideCollectionCase(
    approver,
    String(collection.id),
    {
      expectedVersion: 1,
      reason: "Revised text and original obligation independently verified",
    },
    "approve",
    key(),
  );
  await decideCollectionCase(
    preparer,
    String(collection.id),
    {
      expectedVersion: 2,
      reason:
        "Record exactly-once test adapter delivery; no real customer communication",
    },
    "test-deliver",
    key(),
  );
  const payments = await statement("2026-10-31", "400.00", "-3.00", [
    {
      externalId: "lease-payment",
      transactionDate: "2026-10-01",
      amount: "-100.00",
      currency: "THB",
    },
    {
      externalId: "loan-payment",
      transactionDate: "2026-10-01",
      amount: "-303.00",
      currency: "THB",
    },
  ]);
  for (const [instrument, externalId] of [
    [lease, "lease-payment"],
    [loan, "loan-payment"],
  ] as const) {
    const bank = String(
        await db
          .prepare(
            "SELECT id FROM finance_bank_transactions WHERE statement_id=? AND external_id=?",
          )
          .bind(payments.id, externalId)
          .first("id"),
      ),
      run = await prepareInstrumentRun(
        preparer,
        String(instrument.id),
        {
          expectedVersion: 2,
          periodId: october,
          entryDate: "2026-10-01",
          bankTransactionId: bank,
          bankVersion: 1,
          reason: "Prepare the final actual synthetic financing debit",
        },
        key(),
      );
    await decideInstrumentRun(
      approver,
      String(run.id),
      {
        expectedVersion: 1,
        reason: "Independent final liability, interest and bank match review",
      },
      "post",
      key(),
    );
  }
  const correction = await prepareBillingCorrection(
      preparer,
      bill.id,
      billingCorrectionSchema.parse({
        expectedVersion: 1,
        invoiceExpectedVersion: 2,
        number: key(),
        periodId: october,
        entryDate: "2026-10-20",
        billing: { invoiceDate: "2026-10-20", dueDate: "2026-10-31" },
        pricing: { kind: "flat", amount: "80.01" },
        reason:
          "Retain a full credit and replacement after closing the original service month",
      }),
      key(),
    ),
    corrected = await decideBillingCorrection(
      approver,
      String(correction.id),
      {
        expectedVersion: 1,
        reason:
          "Independent cross-period full-credit and corrected source review",
      },
      "post",
      key(),
    );
  const replacement = await prepareRecognitionRun(
    preparer,
    {
      billingRunId: String(
        "replacementBillId" in corrected
          ? corrected.replacementBillId
          : undefined,
      ),
      itemKey: "service:2026-09-30",
      periodId: october,
      entryDate: "2026-10-31",
      reason: "Recognize corrected delivered service in the open period",
    },
    key(),
  );
  await decideRecognitionRun(
    approver,
    String(replacement.id),
    {
      expectedVersion: 1,
      reason: "Independent replacement allocation and delivery review",
    },
    "post",
    key(),
  );
  const budget = await prepareBudgetEdition(
      preparer,
      budgetEditionSchema.parse({
        chartId: f.chart,
        name: "Approved rehearsal budget",
        year: 2026,
        lines: [
          { accountId: account.DEP, month: "2026-09", amount: "1200.01" },
          { accountId: account.ROU_DEP, month: "2026-10", amount: "100.00" },
          { accountId: account.INTEREST, month: "2026-10", amount: "3.00" },
        ],
      }),
      key(),
    ),
    forecast = await prepareForecastEdition(
      preparer,
      forecastEditionSchema.parse({
        chartId: f.chart,
        name: "Retained twelve-month rehearsal forecast",
        startMonth: "2026-09",
        assumptions:
          "Synthetic ten per month with fixed good and bad scenarios",
        lines: [{ accountId: f.accounts.revenue, monthlyAmount: "10.00" }],
      }),
      key(),
    ),
    report = await prepareFinancialReport(
      preparer,
      financialReportSchema.parse({
        chartId: f.chart,
        name: "Retained rehearsal profit",
        rows: [
          {
            key: "revenue",
            label: "Revenue",
            kind: "accounts",
            accountIds: [f.accounts.revenue],
            sign: "credit",
          },
          {
            key: "expense",
            label: "Expense",
            kind: "accounts",
            accountIds: [account.DEP, account.ROU_DEP, account.INTEREST],
            sign: "debit",
          },
          {
            key: "profit",
            label: "Profit",
            kind: "difference",
            references: ["revenue", "expense"],
          },
        ],
      }),
      key(),
    );
  for (const p of [budget, forecast, report])
    await decideFinancePlan(
      approver,
      String(p.id),
      {
        expectedVersion: 1,
        reason: "Independent retained planning sources and formulas review",
      },
      "approve",
      key(),
    );
  const snapshot = await snapshotFinancialReport(
      preparer,
      String(report.id),
      { startDate: "2026-09-01", endDate: "2026-10-31" },
      key(),
    ),
    reportExport = await exportFinancialReport(preparer, String(snapshot.id));
  await close(f, october);
  await statement("2026-12-31", "-3.00", "-3.00", []);
  const year = await prepareYearClosing(
    preparer,
    {
      chartId: f.chart,
      startDate: "2026-01-01",
      endDate: "2026-12-31",
      periodId: remainder,
      retainedEarningsAccountId: account.RETAINED,
      reason:
        "Reconcile every source owner before the reviewed fiscal transfer",
    },
    key(),
  );
  await decideYearClosing(
    approver,
    String(year.id),
    "post",
    {
      expectedVersion: 1,
      reason: "Independent original annual profit and retained-earnings review",
    },
    key(),
  );
  await close(f, remainder);
  await close(g, g.period);
  const sourceAccounts = (
    await db
      .prepare(
        "SELECT id,code,name,category FROM finance_accounts WHERE chart_id IN (?,?) ORDER BY id",
      )
      .bind(f.chart, g.chart)
      .all<Row>()
  ).results;
  const group = await prepareGroupReportEdition(
    f.owner,
    {
      startDate: "2026-01-01",
      endDate: "2026-12-31",
      currency: "THB",
      entities: [
        {
          companyId: f.company,
          chartId: f.chart,
          periodIds: [f.period, september, october, remainder],
          method: "full",
        },
        {
          companyId: g.company,
          chartId: g.chart,
          periodIds: [g.period],
          method: "full",
        },
      ],
      mappings: sourceAccounts.map((a) => ({
        accountId: String(a.id),
        group: {
          code: String(a.code),
          name: String(a.name),
          category: a.category as
            | "asset"
            | "liability"
            | "equity"
            | "revenue"
            | "expense",
        },
        fxType: ["revenue", "expense"].includes(String(a.category))
          ? ("average" as const)
          : a.category === "equity"
            ? ("historical" as const)
            : ("current" as const),
        historicalRate: a.category === "equity" ? "1.00" : undefined,
      })),
      reason:
        "Retain actual closed Company sources after two consecutive closes and year transfer",
    },
    key(),
  );
  await decideGroupReportEdition(
    approver,
    String(group.id),
    {
      expectedVersion: 1,
      reason:
        "Independent complete cumulative balances, interval profit and source editions review",
    },
    "approve",
    key(),
  );
  const groupExport = await groupReportEditionExport(f.owner, String(group.id));
  const reconciliation = await calculateCloseReconciliations(db, preparer, {
      id: remainder,
      start_date: "2026-11-01",
      end_date: "2026-12-31",
    }),
    balances = await trialBalance(preparer, undefined, f.chart),
    secondaryBalances = await trialBalance(preparer, undefined, secondary);
  return {
    f,
    g,
    preparer,
    approver,
    account,
    secondary,
    bookId: String(book.id),
    budgetId: String(budget.id),
    forecastId: String(forecast.id),
    reportId: String(report.id),
    snapshotId: String(snapshot.id),
    reportExport,
    groupId: String(group.id),
    groupExport,
    scheduledId: String(scheduled.id),
    contractId: String(contract.id),
    billId: bill.id,
    correctionId: String(correction.id),
    replacementBillId: String(
      "replacementBillId" in corrected
        ? corrected.replacementBillId
        : undefined,
    ),
    assetId: String(asset.id),
    policyId: String(policy.id),
    loanId: String(loan.id),
    leaseId: String(lease.id),
    matchId: String(match.id),
    collectionId: String(collection.id),
    originalCollectionId: String(originalCollection.id),
    yearId: String(year.id),
    yearProfit: year.profitMinor,
    reconciliation,
    balances,
    secondaryBalances,
  };
}
