import { beforeAll, describe, expect, it, vi } from "vitest";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { seedFinanceCloseCompany } from "../../../fixtures/finance/builders/engineering-finance-close-browser";
import {
  beginClose,
  calculateCloseReconciliations,
  closePeriod,
  createJournal,
  createJournalSchema,
  postJournalBatch,
} from "../../../src/server/modules/finance/public/finance";
import { applyMigrations, env } from "../../support/runtime-env";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
const db = env.H001_DB, key = () => crypto.randomUUID();
type Seeded = Awaited<ReturnType<typeof seedFinanceCloseCompany>>;
type Row = Record<string, string | number | null>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });

async function reconcile(f: Seeded, periodId: string) {
  const period = await db.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, f.companyId).first<Row>();
  expect(period).not.toBeNull();
  return calculateCloseReconciliations(db, f.preparer, period!);
}
async function retainedSources(f: Seeded) {
  const [journal, lines, invoices, statements] = await Promise.all([
    db.prepare("SELECT * FROM finance_journals WHERE id=?").bind(f.openingJournalId).first<Row>(),
    db.prepare("SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal").bind(f.openingJournalId).all<Row>(),
    db.prepare("SELECT * FROM finance_invoices WHERE company_id=? ORDER BY kind").bind(f.companyId).all<Row>(),
    db.prepare("SELECT * FROM finance_bank_statements WHERE company_id=? ORDER BY statement_date").bind(f.companyId).all<Row>(),
  ]);
  return { journal, lines: lines.results, invoices: invoices.results, statements: statements.results };
}
function expectOriginalBalances(reconciliation: Awaited<ReturnType<typeof reconcile>>) {
  expect(reconciliation.ready).toBe(true);
  expect(reconciliation.rows).toHaveLength(3);
  expect(reconciliation.rows.map(row => [row.kind, row.ledgerMinor, row.sourceMinor, row.differenceMinor])).toEqual(expect.arrayContaining([
    ["bank", 100000, 100000, 0], ["receivable", 30000, 30000, 0], ["payable", 20000, 20000, 0],
  ]));
  expect(reconciliation.rows.find(row => row.kind === "bank")?.source.evidenceComplete).toBe(true);
  for (const row of reconciliation.rows.filter(row => row.kind !== "bank")) {
    expect(row.source).toMatchObject({ invoiceCount: 1, foreignOpenCount: 0, legacyDateCount: 0 });
  }
}

