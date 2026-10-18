import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { saveMaster, saveOrder, orderSchema } from "@/server/modules/commerce/public/commerce";
import { createCustomerNote } from "@/server/modules/sales/public/customer-notes";
import { createSupportTicket, changeSupportTicket, getSupportTicket } from "@/server/modules/support/public/support";
import { createSupportIncident, changeSupportIncident, getSupportIncident } from "@/server/modules/support/public/support-incidents";
import { requestSupportSession, changeSupportSession } from "@/server/modules/support/public/support-sessions";
import { createSupportPolicy, changeSupportPolicy, attachSupportClock } from "@/server/modules/support/public/support-sla";
import { drainSupportSla, supportAlerts, acknowledgeSupportAlert } from "@/server/modules/support/public/support-sla-alerts";
import { supportPackageObligations } from "@/server/modules/support/public/package-obligations";
import { ownerPackageObligations, readOwnerPackageObligations, withOwnerPackageObligations } from "@/server/modules/commercial/public/owner-package-obligations";
import { readPackageObligations } from "@/server/modules/commercial/public/package-obligations";
import { packageGrantStore } from "@/server/modules/commercial/adapters/package-grants";
import { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { CompanyPackageGrant } from "@/server/modules/commercial/public/packages";
import { readPurchasedAccess } from "@/server/modules/commercial/public/purchase-command-boundary";

vi.mock("@/server/modules/runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: supportDatabase ?? env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));
let supportDatabase: SqlDatabase | undefined;
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  for (const [actor, role] of [[f.head, "support_agent"], [f.manager, "support_manager"]] as const) {
    actor.role = role; actor.branchId = null; actor.departmentId = null; actor.assignmentId = await f.grant(actor, role);
  }
});
afterEach(() => { supportDatabase=undefined;vi.restoreAllMocks();vi.unstubAllEnvs(); });
const input = () => ({ companyId: f.company, packageId: "support", packageVersion: 1, moduleId: "support", now: Date.now() });
const observe = () => withOwnerPackageObligations(db, input(), tx => supportPackageObligations(db, tx, input()));
const store = () => packageGrantStore(db, {
  // Authority is an explicit fixture port. Production uses the existing live
  // Platform lease; Company authority is never used to administer purchases.
  authorize: async () => ({ userId: "isolated-provider-fixture" }),
  obligations: (tx, companyId, packageId) => ownerPackageObligations(db, tx, companyId, packageId),
});
const grant = (state: CompanyPackageGrant["state"], version: number) => ({
  expectedVersion: version - 1, idempotencyKey: key(), grant: { companyId: f.company, packageId: "support", packageVersion: 1,
    state, version, startsAt: 0, endsAt: null, reason: "Native Helpdesk settlement review", evidence: "test:owner-command-history" },
});
async function lifecycle(change: ReturnType<typeof grant>, apply = false) {
  const { packageGrantRevision, readCompanyPackageGrants } = await import("@/server/modules/commercial/public/package-grants");
  const request = { companyId: f.company, expectedRevision: packageGrantRevision(f.company, await readCompanyPackageGrants(db, f.company)),
    idempotencyKey: change.idempotencyKey, changes: [{ grant: change.grant, expectedVersion: change.expectedVersion }] };
  return withOwnerPackageObligations(db, input(), tx => apply
    ? store().batch.applyIn(tx, request, async () => {}) : store().batch.previewIn(tx, request));
}
async function ticketAction(id: string, action: string, actor = f.head, extra = {}) {
  const current = await getSupportTicket(actor, id);
  return changeSupportTicket(actor, id, { action, expectedVersion: Number(current.version), reason: `Owner reviewed ${action}`, ...extra }, key());
}
async function orderTicket() {
  const customer = await saveMaster(f.owner, "parties", null, { kind: "customer", code: key(), name: "Private fictional Support customer" }, key());
  const warehouse = await saveMaster(f.owner, "warehouses", null, { branchId: f.branch, code: key(), name: "Helpdesk source warehouse" }, key());
  const product = await saveMaster(f.owner, "products", null, { sku: key(), name: "Helpdesk source item", price: "10" }, key());
  const order = await saveOrder(f.owner, null, orderSchema.parse({ kind: "quote", partyId: customer.id, warehouseId: warehouse.id,
    orderDate: "2026-09-20", dueDate: "2026-09-23", lines: [{ productId: product.id, quantity: "1", price: "10", taxBps: 0 }] }), key());
  const ticket = await createSupportTicket(f.owner, { orderId: order.id, sourceVersion: 1, title: "Private installation issue",
    issue: "Customer requests help with the installation.", ownerId: f.head.userId, dueDate: "2026-09-24" }, key());
  return { id: String(ticket.id), orderId: String(order.id) };
}
async function confirm(id: string, orderId: string) {
  const note = await createCustomerNote(f.owner, { orderId, expectedVersion: 1, channel: "phone", occurredAt: Date.now(), body: "Customer verified the corrected installation" }, key());
  await ticketAction(id, "confirm", f.manager, { noteId: note.id, noteVersion: 1 });
}

