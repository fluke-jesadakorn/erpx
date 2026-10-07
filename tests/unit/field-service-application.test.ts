import { describe, expect, it, vi } from "vitest";
import { createFieldServiceApplication, type FieldServicePorts, type FieldServiceMutation } from "../../src/server/modules/field_service/application/service";
import { orderTransition, type FieldWorkspace, type OrderFacts } from "../../src/server/modules/field_service/domain/service";

type Effect = { marker: string };
const now = Date.parse("2026-10-06T12:00:00Z");
function fixture() {
  const order: OrderFacts = { id: "order", version: 3, status: "in_visit", company_id: "company", branch_id: "branch", department_id: null, technician_id: "technician", created_by: "creator", updated_at: 42, scheduled_at: 100, technician_eligible: 1, can_review: 1, issue: "Recorded issue", site: "Customer site", exception_reason: null, agreement_id: "agreement", visit_rate_minor: 10001, warranty_discount_bps: 3333, warranty_end: "2026-10-06", currency: "THB", agreement_status: "active", ticket_id: "ticket", customer_id: "customer", ticket_title: "Reported issue", customer_name: "Customer" };
  const data: FieldWorkspace = { agreements: [{ id: "agreement", version: 1, status: "draft", created_by: "creator", updated_at: 42, can_review: 1, warranty_discount_bps: 0, title: "Reviewed plan", customer_name: "Customer", start_date: "2026-10-01", end_date: "2026-10-31" }], orders: [order], customers: [], assets: [], tickets: [], people: [], nodes: [], canWrite: true };
  const mutations: FieldServiceMutation<Effect, Effect>[] = [];
  const ports: FieldServicePorts<Effect, Effect, string[]> = {
    actorId: "technician", canRead: true, canReview: true, clock: { now: () => now },
    authorize: vi.fn(async () => {}), authorizeReview: vi.fn(async () => {}), replay: vi.fn(async () => null),
    workspace: vi.fn(async () => data), agreement: vi.fn(async () => data.agreements[0]), order: vi.fn(async () => order),
    detail: vi.fn(async () => ({ spares: ["spare"], parts: [], consumptions: [], history: [], billing: null })),
    reservation: vi.fn(async () => ({ id: "reservation", warehouse_id: "warehouse", product_id: "part", quantity_milli: 1000, consumed_milli: 0, released_milli: 0 })),
    prepareConsumption: vi.fn(async () => ({ effect: { marker: "inventory" }, id: "consumption", valueMinor: 8000, journalId: "journal" })),
    prepareBilling: vi.fn(async () => ({ marker: "finance" })),
    async commit(mutation, result) { mutations.push(mutation); return result; },
  };
  return { ports, order, data, mutations, app: createFieldServiceApplication(ports) };
}
const reason = "Reviewed service order evidence";

