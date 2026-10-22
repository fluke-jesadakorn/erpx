import { reviewedFinanceGrant } from "../../support/helpers/finance-purchase-fixture";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { consolidationEliminationDecisionSchema, consolidationEliminationSchema, consolidationRunSchema, createConsolidationElimination, createConsolidationRun, decideConsolidationElimination, decideConsolidationRun } from "@erpx/server-finance/public/finance-consolidation";
import { createIntercompany, decideIntercompany, intercompanyTransactionSchema } from "@erpx/server-finance/public/finance-intercompany";

vi.mock("server-only", () => ({}));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

afterEach(()=>vi.unstubAllEnvs());
let f: Awaited<ReturnType<typeof expenseFixture>>;
let target: string;

beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  target = `consolidation-target-${key()}`;
  await db.prepare("UPDATE finance_periods SET state='closed',closed_by=?,closed_at=? WHERE state='open'").bind(f.owner.userId, Date.now()).run();
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,status) VALUES(?,'company',?,? ,?,'active')").bind(target, target, "Consolidation target", target),
    db.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,created_at) VALUES(?,?,?,?,?,'active',1)").bind(`entity-${f.company}`, "h001", f.company, "Source company", "THB"),
    db.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,created_at) VALUES(?,?,?,?,?,'active',1)").bind(`entity-${target}`, "h001", target, "Target company", "THB"),
    db.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,close_version,closed_by,closed_at,created_at) VALUES(?,?,?,'2028-01-01','2028-01-31','closed',2,?,?,1)").bind(`consolidation-source-${key()}`, f.company, "Source January", f.owner.userId, Date.now()),
    db.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,close_version,closed_by,closed_at,created_at) VALUES(?,?,?,'2028-01-01','2028-01-31','closed',2,?,?,1)").bind(`consolidation-target-period-${key()}`, target, "Target January", f.owner.userId, Date.now()),
  ]);
});

it("prepares, independently reviews, approves and publishes a closed-period consolidation snapshot", async () => {
  await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,target,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
  const input = consolidationRunSchema.parse({ asOfDate: "2028-01-31", presentationCurrency: "thb" });
  const commandId = key();
  const prepared = await createConsolidationRun(f.reviewer, input, commandId);
  expect(prepared).toMatchObject({ status: "draft", version: 1, companyCount: 2 });
  expect(await createConsolidationRun(f.reviewer, input, commandId)).toEqual(prepared);
  const reviewed = await decideConsolidationRun(f.manager, String(prepared.id), "review", { expectedVersion: 1, reason: "Both company periods are closed and the source snapshot is complete." }, key());
  expect(reviewed).toMatchObject({ status: "review", version: 2 });
  const approved = await decideConsolidationRun(f.owner, String(prepared.id), "approve", { expectedVersion: 2, reason: "Controller approved the group snapshot and source hash." }, key());
  expect(approved).toMatchObject({ status: "approved", version: 3 });
  const published = await decideConsolidationRun(f.manager, String(prepared.id), "publish", { expectedVersion: 3, reason: "Published after independent approval for group reporting." }, key());
  expect(published).toMatchObject({ status: "published", version: 4 });
  expect(await db.prepare("SELECT action,to_status,version FROM finance_consolidation_events WHERE run_id=? ORDER BY version").bind(prepared.id).all()).toMatchObject({ results: [{ action: "prepared", to_status: "draft", version: 1 }, { action: "reviewed", to_status: "review", version: 2 }, { action: "approved", to_status: "approved", version: 3 }, { action: "published", to_status: "published", version: 4 }] });
  await expect(db.prepare("UPDATE finance_consolidation_events SET reason='tampered' WHERE run_id=? AND version=1").bind(prepared.id).run()).rejects.toThrow("FINANCE_CONSOLIDATION_EVENT_IMMUTABLE");
});

it("blocks stale snapshots and duplicate dates", async () => {
  const input = consolidationRunSchema.parse({ asOfDate: "2028-02-28", presentationCurrency: "THB" });
  const prepared = await createConsolidationRun(f.reviewer, input, key());
  await expect(createConsolidationRun(f.reviewer, input, key())).rejects.toMatchObject({ code: "CONSOLIDATION_RUN_EXISTS" });
  await db.prepare("UPDATE finance_periods SET state='open' WHERE company_id=? AND end_date='2028-01-31'").bind(target).run();
  await expect(decideConsolidationRun(f.manager, String(prepared.id), "review", { expectedVersion: 1, reason: "Review after period state changed." }, key())).rejects.toMatchObject({ code: "CONSOLIDATION_PERIODS_REQUIRED" });
});