it("rejects a future replacement preview while real Support work still needs settlement", async () => {
  await store().apply(grant("active",1));
  await createSupportTicket(f.claimant,{kind:"general",title:"Pending printer work",issue:"The printer needs a reviewed correction."},key());
  const next=grant("active",2);next.grant.startsAt=Date.now()+60000;
  await expect(lifecycle(next)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  expect(await db.prepare("SELECT version,starts_at,state FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toEqual({version:1,starts_at:0,state:"active"});
});
it("blocks future replacement Apply atomically, preserves normal cancellation and permits renewal after owner closure", async () => {
  const commercial=store();await commercial.apply(grant("active",1));
  const report=await createSupportTicket(f.claimant,{kind:"general",title:"Unsettled installation report",issue:"The customer needs an installation correction."},key());
  const next=grant("active",2);next.grant.startsAt=Date.now()+60000;
  const before=await db.prepare("SELECT count(*) n FROM company_package_grant_events WHERE company_id=?").bind(f.company).first();
  await expect(lifecycle(next,true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  expect(await db.prepare("SELECT count(*) n FROM company_package_grant_events WHERE company_id=?").bind(f.company).first()).toEqual(before);
  expect(await db.prepare("SELECT count(*) n FROM company_package_change_sets WHERE company_id=?").bind(f.company).first('n')).toBe(0);
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT","enforced");
  expect(await readPurchasedAccess(db,"enforced",{companyId:f.company,capability:"support.tickets"})).toMatchObject({newWork:true,settleExisting:true});
  await ticketAction(String(report.id),"cancel",f.manager);
  expect((await lifecycle(next)).after[0].startsAt).toBe(next.grant.startsAt);
  await lifecycle(next,true);
  expect(await readPurchasedAccess(db,"enforced",{companyId:f.company,capability:"support.tickets"})).toMatchObject({newWork:false,settleExisting:false,history:true});
  expect(await getSupportTicket(f.manager,String(report.id))).toMatchObject({status:"cancelled"});
});
it("requires the concrete source lease even when rescheduling an unstarted empty purchase", async () => {
  const first=grant("active",1);first.grant.startsAt=Date.now()+60000;await store().apply(first);
  const next=grant("active",2);next.grant.startsAt=first.grant.startsAt+60000;
  await expect(store().apply(next)).rejects.toThrow("SUPPORT_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
  await lifecycle(next);await lifecycle(next,true);
  expect(await readPurchasedAccess(db,"enforced",{companyId:f.company,capability:"support.tickets"})).toMatchObject({newWork:false,settleExisting:false,history:false});
});
it.each(["inactive","future"] as const)("rolls %s withdrawal and a late normal owner report back together before the leased transaction commits", async mode => {
  const commercial=store();await commercial.apply(grant("active",1));await commercial.apply(grant("draining",2));
  const next=grant(mode==='inactive'?"inactive":"active",3),reportKey=key();
  if(mode==='future')next.grant.startsAt=Date.now()+60000;
  const {packageGrantRevision,readCompanyPackageGrants}=await import("@/server/modules/commercial/public/package-grants");
  await expect(withOwnerPackageObligations(db,input(),async tx=>{
    const request={companyId:f.company,expectedRevision:packageGrantRevision(f.company,await readCompanyPackageGrants(tx,f.company)),idempotencyKey:next.idempotencyKey,changes:[{grant:next.grant,expectedVersion:2}]};
    await commercial.batch.applyIn(tx,request,async()=>{});
    supportDatabase=tx;
    try { await createSupportTicket(f.claimant,{kind:"general",title:"Late source report",issue:"A new unresolved report reached the leased owner transaction."},reportKey); }
    finally { supportDatabase=undefined; }
  })).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  expect(await db.prepare("SELECT version,state FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toEqual({version:2,state:"draining"});
  expect(await db.prepare("SELECT count(*) n FROM support_tickets WHERE company_id=?").bind(f.company).first('n')).toBe(0);
  expect(await db.prepare("SELECT count(*) n FROM command_receipts WHERE command_id=?").bind(reportKey).first('n')).toBe(0);
  expect(await db.prepare("SELECT count(*) n FROM company_package_change_sets WHERE id=?").bind(next.idempotencyKey).first('n')).toBe(0);
});
it("flushes a deferred owner projection before accepting the final zero-obligation observation", async () => {
  const commercial=store();await commercial.apply(grant("active",1));
  const report=await createSupportTicket(f.claimant,{kind:"general",title:"Closed source report",issue:"The owner has already cancelled this report."},key()),id=String(report.id);
  await ticketAction(id,"cancel",f.manager);await commercial.apply(grant("draining",2));
  expect((await observe()).outstanding).toBe(0);
  const before=await db.prepare("SELECT * FROM support_tickets WHERE id=?").bind(id).first(),fault=`settlement_deferred_${key().replaceAll('-','').slice(0,12)}`;
  expect(f.company).toMatch(/^[a-z0-9-]+$/);expect(id).toMatch(/^[a-z0-9-]+$/);
  await db.exec(`CREATE SEQUENCE ${fault}_attempt;
    CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
      PERFORM nextval('${fault}_attempt');
      UPDATE support_tickets SET status='triage',version=version+1 WHERE id='${id}';RETURN NEW;END;$$;
    CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED
      FOR EACH ROW WHEN (NEW.company_id='${f.company}') EXECUTE FUNCTION ${fault}();`);
  try {
    await expect(lifecycle(grant("inactive",3),true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
    expect(await db.prepare(`SELECT is_called FROM ${fault}_attempt`).first('is_called')).toBe(true);
    expect(await db.prepare("SELECT * FROM support_tickets WHERE id=?").bind(id).first()).toEqual(before);
    expect(await db.prepare("SELECT version,state FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toEqual({version:2,state:"draining"});
    expect(await db.prepare("SELECT count(*) n FROM company_package_change_sets WHERE company_id=?").bind(f.company).first('n')).toBe(0);
  } finally {await db.exec(`DROP TRIGGER IF EXISTS ${fault} ON company_package_change_sets;DROP FUNCTION IF EXISTS ${fault}();DROP SEQUENCE IF EXISTS ${fault}_attempt;`);}
});

it("uses real ticket settlement and independent review in normal Commercial Preview/Apply without deleting history", async () => {
  const commercial = store(); await commercial.apply(grant("active", 1));
  expect((await observe()).outstanding).toBe(0);
  const report = await createSupportTicket(f.claimant, { kind: "general", title: "Private printer issue", issue: "The shared printer stopped responding." }, key()), id = String(report.id);
  const first = await observe(); expect(first.outstanding).toBe(1);
  expect(JSON.stringify(first)).not.toMatch(/Private printer|claimant|userId|issue/);
  await commercial.apply(grant("draining", 2));
  const inactive = grant("inactive", 3);
  await expect(lifecycle(inactive)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  await expect(lifecycle(inactive, true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  await ticketAction(id, "triage", f.manager, { ownerId: f.head.userId, dueDate: "2026-09-27" });
  await ticketAction(id, "start"); await ticketAction(id, "resolve");
  expect((await observe()).outstanding).toBe(1);
  await expect(lifecycle(inactive, true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  await ticketAction(id, "confirm-general", f.manager);
  const closed = await db.prepare("SELECT * FROM support_tickets WHERE id=?").bind(id).first();
  const history = (await db.prepare("SELECT * FROM support_ticket_events WHERE ticket_id=? ORDER BY version").bind(id).all()).results;
  const before = await db.prepare("SELECT count(*) total FROM company_package_grant_events WHERE company_id=?").bind(f.company).first();
  expect((await lifecycle(inactive)).after[0].state).toBe("inactive");
  expect(await db.prepare("SELECT count(*) total FROM company_package_grant_events WHERE company_id=?").bind(f.company).first()).toEqual(before);
  // The batch path must also own the transaction for concrete readers.
  const { packageGrantRevision, readCompanyPackageGrants } = await import("@/server/modules/commercial/public/package-grants");
  const request = { companyId: f.company, expectedRevision: packageGrantRevision(f.company, await readCompanyPackageGrants(db, f.company)),
    idempotencyKey: key(), changes: [{ grant: inactive.grant, expectedVersion: 2 }] };
  expect((await withOwnerPackageObligations(db, input(), tx => commercial.batch.previewIn(tx, request))).after[0].state).toBe("inactive");
  await withOwnerPackageObligations(db, input(), tx => commercial.batch.applyIn(tx, request, async review => { expect(review.after[0].state).toBe("inactive"); }));
  expect(await db.prepare("SELECT * FROM support_tickets WHERE id=?").bind(id).first()).toEqual(closed);
  expect((await db.prepare("SELECT * FROM support_ticket_events WHERE ticket_id=? ORDER BY version").bind(id).all()).results).toEqual(history);
  expect((await observe()).outstanding).toBe(0);
});

it("retains a mitigated incident until its independent close even after the source ticket is cancelled", async () => {
  const ticket = await orderTicket();
  const created = await createSupportIncident(f.manager, { ticketId: ticket.id, ticketVersion: 1, title: "Shared installation defect",
    impact: "Several customers may need a correction", severity: "high", ownerId: f.head.userId, dueDate: "2026-09-27" }, key()), id = String(created.id);
  await ticketAction(ticket.id, "cancel", f.manager);
  expect((await observe()).outstanding).toBe(1);
  await changeSupportIncident(f.head, id, { action: "start", expectedVersion: 1, reason: "Investigate the shared source" }, key());
  await changeSupportIncident(f.head, id, { action: "mitigate", expectedVersion: 2, reason: "Verified the correction", rootCause: "Shared configuration defect", mitigation: "Applied and tested a corrected configuration" }, key());
  expect((await observe()).outstanding).toBe(1);
  await changeSupportIncident(f.manager, id, { action: "close", expectedVersion: 3, reason: "Independent review confirms the mitigation" }, key());
  expect((await observe()).outstanding).toBe(0);
  expect(await getSupportIncident(f.manager, id)).toMatchObject({ status: "closed", version: 4 });
});

it("retains live temporary access after ticket cancellation, releases it on owner revoke or exact expiry and isolates Company revisions", async () => {
  const ticket = await orderTicket(), expiresAt = Date.now() + 3_600_000;
  const session = await requestSupportSession(f.head, { ticketId: ticket.id, ticketVersion: 1, requestedFor: f.head.userId, expiresAt, reason: "Time limited troubleshooting context" }, key()), id = String(session.id);
  expect((await observe()).outstanding).toBe(2);
  await changeSupportSession(f.manager, id, { action: "approve", expectedVersion: 1, reason: "Independently reviewed minimal access" }, key());
  await changeSupportSession(f.manager, id, { action: "issue-capability", expectedVersion: 2, code: "customer_order_context", reason: "Order context is required" }, key());
  await ticketAction(ticket.id, "cancel", f.manager);
  const current = await observe(); expect(current.outstanding).toBe(1);
  const other = await withOwnerPackageObligations(db, { ...input(), companyId: "other-company" }, tx => supportPackageObligations(db, tx, { ...input(), companyId: "other-company" }));
  expect(other.outstanding).toBe(0); expect((await observe()).revision).toBe(current.revision);
  const expired = await withOwnerPackageObligations(db, input(), tx => supportPackageObligations(db, tx, { ...input(), now: expiresAt }));
  expect(expired.outstanding).toBe(0); expect(expired.revision).not.toBe(current.revision);
  await changeSupportSession(f.manager, id, { action: "revoke", expectedVersion: 3, reason: "The original ticket is closed" }, key());
  expect((await observe()).outstanding).toBe(0);
  expect(await db.prepare("SELECT status FROM support_capabilities WHERE session_id=?").bind(id).first()).toEqual({ status: "revoked" });
});

it("detects an undrained SLA breach after ticket closure, then requires the normal manager acknowledgement", async () => {
  const ticket = await orderTicket();
  const policy = await createSupportPolicy(f.owner, { name: "Reviewed Support target", policy: { calendar: { timezone: "UTC", weekdays: [1,2,3,4,5], opensAtMinute: 0, closesAtMinute: 1440, holidays: [] }, targetMinutes: 1, pauseWhileWaiting: true }, reason: "Record an independently reviewed target" }, key());
  await changeSupportPolicy(f.manager, String(policy.id), { action: "approve", expectedVersion: 1, reason: "Approve the working calendar independently" }, key());
  const now = vi.spyOn(Date, "now").mockReturnValue(Date.parse("2026-09-21T02:00:00Z"));
  await attachSupportClock(f.owner, ticket.id, { expectedVersion: 1, policyId: policy.id, policyVersion: 2, reason: "Start the reviewed resolution clock" }, key());
  await ticketAction(ticket.id, "start");
  now.mockReturnValue(Date.parse("2026-09-21T02:02:00Z"));
  await ticketAction(ticket.id, "resolve"); await confirm(ticket.id, ticket.orderId);
  expect(await supportAlerts(f.manager)).toEqual([]);
  const undrained = await observe(); expect(undrained.outstanding).toBe(1);
  await drainSupportSla(db);
  const alert = (await supportAlerts(f.manager))[0]; expect(alert.ticket_id).toBe(ticket.id);
  expect((await observe()).outstanding).toBe(1);
  await acknowledgeSupportAlert(f.manager, alert.id, { expectedVersion: 1, reason: "Manager owns the completed breach follow-up" }, key());
  expect((await observe()).outstanding).toBe(0);
  await drainSupportSla(db); expect((await observe()).outstanding).toBe(0);
});

it("refuses pools, foreign/closed units, missing owners and unimplemented package scopes without a zero fallback", async () => {
  await expect(supportPackageObligations(db, db, input())).rejects.toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED");
  const other = new PostgresDatabase({ ...db.pool.options, max: 1 });
  try { await expect(other.transaction(tx => supportPackageObligations(db, tx, input()))).rejects.toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED"); }
  finally { await other.close(); }
  let escaped!: SqlDatabase;
  await withOwnerPackageObligations(db, input(), async tx => { escaped = tx; await supportPackageObligations(db, tx, input()); });
  await expect(supportPackageObligations(db, escaped, input())).rejects.toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED");
  for (const mismatch of [{ moduleId: "finance" }, { packageId: "support-undeclared-fixture" }, { packageId: "service" }, { packageVersion: 2 }])
    await expect(withOwnerPackageObligations(db, input(), tx => supportPackageObligations(db, tx, { ...input(), ...mismatch }))).rejects.toThrow("SUPPORT_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  // A missing reader still fails closed. Finance now has a concrete registered
  // reader, so omit that reader explicitly instead of assuming its absence.
  await expect(withOwnerPackageObligations(db, input(), tx => readPackageObligations(tx, { ...input(), packageId: "finance" }, {}))).rejects.toThrow("PACKAGE_OBLIGATION_READER_REQUIRED:finance");
  // A Support-only lease cannot stand in for the real Finance source fences.
  await expect(withOwnerPackageObligations(db, input(), tx => readOwnerPackageObligations(db, tx, { ...input(), packageId: "finance" }))).rejects.toThrow("FINANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
});

it("locks every obligation source including absent rows until the normal Commercial transaction ends", async () => {
  await withOwnerPackageObligations(db, input(), async tx => {
    expect((await supportPackageObligations(db, tx, input())).outstanding).toBe(0);
    for (const [table, column] of [["support_tickets", "version"], ["support_incidents", "version"], ["support_sessions", "version"], ["support_capabilities", "version"], ["support_sla_alerts", "version"]]) {
      await expect(db.transaction(async competing => {
        await competing.prepare("SET LOCAL lock_timeout='100ms'").run();
        await competing.prepare(`UPDATE ${table} SET ${column}=${column} WHERE false`).run();
      })).rejects.toMatchObject({ code: "55P03" });
    }
  });
});

it("refuses a pre-existing snapshot after a committed owner report instead of accepting its old zero", async () => {
  let ready!: () => void, resume!: () => void;
  const snapshot = new Promise<void>(resolve => { ready = resolve; }), continueRead = new Promise<void>(resolve => { resume = resolve; });
  const pending = db.transaction(async tx => {
    expect(await tx.prepare("SELECT count(*) total FROM support_tickets WHERE company_id=?").bind(f.company).first()).toEqual({ total: 0 });
    ready(); await continueRead;
    await expect(supportPackageObligations(db, tx, input())).rejects.toThrow("SUPPORT_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
  });
  await snapshot;
  try { await createSupportTicket(f.claimant, { kind: "general", title: "New pending printer report", issue: "Printer stopped during the commercial review." }, key()); }
  finally { resume(); }
  await pending;
  expect((await observe()).outstanding).toBe(1);
});


it("keeps declared Support Portal obligations independently fenced from pending original general Support work", async () => {
  const portalInput = { ...input(), packageId: "support-portal" };
  const readPortal = () => withOwnerPackageObligations(db, portalInput,
    tx => readOwnerPackageObligations(db, tx, portalInput));
  const before = await readPortal();
  expect(before.total).toBe(0);
  expect(before.observations).toHaveLength(1);
  expect(before.observations[0]).toMatchObject({ companyId: f.company, packageId: "support-portal", moduleId: "support", outstanding: 0 });
  const report = await createSupportTicket(f.claimant, { kind: "general", title: "Original general Support work remains pending",
    issue: "This unresolved printer report belongs to Helpdesk rather than the Portal add-on." }, key());
  expect((await observe()).outstanding).toBe(1);
  await withOwnerPackageObligations(db, portalInput, async tx => {
    const portal = await readOwnerPackageObligations(db, tx, portalInput);
    expect(portal.total).toBe(0);
    expect(portal.observations).toEqual(before.observations);
    // Even an empty Portal needs real absent-row fences, not a constant zero.
    // These competing zero-row writes create no business or authority facts.
    for (const [table, column] of [["support_ticket_portal_bindings", "version"], ["support_ticket_portal_responses", "response_version"]]) {
      await expect(db.transaction(async competing => {
        await competing.prepare("SET LOCAL lock_timeout='100ms'").run();
        await competing.prepare(`UPDATE ${table} SET ${column}=${column} WHERE false`).run();
      })).rejects.toMatchObject({ code: "55P03" });
    }
  });
  expect((await getSupportTicket(f.manager, String(report.id))).status).not.toMatch(/^(closed|cancelled)$/);
  expect((await observe()).outstanding).toBe(1);
  expect((await readPortal()).observations).toEqual(before.observations);
  expect(await db.prepare("SELECT count(*) n FROM company_package_grants WHERE company_id=?").bind(f.company).first("n")).toBe(0);
});
