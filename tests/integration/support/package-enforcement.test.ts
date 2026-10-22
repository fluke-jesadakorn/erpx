import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { createSupportTicket, changeSupportTicket, getSupportTicket } from "@erpx/server-support/public/support";
import { createSupportPolicy, changeSupportPolicy } from "@erpx/server-support/public/support-sla";
import { packageGrantStore } from "@erpx/server-commercial/adapters/package-grants";
import { ownerPackageObligations, withOwnerPackageObligations } from "@erpx/server-commercial/public/owner-package-obligations";
import { readCompanyPackageGrants, packageGrantRevision } from "@erpx/server-commercial/public/package-grants";
import type { CompanyPackageGrant } from "@erpx/server-commercial/public/packages";
import { saveMaster, saveOrder, orderSchema } from "@erpx/server-commerce/public/commerce";
import { requestSupportSession, changeSupportSession } from "@erpx/server-support/public/support-sessions";
import { createSupportIncident, changeSupportIncident } from "@erpx/server-support/public/support-incidents";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  for (const [actor, role] of [[f.head, "support_agent"], [f.manager, "support_manager"]] as const) {
    actor.role = role; actor.branchId = null; actor.departmentId = null; actor.assignmentId = await f.grant(actor, role);
  }
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
});
afterEach(() => { vi.unstubAllEnvs(); vi.restoreAllMocks(); });
const report = { kind: "general", title: "Private printer report", issue: "The office printer stopped responding." };
const store = () => packageGrantStore(db, {
  authorize: async () => ({ userId: "isolated-platform-authority-fixture" }),
  obligations: (transaction, companyId, packageId) => ownerPackageObligations(db, transaction, companyId, packageId),
});
async function grant(state: CompanyPackageGrant["state"], endsAt: number | null = null) {
  const current = (await readCompanyPackageGrants(db, f.company)).find(value => value.packageId === "support");
  const value: CompanyPackageGrant = { companyId: f.company, packageId: "support", packageVersion: 1, state,
    startsAt: 0, endsAt, version: (current?.version ?? 0) + 1, reason: "Native owner enforcement review", evidence: "test:normal-commercial-apply" };
  if (state !== "inactive") return store().apply({ grant: value, expectedVersion: current?.version ?? 0, idempotencyKey: key() });
  const input = { companyId: f.company, expectedRevision: packageGrantRevision(f.company, await readCompanyPackageGrants(db, f.company)),
    idempotencyKey: key(), changes: [{ grant: value, expectedVersion: current!.version }] };
  return withOwnerPackageObligations(db, { companyId: f.company, packageId: "support", packageVersion: 1 }, transaction =>
    store().batch.applyIn(transaction, input, async () => {}));
}
async function action(id: string, action: string, actor = f.manager, extra = {}) {
  const row = await getSupportTicket(actor, id);
  return changeSupportTicket(actor, id, { action, expectedVersion: Number(row.version), reason: "Owner reviewed the existing report", ...extra }, key());
}
async function orderTicket() {
  const customer = await saveMaster(f.owner, "parties", null, { kind: "customer", code: key(), name: "Fictional Support customer" }, key());
  const warehouse = await saveMaster(f.owner, "warehouses", null, { branchId: f.branch, code: key(), name: "Support source warehouse" }, key());
  const product = await saveMaster(f.owner, "products", null, { sku: key(), name: "Support source item", price: "10" }, key());
  const order = await saveOrder(f.owner, null, orderSchema.parse({ kind: "quote", partyId: customer.id, warehouseId: warehouse.id,
    orderDate: "2026-10-15", dueDate: "2026-10-20", lines: [{ productId: product.id, quantity: "1", price: "10", taxBps: 0 }] }), key());
  return createSupportTicket(f.owner, { orderId: order.id, sourceVersion: 1, title: "Private installation issue", issue: "The customer needs installation help.", ownerId: f.head.userId, dueDate: "2026-10-20" }, key());
}

