import { packageGrantStore } from "@erpx/server-commercial/public/package-grants";
import { beforeAll, beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { applyMigrations, env, type TestMigration } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import {
  calculateCloseReconciliations,
  createInvoice,
  createParty,
  decideFxRevaluation,
  fxRevaluationWorkspace,
  postInvoice,
  prepareFxRevaluation,
} from "@erpx/server-finance/public/finance";
import { assignFlowPrimary, resolveFlow } from "@erpx/server-workflow/public/flow-routing";

vi.mock("server-only", () => ({}));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

afterEach(()=>vi.unstubAllEnvs());
let f: Awaited<ReturnType<typeof expenseFixture>>;
let ids: { payable: string; receivable: string; usdExpense: string; usdRevenue: string; gain: string; loss: string };

beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]); });
beforeEach(async () => {
  f = await expenseFixture();
  ids = { payable: `fx-ap-${key()}`, receivable: `fx-ar-${key()}`, usdExpense: `fx-exp-${key()}`, usdRevenue: `fx-rev-${key()}`, gain: `fx-gain-${key()}`, loss: `fx-loss-${key()}` };
  await db.batch([
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'liability','USD',1,'payable',1,1)").bind(ids.payable, f.chart, f.company, `USD-AP-${key()}`, "USD trade payables"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'asset','USD',1,'receivable',1,1)").bind(ids.receivable, f.chart, f.company, `USD-AR-${key()}`, "USD trade receivables"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'expense','USD',0,'none',1,1)").bind(ids.usdExpense, f.chart, f.company, `USD-EXP-${key()}`, "USD expense"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'revenue','USD',0,'none',1,1)").bind(ids.usdRevenue, f.chart, f.company, `USD-REV-${key()}`, "USD revenue"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'other_income','THB',0,'none',1,1)").bind(ids.gain, f.chart, f.company, `FX-GAIN-${key()}`, "Unrealized FX gain"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'other_expense','THB',0,'none',1,1)").bind(ids.loss, f.chart, f.company, `FX-LOSS-${key()}`, "Unrealized FX loss"),
  ]);
});

async function foreignInvoice(kind: "payable" | "receivable", amount = "100") {
  const party = await createParty(f.reviewer, { kind: kind === "payable" ? "supplier" : "customer", code: `FX-${key()}`, name: "Foreign counterparty", currency: "USD", paymentTermsDays: 30, creditLimit: kind === "payable" ? "0" : "10000" }, key());
  const invoice = await createInvoice(f.reviewer, { partyId: String(party.id), kind, invoiceNumber: `FX-${key()}`, invoiceDate: "2026-09-10", dueDate: "2026-10-10", currency: "USD", subtotal: amount, tax: "0", debitAccountId: kind === "payable" ? ids.usdExpense : ids.receivable, creditAccountId: kind === "payable" ? ids.payable : ids.usdRevenue }, key());
  const flow = await resolveFlow(db, f.owner, "invoice", String(invoice.id));
  await assignFlowPrimary(db, f.owner, "invoice", String(invoice.id), { stepKey: "draft", userId: f.manager.userId, reason: "Independent foreign invoice reviewer", saveDefault: false, expectedVersion: flow.version, recordVersion: flow.recordVersion }, key());
  await postInvoice(f.manager, String(invoice.id), { periodId: f.period, entryDate: "2026-09-10", expectedVersion: 1, fxRate: "35" }, key());
  return invoice;
}

async function prepare(rate = "36", actor = f.reviewer) {
  return prepareFxRevaluation(actor, { chartId: f.chart, periodId: f.period, entryDate: "2026-12-31", rates: [{ currency: "USD", rate }], gainAccountId: ids.gain, lossAccountId: ids.loss, proof: "Central bank closing rate checked at period end" }, key());
}

