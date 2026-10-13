import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { commercePage, saveMaster } from "../../../src/server/modules/commerce/public/commerce";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));

let fixture: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => { fixture = await expenseFixture(); });

it("keeps customer and supplier directories separate over the shared party records", async () => {
  const customer = await saveMaster(fixture.owner, "parties", null, { kind: "customer", code: key(), name: "Directory customer" }, key());
  const supplier = await saveMaster(fixture.owner, "parties", null, { kind: "supplier", code: key(), name: "Directory supplier" }, key());

  const customers = await commercePage(fixture.owner, { view: "parties", kind: "customer" });
  const suppliers = await commercePage(fixture.owner, { view: "parties", kind: "supplier" });
  const customerIds = customers.parties.map(row => row.id);
  const supplierIds = suppliers.parties.map(row => row.id);

  expect(customerIds).toContain(customer.id);
  expect(customerIds).not.toContain(supplier.id);
  expect(supplierIds).toContain(supplier.id);
  expect(supplierIds).not.toContain(customer.id);
  expect(customers.parties.every(row => row.kind === "customer")).toBe(true);
  expect(suppliers.parties.every(row => row.kind === "supplier")).toBe(true);
});