it("denies a real owner command without a purchase and leaves no record, receipt, audit or outbox", async () => {
  const commandId = key();
  await expect(createSupportTicket(f.claimant, { kind: "general", title: "Private printer report", issue: "The office printer stopped responding." }, commandId))
    .rejects.toMatchObject({ status: 403, code: "PACKAGE_REQUIRED" });
  for (const table of ["command_receipts", "audit_events", "outbox_events"]) {
    expect(await db.prepare(`SELECT count(*) total FROM ${table} WHERE command_id=?`).bind(commandId).first()).toEqual({ total: 0 });
  }
  expect(await db.prepare("SELECT count(*) total FROM support_tickets WHERE company_id=?").bind(f.company).first()).toEqual({ total: 0 });
});

it("preserves normal commands in disabled mode without querying purchase sources", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled");
  const prepare = vi.spyOn(db, "prepare");
  const result = await createSupportTicket(f.claimant, report, key());
  expect(await getSupportTicket(f.claimant, String(result.id))).toMatchObject({ status: "triage" });
  expect(prepare.mock.calls.some(([sql]) => /company_package|commercial.package/.test(sql))).toBe(false);
});

it("observes a denied purchase in shadow mode while the real command commits once", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "shadow");
  const info = vi.spyOn(console, "info").mockImplementation(() => {}), commandId = key();
  const result = await createSupportTicket(f.claimant, report, commandId);
  expect(await createSupportTicket(f.claimant, report, commandId)).toEqual(result);
  expect(await db.prepare("SELECT count(*) total FROM command_receipts WHERE command_id=?").bind(commandId).first()).toEqual({ total: 1 });
  const observations = info.mock.calls.map(([entry]) => JSON.parse(String(entry)));
  expect(observations.some(item => item.event === "commercial.purchase.shadow" && item.phase === "checked" && item.code === "PACKAGE_REQUIRED")).toBe(true);
  expect(JSON.stringify(observations)).not.toMatch(/printer|issue|userId|reason|evidence|grants/);
});

it("does not turn a shadow telemetry outage into a rejected or duplicated owner command", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "shadow");
  vi.spyOn(console, "info").mockImplementation(() => { throw new Error("Fictional unavailable telemetry sink"); });
  const commandId = key(), result = await createSupportTicket(f.claimant, report, commandId);
  expect(await createSupportTicket(f.claimant, report, commandId)).toEqual(result);
  expect(await db.prepare("SELECT count(*) total FROM command_receipts WHERE command_id=?").bind(commandId).first()).toEqual({ total: 1 });
});

it("fails closed on unknown modes and refuses enforcement outside local development", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforce-typo");
  await expect(createSupportTicket(f.claimant, report, key())).rejects.toMatchObject({ code: "PACKAGE_ENFORCEMENT_CONFIGURATION" });
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
  const original = env.ERPX_ENVIRONMENT;
  try {
    env.ERPX_ENVIRONMENT = "production";
    await expect(createSupportTicket(f.claimant, report, key())).rejects.toMatchObject({ code: "PACKAGE_ENFORCEMENT_ENVIRONMENT" });
  } finally { env.ERPX_ENVIRONMENT = original; }
  expect(await db.prepare("SELECT count(*) total FROM support_tickets WHERE company_id=?").bind(f.company).first()).toEqual({ total: 0 });
});

it("allows settlement and replay while draining, denies new work and reopening, and retains inactive history", async () => {
  await grant("active");
  const commandId = key(), result = await createSupportTicket(f.claimant, report, commandId), id = String(result.id);
  await action(id, "triage", f.manager, { ownerId: f.head.userId, dueDate: "2026-10-20" });
  await grant("draining");
  await expect(createSupportTicket(f.claimant, report, key())).rejects.toMatchObject({ code: "PACKAGE_CAPABILITY_CHANGED" });
  expect(await createSupportTicket(f.claimant, report, commandId)).toEqual(result);
  await action(id, "start", f.head); await action(id, "resolve", f.head); await action(id, "confirm-general");
  await expect(action(id, "reopen")).rejects.toMatchObject({ code: "PACKAGE_CAPABILITY_CHANGED" });
  await grant("inactive");
  expect(await getSupportTicket(f.manager, id)).toMatchObject({ status: "closed" });
  expect(await createSupportTicket(f.claimant, report, commandId)).toEqual(result);
  expect(await db.prepare("SELECT count(*) total FROM command_receipts WHERE command_id=?").bind(commandId).first()).toEqual({ total: 1 });
});