describe("Field Service injected owner application", () => {
  it("checks current authority before replay and never mutates after revocation", async () => {
    const f = fixture(); f.ports.authorize = vi.fn(async () => { throw new Error("revoked"); });
    await expect(f.app.changeOrder("order", { action: "complete", expectedVersion: 3, reason, laborMinutes: 1, evidence: "Checked original", customerConfirmation: "Customer confirmed" }, "key")).rejects.toThrow("revoked");
    expect(f.ports.replay).not.toHaveBeenCalled(); expect(f.mutations).toEqual([]);
  });
  it("returns a lost-response receipt before reading mutable source or staging effects", async () => {
    const f = fixture(), prior = { id: "order", version: 4, status: "billed" }; f.ports.replay = vi.fn(async () => prior);
    expect(await f.app.changeOrder("order", { action: "release-billing", expectedVersion: 3, reason }, "original-key")).toEqual(prior);
    expect(f.ports.order).not.toHaveBeenCalled(); expect(f.ports.prepareBilling).not.toHaveBeenCalled(); expect(f.mutations).toEqual([]);
  });
  it("retains agreement operation identity and active-source retirement requirement", async () => {
    const f = fixture(); expect(await f.app.changeAgreement("agreement", { action: "retire", expectedVersion: 4, reason }, "key")).toEqual({ id: "agreement", version: 5, status: "retired" });
    expect(f.ports.authorizeReview).toHaveBeenCalledOnce(); expect(f.mutations[0]).toMatchObject({ action: "agreement", operation: "field.agreement.retire", transition: { expected: "active", status: "retired" }, now });
  });
  it("keeps the source version and first event identity when drafting a scheduled order", async () => {
    const f = fixture(); await f.app.createOrder({ ticketId: "ticket", ticketVersion: 7, agreementId: "agreement", agreementVersion: 4, issue: " Recorded issue ", site: "Customer site", technicianId: "technician", scheduledAt: now }, "key");
    expect(f.mutations[0]).toMatchObject({ action: "create-order", operation: "field.order.create", id: "service-order-key", status: "scheduled", input: { ticketVersion: 7, agreementVersion: 4, issue: "Recorded issue" } });
  });
  it("refuses a revoked assigned technician and actions incompatible with the current state", async () => {
    const f = fixture(); f.order.technician_eligible = 0; f.order.can_review = 0;
    await expect(f.app.changeOrder("order", { action: "cancel", expectedVersion: 3, reason }, "key")).rejects.toMatchObject({ code: "FIELD_SERVICE_OWNER_REQUIRED" });
    f.order.technician_eligible = 1;
    await expect(f.app.changeOrder("order", { action: "dispatch", expectedVersion: 3, reason }, "key")).rejects.toMatchObject({ code: "FIELD_SERVICE_STATE_CHANGED" });
    expect(f.mutations).toEqual([]);
  });
  it("stages lot consumption and the same immutable cost evidence in one owner mutation", async () => {
    const f = fixture(); await f.app.changeOrder("order", { action: "issue-part", expectedVersion: 3, reservationId: "reservation", quantityMilli: 1000, allocations: [{ lotId: "lot", quantityMilli: 1000, balanceVersion: 9 }], reason }, "key");
    expect(f.ports.prepareConsumption).toHaveBeenCalledWith(f.order, expect.objectContaining({ id: "reservation" }), expect.objectContaining({ allocations: [{ lotId: "lot", quantityMilli: 1000, balanceVersion: 9 }] }), "key", now);
    expect(f.mutations[0]).toMatchObject({ action: "order", consumption: { marker: "inventory" }, transition: { status: "in_visit", details: { reservationId: "reservation", consumptionId: "consumption", costMinor: 8000, journalId: "journal" } } });
  });
  it("cannot commit if its reservation is absent or Inventory refuses preparation", async () => {
    const f = fixture(), input = { action: "issue-part", expectedVersion: 3, reservationId: "reservation", quantityMilli: 1000, reason };
    f.ports.reservation = vi.fn(async () => null);
    await expect(f.app.changeOrder("order", input, "missing")).rejects.toMatchObject({ code: "FIELD_SERVICE_PART_NOT_FOUND" }); expect(f.ports.prepareConsumption).not.toHaveBeenCalled();
    f.ports.reservation = vi.fn(async () => ({ id: "reservation" })); f.ports.prepareConsumption = vi.fn(async () => { throw new Error("stock changed"); });
    await expect(f.app.changeOrder("order", input, "changed")).rejects.toThrow("stock changed"); expect(f.mutations).toEqual([]);
  });
  it("requires an independent billing reviewer and snapshots inclusive warranty pricing", async () => {
    const f = fixture(); f.order.status = "completed";
    await expect(f.app.changeOrder("order", { action: "release-billing", expectedVersion: 3, reason }, "self")).rejects.toMatchObject({ code: "INDEPENDENT_BILLING_RELEASE_REQUIRED" });
    expect(f.ports.prepareBilling).not.toHaveBeenCalled(); f.ports.actorId = "reviewer";
    await f.app.changeOrder("order", { action: "release-billing", expectedVersion: 3, reason }, "reviewed");
    expect(f.ports.prepareBilling).toHaveBeenCalledWith(f.order, { invoiceDate: "2026-10-06", dueDate: "2026-11-05", warrantyDiscountBps: 3333, warrantyApplied: true, amountMinor: 6667 }, "reviewed", now);
    expect(f.mutations[0]).toMatchObject({ billing: { marker: "finance" }, transition: { status: "billed", billing: { amountMinor: 6667 } } });
    expect(orderTransition({ ...f.order, warranty_end: "2026-10-05" }, { action: "release-billing", expectedVersion: 3, reason }, "reviewer", true, now).billing?.amountMinor).toBe(10001);
  });
  it("refuses an expired reservation before staging a command", async () => {
    const f = fixture(); f.order.status = "scheduled";
    await expect(f.app.changeOrder("order", { action: "reserve-part", expectedVersion: 3, warehouseId: "warehouse", productId: "part", quantityMilli: 1000, expiresAt: now, reason }, "key")).rejects.toMatchObject({ code: "FIELD_SERVICE_PART_EXPIRY" }); expect(f.mutations).toEqual([]);
  });
  it("retains scoped detail and Work blockers without reading inaccessible sources", async () => {
    const f = fixture(); f.data.orders = [];
    await expect(f.app.detail("outside-scope")).rejects.toMatchObject({ code: "FIELD_SERVICE_ORDER_NOT_FOUND" }); expect(f.ports.detail).not.toHaveBeenCalled();
    f.data.orders = [f.order]; f.order.technician_eligible = 0; f.order.can_review = 0;
    expect((await f.app.work()).find(x => x.id === "order")).toMatchObject({ queue: "waiting", canAct: false, blocker: "Assigned technician is no longer eligible; reviewer reassignment is required" });
    f.ports.canRead = false; vi.mocked(f.ports.workspace).mockClear(); expect(await f.app.work()).toEqual([]); expect(f.ports.workspace).not.toHaveBeenCalled();
  });
});
