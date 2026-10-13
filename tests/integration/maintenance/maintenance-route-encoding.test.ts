import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { isValidElement, type ReactElement } from "react";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseFixture, expenseDb as db, key } from "../../support/helpers/expense-fixture";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { createAsset } from "../../../src/server/modules/assets/public/asset-maintenance";
import { savePreventivePlan, changePreventivePlan } from "../../../src/server/modules/maintenance/public/maintenance-preventive";
import MaintenancePage from "../../../src/app/holdings/[holdingSlug]/maintenance/[id]/page";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: db, files: env.H001_FILES }), runtimeEnv: async () => env }));
vi.mock("../../../src/server/modules/identity_access/public/authorization", () => ({ requireWorkContext: async () => actor, publicScope: () => ({ kind: "holding", holdingSlug: "test" }) }));
vi.mock("../../../src/server/modules/work_projection/public/page-error", () => ({ pageError: (error: unknown) => { throw error; } }));
let actor: WorkContext, f: Awaited<ReturnType<typeof expenseFixture>>, workId: string;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture(); actor = { ...f.owner, modules: ["maintenance", "assets"] };
  const reviewer = { ...f.tie, modules: ["maintenance", "assets"] };
  const asset = await createAsset(actor, { branchId: f.branch, code: key(), name: "Encoded native work source", category: "Inspection equipment", currency: "THB", costMinor: 0, acquiredDate: "2026-01-01" }, key());
  const plan = await savePreventivePlan(actor, null, { assetId: asset.id, assetVersion: 1, title: "Daily encoded inspection", priority: "normal", ownerId: actor.userId, intervalDays: 1, firstDueAt: Date.now() - 86_400_000, timeZone: "Asia/Bangkok", leadDays: 0, reason: "Create the actual owner plan." }, key());
  await changePreventivePlan(reviewer, String(plan.id), { action: "activate", expectedVersion: 1, reason: "Independent review of the owner plan." }, key());
  workId = String((await changePreventivePlan(actor, String(plan.id), { action: "generate", expectedVersion: 2, reason: "Generate the actual due work." }, key())).workOrderId);
});

it("renders the real generated work when the compiled route parameter escapes its original command suffix", async () => {
  expect(workId).toMatch(/:work$/);
  const page = await MaintenancePage({ params: Promise.resolve({ holdingSlug: "test", id: encodeURIComponent(workId) }) });
  const controls = page.props.children.find((child: unknown) => isValidElement(child) && "detail" in (child.props as Record<string, unknown>)) as ReactElement<{ detail: { work: { id: string; status: string; title: string }; history: Array<{ action: string }> } }>;
  expect(controls.props.detail.work).toMatchObject({ id: workId, status: "scheduled", title: "Daily encoded inspection" });
  expect(controls.props.detail.history.map(event => event.action)).toEqual(["create"]);
  expect((await db.prepare("SELECT work_order_id FROM maintenance_preventive_occurrences WHERE work_order_id=?").bind(workId).all()).results).toEqual([{ work_order_id: workId }]);
});

it("preserves current Company and branch source guards for encoded generated work links", async () => {
  actor = { ...actor, companyId: "foreign-company" };
  await expect(MaintenancePage({ params: Promise.resolve({ holdingSlug: "test", id: encodeURIComponent(workId) }) })).rejects.toMatchObject({ status: 404, code: "MAINTENANCE_WORK_NOT_FOUND" });
  actor = { ...f.owner, modules: ["maintenance", "assets"], branchId: "foreign-branch" };
  await expect(MaintenancePage({ params: Promise.resolve({ holdingSlug: "test", id: encodeURIComponent(workId) }) })).rejects.toMatchObject({ status: 404, code: "MAINTENANCE_WORK_NOT_FOUND" });
});
