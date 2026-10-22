import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { orderSchema, saveMaster, saveOrder, commercePage } from "@erpx/server-commerce/public/commerce";
import { customerWorkspaceDirectory, customerWorkspaceProfile, salesToday, customerReviewCounts } from "@erpx/server-sales/public/sales-customers";
import { customerTimeline } from "@erpx/server-customer_360/public/customer-timeline";
import { createCustomerFollowup } from "@erpx/server-sales/public/customer-followups";
import { createSupportTicket } from "@erpx/server-support/public/support";
import * as crm from "@erpx/server-crm/public/crm";
import { editedCustomerParty } from "@erpx/server-finance/public/document-posting-preview";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));
let f: Awaited<ReturnType<typeof expenseFixture>>, customer: string, noDocument: string, order: string;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  customer = String((await saveMaster(f.owner, "parties", null, { kind: "customer", code: key(), name: "Harbor workspace customer" }, key())).id);
  noDocument = String((await saveMaster(f.owner, "parties", null, { kind: "customer", code: key(), name: "Unlinked workspace customer" }, key())).id);
  const warehouse = await saveMaster(f.owner, "warehouses", null, { branchId: f.branch, code: key(), name: "Workspace warehouse" }, key()), product = await saveMaster(f.owner, "products", null, { sku: key(), name: "Workspace product", price: "10" }, key());
  order = String((await saveOrder(f.owner, null, orderSchema.parse({ kind: "sales", partyId: customer, warehouseId: warehouse.id, orderDate: "2026-09-20", dueDate: "2026-09-25", lines: [{ productId: product.id, quantity: "1", price: "10", taxBps: 0 }] }), key())).id);
});
afterEach(() => { vi.restoreAllMocks(); });
it("reads customers without source documents without expanding the timeline API", async () => {
  expect(await editedCustomerParty(f.owner, customer, db)).toMatchObject({ id: customer, kind: "customer", person_id: null });
  expect(await editedCustomerParty({ ...f.owner, companyId: "other" }, customer, db)).toBeNull();
  expect(await editedCustomerParty(f.owner, key(), db)).toBeNull();
  expect((await customerWorkspaceDirectory(f.owner, {})).items.map(row => row.id)).toEqual([customer, noDocument]);
  const profile = await customerWorkspaceProfile(f.owner, noDocument);
  expect(profile.customer.id).toBe(noDocument);
  expect(profile.summary.openOrders).toBe(0);
  expect(profile.timeline).toBeNull();
  await expect(customerTimeline(f.owner, noDocument)).rejects.toMatchObject({ status: 404 });
  await expect(customerWorkspaceDirectory({ ...f.owner, role: "employee" }, {})).rejects.toMatchObject({ status: 403 });
  expect((await customerWorkspaceDirectory({ ...f.owner, companyId: "other" }, {})).items).toEqual([]);
});
it("keeps summary evidence scoped and represents missing service access as restricted", async () => {
  await createCustomerFollowup(f.owner, { orderId: order, expectedVersion: 1, ownerId: f.owner.userId, dueDate: "2026-09-22", nextAction: "Call Harbor buyer" }, key());
  await createSupportTicket(f.owner, { orderId: order, sourceVersion: 1, title: "Install Harbor item", issue: "Customer needs installation assistance", ownerId: f.owner.userId, dueDate: "2026-09-24" }, key());
  const visible = (await customerWorkspaceDirectory(f.owner, { q: "Harbor" })).items[0];
  expect(visible.summary).toMatchObject({ openOrders: 1, openSupport: 1, nextFollowup: { action: "Call Harbor buyer" } });
  const restricted = (await customerWorkspaceDirectory({ ...f.owner, role: "purchasing_operator" }, {})).items.find(row => row.id === customer)!;
  expect(restricted.summary.openSupport).toBeNull();
  for (const scope of [{ branchId: "other" }, { departmentId: "other" }]) {
    const scoped = (await customerWorkspaceDirectory({ ...f.owner, ...scope }, {})).items.find(row => row.id === customer)!;
    expect(scoped.summary).toMatchObject({ openOrders: 0, nextFollowup: null, openSupport: 0 });
    expect((await customerWorkspaceProfile({ ...f.owner, ...scope }, customer)).orders).toEqual([]);
  }
});
it("filters customer and document lists before pagination", async () => {
  expect((await customerWorkspaceDirectory(f.owner, { q: "Harbor" })).items.map(row => row.id)).toEqual([customer]);
  expect((await commercePage(f.owner, { view: "sales", status: "confirmed" })).orders).toEqual([]);
  expect((await commercePage(f.owner, { view: "sales", status: "draft" })).orders.map(row => row.id)).toEqual([order]);
  await expect(customerWorkspaceDirectory(f.owner, { cursor: "invalid" })).rejects.toMatchObject({ code: "INVALID_CURSOR" });
});
it("puts overdue personal work first and reports source failure without a false empty total", async () => {
  f.head.role = "sales_operator"; f.head.branchId = null; f.head.departmentId = null; f.head.assignmentId = await f.grant(f.head, "sales_operator");
  const own = await createCustomerFollowup(f.owner, { orderId: order, expectedVersion: 1, ownerId: f.owner.userId, dueDate: "2026-09-21", nextAction: "Earlier personal task" }, key());
  const other = await createCustomerFollowup(f.owner, { orderId: order, expectedVersion: 1, ownerId: f.head.userId, dueDate: "2026-09-23", nextAction: "Team task" }, key());
  const mine = await salesToday(f.head, { kind: "followups" });
  expect(mine.items.map(item => item.id)).toEqual([other.id]);
  const team = await salesToday(f.owner, { kind: "followups", owner: "team" });
  expect(team.items.map(item => item.id)).toEqual([own.id, other.id]);
  vi.spyOn(crm, "crmWork").mockRejectedValueOnce(new Error("Simulated CRM outage"));
  const partial = await salesToday(f.owner, { owner: "team" });
  expect(partial.unavailable).toEqual(["CRM activities"]);
  expect(partial.items.map(item => item.id)).toContain(own.id);
  expect(await customerReviewCounts({ ...f.owner, role: "purchasing_operator" })).toBeNull();
});