describe("isolated Finance close browser fixture", () => {
  it("posts native opening schedules and closes two months independently without consuming the other browser Company's evidence", async () => {
    const desktop = await seedFinanceCloseCompany(db, env.H001_FILES, "desktop"), mobile = await seedFinanceCloseCompany(db, env.H001_FILES, "mobile");
    expect(desktop.companyId).not.toBe(mobile.companyId);
    const sources = await retainedSources(desktop);
    expect(sources.journal).toMatchObject({ status: "posted", source_type: "opening_import", created_by: desktop.preparer.userId, posted_by: desktop.approver.userId, is_opening: 1, entry_date: "2027-01-01" });
    expect(sources.invoices.map(invoice => [invoice.kind, invoice.status, invoice.total_minor, invoice.base_outstanding_minor, invoice.source_id])).toEqual([
      ["payable", "posted", 20000, 20000, desktop.openingJournalId], ["receivable", "posted", 30000, 30000, desktop.openingJournalId],
    ]);
    expect(sources.statements.map(statement => [statement.statement_number, statement.statement_date, statement.opening_minor, statement.closing_minor])).toEqual([
      ["DEMO-JAN-2027", "2027-01-31", 100000, 100000], ["DEMO-FEB-2027", "2027-02-28", 100000, 100000],
    ]);
    for (const actor of [desktop.preparer, desktop.approver]) {
      expect(await db.prepare("SELECT role,modules_json,scope_kind,revoked_at FROM assignments WHERE id=? AND company_id=? AND user_id=?")
        .bind(actor.assignmentId, desktop.companyId, actor.userId).first()).toEqual({ role: actor.role, modules_json: '["finance"]', scope_kind: "holding", revoked_at: null });
      expect(await db.prepare("SELECT status,login_policy,person_id FROM iam_members WHERE company_id=? AND user_id=?")
        .bind(desktop.companyId, actor.userId).first()).toEqual({ status: "active", login_policy: "local", person_id: `person-${actor.userId}` });
      expect(actor.iam?.permissions).toContain("finance.write");
    }
    for (const periodId of desktop.periodIds) {
      expectOriginalBalances(await reconcile(desktop, periodId));
      const started = await beginClose(desktop.preparer, periodId, key());
      const input = { expectedVersion: Number(started.version), proof: "Independent exact GL, AR/AP and period-end statement review", attachmentSetIds: [] };
      const command = key(), closed = await closePeriod(desktop.approver, periodId, input, command);
      expect(closed).toMatchObject({ state: "closed" });
      expect(await closePeriod(desktop.approver, periodId, input, command)).toEqual(closed);
      expectOriginalBalances(await reconcile(desktop, periodId));
    }
    expect(await retainedSources(desktop)).toEqual(sources);
    expect(await db.prepare("SELECT name,state,close_started_by,closed_by FROM finance_periods WHERE company_id=? AND id IN (?,?) ORDER BY start_date")
      .bind(desktop.companyId, ...desktop.periodIds).all()).toMatchObject({ results: [
      { name: "January 2027", state: "closed", close_started_by: desktop.preparer.userId, closed_by: desktop.approver.userId },
      { name: "February 2027", state: "closed", close_started_by: desktop.preparer.userId, closed_by: desktop.approver.userId },
    ] });
    expect(await db.prepare("SELECT COUNT(*) AS n FROM finance_close_reconciliations WHERE company_id=? AND difference_minor=0")
      .bind(desktop.companyId).first("n")).toBe(6);
    expect((await db.prepare("SELECT state FROM finance_periods WHERE company_id=? AND id IN (?,?) ORDER BY start_date")
      .bind(mobile.companyId, ...mobile.periodIds).all()).results).toEqual([{ state: "open" }, { state: "open" }]);
    for (const periodId of mobile.periodIds) expectOriginalBalances(await reconcile(mobile, periodId));
    await expect(beginClose(mobile.preparer, desktop.periodIds[0], key())).rejects.toMatchObject({ code: "PERIOD_NOT_FOUND" });
  });

  it("retains exact close sources when another Company posts the four operating cash movements that previously changed the shared bank", async () => {
    const close = await seedFinanceCloseCompany(db, env.H001_FILES, "isolated close"), operating = await seedExpenseCompany(db, env.H001_FILES);
    const retained = await retainedSources(close), before = await reconcile(close, close.periodIds[0]);
    await db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'journal','draft',?,?,'Independent synthetic operating receipt reviewer',?)")
      .bind(operating.company, operating.manager.userId, operating.reviewer.userId, Date.now()).run();
    // Declared manual Finance sources reproduce the actual observed cash deltas,
    // without pretending to rerun the already covered POS/Field/Sales adapters.
    for (const amount of ["25.00", "-12.50", "100.00", "10.00"]) {
      const positive = !amount.startsWith("-"), value = amount.replace("-", "");
      const journal = await createJournal(operating.reviewer, createJournalSchema.parse({
        chartId: operating.chart, periodId: operating.period, entryDate: "2026-10-03", currency: "THB",
        memo: "Declared synthetic operating cash source for close Company isolation", lines: [
          { accountId: operating.accounts.cash, debit: positive ? value : "0", credit: positive ? "0" : value },
          { accountId: operating.accounts.revenue, debit: positive ? "0" : value, credit: positive ? value : "0" },
        ],
      }), key());
      await postJournalBatch(operating.manager, { journals: [{ id: String(journal.id), expectedVersion: 1 }], reason: "Independent review of declared operating receipt source and exact balanced amounts" }, key());
    }
    expect(await db.prepare("SELECT SUM(l.base_debit_minor-l.base_credit_minor) AS amount FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted' AND l.account_id=?")
      .bind(operating.company, operating.accounts.cash).first("amount")).toBe(12250);
    expect(await reconcile(close, close.periodIds[0])).toEqual(before);
    expect(await retainedSources(close)).toEqual(retained);
    const started = await beginClose(close.preparer, close.periodIds[0], key());
    await closePeriod(close.approver, close.periodIds[0], { expectedVersion: Number(started.version), proof: "Unchanged dedicated source balances remain independently reconciled", attachmentSetIds: [] }, key());
  });

  it("still rejects an actual native bank difference inside the close Company without adjusting the retained statement", async () => {
    const close = await seedFinanceCloseCompany(db, env.H001_FILES, "unbalanced close"), retained = await retainedSources(close);
    const journal = await createJournal(close.preparer, createJournalSchema.parse({
      chartId: close.chartId, periodId: close.periodIds[0], entryDate: "2027-01-15", currency: "THB",
      memo: "Declared fictional extra cash requiring real bank reconciliation", lines: [
        { accountId: close.accounts.bank, debit: "12.50", credit: "0" },
        { accountId: close.accounts.equity, debit: "0", credit: "12.50" },
      ],
    }), key());
    await postJournalBatch(close.approver, { journals: [{ id: String(journal.id), expectedVersion: 1 }], reason: "Independent review of the real extra fictional cash posting" }, key());
    const reconciliation = await reconcile(close, close.periodIds[0]);
    expect(reconciliation.ready).toBe(false);
    expect(reconciliation.rows.find(row => row.kind === "bank")).toMatchObject({ ledgerMinor: 101250, sourceMinor: 100000, differenceMinor: 1250 });
    await expect(beginClose(close.preparer, close.periodIds[0], key())).rejects.toMatchObject({ code: "CLOSE_RECONCILIATION_FAILED" });
    expect(await retainedSources(close)).toEqual(retained);
    expect(await db.prepare("SELECT state FROM finance_periods WHERE id=?").bind(close.periodIds[0]).first("state")).toBe("open");
    expect(await db.prepare("SELECT COUNT(*) AS n FROM finance_close_reconciliations WHERE company_id=?").bind(close.companyId).first("n")).toBe(0);
  });
});
