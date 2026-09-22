import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { expenseDb as db, expenseFixture, key } from "./helpers/expense-fixture";
import { consolidationRunSchema, createConsolidationRun, decideConsolidationRun } from "../src/server/finance-consolidation";

vi.mock("server-only", () => ({}));
vi.mock("../src/server/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

let f: Awaited<ReturnType<typeof expenseFixture>>;
let target: string;

beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  target = `consolidation-target-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,status) VALUES(?,'company',?,? ,?,'active')").bind(target, target, "Consolidation target", target),
    db.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,created_at) VALUES(?,?,?,?,?,'active',1)").bind(`entity-${f.company}`, "h001", f.company, "Source company", "THB"),
    db.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,created_at) VALUES(?,?,?,?,?,'active',1)").bind(`entity-${target}`, "h001", target, "Target company", "THB"),
    db.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,close_version,closed_by,closed_at,created_at) VALUES(?,?,?,'2028-01-01','2028-01-31','closed',2,?,?,1)").bind(`consolidation-source-${key()}`, f.company, "Source January", f.owner.userId, Date.now()),
    db.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,close_version,closed_by,closed_at,created_at) VALUES(?,?,?,'2028-01-01','2028-01-31','closed',2,?,?,1)").bind(`consolidation-target-period-${key()}`, target, "Target January", f.owner.userId, Date.now()),
  ]);
});

it("prepares, independently reviews, approves and publishes a closed-period consolidation snapshot", async () => {
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
