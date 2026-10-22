import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { isValidElement, type ReactElement, type ReactNode } from "react";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseFixture, expenseDb as db, key } from "../../support/helpers/expense-fixture";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { createAsset, createMaintenanceWork, changeMaintenanceWork, maintenanceDetail } from "@erpx/server-assets/public/asset-maintenance";
import { saveMaster } from "@erpx/server-commerce/public/commerce";
import { assignFlowPrimary, resolveFlow } from "@erpx/server-workflow/public/flow-routing";
import JournalPage from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/journals/[id]/screen";

vi.mock("next/headers", () => ({ headers: async () => new Headers({ host: "console-dev-erpx.jesadakorn.com" }) }));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: db, files: env.H001_FILES }), runtimeEnv: async () => env }));
vi.mock("@erpx/server-identity_access/public/authorization", () => ({ requireWorkContext: async () => actor, publicScope: () => ({ kind: "holding", holdingSlug: "test" }) }));
vi.mock("@erpx/server-work_projection/public/page-error", () => ({ pageError: (error: unknown) => { throw error; } }));
let actor: WorkContext, f: Awaited<ReturnType<typeof expenseFixture>>, workId: string, journalId: string;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  f.owner.modules = ["finance", "assets", "maintenance", "inventory"];
  actor = f.manager;
  const asset = await createAsset(f.owner, { branchId: f.branch, code: key(), name: "Encoded spare cost source", category: "Inspection equipment", currency: "THB", costMinor: 0, acquiredDate: "2026-01-01" }, key());
  const warehouse = String((await saveMaster(f.owner, "warehouses", null, { branchId: f.branch, code: key(), name: "Controlled spare store" }, key())).id);
  const product = String((await saveMaster(f.owner, "products", null, { sku: key(), name: "Replacement fuse", unit: "each", price: "5" }, key())).id);
  // Declared opening stock; the actual Maintenance command retains its issue and journal.
  await db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES(?,?,3000,1001)").bind(warehouse, product).run();
  workId = String((await createMaintenanceWork(f.owner, { assetId: asset.id, assetVersion: 1, title: "Replace the inspected fuse", priority: "normal", ownerId: f.owner.userId, scheduledAt: null, reason: "Native inspection requires a replacement." }, key())).id);
  await changeMaintenanceWork(f.owner, workId, { action: "start", expectedVersion: 1, reason: "Isolate the equipment before work." }, key());
  await changeMaintenanceWork(f.owner, workId, { action: "issue-part", expectedVersion: 2, warehouseId: warehouse, productId: product, quantityMilli: 1000, entryDate: "2026-10-02", reason: "Issue one valued fuse through the native owner." }, key());
  journalId = String((await maintenanceDetail(f.owner, workId)).consumptions[0].journal_id);
  const flow = await resolveFlow(db, f.owner, "journal", journalId);
  await assignFlowPrimary(db, f.owner, "journal", journalId, { stepKey: "draft", userId: actor.userId, reason: "Route the actual spare cost to an independent Finance approver.", saveDefault: false, expectedVersion: flow.version, recordVersion: flow.recordVersion }, key());
});

function elements(node: ReactNode): ReactElement<Record<string, unknown>>[] {
  if (Array.isArray(node)) return node.flatMap(elements);
  if (!isValidElement<Record<string, unknown>>(node)) return [];
  return [node, ...elements(node.props.children as ReactNode)];
}

it("renders the actual spare issue journal from its encoded owner suffix with independent posting controls and retained original source", async () => {
  expect(journalId).toMatch(/:parts$/);
  const page = await JournalPage({ params: Promise.resolve({ holdingSlug: "test", id: encodeURIComponent(journalId) }), searchParams: Promise.resolve({}) });
  const action = elements(page).find(element => element.props.journal && "canPost" in element.props)!;
  const consumption = (await maintenanceDetail(f.owner, workId)).consumptions[0];
  expect(action.props.journal).toMatchObject({ id: journalId, status: "draft", source_type: "maintenance_parts", source_id: consumption.id, created_by: f.owner.userId });
  expect(action.props.canPost).toBe(true);
  expect((await maintenanceDetail(f.owner, workId)).consumptions).toMatchObject([{ journal_id: journalId, value_minor: 334, journal_status: "draft" }]);
  expect(consumption).toMatchObject({ source_kind: "maintenance", source_id: workId });
  expect(await db.prepare("SELECT SUM(base_debit_minor) debit,SUM(base_credit_minor) credit FROM finance_journal_lines WHERE journal_id=?").bind(journalId).first()).toEqual({ debit: 334, credit: 334 });
});

it("preserves Finance Company and read authority checks when an encoded native journal link is opened", async () => {
  actor = { ...f.manager, companyId: "foreign-company" };
  await expect(JournalPage({ params: Promise.resolve({ holdingSlug: "test", id: encodeURIComponent(journalId) }), searchParams: Promise.resolve({}) })).rejects.toMatchObject({ status: 404, code: "JOURNAL_NOT_FOUND" });
  actor = { ...f.claimant };
  await expect(JournalPage({ params: Promise.resolve({ holdingSlug: "test", id: encodeURIComponent(journalId) }), searchParams: Promise.resolve({}) })).rejects.toMatchObject({ status: 403 });
  expect((await maintenanceDetail(f.owner, workId)).consumptions).toMatchObject([{ journal_id: journalId, journal_status: "draft" }]);
});
