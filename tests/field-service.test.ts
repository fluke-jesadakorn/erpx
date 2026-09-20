import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import { orderSchema, saveMaster, saveOrder } from "../src/server/commerce";
import { createSupportTicket } from "../src/server/support";
import {
  changeAgreement,
  changeServiceOrder,
  createAgreement,
  createServiceOrder,
  fieldServiceDetail,
  fieldServiceWork,
} from "../src/server/field-service";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  customer: string,
  warehouse: string,
  product: string,
  ticket: string;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  f.head.role = "support_agent";
  f.head.branchId = null;
  f.head.departmentId = null;
  f.head.assignmentId = await f.grant(f.head, "support_agent");
  for (const a of [f.owner, f.head, f.tie, f.claimant])
    a.modules = ["field_service", "support"];
  const c = await saveMaster(
      f.owner,
      "parties",
      null,
      { kind: "customer", code: key(), name: "Field service customer" },
      key(),
    ),
    w = await saveMaster(
      f.owner,
      "warehouses",
      null,
      { branchId: f.branch, code: key(), name: "Parts warehouse" },
      key(),
    ),
    p = await saveMaster(
      f.owner,
      "products",
      null,
      { sku: key(), name: "Replacement controller", price: "80" },
      key(),
    );
  customer = String(c.id);
  warehouse = String(w.id);
  product = String(p.id);
  await db
    .prepare(
      "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,1000,8000,1)",
    )
    .bind(warehouse, product)
    .run();
  const order = await saveOrder(
    f.owner,
    null,
    orderSchema.parse({
      kind: "quote",
      partyId: customer,
      warehouseId: warehouse,
      orderDate: "2026-09-20",
      dueDate: "2026-09-25",
      lines: [{ productId: product, quantity: "1", price: "80", taxBps: 0 }],
    }),
    key(),
  );
  ticket = String(
    (
      await createSupportTicket(
        f.owner,
        {
          orderId: String(order.id),
          sourceVersion: 1,
          title: "Controller fails on site",
          issue: "Customer reports the installed controller does not start.",
          ownerId: f.head.userId,
          dueDate: "2026-09-25",
        },
        key(),
      )
    ).id,
  );
});
async function agreement() {
  const x = await createAgreement(
    f.owner,
    {
      branchId: f.branch,
      departmentId: null,
      customerId: customer,
      assetId: null,
      title: "On-site support agreement",
      startDate: "2026-01-01",
      endDate: "2027-12-31",
      currency: "THB",
      visitRateMinor: 150000,
    },
    key(),
  );
  await expect(
    changeAgreement(
      f.owner,
      String(x.id),
      {
        action: "activate",
        expectedVersion: 1,
        reason: "Self approval must be rejected",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await changeAgreement(
    f.tie,
    String(x.id),
    {
      action: "activate",
      expectedVersion: 1,
      reason: "Independent service manager checked coverage and rate",
    },
    key(),
  );
  return String(x.id);
}
async function serviceOrder() {
  const g = await agreement();
  return String(
    (
      await createServiceOrder(
        f.owner,
        {
          ticketId: ticket,
          ticketVersion: 1,
          agreementId: g,
          agreementVersion: 2,
          assetId: null,
          issue: "Diagnose and replace the failed controller.",
          site: "Customer plant A",
          technicianId: f.claimant.userId,
          scheduledAt: Date.now() + 60000,
        },
        key(),
      )
    ).id,
  );
}
const act = (
  actor: typeof f.owner,
  id: string,
  input: Record<string, unknown>,
) => changeServiceOrder(actor, id, input, key());
it("runs agreement, dispatch, parts, exception, customer sign-off and independent billing release", async () => {
  const id = await serviceOrder();
  expect(
    (await fieldServiceWork(f.claimant)).find((x) => x.id === id),
  ).toMatchObject({ queue: "todo", canAct: true });
  await act(f.claimant, id, {
    action: "reserve-part",
    expectedVersion: 1,
    warehouseId: warehouse,
    productId: product,
    quantityMilli: 1000,
    expiresAt: Date.now() + 3600000,
    reason: "Controller reserved for confirmed appointment",
  });
  expect(
    await db
      .prepare(
        "SELECT quantity_milli FROM inventory_active_reservations WHERE source_kind='field_service'",
      )
      .first("quantity_milli"),
  ).toBe(1000);
  await act(f.claimant, id, {
    action: "dispatch",
    expectedVersion: 2,
    reason: "Technician received route and required safety context",
  });
  await act(f.claimant, id, {
    action: "start",
    expectedVersion: 3,
    reason: "Customer granted site access and visit started",
  });
  await act(f.claimant, id, {
    action: "exception",
    expectedVersion: 4,
    reason: "Power isolation was not ready at arrival",
  });
  expect(
    (await fieldServiceWork(f.claimant)).find((x) => x.id === id)?.blocker,
  ).toContain("Power isolation");
  await act(f.claimant, id, {
    action: "resume",
    expectedVersion: 5,
    reason: "Customer completed power isolation",
  });
  const reservation = await db
    .prepare("SELECT id FROM field_service_part_reservations WHERE order_id=?")
    .bind(id)
    .first("id");
  await act(f.claimant, id, {
    action: "issue-part",
    expectedVersion: 6,
    reservationId: String(reservation),
    quantityMilli: 1000,
    reason: "Installed the reserved replacement controller",
  });
  expect(
    await db
      .prepare(
        "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?",
      )
      .bind(warehouse, product)
      .first(),
  ).toEqual({ quantity_milli: 0, value_minor: 0 });
  await act(f.claimant, id, {
    action: "complete",
    expectedVersion: 7,
    laborMinutes: 90,
    evidence: "Startup test and safety checks passed; serial photographed.",
    customerConfirmation:
      "Customer representative confirmed normal operation on site.",
    reason: "Visit completed with customer sign-off",
  });
  await expect(
    act(f.claimant, id, {
      action: "release-billing",
      expectedVersion: 8,
      reason: "Technician cannot release own billing",
    }),
  ).rejects.toMatchObject({ code: "INDEPENDENT_BILLING_RELEASE_REQUIRED" });
  await act(f.owner, id, {
    action: "release-billing",
    expectedVersion: 8,
    reason:
      "Independent reviewer checked agreement, evidence and customer sign-off",
  });
  const d = await fieldServiceDetail(f.owner, id);
  expect(d.order).toMatchObject({
    status: "billed",
    version: 9,
    labor_minutes: 90,
  });
  expect(d.billing).toMatchObject({
    currency: "THB",
    amount_minor: 150000,
    source_order_version: 9,
    status: "released",
  });
  expect(d.history.map((x) => x.action)).toEqual([
    "create",
    "reserve-part",
    "dispatch",
    "start",
    "exception",
    "resume",
    "issue-part",
    "complete",
    "release-billing",
  ]);
});
it("serializes last-unit reservations and protects scope, versions and immutable history", async () => {
  const first = await serviceOrder(),
    second = await serviceOrder(),
    reserve = (id: string) =>
      act(f.owner, id, {
        action: "reserve-part",
        expectedVersion: 1,
        warehouseId: warehouse,
        productId: product,
        quantityMilli: 1000,
        expiresAt: Date.now() + 3600000,
        reason: "Competing last controller reservation",
      }),
    outcomes = await Promise.allSettled([reserve(first), reserve(second)]);
  expect(outcomes.filter((x) => x.status === "fulfilled")).toHaveLength(1);
  const winner = outcomes[0].status === "fulfilled" ? first : second;
  await expect(
    fieldServiceDetail({ ...f.owner, branchId: "foreign" }, winner),
  ).rejects.toMatchObject({ status: 404 });
  await expect(
    act(f.owner, winner, {
      action: "dispatch",
      expectedVersion: 1,
      reason: "Stale version must fail",
    }),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await expect(
    db
      .prepare("DELETE FROM field_service_order_events WHERE order_id=?")
      .bind(winner)
      .run(),
  ).rejects.toThrow("FIELD_SERVICE_HISTORY_IMMUTABLE");
  await expect(
    db
      .prepare("UPDATE field_service_orders SET ticket_id=? WHERE id=?")
      .bind("different", winner)
      .run(),
  ).rejects.toThrow("FIELD_SERVICE_ORDER_SOURCE_IMMUTABLE");
});
it("surfaces a revoked technician and permits reviewed reassignment", async () => {
  const id = await serviceOrder();
  await db
    .prepare("UPDATE assignments SET revoked_at=? WHERE id=?")
    .bind(Date.now(), f.claimant.assignmentId)
    .run();
  expect(
    (await fieldServiceWork(f.owner)).find((x) => x.id === id)?.blocker,
  ).toContain("no longer eligible");
  await act(f.owner, id, {
    action: "schedule",
    expectedVersion: 1,
    technicianId: f.owner.userId,
    scheduledAt: Date.now() + 120000,
    reason: "Manager reassigns after technician access ended",
  });
  expect((await fieldServiceDetail(f.owner, id)).order.technician_id).toBe(
    f.owner.userId,
  );
});
