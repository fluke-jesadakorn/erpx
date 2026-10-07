import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { setFinanceCompanyTool } from "../src/server/finance-company-tools";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  createBankCsvMapping,
  importMappedBankCsv,
  bankCsvMappingSchema,
  proposeBankMatchGroup,
  decideBankMatchGroup,
  bankAutomationWorkspace,
} from "../src/server/finance-bank-automation";
import {
  prepareCollectionCase,
  decideCollectionCase,
  reviseCollectionCase,
  collectionWorkspace,
  cashDueForecast,
} from "../src/server/finance-collections";
import {
  createJournal,
  createJournalSchema,
  postJournal,
} from "../src/server/finance";
vi.mock("../src/server/runtime", () => ({
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
      tool: "bank_automation",
      enabled: true,
      expectedVersion: 0,
      reason: "Disposable engineering fixture",
    },
    key(),
  );
  await setFinanceCompanyTool(
    f.owner,
    {
      tool: "collections",
      enabled: true,
      expectedVersion: 0,
      reason: "Disposable engineering fixture",
    },
    key(),
  );
  await db
    .prepare(
      "UPDATE finance_accounts SET control_kind='bank',is_control=1 WHERE id=?",
    )
    .bind(f.accounts.cash)
    .run();
});
async function receipt(amount: string) {
  const j = await createJournal(
    f.owner,
    createJournalSchema.parse({
      chartId: f.chart,
      periodId: f.period,
      entryDate: "2026-09-04",
      currency: "THB",
      lines: [
        { accountId: f.accounts.cash, debit: amount, credit: "0" },
        { accountId: f.accounts.revenue, debit: "0", credit: amount },
      ],
    }),
    key(),
  );
  await postJournal(f.manager, String(j.id), key(), { expectedVersion: 1 });
  return String(
    await db
      .prepare(
        "SELECT id FROM finance_journal_lines WHERE journal_id=? AND account_id=?",
      )
      .bind(j.id, f.accounts.cash)
      .first("id"),
  );
}
async function statement() {
  const mapping = await createBankCsvMapping(
    f.owner,
    bankCsvMappingSchema.parse({
      name: "Bank export",
      columns: {
        externalId: "Reference",
        transactionDate: "Date",
        amount: "Amount",
        description: "Details",
      },
      dateFormat: "DD/MM/YYYY",
    }),
    key(),
  );
  const input = {
    mappingId: String(mapping.id),
    bankAccountId: f.accounts.cash,
    statementNumber: key(),
    statementDate: "2026-09-30",
    opening: "0",
    closing: "100.00",
    csv: 'Reference,Date,Amount,Details\r\nbank1,04/09/2026,100.00,"Customer, combined receipt"\r\n',
  };
  const k = key(),
    r = await importMappedBankCsv(f.owner, input, k);
  expect(await importMappedBankCsv(f.owner, input, k)).toEqual(r);
  const bank = await db
    .prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?")
    .bind(r.id)
    .first<{ id: string }>();
  return { input, statement: r, bank: bank!.id };
}
it("imports a saved mapping atomically, rejects duplicate files and proposes one bank movement to multiple posted lines", async () => {
  const a = await receipt("60.01"),
    b = await receipt("39.99"),
    s = await statement();
  await expect(
    importMappedBankCsv(f.owner, { ...s.input, statementNumber: key() }, key()),
  ).rejects.toMatchObject({code:"BANK_CSV_ALREADY_IMPORTED"});
  expect(
    await db
      .prepare(
        "SELECT COUNT(*) value FROM finance_bank_statements WHERE company_id=?",
      )
      .bind(f.company)
      .first("value"),
  ).toBe(1);
  const workspace = await bankAutomationWorkspace(f.owner);
  expect(workspace.suggestions).toContainEqual(
    expect.objectContaining({
      bankTransactionIds: [s.bank],
      journalLineIds: expect.arrayContaining([a, b]),
      confidence: "same_day_total",
    }),
  );
  const proposal = await proposeBankMatchGroup(
    f.owner,
    {
      bankTransactionIds: [s.bank],
      journalLineIds: [a, b],
      reason: "Combined customer bank receipt",
    },
    key(),
  );
  const self = {
    ...f.owner,
    role: "finance_approver",
    assignmentId: await f.grant(f.owner, "finance_approver"),
  };
  await expect(
    decideBankMatchGroup(
      self,
      String(proposal.id),
      { expectedVersion: 1, reason: "Self review refused" },
      "confirm",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  const k = key(),
    input = { expectedVersion: 1, reason: "Verified both cash receipts" };
  const matched = await decideBankMatchGroup(
    f.manager,
    String(proposal.id),
    input,
    "confirm",
    k,
  );
  expect(
    await decideBankMatchGroup(
      f.manager,
      String(proposal.id),
      input,
      "confirm",
      k,
    ),
  ).toEqual(matched);
  expect(
    await db
      .prepare(
        "SELECT SUM(amount_minor) value FROM finance_bank_match_allocations WHERE group_id=?",
      )
      .bind(proposal.id)
      .first("value"),
  ).toBe(10000);
  expect(
    await db
      .prepare("SELECT match_status FROM finance_bank_transactions WHERE id=?")
      .bind(s.bank)
      .first("match_status"),
  ).toBe("matched");
  await decideBankMatchGroup(
    f.manager,
    String(proposal.id),
    { expectedVersion: 2, reason: "Correct review decision" },
    "reverse",
    key(),
  );
  expect(
    await db
      .prepare("SELECT match_status FROM finance_bank_transactions WHERE id=?")
      .bind(s.bank)
      .first("match_status"),
  ).toBe("unmatched");
  expect(
    await db
      .prepare(
        "SELECT COUNT(*) value FROM finance_bank_match_targets WHERE group_id=?",
      )
      .bind(proposal.id)
      .first("value"),
  ).toBe(0);
});
it("prevents cross Company matches and stale approvals from consuming the same movement twice", async () => {
  const a = await receipt("100"),
    s = await statement();
  const p1 = await proposeBankMatchGroup(
      f.owner,
      {
        bankTransactionIds: [s.bank],
        journalLineIds: [a],
        reason: "Matching amount and date",
      },
      key(),
    ),
    p2 = await proposeBankMatchGroup(
      f.owner,
      {
        bankTransactionIds: [s.bank],
        journalLineIds: [a],
        reason: "Competing proposal",
      },
      key(),
    );
  await decideBankMatchGroup(
    f.manager,
    String(p1.id),
    { expectedVersion: 1, reason: "Reviewed" },
    "confirm",
    key(),
  );
  await expect(
    decideBankMatchGroup(
      f.manager,
      String(p2.id),
      { expectedVersion: 1, reason: "Reviewed" },
      "confirm",
      key(),
    ),
  ).rejects.toMatchObject({ code: "BANK_ROW_UNAVAILABLE" });
  await expect(
    decideBankMatchGroup(
      { ...f.manager, companyId: key() },
      String(p1.id),
      { expectedVersion: 2, reason: "Wrong company" },
      "reverse",
      key(),
    ),
  ).rejects.toMatchObject({ code: "BANK_GROUP_NOT_FOUND" });
});
it("pages every bank movement, cash line and group without treating a hidden duplicate as a unique suggestion", async () => {
  const original = await receipt("100.00"),
    s = await statement();
  const background = await createJournal(
    f.owner,
    createJournalSchema.parse({
      chartId: f.chart,
      periodId: f.period,
      entryDate: "2026-09-05",
      currency: "THB",
      lines: [
        ...Array.from({ length: 201 }, (_, index) => ({
          accountId: f.accounts.cash,
          debit: "1.00",
          credit: "0",
          description: `Paged cash ${index}`,
        })),
        { accountId: f.accounts.revenue, debit: "0", credit: "201.00" },
      ],
    }),
    key(),
  );
  await postJournal(f.manager, String(background.id), key(), {
    expectedVersion: 1,
  });
  let view = await bankAutomationWorkspace(f.owner);
  expect(view.targets).toHaveLength(30);
  expect(view.targets.some((row) => row.id === original)).toBe(false);
  expect(
    view.suggestions.find((row) => row.bankTransactionIds[0] === s.bank)
      ?.journalLineIds,
  ).toEqual([original]);
  expect(view.suggestionTargets.some((row) => row.id === original)).toBe(true);
  const originalNumber = await db
    .prepare(
      "SELECT j.journal_number FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE l.id=?",
    )
    .bind(original)
    .first<string>("journal_number");
  await receipt("100.00");
  view = await bankAutomationWorkspace(f.owner, { cashQuery: originalNumber! });
  expect(view.targets).toHaveLength(1);
  expect(view.targets[0].id).toBe(original);
  expect(
    view.suggestions.find((row) => row.bankTransactionIds[0] === s.bank)
      ?.confidence,
  ).toBe("needs_review");
  await importMappedBankCsv(
    f.owner,
    {
      ...s.input,
      statementNumber: key(),
      opening: "100.00",
      closing: "301.00",
      csv:
        "Reference,Date,Amount,Details\n" +
        Array.from(
          { length: 201 },
          (_, index) => `paged-${index},06/09/2026,1.00,Paged bank ${index}`,
        ).join("\n"),
    },
    key(),
  );
  const first = await bankAutomationWorkspace(f.owner),
    second = await bankAutomationWorkspace(f.owner, {
      bankCursor: first.pages.bank.nextCursor!,
    });
  const back = await bankAutomationWorkspace(f.owner, {
    bankCursor: second.pages.bank.previousCursor!,
  });
  expect(back.rows.map((row) => row.id)).toEqual(
    first.rows.map((row) => row.id),
  );
  const bankIds: string[] = [],
    cashIds: string[] = [];
  let bankCursor: string | undefined, cashCursor: string | undefined;
  do {
    const page = await bankAutomationWorkspace(f.owner, { bankCursor });
    bankIds.push(...page.rows.map((row) => row.id));
    bankCursor = page.pages.bank.nextCursor ?? undefined;
  } while (bankCursor);
  do {
    const page = await bankAutomationWorkspace(f.owner, { cashCursor });
    cashIds.push(...page.targets.map((row) => row.id));
    cashCursor = page.pages.cash.nextCursor ?? undefined;
  } while (cashCursor);
  expect(bankIds).toHaveLength(202);
  expect(new Set(bankIds).size).toBe(202);
  expect(cashIds).toHaveLength(203);
  expect(new Set(cashIds).size).toBe(203);
  await expect(
    bankAutomationWorkspace(
      { ...f.owner, companyId: key() },
      { bankCursor: first.pages.bank.nextCursor! },
    ),
  ).rejects.toMatchObject({ code: "INVALID_CURSOR" });
  await expect(
    bankAutomationWorkspace(f.owner, {
      cashCursor: first.pages.cash.nextCursor!,
      cashQuery: "Changed search",
    }),
  ).rejects.toMatchObject({ code: "INVALID_CURSOR" });
  for (let index = 0; index < 201; index++)
    await proposeBankMatchGroup(
      f.owner,
      {
        bankTransactionIds: [s.bank],
        journalLineIds: [original],
        reason: `Independent proposed review edition ${index}`,
      },
      key(),
    );
  const groupIds: string[] = [];
  let groupCursor: string | undefined;
  do {
    const page = await bankAutomationWorkspace(f.owner, { groupCursor });
    groupIds.push(...page.groups.map((row) => row.id));
    groupCursor = page.pages.groups.nextCursor ?? undefined;
  } while (groupCursor);
  expect(groupIds).toHaveLength(201);
  expect(new Set(groupIds).size).toBe(201);
}, 60_000);
async function obligation() {
  const party = key(),
    invoice = key(),
    receivable = key();
  await db
    .prepare(
      "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,'AR','Receivable','asset','THB',1,'receivable',1,1)",
    )
    .bind(receivable, f.chart, f.company)
    .run();
  await db
    .prepare(
      "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Collection customer','THB',1,1)",
    )
    .bind(party, f.company, party)
    .run();
  const j = await createJournal(
    f.owner,
    createJournalSchema.parse({
      chartId: f.chart,
      periodId: f.period,
      entryDate: "2026-09-02",
      currency: "THB",
      lines: [
        { accountId: receivable, debit: "100.01", credit: "0" },
        { accountId: f.accounts.revenue, debit: "0", credit: "100.01" },
      ],
    }),
    key(),
  );
  await postJournal(f.manager, String(j.id), key(), { expectedVersion: 1 });
  await db
    .prepare(
      "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,journal_id,status,created_by,created_at,updated_at) VALUES(?,?,?,'receivable',?,'2026-09-02','2026-09-03','THB',10001,10001,10001,10001,10001,?,?,?,'posted',?,1,1)",
    )
    .bind(
      invoice,
      f.company,
      party,
      invoice,
      receivable,
      f.accounts.revenue,
      j.id,
      f.owner.userId,
    )
    .run();
  return invoice;
}
it("prepares reviewed collection text, records test delivery once, and forecasts the due carrying amount", async () => {
  const invoice = await obligation(),
    r = await prepareCollectionCase(
      f.owner,
      { invoiceId: invoice, expectedVersion: 1, asOf: "2026-09-30" },
      key(),
    );
  expect(r.message).toContain("THB 100.01");
  await decideCollectionCase(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Customer and amount checked" },
    "approve",
    key(),
  );
  const k = key(),
    input = { expectedVersion: 2, reason: "Test the delivery adapter" };
  const delivered = await decideCollectionCase(
    f.owner,
    String(r.id),
    input,
    "test-deliver",
    k,
  );
  expect(
    await decideCollectionCase(f.owner, String(r.id), input, "test-deliver", k),
  ).toEqual(delivered);
  expect(delivered).toMatchObject({
    status: "test_delivered",
    adapter: "test",
  });
  expect(
    await db
      .prepare(
        "SELECT COUNT(*) value FROM finance_collection_deliveries WHERE case_id=?",
      )
      .bind(r.id)
      .first("value"),
  ).toBe(1);
  const forecast = await cashDueForecast(f.owner, {
    chartId: f.chart,
    asOf: "2026-09-30",
  });
  expect(forecast.months).toHaveLength(12);
  expect(forecast.months[0]).toMatchObject({
    movement: "100.01",
    invoiceIds: [invoice],
  });
});
it("blocks stale collection amounts and self approval", async () => {
  const invoice = await obligation(),
    r = await prepareCollectionCase(
      f.owner,
      { invoiceId: invoice, expectedVersion: 1, asOf: "2026-09-30" },
      key(),
    ),
    self = {
      ...f.owner,
      role: "finance_approver",
      assignmentId: await f.grant(f.owner, "finance_approver"),
    };
  await expect(
    decideCollectionCase(
      self,
      String(r.id),
      { expectedVersion: 1, reason: "Self review refused" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await db
    .prepare(
      "UPDATE finance_invoices SET outstanding_minor=5000,version=version+1 WHERE id=?",
    )
    .bind(invoice)
    .run();
  await expect(
    decideCollectionCase(
      f.manager,
      String(r.id),
      { expectedVersion: 1, reason: "Source changed" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "STALE_COLLECTION_EVIDENCE" });
});
it("retains independently reviewed unsent message editions and delivers the current edition once", async () => {
  const invoice = await obligation(),
    original = await prepareCollectionCase(
      f.owner,
      {
        invoiceId: invoice,
        expectedVersion: 1,
        asOf: "2026-09-30",
        message: "Original fictional message for THB 100.01",
      },
      key(),
    );
  await decideCollectionCase(
    f.manager,
    String(original.id),
    { expectedVersion: 1, reason: "Original text checked independently" },
    "approve",
    key(),
  );
  const input = {
      expectedVersion: 2,
      expectedInvoiceVersion: 1,
      asOf: "2026-09-30",
      message: "Revised fictional message: please confirm THB 100.01 due",
      reason: "Clarify the unsent request before delivery",
    },
    command = key();
  const revised = await reviseCollectionCase(
    f.owner,
    String(original.id),
    input,
    command,
  );
  expect(
    await reviseCollectionCase(f.owner, String(original.id), input, command),
  ).toEqual(revised);
  expect(
    await db
      .prepare(
        "SELECT message,status,reviewed_by,review_reason FROM finance_collection_cases WHERE id=?",
      )
      .bind(original.id)
      .first(),
  ).toEqual({
    message: "Original fictional message for THB 100.01",
    status: "superseded",
    reviewed_by: f.manager.userId,
    review_reason: "Original text checked independently",
  });
  expect(
    await db
      .prepare(
        "SELECT root_id,parent_case_id,parent_version,revision_number FROM finance_collection_cases WHERE id=?",
      )
      .bind(revised.id)
      .first(),
  ).toEqual({
    root_id: original.id,
    parent_case_id: original.id,
    parent_version: 3,
    revision_number: 2,
  });
  await expect(
    prepareCollectionCase(
      f.owner,
      { invoiceId: invoice, expectedVersion: 1, asOf: "2026-09-30" },
      key(),
    ),
  ).rejects.toMatchObject({ code: "COLLECTION_CASE_ALREADY_PREPARED" });
  await expect(
    decideCollectionCase(
      f.owner,
      String(original.id),
      { expectedVersion: 3, reason: "Old text must remain retired" },
      "test-deliver",
      key(),
    ),
  ).rejects.toMatchObject({ code: "STALE_VERSION" });
  const self = {
    ...f.owner,
    role: "finance_approver",
    assignmentId: await f.grant(f.owner, "finance_approver"),
  };
  await expect(
    decideCollectionCase(
      self,
      String(revised.id),
      { expectedVersion: 1, reason: "Same preparer cannot approve" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await decideCollectionCase(
    f.manager,
    String(revised.id),
    {
      expectedVersion: 1,
      reason: "Revised message and invoice independently checked",
    },
    "approve",
    key(),
  );
  const delivery = key(),
    deliveryInput = {
      expectedVersion: 2,
      reason: "Only the current fictional edition is test-delivered",
    };
  await decideCollectionCase(
    f.owner,
    String(revised.id),
    deliveryInput,
    "test-deliver",
    delivery,
  );
  await decideCollectionCase(
    f.owner,
    String(revised.id),
    deliveryInput,
    "test-deliver",
    delivery,
  );
  expect(
    await db
      .prepare(
        "SELECT COUNT(*) value FROM finance_collection_deliveries d JOIN finance_collection_cases c ON c.id=d.case_id WHERE c.root_id=?",
      )
      .bind(original.id)
      .first("value"),
  ).toBe(1);
  await expect(
    reviseCollectionCase(
      f.owner,
      String(revised.id),
      { ...input, expectedVersion: 3 },
      key(),
    ),
  ).rejects.toMatchObject({ code: "COLLECTION_EDITION_CHANGED" });
  await expect(
    decideCollectionCase(
      f.owner,
      String(revised.id),
      { expectedVersion: 3, reason: "Retain delivered history" },
      "cancel",
      key(),
    ),
  ).rejects.toMatchObject({ code: "STALE_VERSION" });
  await expect(
    db
      .prepare(
        "UPDATE finance_collection_cases SET message='Lost history' WHERE id=?",
      )
      .bind(original.id)
      .run(),
  ).rejects.toThrow(/COLLECTION_SOURCE_IMMUTABLE/);
});
it("refreshes changed customer evidence only into a new unsent edition and rejects stale or concurrent revisions", async () => {
  const invoice = await obligation(),
    original = await prepareCollectionCase(
      f.owner,
      { invoiceId: invoice, expectedVersion: 1, asOf: "2026-09-30" },
      key(),
    );
  await db
    .prepare(
      "UPDATE finance_parties SET name='Revised fictional customer',version=version+1 WHERE id=(SELECT party_id FROM finance_invoices WHERE id=?)",
    )
    .bind(invoice)
    .run();
  await expect(
    decideCollectionCase(
      f.manager,
      String(original.id),
      { expectedVersion: 1, reason: "Changed customer is stale evidence" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "STALE_COLLECTION_EVIDENCE" });
  const input = {
    expectedVersion: 1,
    expectedInvoiceVersion: 1,
    asOf: "2026-09-30",
    reason: "Refresh changed customer evidence",
  };
  await expect(
    reviseCollectionCase(
      { ...f.owner, companyId: key() },
      String(original.id),
      input,
      key(),
    ),
  ).rejects.toMatchObject({ code: "COLLECTION_CASE_NOT_FOUND" });
  await expect(
    reviseCollectionCase(
      f.owner,
      String(original.id),
      { ...input, expectedInvoiceVersion: 2 },
      key(),
    ),
  ).rejects.toMatchObject({ code: "STALE_VERSION" });
  const attempts = await Promise.allSettled([
    reviseCollectionCase(f.owner, String(original.id), input, key()),
    reviseCollectionCase(f.owner, String(original.id), input, key()),
  ]);
  expect(
    attempts.filter((result) => result.status === "fulfilled"),
  ).toHaveLength(1);
  const revised = attempts.find(
    (result) => result.status === "fulfilled",
  )! as PromiseFulfilledResult<
    Awaited<ReturnType<typeof reviseCollectionCase>>
  >;
  expect(revised.value.message).toContain("Revised fictional customer");
  await decideCollectionCase(
    f.manager,
    String(revised.value.id),
    {
      expectedVersion: 1,
      reason: "Fresh customer evidence and retained amount checked",
    },
    "approve",
    key(),
  );
  const data = await collectionWorkspace(f.owner, "2026-09-30");
  expect(data.cases.find((row) => row.id === original.id)?.is_current).toBe(0);
  expect(
    data.cases.find((row) => row.id === revised.value.id)?.revision_number,
  ).toBe(2);
  expect(
    (await cashDueForecast(f.owner, { chartId: f.chart, asOf: "2026-09-30" }))
      .months[0].movement,
  ).toBe("100.01");
});
it("retains and pages more than 200 unsent collection editions without hiding the original message", async () => {
  const invoice = await obligation(),
    original = await prepareCollectionCase(
      f.owner,
      {
        invoiceId: invoice,
        expectedVersion: 1,
        asOf: "2026-09-30",
        message: "Original retained collection message",
      },
      key(),
    );
  let currentId = String(original.id);
  for (let index = 1; index <= 200; index++) {
    const revised = await reviseCollectionCase(
      f.owner,
      currentId,
      {
        expectedVersion: 1,
        expectedInvoiceVersion: 1,
        asOf: "2026-09-30",
        message: `Unsent fictional collection edition ${index + 1}`,
        reason: "Retained synthetic revision history for pagination",
      },
      key(),
    );
    currentId = String(revised.id);
  }
  const first = await collectionWorkspace(f.owner, "2026-09-30"),
    second = await collectionWorkspace(f.owner, "2026-09-30", {
      caseCursor: first.pages.cases.nextCursor!,
    });
  const back = await collectionWorkspace(f.owner, "2026-09-30", {
    caseCursor: second.pages.cases.previousCursor!,
  });
  expect(back.cases.map((row) => row.id)).toEqual(
    first.cases.map((row) => row.id),
  );
  const ids: unknown[] = [];
  let caseCursor: string | undefined;
  do {
    const page = await collectionWorkspace(f.owner, "2026-09-30", {
      caseCursor,
    });
    ids.push(...page.cases.map((row) => row.id));
    caseCursor = page.pages.cases.nextCursor ?? undefined;
  } while (caseCursor);
  expect(ids).toHaveLength(201);
  expect(new Set(ids).size).toBe(201);
  expect(ids).toContain(original.id);
  expect(first.cases.find((row) => row.id === currentId)?.revision_number).toBe(
    201,
  );
  await expect(
    collectionWorkspace({ ...f.owner, companyId: key() }, "2026-09-30", {
      caseCursor: first.pages.cases.nextCursor!,
    }),
  ).rejects.toMatchObject({ code: "INVALID_CURSOR" });
}, 60_000);

it('rolls back bank movement targets group version and command proof after final confirmation outbox failure, then retries once',async()=>{
 const a=await receipt('60.01'),b=await receipt('39.99'),s=await statement(),proposal=await proposeBankMatchGroup(f.owner,{bankTransactionIds:[s.bank],journalLineIds:[a,b],reason:'Reviewed original aggregate bank receipt'},key()),id=String(proposal.id),command=key(),review={expectedVersion:1,reason:'Independent reviewer verified every cash receipt'},fault=`bank_group_fault_${crypto.randomUUID().replaceAll('-','')}`;
 const originalGroup=await db.prepare('SELECT * FROM finance_bank_match_groups WHERE id=?').bind(id).first(),originalBank=await db.prepare('SELECT * FROM finance_bank_transactions WHERE id=?').bind(s.bank).first(),allocations=(await db.prepare('SELECT * FROM finance_bank_match_allocations WHERE group_id=? ORDER BY id').bind(id).all()).results;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'BANK_GROUP_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.bank-group.confirm') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(decideBankMatchGroup(f.manager,id,review,'confirm',command)).rejects.toThrow();
  expect(await db.prepare('SELECT * FROM finance_bank_match_groups WHERE id=?').bind(id).first()).toEqual(originalGroup);
  expect(await db.prepare('SELECT * FROM finance_bank_transactions WHERE id=?').bind(s.bank).first()).toEqual(originalBank);
  expect((await db.prepare('SELECT * FROM finance_bank_match_allocations WHERE group_id=? ORDER BY id').bind(id).all()).results).toEqual(allocations);
  expect(Number(await db.prepare('SELECT count(*) n FROM finance_bank_match_targets WHERE group_id=?').bind(id).first('n'))).toBe(0);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const matched=await decideBankMatchGroup(f.manager,id,review,'confirm',command);expect(await decideBankMatchGroup(f.manager,id,review,'confirm',command)).toEqual(matched);expect(matched).toMatchObject({status:'matched',version:2});
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_bank_match_targets WHERE group_id=?').bind(id).first('n'))).toBe(2);
 expect(await db.prepare('SELECT match_status,version FROM finance_bank_transactions WHERE id=?').bind(s.bank).first()).toEqual({match_status:'matched',version:Number(originalBank!.version)+1});
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(1);
});

it('rolls back mapped file and its inner statement owner command after final outer outbox failure, then retries once',async()=>{
 const mapping=await createBankCsvMapping(f.owner,bankCsvMappingSchema.parse({name:'Outer rollback mapping',columns:{externalId:'ID',transactionDate:'Date',amount:'Amount'}}),key()),input={mappingId:String(mapping.id),bankAccountId:f.accounts.cash,statementNumber:key(),statementDate:'2026-09-30',opening:'0',closing:'100.00',csv:'ID,Date,Amount\nBank,2026-09-04,100.00\n'},command=key(),fault=`mapped_bank_fault_${crypto.randomUUID().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'MAPPED_BANK_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.bank-csv.import') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(importMappedBankCsv(f.owner,input,command)).rejects.toThrow();
  for(const table of ['finance_bank_import_files','finance_bank_statements','finance_bank_transactions'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(f.company).first('n'))).toBe(0);
  for(const ownerCommand of [command,`${command}-statement`])for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(ownerCommand).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const imported=await importMappedBankCsv(f.owner,input,command);expect(await importMappedBankCsv(f.owner,input,command)).toEqual(imported);
 for(const table of ['finance_bank_import_files','finance_bank_statements','finance_bank_transactions'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(f.company).first('n'))).toBe(1);
 for(const ownerCommand of [command,`${command}-statement`])for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(ownerCommand).first('n'))).toBe(1);
});

it('retains unsent collection editions and delivery proof after final outbox failure, then retries each command once',async()=>{
 const invoice=await obligation(),original=await prepareCollectionCase(f.owner,{invoiceId:invoice,expectedVersion:1,asOf:'2026-09-30',message:'Retained original fictional request'},key()),id=String(original.id),command=key(),input={expectedVersion:1,expectedInvoiceVersion:1,asOf:'2026-09-30',message:'Retained revised fictional request',reason:'Independent immutable message edition'},fault=`collection_fault_${crypto.randomUUID().replaceAll('-','')}`;
 const before=await db.prepare('SELECT * FROM finance_collection_cases WHERE id=?').bind(id).first();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'COLLECTION_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.collection.revise') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(reviseCollectionCase(f.owner,id,input,command)).rejects.toThrow();
  expect(await db.prepare('SELECT * FROM finance_collection_cases WHERE id=?').bind(id).first()).toEqual(before);
  expect(Number(await db.prepare('SELECT count(*) n FROM finance_collection_cases WHERE parent_case_id=?').bind(id).first('n'))).toBe(0);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const revised=await reviseCollectionCase(f.owner,id,input,command);expect(await reviseCollectionCase(f.owner,id,input,command)).toEqual(revised);
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_collection_cases WHERE parent_case_id=?').bind(id).first('n'))).toBe(1);
 await decideCollectionCase(f.manager,String(revised.id),{expectedVersion:1,reason:'Reviewed retained revised text'},'approve',key());
 const delivery=key(),review={expectedVersion:2,reason:'Only test delivery of approved retained text'},approved=await db.prepare('SELECT * FROM finance_collection_cases WHERE id=?').bind(revised.id).first();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'COLLECTION_DELIVERY_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.collection.test-deliver') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(decideCollectionCase(f.owner,String(revised.id),review,'test-deliver',delivery)).rejects.toThrow();
  expect(await db.prepare('SELECT * FROM finance_collection_cases WHERE id=?').bind(revised.id).first()).toEqual(approved);
  expect(Number(await db.prepare('SELECT count(*) n FROM finance_collection_deliveries WHERE case_id=?').bind(revised.id).first('n'))).toBe(0);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(delivery).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const delivered=await decideCollectionCase(f.owner,String(revised.id),review,'test-deliver',delivery);expect(await decideCollectionCase(f.owner,String(revised.id),review,'test-deliver',delivery)).toEqual(delivered);
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_collection_deliveries WHERE case_id=?').bind(revised.id).first('n'))).toBe(1);
 for(const ownerCommand of [command,delivery])for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(ownerCommand).first('n'))).toBe(1);
});