it("uses expired active grants as draining and never treats another Company's purchase as this Company's", async () => {
  await grant("active", Date.now() - 1);
  await expect(createSupportTicket(f.claimant, report, key())).rejects.toMatchObject({ code: "PACKAGE_CAPABILITY_CHANGED" });
  await expect(createSupportTicket({ ...f.claimant, companyId: "unrelated-company" }, report, key())).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" });
});

it("denies temporary-access expansion while draining and permits independent rejection and revocation", async () => {
  await grant("active");
  const ticket = await orderTicket(), request = { ticketId: ticket.id, ticketVersion: 1, requestedFor: f.head.userId, expiresAt: Date.now() + 60000, reason: "Review the customer order for this ticket" };
  const approved = await requestSupportSession(f.head, request, key()), pending = await requestSupportSession(f.head, request, key());
  await changeSupportSession(f.manager, String(approved.id), { action: "approve", expectedVersion: 1, reason: "Independent scoped access approval" }, key());
  await grant("draining");
  await expect(requestSupportSession(f.head, request, key())).rejects.toMatchObject({ code: "PACKAGE_CAPABILITY_CHANGED" });
  await expect(changeSupportSession(f.manager, String(pending.id), { action: "approve", expectedVersion: 1, reason: "No new approvals while draining" }, key())).rejects.toMatchObject({ code: "PACKAGE_CAPABILITY_CHANGED" });
  await expect(changeSupportSession(f.manager, String(approved.id), { action: "issue-capability", code: "customer_order_context", expectedVersion: 2, reason: "No expanded access while draining" }, key())).rejects.toMatchObject({ code: "PACKAGE_CAPABILITY_CHANGED" });
  await changeSupportSession(f.manager, String(pending.id), { action: "reject", expectedVersion: 1, reason: "Settle the pending access request" }, key());
  await changeSupportSession(f.manager, String(approved.id), { action: "revoke", expectedVersion: 2, reason: "Withdraw temporary access" }, key());
  expect((await db.prepare("SELECT status FROM support_sessions WHERE id IN (?,?) ORDER BY status").bind(approved.id, pending.id).all()).results).toEqual([{ status: "rejected" }, { status: "revoked" }]);
});

it("permits an existing incident's cancellation while draining but denies creating another incident", async () => {
  await grant("active");
  const ticket = await orderTicket(), input = { ticketId: ticket.id, ticketVersion: 1, title: "Shared installation issue", impact: "Several installations need investigation", severity: "high", ownerId: f.head.userId, dueDate: "2026-10-20" };
  const incident = await createSupportIncident(f.manager, input, key());
  await grant("draining");
  await expect(createSupportIncident(f.manager, input, key())).rejects.toMatchObject({ code: "PACKAGE_CAPABILITY_CHANGED" });
  await changeSupportIncident(f.manager, String(incident.id), { action: "cancel", expectedVersion: 1, reason: "Settle the duplicate investigation" }, key());
  expect(await db.prepare("SELECT status FROM support_incidents WHERE id=?").bind(incident.id).first()).toEqual({ status: "cancelled" });
});

it("keeps independent policy review and current owner authority when a purchase is active", async () => {
  await grant("active");
  const policy = { targetMinutes: 60, pauseWhileWaiting: true,
    calendar: { timezone: "Asia/Bangkok", weekdays: [1, 2, 3, 4, 5], opensAtMinute: 540, closesAtMinute: 1020, holidays: [] } };
  const created = await createSupportPolicy(f.manager, { name: "Reviewed working target", policy, reason: "Normal independent target review" }, key());
  await expect(createSupportPolicy(f.head, { name: "Agent cannot manage targets", policy, reason: "A purchase cannot supply management permission" }, key())).rejects.toMatchObject({ status: 403, code: "PERMISSION_REQUIRED" });
  await expect(changeSupportPolicy(f.manager, String(created.id), { action: "approve", expectedVersion: 1, reason: "Self approval stays forbidden" }, key())).rejects.toMatchObject({ code: "INDEPENDENT_APPROVER_REQUIRED" });
  await grant("draining");
  await expect(createSupportPolicy(f.manager, { name: "New target while draining", policy, reason: "Must require an active purchase" }, key())).rejects.toMatchObject({ code: "PACKAGE_CAPABILITY_CHANGED" });
  await changeSupportPolicy(f.manager, String(created.id), { action: "retire", expectedVersion: 1, reason: "Retire the existing proposed target" }, key());
});