describe("controlled period-end FX revaluation", () => {
  it("blocks close until an independent approver posts the payable revaluation", async () => {
    const invoice = await foreignInvoice("payable");
    const period = await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first<Record<string, string | number | null>>();
    const before = await calculateCloseReconciliations(db, f.owner, period!);
    expect(before.rows.find(row => row.accountId === ids.payable)).toMatchObject({ ledgerMinor: 350_000, sourceMinor: 350_000, source: expect.objectContaining({ foreignOpenCount: 1 }) });
    expect(before.ready).toBe(false);


    const store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-fx-close-review"}),obligations:async()=>{throw Error("No revaluation withdrawal witness");}}),grant={companyId:f.company,packageId:"finance",packageVersion:1,state:"active" as const,startsAt:0,endsAt:null,version:1,reason:"Native retained FX close review",evidence:"test:finance-draining-fx"};
    await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
    const run = await prepare("36", f.manager);
    expect(run).toMatchObject({ status: "prepared", version: 1, lines: 1, netAdjustment: "100.00" });
    await expect(decideFxRevaluation(f.manager, String(run.id), { expectedVersion: 1, proof: "Self review is forbidden" }, "approve", key())).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
    const command = key();
    const posted = await decideFxRevaluation(f.owner, String(run.id), { expectedVersion: 1, proof: "Controller checked exposure, closing rate, and accounts" }, "approve", command);
    expect(posted).toMatchObject({ status: "posted", version: 2, gain: "0.00", loss: "100.00" });
    expect(await decideFxRevaluation(f.owner, String(run.id), { expectedVersion: 1, proof: "Controller checked exposure, closing rate, and accounts" }, "approve", command)).toEqual(posted);
    expect(await db.prepare("SELECT base_outstanding_minor FROM finance_invoices WHERE id=?").bind(invoice.id).first("base_outstanding_minor")).toBe(360_000);
    const journal = await db.prepare("SELECT id,status,source_type,source_id FROM finance_journals WHERE id=?").bind(posted.journalId).first();
    expect(journal).toMatchObject({ status: "posted", source_type: "fx_revaluation", source_id: run.id });
    const balances = await db.prepare("SELECT account_id,SUM(base_debit_minor-base_credit_minor) balance FROM finance_journal_lines WHERE journal_id=? GROUP BY account_id").bind(posted.journalId).all<{ account_id: string; balance: number }>();
    expect(Object.fromEntries(balances.results.map(row => [row.account_id, Number(row.balance)]))).toEqual({ [ids.payable]: -10_000, [ids.loss]: 10_000 });
    const after = await calculateCloseReconciliations(db, f.owner, period!);
    expect(after.rows.find(row => row.accountId === ids.payable)).toMatchObject({ ledgerMinor: 360_000, sourceMinor: 360_000, differenceMinor: 0, source: expect.objectContaining({ foreignOpenCount: 0 }) });
    const workspace = await fxRevaluationWorkspace(f.owner, f.period);
    expect(workspace.runs[0]).toMatchObject({ id: run.id, status: "posted", approved_by: f.owner.userId });
  });

  it("posts receivable gains and rejects a prepared run when exposure changes", async () => {
    await foreignInvoice("receivable");
    const stale = await prepare();
    await foreignInvoice("payable", "25");
    await expect(decideFxRevaluation(f.manager, String(stale.id), { expectedVersion: 1, proof: "Review after a concurrent change" }, "approve", key())).rejects.toMatchObject({ code: "FX_REVALUATION_STALE" });
    await decideFxRevaluation(f.manager, String(stale.id), { expectedVersion: 1, proof: "Reject stale preparation and recalculate" }, "reject", key());
    const fresh = await prepare();
    const posted = await decideFxRevaluation(f.manager, String(fresh.id), { expectedVersion: 1, proof: "Controller reviewed the refreshed exposure" }, "approve", key());
    expect(posted).toMatchObject({ status: "posted", gain: "100.00", loss: "25.00" });
    const balances = await db.prepare("SELECT account_id,SUM(base_debit_minor-base_credit_minor) balance FROM finance_journal_lines WHERE journal_id=? GROUP BY account_id ORDER BY account_id").bind(posted.journalId).all<{ account_id: string; balance: number }>();
    expect(Object.fromEntries(balances.results.map(row => [row.account_id, Number(row.balance)]))).toEqual({ [ids.receivable]: 10_000, [ids.payable]: -2_500, [ids.gain]: -10_000, [ids.loss]: 2_500 });
  });
});
