import { expect, it } from "vitest";
import { createAssetApplication, type AssetRepository } from "../../src/server/modules/assets/application/assets";
import type { Asset } from "../../src/server/modules/assets/domain/assets";
import { createMaintenanceApplication, type MaintenanceRepository } from "../../src/server/modules/maintenance/application/work";
import type { MaintenanceWork } from "../../src/server/modules/maintenance/domain/work";
import { createPreventiveApplication, type PreventiveRepository } from "../../src/server/modules/maintenance/application/preventive-plan";
import type { Plan } from "../../src/server/modules/maintenance/domain/preventive-plan";

function repository<Repository extends object>(operations: Partial<Repository>): Repository {
  return new Proxy(operations as Repository, { get(target, property) {
    if (property === "then" || property === "transaction" && !(property in target)) return undefined;
    if (property in target) return Reflect.get(target, property);
    return () => { throw new Error(`unexpected owner operation: ${String(property)}`); };
  } });
}
const error = (_status: number, code: string) => new Error(code);
const asset: Asset = { id: "asset", version: 2, created_by: "author", updated_at: 50, lifecycle_status: "retirement_pending", condition: "good", placement: "Floor 1", custodian_id: "custodian", retirement_reason: "End of service", finance_asset_id: "finance-asset", finance_asset_version: 2, cost_minor: 50000 };
function assets(source: AssetRepository, overrides: Partial<Parameters<typeof createAssetApplication>[0]> = {}) {
  return createAssetApplication({ actorId: "reviewer", authorize: () => {}, authorizeFinance: () => {}, authorizeReview: () => {}, canReadFinance: () => true, canWrite: () => true, canWriteFinance: () => true, repository: async () => source, clock: { now: () => 100 }, error, ...overrides });
}
it("physical retirement cannot persist before Finance has posted the linked disposal", async () => {
  const source = repository<AssetRepository>({ replay: async () => null, asset: async () => asset, hasPostedDisposal: async id => { expect(id).toBe("finance-asset"); return false; } });
  await expect(assets(source).changeAsset("asset", { action: "retire-approve", expectedVersion: 2, reason: "Independent approval" }, "key")).rejects.toThrow("ASSET_FINANCE_DISPOSAL_REQUIRED");
});
it("an independently approved retirement carries the original request reason and clears physical custody", async () => {
  let changes = 0;
  const source = repository<AssetRepository>({ replay: async () => null, asset: async () => asset, hasPostedDisposal: async () => true, change: async (command, current, state, now) => { expect(current).toBe(asset); expect(command.operation).toBe("asset.retire-approve"); expect(state).toEqual({ status: "retired", condition: "good", placement: null, custodian: null, retirement: "End of service" }); expect(now).toBe(100); changes++; return { id: "asset", version: 3, status: state.status }; } });
  expect(await assets(source).changeAsset("asset", { action: "retire-approve", expectedVersion: 2, reason: "Independent approval" }, "key")).toMatchObject({ status: "retired", version: 3 });
  expect(changes).toBe(1);
});
it("unsafe placement and revoked access stop before owner persistence", async () => {
  const source = repository<AssetRepository>({ replay: async () => null, asset: async () => ({ ...asset, lifecycle_status: "registered", condition: "unsafe" }) });
  await expect(assets(source).changeAsset("asset", { action: "place", expectedVersion: 2, reason: "Place at branch", placement: "Floor 1", custodianId: "custodian" }, "key")).rejects.toThrow("ASSET_UNSAFE");
  await expect(assets(source, { authorize: () => { throw new Error("revoked"); } }).changeAsset("asset", {}, "key")).rejects.toThrow("revoked");
});
it("an asset receipt replay uses no clock, source read, or new physical effect", async () => {
  const source = repository<AssetRepository>({ replay: async () => ({ id: "asset", version: 3, status: "retired" }) });
  expect(await assets(source, { clock: { now: () => { throw new Error("unexpected clock"); } } }).changeAsset("asset", { action: "retire-approve", expectedVersion: 2, reason: "Independent approval" }, "key")).toMatchObject({ version: 3 });
});
const work: MaintenanceWork = { id: "work", version: 2, created_by: "author", updated_at: 50, title: "Inspection", priority: "normal", asset_code: "A-1", asset_name: "Lathe", blocked_reason: null, owner_id: "owner", owner_eligible: 1, can_review: 0, status: "in_progress", asset_id: "asset", asset_version: 4, scheduled_at: 20, downtime_started_at: 50, branch_id: "branch", department_id: "department" };
function maintenance(source: MaintenanceRepository, overrides: Partial<Parameters<typeof createMaintenanceApplication>[0]> = {}) {
  return createMaintenanceApplication({ actorId: "owner", authorize: () => {}, canRead: () => true, canWrite: () => true, canReview: () => false, repository: async () => source, clock: { now: () => 100 }, error, ...overrides });
}
it("Maintenance refuses a revoked owner and an invalid transition before consuming spare stock", async () => {
  let current = { ...work, owner_eligible: 0 };
  const source = repository<MaintenanceRepository>({ replay: async () => null, work: async () => current });
  const input = { action: "issue-part", expectedVersion: 2, warehouseId: "warehouse", productId: "part", quantityMilli: 1000, entryDate: "2026-10-05", allocations: [], reason: "Replace worn part" };
  await expect(maintenance(source).changeMaintenanceWork("work", input, "key")).rejects.toThrow("MAINTENANCE_OWNER_REQUIRED");
  current = { ...work, status: "completed" };
  await expect(maintenance(source).changeMaintenanceWork("work", input, "key")).rejects.toThrow("MAINTENANCE_STATE_CHANGED");
});
it("Maintenance stages Inventory consumption and commits its work state through the same repository", async () => {
  const calls: string[] = [];
  const source = repository<MaintenanceRepository>({ replay: async () => null, work: async () => work,
    prepareConsumption: async (command, current) => { expect(command.input.action).toBe("issue-part"); expect(current).toBe(work); calls.push("inventory-prepare"); },
    change: async (command, current, state, now) => { expect(current).toBe(work); expect(command.operation).toBe("maintenance.issue-part"); expect(state).toEqual({ status: "in_progress", owner: "owner", scheduled: 20, downtimeStart: 50, downtimeEnd: null }); expect(now).toBe(100); calls.push("receipt-commit"); return { id: "work", status: state.status, version: 3 }; } });
  expect(await maintenance(source).changeMaintenanceWork("work", { action: "issue-part", expectedVersion: 2, warehouseId: "warehouse", productId: "part", quantityMilli: 1000, entryDate: "2026-10-05", allocations: [], reason: "Replace worn part" }, "key")).toMatchObject({ version: 3 });
  expect(calls).toEqual(["inventory-prepare", "receipt-commit"]);
});
it("completion retains the measured downtime start and closes it at the injected clock", async () => {
  const source = repository<MaintenanceRepository>({ replay: async () => null, work: async () => work, change: async (_command, _current, state) => { expect(state.downtimeStart).toBe(50); expect(state.downtimeEnd).toBe(100); expect(state.status).toBe("completed"); return { id: "work", version: 3 }; } });
  await maintenance(source).changeMaintenanceWork("work", { action: "complete", expectedVersion: 2, resultingCondition: "good", evidence: "Inspection evidence retained", reason: "Repair independently observed" }, "key");
});
it("Work projection needs no database when Maintenance is outside the current role", async () => {
  expect(await maintenance(repository<MaintenanceRepository>({}), { canRead: () => false, repository: async () => { throw new Error("unexpected database"); } }).maintenanceWork()).toEqual([]);
});
const plan = { id: "plan", status: "active", version: 2, asset_id: "asset", title: "Weekly inspection", priority: "normal", owner_id: "owner", edited_by: "owner", interval_days: 7, time_zone: "Asia/Bangkok", lead_days: 2, next_due_local: "2026-10-05T08:00", next_due_at: Date.parse("2026-10-05T01:00Z") } as Plan;
function preventive(source: PreventiveRepository, overrides: Partial<Parameters<typeof createPreventiveApplication>[0]> = {}) {
  return createPreventiveApplication({ actorId: "owner", authorize: () => {}, authorizeReview: () => {}, canRead: () => true, canWrite: () => true, canReview: () => false, repository: async () => source, clock: { now: () => Date.parse("2026-10-05T02:00Z") }, error, ...overrides });
}
it("preventive generation requires a transaction before replay or source acquisition", async () => {
  await expect(preventive(repository<PreventiveRepository>({})).changePreventivePlan("plan", { action: "generate", expectedVersion: 2, reason: "Due inspection" }, "key")).rejects.toThrow("MAINTENANCE_TRANSACTION_REQUIRED");
});
it("preventive activation cannot be approved by the definition's last editor", async () => {
  const source = repository<PreventiveRepository>({ replay: async () => null, plan: async () => ({ ...plan, status: "draft" }) });
  await expect(preventive(source).changePreventivePlan("plan", { action: "activate", expectedVersion: 2, reason: "Review definition" }, "key")).rejects.toThrow("INDEPENDENT_PLAN_REVIEW_REQUIRED");
});
it("preventive generation stages its child work and occurrence through one transaction", async () => {
  const calls: string[] = [], transaction = repository<PreventiveRepository>({ replay: async () => null, lockPlan: async id => { expect(id).toBe("plan"); calls.push("plan-lock"); }, plan: async () => plan,
    assetVersion: async (id, lock) => { expect([id, lock]).toEqual(["asset", true]); return { version: 4 }; },
    createWork: async (input, key) => { expect(input).toMatchObject({ assetId: "asset", assetVersion: 4, ownerId: "owner", scheduledAt: plan.next_due_at }); expect(key).toBe("key:work"); calls.push("child-work"); return { id: "work" }; },
    change: async (_command, current, facts) => { expect(current).toBe(plan); expect(facts.generation).toMatchObject({ workOrderId: "work", assetVersion: 4, definition: { dueLocal: plan.next_due_local } }); expect(facts.result).toMatchObject({ id: "plan", version: 3, workOrderId: "work", status: "active", nextDueLocal: "2026-10-12T08:00" }); calls.push("occurrence-receipt"); return facts.result; } });
  const pooled = repository<PreventiveRepository>({ transaction: async action => { calls.push("begin"); const result = await action(transaction); calls.push("commit"); return result; } });
  expect(await preventive(pooled).changePreventivePlan("plan", { action: "generate", expectedVersion: 2, reason: "Due inspection" }, "key")).toMatchObject({ workOrderId: "work" });
  expect(calls).toEqual(["begin", "plan-lock", "child-work", "occurrence-receipt", "commit"]);
});
it("a replay inside a supplied preventive transaction does not lock, generate, or read the clock", async () => {
  const source = repository<PreventiveRepository>({ replay: async () => ({ id: "plan", version: 3, workOrderId: "original-work" }) });
  expect(await preventive(source, { transactionBound: true, clock: { now: () => { throw new Error("unexpected clock"); } } }).changePreventivePlan("plan", { action: "generate", expectedVersion: 2, reason: "Due inspection" }, "key")).toMatchObject({ workOrderId: "original-work" });
});
