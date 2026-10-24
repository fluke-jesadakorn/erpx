import {calendarData} from "@erpx/server-work_projection/public/calendar/index";
import { afterEach } from "vitest";
import { reviewedFinanceGrant } from "../../support/helpers/finance-purchase-fixture";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  businessContractSchema,
  prepareBusinessContract,
  decideBusinessContract,
  prepareBusinessBilling,
  prepareRecognitionRun,
  decideRecognitionRun,
} from "@erpx/server-finance/public/finance-business-contracts";
import {
  billingCorrectionSchema,
  prepareBillingCorrection,
  decideBillingCorrection,
} from "@erpx/server-finance/public/finance-billing-corrections";
import { setFinanceCompanyTool } from "@erpx/server-finance/public/finance-company-tools";
import {postInvoice,trialBalance,createPayment,approvePayment,releasePayment,confirmPayment,importBankStatement,reverseJournal,reversePayment} from '@erpx/server-finance/public/finance';
import {
  approveRefund,
  prepareRefundInstruction,
  exportRefundInstructions,
  confirmRefundInstruction,
  refundConfirmationSchema,
  refundWorkspace,
} from "@erpx/server-finance/public/refunds";
import {
  prepareAccountingProfile,
  decideAccountingProfile,
} from "@erpx/server-finance/public/accounting-profiles";
import {
  prepareVatEntry,
  reviewVatEntry,
  vatEntrySchema,
  vatWorkspace,
} from "@erpx/server-finance/public/finance-tax-register";
import {
  bookRuleSchema,
  prepareBookRule,
  decideBookRule,
} from "@erpx/server-finance/public/finance-books";
import { invoiceHistoryCte } from "@erpx/server-finance/public/finance-history";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  ar: string,
  deferred: string,
  income: string,
  tax: string,
  party: string;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  for (const [actor, role] of [
    [f.owner, "finance_preparer"],
    [f.manager, "finance_approver"],
    [f.payer, "treasury"],
  ] as const) {
    actor.role = role;
    actor.branchId = null;
    actor.departmentId = null;
    actor.assignmentId = await f.grant(actor, role);
  }
  await db
    .prepare(
      "UPDATE finance_accounts SET is_control=1,control_kind='bank' WHERE id=?",
    )
    .bind(f.accounts.cash)
    .run();
  ar = key();
  deferred = key();
  income = key();
  tax = key();
  party = key();
  for (const [id, code, category, control] of [
    [ar, "AR", "asset", "receivable"],
    [deferred, "DEF", "liability", "none"],
    [income, "REV", "revenue", "none"],
    [tax, "OVAT", "liability", "none"],
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
        control === "none" ? 0 : 1,
        control,
      )
      .run();
  await db
    .prepare(
      "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,created_at,updated_at) VALUES(?,?,'customer',?,'Fictional corrected billing customer','THB','0105559999999',1,1)",
    )
    .bind(party, f.company, party)
    .run();
  await db
    .prepare(
      "INSERT INTO finance_tax_codes(id,company_id,code,name,rate_micros,effective_from) VALUES(?,?,'VAT7','Ordinary fixture VAT',70000,'2026-01-01')",
    )
    .bind(key(), f.company)
    .run();
  await setFinanceCompanyTool(
    f.owner,
    {
      tool: "business_contracts",
      enabled: true,
      expectedVersion: 0,
      reason: "Disposable correction accounting fixture",
    },
    key(),
  );
});
async function billed() {
  const c = await prepareBusinessContract(
    f.owner,
    businessContractSchema.parse({
      chartId: f.chart,
      partyId: party,
      number: key(),
      name: "Retained service bill",
      currency: "THB",
      startDate: "2026-09-01",
      endDate: "2026-09-30",
      reason: "Fictional signed service charge",
      cycle: "once",
      pricing: { kind: "flat", amount: "100.01" },
      receivableAccountId: ar,
      deferredAccountId: deferred,
      taxBps: 700,
      taxCode: "VAT7",
      taxAccountId: tax,
      obligations: [
        {
          key: "support",
          name: "Support",
          standalonePrice: "3.00",
          revenueAccountId: income,
          mode: "point",
        },
        {
          key: "delivery",
          name: "Delivery",
          standalonePrice: "1.00",
          revenueAccountId: income,
          mode: "point",
        },
      ],
    }),
    key(),
  );
  await decideBusinessContract(
    f.manager,
    String(c.id),
    { expectedVersion: 1, reason: "Reviewed signed price and SSP" },
    "approve",
    key(),
  );
  const bill = await prepareBusinessBilling(
    f.owner,
    String(c.id),
    { expectedVersion: 2, invoiceDate: "2026-09-01", dueDate: "2026-09-30" },
    key(),
  );
  await postInvoice(
    f.manager,
    String(bill.invoiceId),
    { expectedVersion: 1, periodId: f.period, entryDate: "2026-09-01" },
    key(),
  );
  return bill;
}
function correction(invoiceVersion = 2) {
  return billingCorrectionSchema.parse({
    expectedVersion: 1,
    invoiceExpectedVersion: invoiceVersion,
    number: key(),
    periodId: f.period,
    entryDate: "2026-09-30",
    billing: { invoiceDate: "2026-09-30", dueDate: "2026-10-15" },
    pricing: { kind: "flat", amount: "80.01" },
    reason:
      "Correct the retained agreed service price with a full credit and replacement",
  });
}
async function vat(
  sourceType: "invoice" | "billing_credit",
  sourceId: string,
  expectedSourceVersion: number,
  date: string,
) {
  const e = await prepareVatEntry(
    f.owner,
    vatEntrySchema.parse({
      sourceType,
      sourceId,
      expectedSourceVersion,
      taxPointDate: date,
      claimMonth: "2026-09",
      documentReference: key(),
      treatment: "standard",
      deductibleBps: 10000,
      legalBasis: "Reviewed fictional source document tax treatment",
      taxPointEvidence:
        "Independent fixture tax-point evidence on the retained date",
      documentEvidence:
        "Fictional credit or invoice retained with its original source",
    }),
    key(),
  );
  await reviewVatEntry(
    f.manager,
    String(e.id),
    "approve",
    {
      expectedVersion: 1,
      reason: "Independent source amount and tax-date review",
    },
    key(),
  );
  return e;
}
async function secondary() {
  const chart = key();
  await db
    .prepare(
      "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,?,'THB',?,1,1)",
    )
    .bind(chart, f.company, "Correction secondary book", f.owner.userId)
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
      .bind(to, chart, a.id)
      .run();
    mappings.push({ primaryAccountId: a.id, secondaryAccountId: to });
  }
  const rule = await prepareBookRule(
    f.owner,
    bookRuleSchema.parse({
      primaryChartId: f.chart,
      secondaryChartId: chart,
      effectiveDate: "2026-09-01",
      openingPeriodId: f.period,
      reason: "Independent all-book correction fixture",
      mappings,
    }),
    key(),
  );
  await decideBookRule(
    f.manager,
    String(rule.id),
    {
      expectedVersion: 1,
      reason: "Reviewed full opening and account mappings",
    },
    "approve",
    key(),
  );
  return { chart, mappings };
}
it("credits a partly paid bill, reverses its recognition, posts the replacement in two books and settles only the retained refund from bank evidence", async () => {
  const s = await secondary(),
    bill = await billed();
  const profile = await prepareAccountingProfile(
    f.owner,
    {
      statutoryChartId: f.chart,
      standard: "TFRS for NPAEs",
      fiscalStartMonth: 1,
      effectiveDate: "2026-01-01",
      eligibilityEvidence:
        "Fictional eligibility evidence for the isolated Company",
      policyEvidence:
        "Reviewed fictional THB tax and accounting policy evidence",
      taxBasis: "current_tax",
      inventoryPolicy: "moving_average",
      depreciationPolicy: "straight_line",
    },
    key(),
  );
  await decideAccountingProfile(
    f.manager,
    profile.id,
    "approve",
    {
      expectedVersion: 1,
      reason: "Reviewed fictional policy and statutory ledger",
    },
    key(),
  );
  await vat("invoice", String(bill.invoiceId), 2, "2026-09-01");
  const r = await prepareRecognitionRun(
    f.owner,
    {
      billingRunId: String(bill.id),
      itemKey: "support:2026-09-30",
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "Delivered support independently evidenced",
    },
    key(),
  );
  await decideRecognitionRun(
    f.manager,
    String(r.id),
    {
      expectedVersion: 1,
      reason: "Independent delivered support and allocation review",
    },
    "post",
    key(),
  );
  const payment = await createPayment(
    f.owner,
    {
      partyId: party,
      kind: "receipt",
      paymentNumber: key(),
      paymentDate: "2026-09-20",
      currency: "THB",
      amount: "40.01",
      bankAccount: f.accounts.cash,
    },
    key(),
  );
  await approvePayment(
    f.manager,
    String(payment.id),
    { expectedVersion: 1, proof: "Reviewed exact fictional receipt" },
    key(),
  );
  await releasePayment(
    f.payer,
    String(payment.id),
    { expectedVersion: 2, proof: "Treasury recorded the fictional receipt" },
    key(),
  );
  const statement = await importBankStatement(
      f.owner,
      {
        bankAccount: f.accounts.cash,
        statementNumber: key(),
        statementDate: "2026-09-20",
        opening: "0",
        closing: "40.01",
        sourceHash: key(),
        rows: [
          {
            externalId: key(),
            transactionDate: "2026-09-20",
            amount: "40.01",
            currency: "THB",
          },
        ],
      },
      key(),
    ),
    bankId = String(
      await db
        .prepare(
          "SELECT id FROM finance_bank_transactions WHERE statement_id=?",
        )
        .bind(statement.id)
        .first("id"),
    );
  await confirmPayment(
    f.payer,
    String(payment.id),
    {
      expectedVersion: 3,
      periodId: f.period,
      bankTransactionId: bankId,
      bankVersion: 1,
      bankReference: key(),
      proof: "Exact imported synthetic receipt",
      allocations: [{ invoiceId: String(bill.invoiceId), amount: "40.01" }],
    },
    key(),
  );
  const old = await db
      .prepare(
        "SELECT input_json,result_json,source_hash FROM finance_business_billing_runs WHERE id=?",
      )
      .bind(bill.id)
      .first(),
    input = correction(3),
    draft = await prepareBillingCorrection(
      f.owner,
      String(bill.id),
      input,
      key(),
    ),
    command = key(),
    result = await decideBillingCorrection(
      f.manager,
      String(draft.id),
      {
        expectedVersion: 1,
        reason:
          "Reviewed full credit, reversed earned revenue, corrected price and refund",
      },
      "post",
      command,
    );
  expect(result).toMatchObject({
    creditedMinor: 6700,
    refundMinor: 4001,
    status: "posted",
  });
  expect(
    await decideBillingCorrection(
      f.manager,
      String(draft.id),
      {
        expectedVersion: 1,
        reason:
          "Reviewed full credit, reversed earned revenue, corrected price and refund",
      },
      "post",
      command,
    ),
  ).toEqual(result);
  const newBill = await db
    .prepare("SELECT * FROM finance_business_billing_runs WHERE id=?")
    .bind(result.replacementBillId)
    .first<Record<string, string | number | null>>();
  expect(newBill).toMatchObject({ parent_run_id: bill.id, bill_edition: 2 });
  expect(
    await db
      .prepare(
        "SELECT input_json,result_json,source_hash FROM finance_business_billing_runs WHERE id=?",
      )
      .bind(bill.id)
      .first(),
  ).toEqual(old);
  expect(
    await db
      .prepare(
        "SELECT status,subtotal_minor,tax_minor,total_minor,outstanding_minor FROM finance_invoices WHERE id=?",
      )
      .bind(newBill!.invoice_id)
      .first(),
  ).toEqual({
    status: "posted",
    subtotal_minor: 8001,
    tax_minor: 560,
    total_minor: 8561,
    outstanding_minor: 8561,
  });
  for (const chart of [f.chart, s.chart]) {
    const tb = await trialBalance(f.owner, undefined, chart),
      find = (account: string) =>
        tb.find(
          (x) =>
            x.id ===
            (chart === f.chart
              ? account
              : s.mappings.find((m) => m.primaryAccountId === account)!
                  .secondaryAccountId),
        )?.balance;
    expect(find(ar)).toBe("45.60");
    expect(find(income)).toBe("0.00");
    expect(find(deferred)).toBe("-80.01");
    expect(find(tax)).toBe("-5.60");
    expect(find(f.accounts.cash)).toBe("40.01");
  }
  expect(
    await db
      .prepare(
        `${invoiceHistoryCte} SELECT historical_outstanding_minor FROM historical_invoice_balances WHERE id=?`,
      )
      .bind(f.company, "2026-09-29", bill.invoiceId)
      .first("historical_outstanding_minor"),
  ).toBe(6700);
  expect(
    await db
      .prepare(
        `${invoiceHistoryCte} SELECT historical_outstanding_minor FROM historical_invoice_balances WHERE id=?`,
      )
      .bind(f.company, "2026-09-30", bill.invoiceId)
      .first("historical_outstanding_minor"),
  ).toBe(-4001);
  await expect(
    prepareRecognitionRun(
      f.owner,
      {
        billingRunId: String(bill.id),
        itemKey: "delivery:2026-09-30",
        periodId: f.period,
        entryDate: "2026-09-30",
        reason: "Old credited plan cannot continue",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "RECOGNITION_BILL_CREDITED" });
  await expect(
    reverseJournal(f.manager, String(result.creditJournalId), key(), {
      periodId: f.period,
      entryDate: "2026-09-30",
    }),
  ).rejects.toMatchObject({ code: "REVERSE_AT_SOURCE" });
  expect(
    (await vatWorkspace(f.manager, { filingMonth: "2026-09" })).reconciliation,
  ).toMatchObject({ ready: false, missing: 2 });
  await vat("billing_credit", String(draft.id), 2, "2026-09-30");
  await vat("invoice", String(newBill!.invoice_id), 2, "2026-09-30");
  expect(
    (await vatWorkspace(f.manager, { filingMonth: "2026-09" })).reconciliation,
  ).toMatchObject({ ready: true, missing: 0 });
  await approveRefund(
    f.manager,
    String(result.refundId),
    {
      expectedVersion: 1,
      reason: "Reviewed original customer and retained refund balance",
      beneficiaryName: "Fictional customer",
      beneficiaryAccount: "FIXTURE-ONLY",
    },
    key(),
  );
  const refund = await prepareRefundInstruction(
    f.payer,
    String(result.refundId),
    {
      expectedVersion: 2,
      amount: "40.01",
      plannedDate: "2026-09-30",
      bankAccount: f.accounts.cash,
      cashAccountId: f.accounts.cash,
    },
    key(),
  );
  await exportRefundInstructions(
    f.payer,
    { instructions: [{ id: String(refund.id), expectedVersion: 1 }] },
    key(),
  );
  const refundStatement = await importBankStatement(
      f.owner,
      {
        bankAccount: f.accounts.cash,
        statementNumber: key(),
        statementDate: "2026-09-30",
        opening: "40.01",
        closing: "0",
        sourceHash: key(),
        rows: [
          {
            externalId: key(),
            transactionDate: "2026-09-30",
            amount: "-40.01",
            currency: "THB",
          },
        ],
      },
      key(),
    ),
    refundBank = String(
      await db
        .prepare(
          "SELECT id FROM finance_bank_transactions WHERE statement_id=?",
        )
        .bind(refundStatement.id)
        .first("id"),
    );
  await confirmRefundInstruction(
    f.payer,
    String(refund.id),
    refundConfirmationSchema.parse({
      expectedVersion: 2,
      bankTransactionId: refundBank,
      bankVersion: 1,
      periodId: f.period,
      bankReference: key(),
      evidence: "Exact fictional outgoing bank debit; no real transfer",
    }),
    key(),
  );
  expect(
    (await refundWorkspace(f.payer, String(result.refundId))).owner,
  ).toMatchObject({ status: "settled", outstanding_minor: 0 });
  expect(
    await db
      .prepare(
        `${invoiceHistoryCte} SELECT historical_outstanding_minor FROM historical_invoice_balances WHERE id=?`,
      )
      .bind(f.company, "2026-09-30", bill.invoiceId)
      .first("historical_outstanding_minor"),
  ).toBe(0);
  for (const chart of [f.chart, s.chart]) {
    const tb = await trialBalance(f.owner, undefined, chart);
    expect(tb.find((x) => x.code === "AR")?.balance).toBe("85.61");
    expect(
      tb.find(
        (x) =>
          x.id ===
          (chart === f.chart
            ? f.accounts.cash
            : s.mappings.find((m) => m.primaryAccountId === f.accounts.cash)!
                .secondaryAccountId),
      )?.balance,
    ).toBe("0.00");
  }
  await expect(
    reversePayment(
      f.manager,
      String(payment.id),
      {
        expectedVersion: 4,
        periodId: f.period,
        entryDate: "2026-09-30",
        reason: "Credited receipts require reconciliation with their refund",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "PAYMENT_CREDITED_SOURCE_REVIEW" });
  const calendarInstruction=await db.prepare("SELECT id,planned_date FROM finance_refund_instructions WHERE obligation_id=? ORDER BY id LIMIT 1").bind(result.refundId).first<{id:string;planned_date:string}>();
  expect(calendarInstruction).not.toBeNull();
  const calendar=await calendarData(db,f.payer,{date:calendarInstruction!.planned_date,view:"day",layers:"refunds",mode:"team"});
  expect(calendar.sources).toContainEqual({source:"refunds",status:"ready"});expect(calendar.items.some(item=>item.sourceId===calendarInstruction!.id)).toBe(true);
});
it("blocks self review, cross-Company access and stale evidence while retaining a rejected correction without GL", async () => {
  const bill = await billed(),
    draft = await prepareBillingCorrection(
      f.owner,
      String(bill.id),
      correction(),
      key(),
    ),
    decision = {
      expectedVersion: 1,
      reason: "Review the exact retained correction source",
    };
  await expect(
    decideBillingCorrection(
      { ...f.owner, role: "finance_approver" },
      String(draft.id),
      decision,
      "post",
      key(),
    ),
  ).rejects.toMatchObject({ code: "INDEPENDENT_APPROVER_REQUIRED" });
  await expect(
    decideBillingCorrection(
      { ...f.manager, companyId: "other" },
      String(draft.id),
      decision,
      "post",
      key(),
    ),
  ).rejects.toMatchObject({ code: "BILLING_CORRECTION_NOT_FOUND" });
  const r = await prepareRecognitionRun(
    f.owner,
    {
      billingRunId: String(bill.id),
      itemKey: "support:2026-09-30",
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "Changed delivered support evidence",
    },
    key(),
  );
  await decideRecognitionRun(
    f.manager,
    String(r.id),
    {
      expectedVersion: 1,
      reason: "Review delivered support after correction prepared",
    },
    "post",
    key(),
  );
  await expect(
    decideBillingCorrection(
      f.manager,
      String(draft.id),
      decision,
      "post",
      key(),
    ),
  ).rejects.toMatchObject({ code: "BILLING_CORRECTION_SOURCE_CHANGED" });
  await decideBillingCorrection(
    f.manager,
    String(draft.id),
    decision,
    "reject",
    key(),
  );
  expect(
    await db
      .prepare(
        "SELECT count(*) n FROM finance_journals WHERE company_id=? AND source_type='business_billing_credit'",
      )
      .bind(f.company)
      .first("n"),
  ).toBe(0);
  expect(
    await db
      .prepare("SELECT outstanding_minor FROM finance_invoices WHERE id=?")
      .bind(bill.invoiceId)
      .first("outstanding_minor"),
  ).toBe(10701);
  await expect(
    db
      .prepare(
        "UPDATE finance_billing_corrections SET source_hash='tampered' WHERE id=?",
      )
      .bind(draft.id)
      .run(),
  ).rejects.toThrow("BILLING_CORRECTION_EVIDENCE_IMMUTABLE");
});
it("reconciles two original receipts and two partial refunds at their actual dates in both books without reopening the credited invoice", async () => {
  const s = await secondary(),
    bill = await billed(),
    receiptAmounts = ["23.45", "36.55"];
  for (const [index, amount] of receiptAmounts.entries()) {
    const date = `2026-09-${20 + index}`,
      payment = await createPayment(
        f.owner,
        {
          partyId: party,
          kind: "receipt",
          paymentNumber: key(),
          paymentDate: date,
          currency: "THB",
          amount,
          bankAccount: f.accounts.cash,
        },
        key(),
      );
    await approvePayment(
      f.manager,
      String(payment.id),
      {
        expectedVersion: 1,
        proof: "Independent exact synthetic partial receipt reviewed",
      },
      key(),
    );
    await releasePayment(
      f.payer,
      String(payment.id),
      {
        expectedVersion: 2,
        proof: "Synthetic receipt prepared by independent treasury",
      },
      key(),
    );
    const statement = await importBankStatement(
        f.owner,
        {
          bankAccount: f.accounts.cash,
          statementNumber: key(),
          statementDate: date,
          opening: index === 0 ? "0" : "23.45",
          closing: index === 0 ? "23.45" : "60.00",
          sourceHash: key(),
          rows: [
            {
              externalId: key(),
              transactionDate: date,
              amount,
              currency: "THB",
            },
          ],
        },
        key(),
      ),
      bank = String(
        await db
          .prepare(
            "SELECT id FROM finance_bank_transactions WHERE statement_id=?",
          )
          .bind(statement.id)
          .first("id"),
      );
    await confirmPayment(
      f.payer,
      String(payment.id),
      {
        expectedVersion: 3,
        periodId: f.period,
        bankTransactionId: bank,
        bankVersion: 1,
        bankReference: key(),
        proof: "Exact synthetic bank partial receipt",
        allocations: [{ invoiceId: String(bill.invoiceId), amount }],
      },
      key(),
    );
  }
  const draft = await prepareBillingCorrection(
      f.owner,
      String(bill.id),
      correction(4),
      key(),
    ),
    posted = await decideBillingCorrection(
      f.manager,
      String(draft.id),
      {
        expectedVersion: 1,
        reason:
          "Full credit and corrected price after both retained partial receipts reviewed",
      },
      "post",
      key(),
    );
  expect(posted).toMatchObject({ creditedMinor: 4701, refundMinor: 6000 });
  await approveRefund(
    f.manager,
    String(posted.refundId),
    {
      expectedVersion: 1,
      reason: "Both original receipts reconcile to a sixty baht refund",
      beneficiaryName: "Fictional partial-refund customer",
      beneficiaryAccount: "FIXTURE-ONLY",
    },
    key(),
  );
  const history = async (date: string) =>
    db
      .prepare(
        `${invoiceHistoryCte} SELECT historical_outstanding_minor FROM historical_invoice_balances WHERE id=?`,
      )
      .bind(f.company, date, bill.invoiceId)
      .first("historical_outstanding_minor");
  expect(await history("2026-09-29")).toBe(4701);
  expect(await history("2026-09-30")).toBe(-6000);
  const bankStatement = await importBankStatement(
    f.owner,
    {
      bankAccount: f.accounts.cash,
      statementNumber: key(),
      statementDate: "2026-10-01",
      opening: "60.00",
      closing: "0",
      sourceHash: key(),
      rows: [
        {
          externalId: "refund-one",
          transactionDate: "2026-09-30",
          amount: "-20.01",
          currency: "THB",
        },
        {
          externalId: "refund-two",
          transactionDate: "2026-10-01",
          amount: "-39.99",
          currency: "THB",
        },
      ],
    },
    key(),
  );
  const bankRows = (
    await db
      .prepare(
        "SELECT id,transaction_date FROM finance_bank_transactions WHERE statement_id=? ORDER BY transaction_date",
      )
      .bind(bankStatement.id)
      .all<{ id: string; transaction_date: string }>()
  ).results;
  for (const [index, amount] of ["20.01", "39.99"].entries()) {
    const owner = await refundWorkspace(f.payer, String(posted.refundId)),
      instruction = await prepareRefundInstruction(
        f.payer,
        String(posted.refundId),
        {
          expectedVersion: Number(owner.owner.version),
          amount,
          plannedDate: bankRows[index].transaction_date,
          bankAccount: f.accounts.cash,
          cashAccountId: f.accounts.cash,
        },
        key(),
      );
    await exportRefundInstructions(
      f.payer,
      { instructions: [{ id: String(instruction.id), expectedVersion: 1 }] },
      key(),
    );
    const input = refundConfirmationSchema.parse({
        expectedVersion: 2,
        bankTransactionId: bankRows[index].id,
        bankVersion: 1,
        periodId: f.period,
        bankReference: key(),
        evidence: "Exact dated synthetic partial refund debit",
      }),
      command = key();
    const result = await confirmRefundInstruction(
      f.payer,
      String(instruction.id),
      input,
      command,
    );
    expect(
      await confirmRefundInstruction(
        f.payer,
        String(instruction.id),
        input,
        command,
      ),
    ).toEqual(result);
    for (const chart of [f.chart, s.chart]) {
      const tb = await trialBalance(f.owner, undefined, chart);
      expect(tb.find((row) => row.code === "AR")?.balance).toBe(
        index === 0 ? "45.62" : "85.61",
      );
      expect(
        tb.find(
          (row) =>
            row.id ===
            (chart === f.chart
              ? f.accounts.cash
              : s.mappings.find((m) => m.primaryAccountId === f.accounts.cash)!
                  .secondaryAccountId),
        )?.balance,
      ).toBe(index === 0 ? "39.99" : "0.00");
    }
    expect(
      (await refundWorkspace(f.payer, String(posted.refundId))).owner,
    ).toMatchObject({
      status: index === 0 ? "approved" : "settled",
      outstanding_minor: index === 0 ? 3999 : 0,
    });
  }
  expect(await history("2026-09-30")).toBe(-3999);
  expect(await history("2026-10-01")).toBe(0);
  expect(
    await db
      .prepare(
        "SELECT status,outstanding_minor FROM finance_invoices WHERE id=?",
      )
      .bind(bill.invoiceId)
      .first(),
  ).toMatchObject({ status: "paid", outstanding_minor: 0 });
  expect(
    await db
      .prepare(
        "SELECT count(*) n FROM finance_payments p JOIN finance_refund_instructions i ON i.payment_id=p.id WHERE i.obligation_id=?",
      )
      .bind(posted.refundId)
      .first("n"),
  ).toBe(2);
});
it("rolls back every book, credit and replacement on failure and allows only one concurrent reviewed correction", async () => {
  const bill = await billed(),
    draft = await prepareBillingCorrection(
      f.owner,
      String(bill.id),
      correction(),
      key(),
    ),
    decision = {
      expectedVersion: 1,
      reason: "Concurrent independent reviewed correction",
    };
  const failedCommand = key(),
    replacementInvoice = `invoice-${failedCommand}:replacement-invoice`;
  await db
    .prepare(
      `ALTER TABLE finance_journals ADD CONSTRAINT billing_correction_fixture_fault CHECK (company_id<>'${f.company}' OR source_type<>'invoice' OR source_id<>'${replacementInvoice}')`,
    )
    .run();
  // A failure at replacement posting occurs after the owner prepared its credit;
  // none of those steps may escape the outer transaction.
  try {
    await expect(
      decideBillingCorrection(
        f.manager,
        String(draft.id),
        decision,
        "post",
        failedCommand,
      ),
    ).rejects.toThrow();
  } finally {
    await db
      .prepare(
        "ALTER TABLE finance_journals DROP CONSTRAINT billing_correction_fixture_fault",
      )
      .run();
  }
  expect(
    await db
      .prepare("SELECT status FROM finance_billing_corrections WHERE id=?")
      .bind(draft.id)
      .first("status"),
  ).toBe("draft");
  expect(
    await db
      .prepare(
        "SELECT count(*) n FROM finance_business_billing_runs WHERE parent_run_id=?",
      )
      .bind(bill.id)
      .first("n"),
  ).toBe(0);
  const results = await Promise.allSettled([
    decideBillingCorrection(
      f.manager,
      String(draft.id),
      decision,
      "post",
      key(),
    ),
    decideBillingCorrection(
      f.manager,
      String(draft.id),
      decision,
      "post",
      key(),
    ),
  ]);
  expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
  expect(
    await db
      .prepare(
        "SELECT count(*) n FROM finance_business_billing_runs WHERE parent_run_id=?",
      )
      .bind(bill.id)
      .first("n"),
  ).toBe(1);
});

afterEach(()=>vi.unstubAllEnvs());

it("purchased draining billing correction keeps the replacement and two books atomic through late reviewer revocation",async()=>{
 const second=await secondary();await reviewedFinanceGrant(db,f.company,"finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const bill=await billed();
 await reviewedFinanceGrant(db,f.company,"finance","draining");const draft=await prepareBillingCorrection(f.owner,String(bill.id),correction(),key());
 const command=key(),decision={expectedVersion:1,reason:"Independent retained full credit and replacement review"},trigger=`correction_purchase_${key().replaceAll("-","")}`;
 const snapshots=async()=>Promise.all(["finance_journals","finance_journal_lines","finance_invoices","finance_business_billing_runs","finance_billing_corrections","money_transaction_snapshots","command_receipts","audit_events","outbox_events"].map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshots();
 await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${command}') EXECUTE FUNCTION ${trigger}()`);
 try{await expect(decideBillingCorrection(f.manager,String(draft.id),decision,"post",command)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(await snapshots()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${trigger} ON outbox_events;DROP FUNCTION ${trigger}()`);}
 const result=await decideBillingCorrection(f.manager,String(draft.id),decision,"post",command);expect(await decideBillingCorrection(f.manager,String(draft.id),decision,"post",command)).toEqual(result);
 expect(await db.prepare("SELECT i.total_minor,i.status FROM finance_business_billing_runs b JOIN finance_invoices i ON i.id=b.invoice_id WHERE b.id=?").bind(result.replacementBillId).first()).toEqual({total_minor:8561,status:"posted"});
 expect(await db.prepare("SELECT count(*) n FROM finance_journals WHERE chart_id=? AND source_type='book_projection' AND source_id=? AND status='posted'").bind(second.chart,result.creditJournalId).first("n")).toBe(1);
 expect(await db.prepare("SELECT count(*) n FROM command_assertions").first("n")).toBe(0);
});