it("queues a new owner command before Company locking while a settlement snapshot applies inactivation", async () => {
  await grant("active"); await grant("draining");
  const current = (await readCompanyPackageGrants(db, f.company))[0], commandId = key();
  const request = { companyId: f.company, expectedRevision: packageGrantRevision(f.company, [current]), idempotencyKey: key(),
    changes: [{ expectedVersion: current.version, grant: { ...current, state: "inactive" as const, version: current.version + 1 } }] };
  let pending: Promise<unknown> | undefined;
  await withOwnerPackageObligations(db, { companyId: f.company, packageId: "support", packageVersion: 1 }, async transaction => {
    pending = createSupportTicket(f.claimant, report, commandId).then(value => ({ value }), error => ({ error }));
    let queued = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      queued = Boolean(await db.prepare("SELECT 1 FROM pg_locks WHERE relation='company_package_grants'::regclass AND mode='RowExclusiveLock' AND NOT granted").first());
      if (queued) break;
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    expect(queued).toBe(true);
    // If the waiting writer held the Company advisory lock, this would deadlock.
    await store().batch.applyIn(transaction, request, async () => {});
  });
  expect(await pending).toMatchObject({ error: { code: "PACKAGE_CAPABILITY_CHANGED" } });
  expect(await db.prepare("SELECT count(*) total FROM command_receipts WHERE command_id=?").bind(commandId).first()).toEqual({ total: 0 });
});

it("lets an existing writer finish before a settlement snapshot acquires owner source locks", async () => {
  await grant("active"); await grant("draining");
  let pending: Promise<unknown> | undefined;
  await db.transaction(async transaction => {
    await transaction.prepare("LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE").run();
    await transaction.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`commercial.package:company:${f.company}`).run();
    pending = withOwnerPackageObligations(db, { companyId: f.company, packageId: "support", packageVersion: 1 }, async source => {
      expect(await source.prepare("SELECT count(*) total FROM support_tickets WHERE company_id=?").bind(f.company).first()).toEqual({ total: 0 });
    });
    let queued = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      queued = Boolean(await db.prepare("SELECT 1 FROM pg_locks WHERE relation='company_package_grants'::regclass AND mode='ShareRowExclusiveLock' AND NOT granted").first());
      if (queued) break;
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    expect(queued).toBe(true);
    await transaction.prepare("SET LOCAL lock_timeout='200ms'").run();
    // Even an absent future row is writable: settlement has not locked Support.
    await transaction.prepare("UPDATE support_tickets SET version=version WHERE false").run();
  });
  await pending;
});

it("rolls back the owner result, receipt, audit and outbox when expiry occurs after outbox insertion", async () => {
  const commandId = key();
  await db.exec(`CREATE FUNCTION test_purchase_expiry_after_outbox() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN IF NEW.command_id='${commandId}' THEN
      IF NOT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=NEW.command_id) OR NOT EXISTS(SELECT 1 FROM audit_events WHERE command_id=NEW.command_id)
        THEN RAISE EXCEPTION 'TEST_EXPECTED_RECEIPT_AND_AUDIT'; END IF;
      PERFORM pg_sleep(1.2);
    END IF; RETURN NEW; END $$;
    CREATE TRIGGER test_purchase_expiry_after_outbox AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION test_purchase_expiry_after_outbox();`);
  try {
    await grant("active", Date.now() + 700);
    await expect(createSupportTicket(f.claimant, report, commandId)).rejects.toMatchObject({ status: 409, code: "PACKAGE_CAPABILITY_CHANGED" });
    for (const table of ["command_receipts", "audit_events", "outbox_events"]) {
      expect(await db.prepare(`SELECT count(*) total FROM ${table} WHERE command_id=?`).bind(commandId).first()).toEqual({ total: 0 });
    }
    expect(await db.prepare("SELECT count(*) total FROM support_tickets WHERE company_id=?").bind(f.company).first()).toEqual({ total: 0 });
  } finally { await db.exec("DROP TRIGGER test_purchase_expiry_after_outbox ON outbox_events; DROP FUNCTION test_purchase_expiry_after_outbox()"); }
});
