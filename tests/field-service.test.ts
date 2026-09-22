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
import { postInvoice } from "../src/server/finance";
import { escalateInventoryReservations } from "../src/server/inventory-reservation-alerts";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  customer: string,
  warehouse: string,
  product: string,
  ticket: string,
  warrantyAsset: string;
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
  warrantyAsset = `asset-${key()}`;
  await db
    .prepare(
      "INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(
      warrantyAsset,
      f.company,
      f.branch,
      null,
      key(),
      "Warranty covered controller",
      "equipment",
      "THB",
      8000,
      "2026-01-01",
      "2099-12-31",
      f.owner.userId,
      Date.now(),
      Date.now(),
    )
    .run();
  await db
    .prepare(
      "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,1000,8000,1)",
    )
    .bind(warehouse, product)
    .run();
  await db
    .prepare(
      "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'asset','THB',1,'receivable',1,1)",
    )
    .bind(`${f.company}-receivable`, f.chart, f.company, "1200", "Trade receivables")
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
async function agreement(options: { assetId?: string; warrantyDiscountBps?: number } = {}) {
  const x = await createAgreement(
    f.owner,
    {
      branchId: f.branch,
      departmentId: null,
      customerId: customer,
      assetId: options.assetId ?? null,
      title: "On-site support agreement",
      startDate: "2026-01-01",
      endDate: "2027-12-31",
      currency: "THB",
      visitRateMinor: 150000,
      warrantyDiscountBps: options.warrantyDiscountBps ?? 0,
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
async function serviceOrder(options: { assetId?: string; warrantyDiscountBps?: number } = {}) {
  const g = await agreement(options);
  return String(
    (
      await createServiceOrder(
        f.owner,
        {
          ticketId: ticket,
          ticketVersion: 1,
          agreementId: g,
          agreementVersion: 2,
          assetId: options.assetId ?? null,
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
  const invoice = await db
    .prepare(
      "SELECT kind,source_type,source_id,status,total_minor,debit_account_id,credit_account_id FROM finance_invoices WHERE source_type='field_service' AND source_id=?",
    )
    .bind(id)
    .first();
  expect(invoice).toMatchObject({
    kind: "receivable",
    source_type: "field_service",
    source_id: id,
    status: "draft",
    total_minor: 150000,
    debit_account_id: `${f.company}-receivable`,
    credit_account_id: f.accounts.revenue,
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
it("applies the approved warranty discount only while the service asset is covered", async () => {
  const id = await serviceOrder({ assetId: warrantyAsset, warrantyDiscountBps: 5000 });
  await act(f.claimant, id, {
    action: "dispatch",
    expectedVersion: 1,
    reason: "Technician accepted the warranty appointment",
  });
  await act(f.claimant, id, {
    action: "start",
    expectedVersion: 2,
    reason: "Customer granted access for the covered asset",
  });
  await act(f.claimant, id, {
    action: "complete",
    expectedVersion: 3,
    laborMinutes: 45,
    evidence: "Warranty replacement completed and tested.",
    customerConfirmation: "Customer confirmed the covered repair is complete.",
    reason: "Warranty visit signed off by customer",
  });
  await act(f.owner, id, {
    action: "release-billing",
    expectedVersion: 4,
    reason: "Finance reviewer checked warranty coverage and customer evidence",
  });
  const release = await db
    .prepare("SELECT amount_minor,warranty_applied,warranty_discount_bps,warranty_end_snapshot,pricing_date FROM field_service_billing_releases WHERE order_id=?")
    .bind(id)
    .first();
  expect(release).toMatchObject({ amount_minor: 75000, warranty_applied: true, warranty_discount_bps: 5000, warranty_end_snapshot: "2099-12-31" });
  expect(String(release?.pricing_date)).toMatch(/^2026-09-2[12]$/);
  const invoice = await db
    .prepare("SELECT id,total_minor,outstanding_minor FROM finance_invoices WHERE source_type='field_service' AND source_id=?")
    .bind(id)
    .first();
  expect(invoice).toMatchObject({ total_minor: 75000, outstanding_minor: 75000 });
  await postInvoice(
    f.manager,
    String(invoice.id),
    {
      periodId: f.period,
      entryDate: new Date().toISOString().slice(0, 10),
      expectedVersion: 1,
    },
    key(),
  );
  expect(
    await db
      .prepare("SELECT status,journal_id FROM finance_invoices WHERE source_type='field_service' AND source_id=?")
      .bind(id)
      .first(),
  ).toMatchObject({ status: "posted" });
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
it("alerts both the service owner and assigned technician before reserved parts expire", async () => {
  const now = Date.now(), id = await serviceOrder();
  await act(f.claimant, id, { action: "reserve-part", expectedVersion: 1, warehouseId: warehouse, productId: product, quantityMilli: 1000, expiresAt: now + 3600000, reason: "Part needed for the confirmed visit" });
  expect((await escalateInventoryReservations(db, now)).expiring).toBeGreaterThanOrEqual(2);
  const alerts = await db.prepare("SELECT user_id,record_type,record_id,message FROM staff_notifications WHERE record_id=? AND message LIKE 'Stock reservation%'").bind(id).all<Record<string,string>>();
  expect(alerts.results.map(row => row.user_id).sort()).toEqual([f.claimant.userId, f.owner.userId].sort());
  expect(alerts.results.every(row => row.record_type === "field_service_order")).toBe(true);
  expect(await escalateInventoryReservations(db, now)).toEqual({ expiring: 0, expired: 0 });
  await db.prepare("UPDATE field_service_part_reservations SET released_milli=quantity_milli WHERE order_id=?").bind(id).run();
  await db.prepare("UPDATE field_service_part_reservations SET created_at=?,expires_at=? WHERE order_id=?").bind(now - 2, now - 1, id).run();
  expect(await escalateInventoryReservations(db, now)).toEqual({ expiring: 0, expired: 0 });
});
