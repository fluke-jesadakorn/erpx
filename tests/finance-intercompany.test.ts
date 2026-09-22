import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { expenseDb as db, expenseFixture, key } from "./helpers/expense-fixture";
import { createIntercompany, decideIntercompany, intercompanyTransactionSchema, intercompanyWorkspace } from "../src/server/finance-intercompany";

vi.mock("server-only", () => ({}));
vi.mock("../src/server/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

let f: Awaited<ReturnType<typeof expenseFixture>>;
let target: string;

beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  target = `target-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,created_at) VALUES(?,?,?,?,?,'active',1)").bind(`entity-${f.company}`, "h001", f.company, "Source company", "THB"),
    db.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,created_at) VALUES(?,?,?,?,?,'active',1)").bind(`entity-${target}`, "h001", target, "Target company", "THB"),
  ]);
});

it("runs an idempotent intercompany create, independent match, settlement and elimination", async () => {
  const input = intercompanyTransactionSchema.parse({ targetCompanyId: target, reference: `IC-${key()}`, currency: "THB", amount: "125.50" });
  const created = await createIntercompany(f.owner, input, key());
  expect(created).toMatchObject({ status: "unmatched", version: 1, amountMinor: 12550 });
  const matched = await decideIntercompany(f.manager, String(created.id), "match", { expectedVersion: 1, reason: "Both posted company-side journals agree." }, key());
  expect(matched).toMatchObject({ status: "matched", version: 2 });
  const settled = await decideIntercompany(f.payer, String(created.id), "settle", { expectedVersion: 2, reason: "Settlement evidence reviewed against the intercompany reference." }, key());
  expect(settled).toMatchObject({ status: "settled", version: 3 });
  const eliminated = await decideIntercompany(f.owner, String(created.id), "eliminate", { expectedVersion: 3, reason: "Approved group elimination for the closed period." }, key());
  expect(eliminated).toMatchObject({ status: "eliminated", version: 4 });
  const workspace = await intercompanyWorkspace(f.owner);
  expect(workspace.statusCounts).toEqual({ eliminated: 1 });
  expect(await db.prepare("SELECT action,version FROM finance_intercompany_events WHERE transaction_id=? ORDER BY version").bind(created.id).all()).toMatchObject({ results: [{ action: "created", version: 1 }, { action: "matched", version: 2 }, { action: "settled", version: 3 }, { action: "eliminated", version: 4 }] });
  await expect(db.prepare("UPDATE finance_intercompany_events SET reason='tampered' WHERE transaction_id=? AND version=1").bind(created.id).run()).rejects.toThrow("FINANCE_INTERCOMPANY_EVENT_IMMUTABLE");
});

it("rejects self-review, stale versions and missing group entities", async () => {
  const input = intercompanyTransactionSchema.parse({ targetCompanyId: target, reference: `IC-${key()}`, currency: "THB", amount: "10" });
  const created = await createIntercompany(f.owner, input, key());
  await expect(decideIntercompany(f.owner, String(created.id), "match", { expectedVersion: 1, reason: "I created this item." }, key())).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  const matched = await decideIntercompany(f.manager, String(created.id), "match", { expectedVersion: 1, reason: "Independent review." }, key());
  await expect(decideIntercompany(f.payer, String(created.id), "settle", { expectedVersion: 1, reason: "Stale decision." }, key())).rejects.toMatchObject({ code: "INTERCOMPANY_CHANGED" });
  expect(matched.status).toBe("matched");
  const missing = `missing-${key()}`;
  await expect(createIntercompany(f.owner, { ...input, targetCompanyId: missing, reference: `IC-${key()}` }, key())).rejects.toMatchObject({ code: "GROUP_ENTITY_REQUIRED" });
});