it("requires and stores a source-currency translation rate", async () => {
  await db.prepare("UPDATE finance_group_entities SET functional_currency='USD' WHERE company_id=?").bind(target).run();
  const withoutRate = consolidationRunSchema.parse({ asOfDate: "2029-01-31", presentationCurrency: "THB" });
  await expect(createConsolidationRun(f.reviewer, withoutRate, key())).rejects.toMatchObject({ code: "CONSOLIDATION_RATE_REQUIRED" });
  const withRate = consolidationRunSchema.parse({ asOfDate: "2029-01-31", presentationCurrency: "THB", translationRates: [{ currency: "usd", rate: "35.50" }] });
  const prepared = await createConsolidationRun(f.reviewer, withRate, key());
  const row = await db.prepare("SELECT source_snapshot_json FROM finance_consolidation_runs WHERE id=?").bind(prepared.id).first<{ source_snapshot_json: string }>();
  const source = JSON.parse(String(row?.source_snapshot_json)) as { translationRates: Record<string, number>; companies: Array<{ companyId: string; translationRateMicros: number }> };
  expect(source.translationRates.USD).toBe(35500000);
  expect(source.companies.find(company => company.companyId === target)?.translationRateMicros).toBe(35500000);
});

it("requires controlled elimination accounts and posts a report-level consolidation journal", async () => {
  const transaction = await createIntercompany(f.owner, intercompanyTransactionSchema.parse({ targetCompanyId: target, reference: `ELIM-${key()}`, currency: "THB", amount: "125.50" }), key());
  await decideIntercompany(f.manager, String(transaction.id), "match", { expectedVersion: 1, reason: "Independent company-side matching completed." }, key());
  await decideIntercompany(f.payer, String(transaction.id), "settle", { expectedVersion: 2, reason: "Settlement evidence agrees to the source reference." }, key());
  const prepared = await createConsolidationRun(f.reviewer, consolidationRunSchema.parse({ asOfDate: "2030-01-31", presentationCurrency: "THB", translationRates: [{ currency: "USD", rate: "35.50" }] }), key());
  const eliminationInput = consolidationEliminationSchema.parse({ sourceTransactionId: transaction.id, debitAccountId: f.accounts.expense, creditAccountId: f.accounts.revenue, amount: "125.50", description: "Eliminate intercompany balance" });
  const proposed = await createConsolidationElimination(f.reviewer, String(prepared.id), eliminationInput, key());
  expect(await createConsolidationElimination(f.reviewer, String(prepared.id), eliminationInput, String(proposed.id).replace("elimination-", ""))).toEqual(proposed);
  await decideConsolidationRun(f.manager, String(prepared.id), "review", { expectedVersion: 1, reason: "Closed periods and elimination source are ready for review." }, key());
  await expect(decideConsolidationRun(f.owner, String(prepared.id), "approve", { expectedVersion: 2, reason: "Cannot approve before elimination review." }, key())).rejects.toMatchObject({ code: "CONSOLIDATION_ELIMINATIONS_UNREVIEWED" });
  const approvedElimination = await decideConsolidationElimination(f.owner, String(proposed.id), "approve", consolidationEliminationDecisionSchema.parse({ expectedVersion: 1, reason: "Approved mapped intercompany accounts." }), key());
  expect(approvedElimination).toMatchObject({ status: "approved", version: 2 });
  await decideConsolidationRun(f.owner, String(prepared.id), "approve", { expectedVersion: 2, reason: "Approved after independent elimination review." }, key());
  const posted = await decideConsolidationElimination(f.manager, String(proposed.id), "post", consolidationEliminationDecisionSchema.parse({ expectedVersion: 2, reason: "Posted to the controlled consolidation journal." }), key());
  expect(posted).toMatchObject({ status: "posted", version: 3, journalId: `consolidation-journal-${prepared.id}` });
  const journal = await db.prepare("SELECT status,currency,source_hash FROM finance_consolidation_journals WHERE id=?").bind(`consolidation-journal-${prepared.id}`).first();
  expect(journal).toMatchObject({ status: "posted", currency: "THB", source_hash: prepared.sourceHash });
  expect(await db.prepare("SELECT SUM(debit_minor) debit,SUM(credit_minor) credit,COUNT(*) lines FROM finance_consolidation_journal_lines WHERE journal_id=?").bind(`consolidation-journal-${prepared.id}`).first()).toMatchObject({ debit: 12550, credit: 12550, lines: 2 });
  const published = await decideConsolidationRun(f.manager, String(prepared.id), "publish", { expectedVersion: 3, reason: "Published after the elimination journal was posted." }, key());
  expect(published).toMatchObject({ status: "published", version: 4 });
  expect(await db.prepare("SELECT action,to_status,version FROM finance_consolidation_elimination_events WHERE elimination_id=? ORDER BY version").bind(proposed.id).all()).toMatchObject({ results: [{ action: "proposed", to_status: "proposed", version: 1 }, { action: "approved", to_status: "approved", version: 2 }, { action: "posted", to_status: "posted", version: 3 }] });
});
