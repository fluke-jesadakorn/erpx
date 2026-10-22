import { crmDetail, createCRMLead, getCRMLead } from "@erpx/server-crm/public/crm";
import { createHash } from "node:crypto";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { changeCRMIdentity } from "@erpx/server-crm/public/crm-identities";
import { readExecutiveMetricProjection } from "@erpx/server-support/public/executive-metric-projections";
import { executiveMainEvidenceSources as supportMetricEvidence } from "@erpx/server-support/public/executive-main-evidence-sources";
import { metricCatalogFor, queryMetric } from "@erpx/server-runtime/public/executive/metrics";
import { createAsset } from "@erpx/server-assets/public/asset-maintenance";
import { pmsCommand, loadPms } from "@erpx/server-pms/public/pms";
import { readablePmsSource, readablePmsSourceIds } from "@erpx/server-pms/public/pms/source";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { createSupportTicket, changeSupportTicket, supportWorkspace, supportDetail, getSupportTicket, supportWork } from "@erpx/server-support/public/support";
import { supportEvidence } from "@erpx/server-support/public/support-evidence";
import { incidentWorkspace } from "@erpx/server-support/public/support-incidents";
import { createSupportIncident } from "@erpx/server-support/public/support-incidents";
import { supportSlaWorkspace } from "@erpx/server-support/public/support-sla";
import { supportSessionWorkspace } from "@erpx/server-support/public/support-sessions";
import { packageGrantStore } from "@erpx/server-commercial/adapters/package-grants";
import { readCompanyPackageGrants, packageGrantRevision } from "@erpx/server-commercial/public/package-grants";
import { ownerPackageObligations, withOwnerPackageObligations } from "@erpx/server-commercial/public/owner-package-obligations";
import type { CompanyPackageGrant } from "@erpx/server-commercial/public/packages";
import { attachmentPolicy, authorizeAttachmentTarget } from "@erpx/server-files/public/attachments/policy";
import { createAttachmentSet } from "@erpx/server-files/public/attachments/service";
import { fieldServiceSourceTickets } from "@erpx/server-support/public/field-service-sources";
import { fieldServiceWorkspace } from "@erpx/server-field_service/public/field-service";
import { saveMaster, saveOrder, orderSchema } from "@erpx/server-commerce/public/commerce";
import { requestSupportSession, changeSupportSession } from "@erpx/server-support/public/support-sessions";
import { createSupportPolicy, changeSupportPolicy, attachSupportClock } from "@erpx/server-support/public/support-sla";
import { drainSupportSla } from "@erpx/server-support/public/support-sla-alerts";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { POST as reserveFile } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/uploads/route";
import { PUT as uploadFile } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/uploads/[id]/content/route";
import { GET as downloadFile } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/attachments/[id]/content/route";
import { holdingResources } from "@erpx/server-runtime/public/runtime";
import { salesCustomersAccess, customerSummaries, customerWorkspaceProfile } from "@erpx/server-sales/public/sales-customers";
import { calendarSupportSupportFacts } from "@erpx/server-support/public/calendar-support-facts";
import { executiveMainEvidenceSources } from "@erpx/server-customer_360/public/executive-main-evidence-sources";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: vi.fn(async () => ({ write: env.H001_DB, files: env.H001_FILES })), runtimeEnv: async () => env }));
vi.mock("@erpx/server-files/public/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({ enabled: true, ready: true, engine: "Declared native fixture", definitionUpdatedAt: Date.now(), error: null }),
  scanAttachmentBytes: async () => ({ clean: true, signature: null, engine: "Declared native fixture", definitionUpdatedAt: Date.now() }),
}));
vi.mock("@erpx/server-files/public/attachments/preview", () => ({
  attachmentPreviewReadiness: async () => ({ ready: true, heifConvert: "/fixture/heif-convert", pdftoppm: "/fixture/pdftoppm", error: null }),
  detectedAttachmentMediaType: (_bytes: Uint8Array, declared: string) => declared,
  sanitizedAttachmentPreview: async () => null,
}));
const session = vi.hoisted(() => ({ actor: null as WorkContext | null }));
vi.mock("@erpx/server-identity_access/public/authorization", async original => ({ ...await original<typeof import("@erpx/server-identity_access/public/authorization")>(), requireWorkContext: async () => session.actor! }));
let f: Awaited<ReturnType<typeof expenseFixture>>, ticketId: string;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  vi.stubEnv("ERPX_ATTACHMENTS_V2", "true");
  for (const [actor, role] of [[f.head, "support_agent"], [f.manager, "support_manager"]] as const) {
    actor.role = role; actor.branchId = null; actor.departmentId = null; actor.assignmentId = await f.grant(actor, role);
  }
  session.actor = f.head;
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled");
  ticketId = String((await createSupportTicket(f.claimant, { kind: "general", title: "Private office report", issue: "The shared printer stopped responding." }, key())).id);
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
});
afterEach(() => { vi.unstubAllEnvs(); vi.restoreAllMocks(); });
const store = () => packageGrantStore(db, { authorize: async () => ({ userId: "isolated-provider-fixture" }),
  obligations: (tx, companyId, packageId) => ownerPackageObligations(db, tx, companyId, packageId) });
async function grant(state: CompanyPackageGrant["state"], packageId = "support", endsAt: number | null = null) {
  const current = (await readCompanyPackageGrants(db, f.company)).find(row => row.packageId === packageId);
  const value: CompanyPackageGrant = { companyId: f.company, packageId, packageVersion: 1, state, startsAt: 0, endsAt,
    version: (current?.version ?? 0) + 1, reason: "Native surface parity review", evidence: "test:normal-commercial-apply" };
  if (state !== "inactive") return store().apply({ grant: value, expectedVersion: current?.version ?? 0, idempotencyKey: key() });
  const request = { companyId: f.company, expectedRevision: packageGrantRevision(f.company, await readCompanyPackageGrants(db, f.company)),
    idempotencyKey: key(), changes: [{ grant: value, expectedVersion: current!.version }] };
  return withOwnerPackageObligations(db, { companyId: f.company, packageId, packageVersion: 1 }, tx => store().batch.applyIn(tx, request, async () => {}));
}
/** Deliberately unsafe pre-repair storage shape in an isolated fixture. Normal
 * Commercial now rejects this move with live work. This proves containment by
 * an already queued Files command, not approval of an older provider writer. */
async function preSettlementReviewWriter(transaction: SqlDatabase, current: CompanyPackageGrant) {
  const next={...current,startsAt:Date.now()+60000,version:current.version+1},id=key(),instant=Date.now();
  const request={...next,expectedVersion:current.version};
  await transaction.prepare(`INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,package_id) DO UPDATE SET package_version=excluded.package_version,state=excluded.state,
    starts_at=excluded.starts_at,ends_at=excluded.ends_at,version=excluded.version,reason=excluded.reason,evidence=excluded.evidence,updated_at=excluded.updated_at`)
    .bind(next.companyId,next.packageId,next.packageVersion,next.state,next.startsAt,next.endsAt,next.version,next.reason,next.evidence,instant).run();
  await transaction.prepare(`INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES(?,?,?,?,?,?,?,?)`)
    .bind(id,next.companyId,next.packageId,'declared-pre-settlement-review-writer',createHash('sha256').update(JSON.stringify(request)).digest('hex'),JSON.stringify(current),JSON.stringify(next),instant).run();
}
async function triage() {
  return changeSupportTicket(f.manager, ticketId, { action: "triage", expectedVersion: 1, ownerId: f.head.userId, dueDate: "2026-10-20", reason: "Assign the existing report" }, key());
}
const fileInput = () => ({ targetType: "support_ticket", targetId: ticketId, componentKind: "action" as const,
  componentRef: "support-evidence:support_evidence", purposeKey: "support_evidence", expectedTargetVersion: 2 });
const policy = { targetMinutes: 60, pauseWhileWaiting: true,
  calendar: { timezone: "Asia/Bangkok", weekdays: [1, 2, 3, 4, 5], opensAtMinute: 540, closesAtMinute: 1020, holidays: [] } };
async function orderTicket() {
  const customer = await saveMaster(f.owner, "parties", null, { kind: "customer", code: key(), name: "Fictional service customer" }, key());
  const warehouse = await saveMaster(f.owner, "warehouses", null, { branchId: f.branch, code: key(), name: "Service source warehouse" }, key());
  const product = await saveMaster(f.owner, "products", null, { sku: key(), name: "Service source item", price: "10" }, key());
  const order = await saveOrder(f.owner, null, orderSchema.parse({ kind: "quote", partyId: customer.id, warehouseId: warehouse.id,
    orderDate: "2026-10-15", dueDate: "2026-10-20", lines: [{ productId: product.id, quantity: "1", price: "10", taxBps: 0 }] }), key());
  return createSupportTicket(f.owner, { orderId: order.id, sourceVersion: 1, title: "Private source ticket", issue: "Installation details are private.", ownerId: f.head.userId, dueDate: "2026-10-20" }, key());
}

it("denies real Support collections, detail and evidence without a purchase", async () => {
  for (const read of [() => supportWorkspace(f.manager), () => getSupportTicket(f.manager, ticketId), () => supportEvidence(f.manager, ticketId),
    () => incidentWorkspace(f.manager), () => supportSlaWorkspace(f.manager), () => supportSessionWorkspace(f.manager)])
    await expect(read()).rejects.toMatchObject({ status: 403, code: "PACKAGE_REQUIRED" });
});

it("omits unpurchased Support from the real Work reader instead of exposing its triage work", async () => {
  expect(await supportWork(f.manager)).toEqual([]);
});

it("retains real ticket history after a started purchase is replaced by a future renewal", async () => {
  await grant("active"); await triage();
  await changeSupportTicket(f.manager, ticketId, { action: "cancel", expectedVersion: 2, reason: "Retain the completed Company record" }, key());
  await grant("draining"); await grant("inactive");
  const current = (await readCompanyPackageGrants(db, f.company))[0];
  await store().apply({ grant: { ...current, state: "active", startsAt: Date.now() + 60000, version: current.version + 1 }, expectedVersion: current.version, idempotencyKey: key() });
  expect(await supportDetail(f.manager, ticketId)).toMatchObject({ row: { status: "cancelled" }, canReopen: false, canManage: false });
});

it("hides the customer Service tab and counts without purchase and removes new-case sources while draining", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled");
  const ticket = await orderTicket(), row = await db.prepare("SELECT customer_id FROM support_tickets WHERE id=?").bind(ticket.id).first<{ customer_id: string }>();
  const customerId = row!.customer_id;
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
  expect((await salesCustomersAccess(f.owner)).support).toBe(false);
  expect((await customerSummaries(f.owner, [customerId])).get(customerId)?.openSupport).toBeNull();
  const unpurchased = await customerWorkspaceProfile(f.owner, customerId);
  expect(unpurchased).toMatchObject({ cases: null, supportSource: null });
  expect(unpurchased.timeline?.items.filter(item => item.source_type === "support")).toEqual([]);
  await grant("active");
  expect((await salesCustomersAccess(f.owner)).support).toBe(true);
  expect((await customerSummaries(f.owner, [customerId])).get(customerId)?.openSupport).toBe(1);
  const purchased = await customerWorkspaceProfile(f.owner, customerId);
  expect(purchased.supportSource).not.toBeNull();
  expect(purchased.timeline?.items.filter(item => item.source_type === "support").map(item => item.source_id)).toContain(ticket.id);
  await grant("draining");
  expect((await salesCustomersAccess(f.owner)).support).toBe(true);
  expect(await customerWorkspaceProfile(f.owner, customerId)).toMatchObject({ supportSource: null });
  expect((await customerSummaries(f.owner, [customerId])).get(customerId)?.openSupport).toBe(1);
});

it("filters unpurchased Support Calendar sources while retaining draining owner work", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled"); await triage();
  const source = await orderTicket(), incident = await createSupportIncident(f.manager, { ticketId: source.id, ticketVersion: 1,
    title: "Calendar fixture incident", impact: "Scoped service work", severity: "normal", ownerId: f.head.userId, dueDate: "2026-10-20" }, key());
  const calendar = calendarSupportSupportFacts(db), read = () => calendar.read1({ value1: "Asia/Bangkok", value2: f.company,
    value3: null, value4: null, value5: JSON.stringify([f.company, f.branch, f.department]), value6: "2026-10-01", value7: "2026-11-01",
    value8: "team", value9: f.manager.userId, value10: null, value11: "", value12: 100, actor: f.manager });
  const incidents = () => calendar.read2({ value1: f.company, value2: null, value3: null, value4: JSON.stringify([f.company, f.branch, f.department]),
    value5: "2026-10-01", value6: "2026-11-01", value7: "team", value8: f.manager.userId, value9: null, value10: "", value11: 100, actor: f.manager });
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
  expect(await read()).toEqual([]); expect(await incidents()).toEqual([]);
  await grant("active"); expect((await read()).map(row => row.id)).toContain(ticketId);
  expect((await incidents()).map(row => row.id)).toContain(incident.id);
  await grant("draining"); expect((await read()).map(row => row.id)).toContain(ticketId);
  expect((await incidents()).map(row => row.id)).toContain(incident.id);
});

it("applies purchase and current owner authority to actual Executive customer evidence without a second connection", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled"); const ticket = await orderTicket();
  const event = await db.prepare("SELECT id FROM support_ticket_events WHERE ticket_id=? ORDER BY version LIMIT 1").bind(ticket.id).first<{ id: string }>();
  const filters = { from: "2026-10-01", to: "2026-11-01", currency: "THB" }, recordId = `support:${event!.id}`;
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
  expect((await executiveMainEvidenceSources(db).factsReadCustomerActivityEvidence(f.owner, recordId, filters)).row).toBeNull();
  expect((await executiveMainEvidenceSources(db).factsSearchCustomerActivityEvidence(f.owner, { filters, query: "Private source ticket", limit: 100, metric: "recent-customer-activity" }, null)).rows).toEqual([]);
  await grant("active"); vi.mocked(holdingResources).mockClear();
  await db.transaction(async tx => {
    expect((await executiveMainEvidenceSources(tx).factsReadCustomerActivityEvidence(f.owner, recordId, filters)).row).toMatchObject({ source_id: ticket.id, source_type: "support" });
    expect((await executiveMainEvidenceSources(tx).factsReadCustomerActivityEvidence({ ...f.owner, assignmentId: "stale-fixture-assignment" }, recordId, filters)).row).toBeNull();
  });
  expect(holdingResources).not.toHaveBeenCalled();
});

it("intersects real workspace/detail controls with draining, expiry and retained inactive history", async () => {
  await grant("active"); await triage();
  expect((await supportWorkspace(f.manager)).items.map(row => row.id)).toContain(ticketId);
  expect((await supportDetail(f.manager, ticketId)).canManage).toBe(true);
  await grant("draining");
  expect(await supportWorkspace(f.manager)).toMatchObject({ canCreate: false, canCreateOrder: false, sources: [] });
  expect(await supportDetail(f.manager, ticketId)).toMatchObject({ canManage: true, canReopen: false });
  expect(await supportSlaWorkspace(f.manager)).toMatchObject({ canCreate: false, canManage: true });
  expect(await supportEvidence(f.manager, ticketId)).toMatchObject({ canChange: true });
  await grant("active", "support", Date.now() - 1);
  expect(await supportDetail(f.manager, ticketId)).toMatchObject({ canManage: true, canReopen: false });
  await changeSupportTicket(f.manager, ticketId, { action: "cancel", expectedVersion: 2, reason: "Close the existing report" }, key());
  await grant("draining"); await grant("inactive");
  expect(await supportDetail(f.manager, ticketId)).toMatchObject({ canManage: false, canReopen: false, canWork: false });
  expect((await supportWorkspace(f.manager)).items).toHaveLength(1);
  expect(await getSupportTicket(f.manager, ticketId)).toMatchObject({ status: "cancelled" });
});

it("denies generic file reads and creation without purchase, allows draining settlement and retains inactive reads", async () => {
  const filePolicy = attachmentPolicy("support_ticket", "support_evidence", "action");
  await expect(authorizeAttachmentTarget(db, f.manager, filePolicy, ticketId, "read")).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" });
  await grant("active"); await triage();
  const created = await createAttachmentSet(db, f.manager, fileInput(), key());
  expect(created).toBeTruthy();
  await grant("draining");
  expect(await authorizeAttachmentTarget(db, f.manager, filePolicy, ticketId, "write")).toMatchObject({ editable: true });
  await changeSupportTicket(f.manager, ticketId, { action: "cancel", expectedVersion: 2, reason: "Freeze the retained file history" }, key());
  await grant("inactive");
  expect(await authorizeAttachmentTarget(db, f.manager, filePolicy, ticketId, "read")).toMatchObject({ editable: false });
  await expect(createAttachmentSet(db, f.manager, { ...fileInput(), expectedTargetVersion: 3 }, key())).rejects.toMatchObject({ status: 409, code: "SUPPORT_EVIDENCE_LOCKED" });
});

it("rechecks a queued generic file write after an explicitly unsafe pre-repair writer changes the period", async () => {
  await grant("active"); await triage();
  let pending: Promise<unknown> | undefined;
  await db.transaction(async tx => {
    await tx.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`commercial.package:company:${f.company}`).run();
    pending = createAttachmentSet(db, f.manager, fileInput(), key()).then(value => ({ value }), error => ({ error }));
    let queued = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      queued = Boolean(await db.prepare("SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database())").first());
      if (queued) break;
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    expect(queued).toBe(true);
    const grants = await readCompanyPackageGrants(tx, f.company), current = grants[0];
    await preSettlementReviewWriter(tx,current);
  });
  expect(await pending).toMatchObject({ error: { status: 409, code: "PACKAGE_CAPABILITY_CHANGED" } });
  expect(await db.prepare("SELECT count(*) total FROM attachment_sets WHERE target_id=?").bind(ticketId).first()).toEqual({ total: 0 });
});
it("refuses a normal withdrawal with live work and lets the queued existing-evidence write finish under its preserved grant", async () => {
  await grant("active");await triage();let pending:Promise<unknown>|undefined;
  await withOwnerPackageObligations(db,{companyId:f.company,packageId:'support',packageVersion:1},async tx=>{
    pending=createAttachmentSet(db,f.manager,fileInput(),key()).then(value=>({value}),error=>({error}));
    let queued=false;
    for(let attempt=0;attempt<100;attempt++){
      queued=Boolean(await db.prepare("SELECT 1 FROM pg_locks WHERE NOT granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database())").first());
      if(queued)break;await new Promise(resolve=>setTimeout(resolve,10));
    }
    expect(queued).toBe(true);
    const grants=await readCompanyPackageGrants(tx,f.company),current=grants[0];
    await expect(store().batch.applyIn(tx,{companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,startsAt:Date.now()+60000,version:current.version+1}}]},async()=>{})).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
  });
  expect(await pending).toHaveProperty('value');expect(await pending).not.toHaveProperty('error');
  expect(await db.prepare("SELECT count(*) total FROM attachment_sets WHERE target_id=?").bind(ticketId).first()).toEqual({total:1});
  expect(await db.prepare("SELECT version,starts_at FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toEqual({version:1,starts_at:0});
});

it("keeps Service-only ticket handoff scoped without opening the Helpdesk management UI", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled"); const ticket = await orderTicket();
  f.owner.modules = ["field_service", "support"];
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced"); await grant("active", "service");
  const sources = await fieldServiceSourceTickets(db, f.owner, "new-work");
  expect(sources.map(row => row.id)).toContain(ticket.id);
  expect((await fieldServiceWorkspace(f.owner)).tickets.map(row => row.id)).toContain(ticket.id);
  expect(JSON.stringify(sources)).not.toMatch(/Installation details|issue|evidence|customer_name/);
  expect(await fieldServiceSourceTickets(db, { ...f.owner, branchId: "other-branch" }, "new-work")).toEqual([]);
  await expect(supportWorkspace(f.owner)).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" });
  await grant("draining", "service");
  expect(await fieldServiceSourceTickets(db, f.owner, "new-work")).toEqual([]);
  expect((await fieldServiceSourceTickets(db, f.owner, "settle-existing")).map(row => row.id)).toContain(ticket.id);
});

it("reads incident file authority through the supplied owner transaction without checking out another Holding connection", async () => {
  await grant("active"); const ticket = await orderTicket();
  const incident = await createSupportIncident(f.manager, { ticketId: ticket.id, ticketVersion: 1, title: "Shared fixture incident",
    impact: "Installation investigation", severity: "normal", ownerId: f.head.userId, dueDate: "2026-10-20" }, key());
  vi.mocked(holdingResources).mockClear();
  await db.transaction(async transaction => {
    expect(await authorizeAttachmentTarget(transaction, f.manager, attachmentPolicy("support_incident", "incident_evidence", "action"), String(incident.id), "read"))
      .toMatchObject({ companyId: f.company, editable: false });
  });
  expect(holdingResources).not.toHaveBeenCalled();
});

it("keeps rejection/revocation while draining, hides access expansion and expires pending sessions in Work", async () => {
  await grant("active"); const ticket = await orderTicket();
  const input = { ticketId: ticket.id, ticketVersion: 1, requestedFor: f.head.userId, expiresAt: Date.now() + 60000, reason: "Independent access review" };
  const approved = await requestSupportSession(f.head, input, key()), pending = await requestSupportSession(f.head, input, key());
  await changeSupportSession(f.manager, String(approved.id), { action: "approve", expectedVersion: 1, reason: "Current manager review" }, key());
  await grant("draining");
  const data = await supportSessionWorkspace(f.manager);
  expect(data.tickets).toEqual([]);
  expect(data.rows.find(row => row.id === pending.id)).toMatchObject({ canReview: true, canApprove: false });
  expect(data.rows.find(row => row.id === approved.id)).toMatchObject({ canManage: true, canIssueCapability: false });
  vi.spyOn(Date, "now").mockReturnValue(input.expiresAt + 1);
  const expired = await supportSessionWorkspace(f.manager);
  expect(expired.rows.find(row => row.id === pending.id)).toMatchObject({ effective_status: "expired", canReview: false, canApprove: false });
});

it("gates actual SLA sweep and notifications while preserving draining and expired settlement", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT","disabled");await triage();
  const p = await createSupportPolicy(f.manager, { name: "Native escalation target", policy, reason: "Scoped SLA parity" }, key());
  await changeSupportPolicy(f.owner, String(p.id), { action: "approve", expectedVersion: 1, reason: "Independent Company policy review" }, key());
  await attachSupportClock(f.manager, ticketId, { expectedVersion: 2, policyId: p.id, policyVersion: 2, reason: "Attach the current target" }, key());
  const row = await getSupportTicket(f.manager, ticketId), clock = JSON.parse(String(row.sla_clock_json));
  // A first future purchase gates a legacy report; it never withdraws an
  // already effective period while the report still needs settlement.
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT","enforced");
  expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);
  await store().apply({grant:{companyId:f.company,packageId:'support',packageVersion:1,state:'active',startsAt:Date.now()+60000,endsAt:null,version:1,reason:'First future native purchase',evidence:'test:sla-unstarted-purchase'},expectedVersion:0,idempotencyKey:key()});
  await drainSupportSla(db, Number(clock.dueAt) + 1);
  expect(await db.prepare("SELECT count(*) total FROM support_sla_alerts WHERE ticket_id=?").bind(ticketId).first()).toEqual({ total: 0 });
  await grant("draining"); await drainSupportSla(db, Number(clock.dueAt) + 1);
  expect(await db.prepare("SELECT count(*) total FROM support_sla_alerts WHERE ticket_id=?").bind(ticketId).first()).toEqual({ total: 1 });
  const notices = await db.prepare("SELECT count(*) total FROM staff_notifications WHERE record_type='support_sla_alert' AND company_id=?").bind(f.company).first();
  expect(Number(notices?.total)).toBeGreaterThan(0);
  await grant("active", "support", Date.now() - 1); await drainSupportSla(db, Number(clock.dueAt) + 1);
  expect(await db.prepare("SELECT count(*) total FROM staff_notifications WHERE record_type='support_sla_alert' AND company_id=?").bind(f.company).first()).toEqual(notices);
});

it("protects actual private-file HTTP bytes without purchase and retains held history across draining, inactivation and future renewal", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled"); await triage();
  const bytes = new TextEncoder().encode("%PDF-1.4\nPrivate fictional Helpdesk bytes\n%%EOF"), params = { params: Promise.resolve({ holdingSlug: "test" }) };
  const reserved = await reserveFile(new Request("http://localhost:3100/api/v1/holdings/test/uploads", { method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": key() }, body: JSON.stringify({ ownerType: "support_ticket", ownerId: ticketId,
      expectedVersion: 2, purpose: "support_evidence", filename: "fixture.pdf", mediaType: "application/pdf", bytes: bytes.length }) }), params);
  expect(reserved.status).toBe(201);
  const reservation = (await reserved.json()).data;
  const uploaded = await uploadFile(new Request(`http://localhost:3100/api/v1/holdings/test/uploads/${reservation.id}/content`, { method: "PUT",
    headers: { "Content-Type": "application/pdf", "Content-Length": String(bytes.length) }, body: bytes }), { params: Promise.resolve({ holdingSlug: "test", id: reservation.id }) });
  expect(uploaded.status).toBe(201);
  const sourceId = (await uploaded.json()).data.id;
  // Existing held evidence is fixture state, not an entitlement mutation.
  await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(sourceId).run();
  const read = () => downloadFile(new Request(`http://localhost:3100/api/v1/holdings/test/attachments/${sourceId}/content`), { params: Promise.resolve({ holdingSlug: "test", id: sourceId }) });
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
  const denied = await read(); expect(denied.status).toBe(403); expect(await denied.text()).not.toContain("Private fictional Helpdesk bytes");
  await grant("active"); expect(await (await read()).text()).toBe(new TextDecoder().decode(bytes));
  await grant("draining"); expect((await read()).status).toBe(200);
  const row = await getSupportTicket(f.manager, ticketId);
  await changeSupportTicket(f.manager, ticketId, { action: "cancel", expectedVersion: Number(row.version), reason: "Retain immutable closed evidence" }, key());
  await grant("inactive"); expect((await read()).status).toBe(200);
  const facts = await db.prepare("SELECT checksum_sha256,revoked_at,legal_hold FROM attachments WHERE id=?").bind(sourceId).first();
  expect(facts).toMatchObject({ revoked_at: null, legal_hold: 1 });
  const current = (await readCompanyPackageGrants(db, f.company))[0];
  await store().apply({ grant: { ...current, state: "active", startsAt: Date.now() + 60000, version: current.version + 1 }, expectedVersion: current.version, idempotencyKey: key() });
  const renewedHistory = await read(); expect(renewedHistory.status).toBe(200); expect(await renewedHistory.text()).toBe(new TextDecoder().decode(bytes));
  expect(await db.prepare("SELECT checksum_sha256,revoked_at,legal_hold FROM attachments WHERE id=?").bind(sourceId).first()).toEqual(facts);
});

it("does not expose actual legacy private-file bytes through a first unstarted future purchase", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled"); await triage();
  const bytes = new TextEncoder().encode("%PDF-1.4\nUnpurchased fictional evidence\n%%EOF");
  const reserved = await reserveFile(new Request("http://localhost:3100/api/v1/holdings/test/uploads", { method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": key() }, body: JSON.stringify({ ownerType: "support_ticket", ownerId: ticketId,
      expectedVersion: 2, purpose: "support_evidence", filename: "future-fixture.pdf", mediaType: "application/pdf", bytes: bytes.length }) }), { params: Promise.resolve({ holdingSlug: "test" }) });
  expect(reserved.status).toBe(201); const intent = (await reserved.json()).data;
  const uploaded = await uploadFile(new Request(`http://localhost:3100/api/v1/holdings/test/uploads/${intent.id}/content`, { method: "PUT",
    headers: { "Content-Type": "application/pdf", "Content-Length": String(bytes.length) }, body: bytes }), { params: Promise.resolve({ holdingSlug: "test", id: intent.id }) });
  expect(uploaded.status).toBe(201); const id = (await uploaded.json()).data.id;
  const read = () => downloadFile(new Request(`http://localhost:3100/api/v1/holdings/test/attachments/${id}/content`), { params: Promise.resolve({ holdingSlug: "test", id }) });
  expect((await read()).status).toBe(200);
  await store().apply({ grant: { companyId: f.company, packageId: "support", packageVersion: 1, state: "active", startsAt: Date.now() + 60000,
    endsAt: null, version: 1, reason: "Future fixture purchase", evidence: "test:future-without-history" }, expectedVersion: 0, idempotencyKey: key() });
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
  const denied = await read(); expect(denied.status).toBe(403); expect(await denied.text()).not.toContain("Unpurchased fictional evidence");
});

it("keeps shadow UI/file behavior and observes an unpurchased SLA batch without exposing private facts", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "shadow");
  const log = vi.spyOn(console, "info").mockImplementation(() => {});
  await triage();
  expect((await supportWorkspace(f.claimant)).canCreate).toBe(true);
  expect(await createAttachmentSet(db, f.manager, fileInput(), key())).toBeTruthy();
  const p = await createSupportPolicy(f.manager, { name: "Shadow escalation target", policy, reason: "Observe existing behavior" }, key());
  await changeSupportPolicy(f.owner, String(p.id), { action: "approve", expectedVersion: 1, reason: "Independent shadow policy review" }, key());
  await attachSupportClock(f.manager, ticketId, { expectedVersion: 2, policyId: p.id, policyVersion: 2, reason: "Observe the existing target" }, key());
  const clock = JSON.parse(String((await getSupportTicket(f.manager, ticketId)).sla_clock_json));
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced"); await drainSupportSla(db, Number(clock.dueAt) + 1);
  expect(await db.prepare("SELECT count(*) total FROM support_sla_alerts WHERE ticket_id=?").bind(ticketId).first()).toEqual({ total: 0 });
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "shadow"); await drainSupportSla(db, Number(clock.dueAt) + 1);
  expect(await db.prepare("SELECT count(*) total FROM support_sla_alerts WHERE ticket_id=?").bind(ticketId).first()).toEqual({ total: 1 });
  const observations = log.mock.calls.map(([value]) => JSON.parse(String(value)));
  expect(observations.some(value => value.phase === "checked" && value.capability === "support.sla" && value.code === "PACKAGE_REQUIRED")).toBe(true);
  const keys = ["event", "phase", "companyId", "capability", "purpose", "code", "purchaseRevision"];
  for (const observation of observations) expect(Object.keys(observation).sort()).toEqual([...keys].sort());
  expect(JSON.stringify(observations)).not.toMatch(/Private office|printer|userId|"reason"|"evidence"|"grants"/);
});

const metricFilters = { from: "2026-10-01", to: "2026-10-31", currency: "THB" };
it("keeps CRM customer source cases behind purchase and current Support authority", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled");
  const ticket = await orderTicket(), customerId = String(await db.prepare("SELECT customer_id FROM support_tickets WHERE id=?").bind(ticket.id).first("customer_id"));
  const lead = await createCRMLead(f.owner, { branchId: f.branch, title: "Existing customer service inquiry", contactName: "Synthetic customer", contactDetail: "Declared fixture call", source: "Phone", brief: "Read the existing customer case", dueDate: "2026-10-20" }, key());
  const row = await getCRMLead(f.owner, lead.id), reviewer = await f.member("independent-crm-reviewer", "sales_manager");
  reviewer.branchId = null; reviewer.departmentId = null;
  await changeCRMIdentity(reviewer, { action: "link-finance", accountId: row.account_id, customerId, expectedVersion: 1, evidence: "Independent customer identity review" }, key());
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
  expect((await crmDetail(f.owner, lead.id)).sourceSupport).toEqual([]);
  await grant("active"); expect((await crmDetail(f.owner, lead.id)).sourceSupport.map(row => row.id)).toContain(ticket.id);
  await grant("draining"); expect((await crmDetail(f.owner, lead.id)).sourceSupport.map(row => row.id)).toContain(ticket.id);
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), f.owner.assignmentId).run();
  expect((await crmDetail(f.owner, lead.id)).sourceSupport).toEqual([]);
});

it("aligns real executive catalogue, metric rows, drilldown and evidence with purchase and current assignment", async () => {
  const read = () => readExecutiveMetricProjection(db, f.manager, "open-support-tickets", metricFilters, "status"), evidence = supportMetricEvidence(db);
  const search = () => evidence.factsSearchSupportTicketEvidence({ filters: metricFilters, query: "", limit: 30, metric: "open-support-tickets" }, null, false, f.manager);
  expect((await metricCatalogFor(db, f.manager)).find(row => row.id === "open-support-tickets")?.available).toBe(false);
  await expect(queryMetric(db, f.manager, "open-support-tickets", metricFilters, "status")).rejects.toMatchObject({ status: 403 });
  await expect(read()).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" }); expect((await search()).rows).toEqual([]);
  expect(await evidence.factsReadSupportTicketEvidence(false, "open-support-tickets", ticketId, f.manager)).toEqual({ row: null });
  await grant("active");
  expect((await metricCatalogFor(db, f.manager)).find(row => row.id === "open-support-tickets")?.available).toBe(true);
  expect(await queryMetric(db, f.manager, "open-support-tickets", metricFilters, "status")).toMatchObject({ total: "1" });
  const snapshot = await read(); expect(snapshot.raw).toEqual([{ label: "triage", value: "1" }]);
  expect((await snapshot.source({ label: "triage", page: 1 })).rows.map(row => row.id)).toEqual([ticketId]);
  expect((await search()).rows.map(row => row.id)).toEqual([ticketId]);
  await grant("draining"); expect((await read()).raw).toEqual(snapshot.raw);
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), f.manager.assignmentId).run();
  expect((await read()).raw).toEqual([]); expect((await search()).rows).toEqual([]);
  expect(await evidence.factsReadSupportTicketEvidence(false, "open-support-tickets", ticketId, f.manager)).toEqual({ row: null });
});

it("applies each Support metric capability and the incident record gate", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled");
  const source = await orderTicket(), incident = await createSupportIncident(f.manager, { ticketId: source.id, ticketVersion: 1, title: "Metric incident fixture", impact: "Declared synthetic service outage", severity: "normal", ownerId: f.head.userId, dueDate: "2026-10-20" }, key());
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
  const evidence = supportMetricEvidence(db), search = (actor = f.manager) => evidence.factsSearchSupportIncidentEvidence(null, { filters: metricFilters, query: "", limit: 30, metric: "active-support-incidents" }, actor);
  for (const id of ["overdue-support-sla", "active-support-incidents"] as const)
    await expect(readExecutiveMetricProjection(db, f.manager, id, metricFilters, "status")).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" });
  expect((await search()).rows).toEqual([]);
  await grant("active"); expect((await search()).rows.map(row => row.id)).toEqual([incident.id]);
  expect((await readExecutiveMetricProjection(db, f.manager, "active-support-incidents", metricFilters, "status")).raw).toEqual([{ label: "open", value: "1" }]);
  expect((await search({ ...f.manager, branchId: "different-branch" })).rows).toEqual([]);
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), f.manager.assignmentId).run();
  expect((await search()).rows).toEqual([]);
  expect((await readExecutiveMetricProjection(db, f.manager, "active-support-incidents", metricFilters, "status")).raw).toEqual([]);
});

it("removes source-linked asset tickets from metrics and evidence when the source workspace or current assignment is lost", async () => {
  f.owner.modules = ["support", "assets"];
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled");
  const asset = await createAsset(f.owner, { branchId: f.branch, departmentId: f.department, code: key(), name: "Private source fixture", category: "Display", currency: "THB", costMinor: 0, acquiredDate: "2026-10-01" }, key());
  const ticket = await createSupportTicket(f.owner, { kind: "source", source: { kind: "asset", id: asset.id, version: 1 }, title: "Asset source authority fixture", issue: "Do not disclose this source after access changes" }, key());
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced"); await grant("active");
  const read = () => readExecutiveMetricProjection(db, f.owner, "open-support-tickets", metricFilters, "status"), evidence = supportMetricEvidence(db);
  const search = () => evidence.factsSearchSupportTicketEvidence({ filters: metricFilters, query: "", limit: 30, metric: "open-support-tickets" }, null, false, f.owner);
  expect((await search()).rows.map(row => row.id)).toContain(ticket.id);
  expect((await read()).raw).toEqual([{ label: "triage", value: "2" }]);
  f.owner.modules = ["support"];
  expect((await search()).rows.map(row => row.id)).toEqual([ticketId]);
  expect((await read()).raw).toEqual([{ label: "triage", value: "1" }]);
  expect(await evidence.factsReadSupportTicketEvidence(false, "open-support-tickets", String(ticket.id), f.owner)).toEqual({ row: null });
  f.owner.modules = ["support", "assets"];
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), f.owner.assignmentId).run();
  expect((await read()).raw).toEqual([]); expect((await search()).rows).toEqual([]);
});

it("keeps PMS source-set projection equal to its normal reader across cached and current source permissions", async () => {
  f.owner.modules = ["pms", "support"];
  const property = await pmsCommand(db, f.owner, { operation: "property.create", input: { name: "Source policy fixture hotel", branchId: f.branch, businessDate: "2026-10-15" } }, key());
  const command = async (operation: string, input: unknown) => pmsCommand(db, f.owner, { propertyId: property.id, expectedVersion: (await loadPms(db, f.owner, property.id)).property.version, operation, input }, key());
  const type = await command("room_type.save", { name: "Source room", capacity: 2, amenities: "Desk" }), room = await command("room.save", { number: "101", typeId: type.id, active: true });
  const ids = async (actor: WorkContext) => { const projection = await readablePmsSourceIds(actor); return (await db.prepare(projection.sql).bind(...projection.bindings).all<{ id: string }>()).results.map(row => row.id); };
  expect(await readablePmsSource(db, f.owner, room.id)).toMatchObject({ id: room.id, kind: "room" });
  expect(await ids(f.owner)).toContain(room.id);
  vi.stubEnv("ERPX_PMS_PACKAGE_ENFORCEMENT", "enforced");
  await expect(readablePmsSource(db, f.owner, room.id)).rejects.toMatchObject({ status: 404 });expect(await ids(f.owner)).not.toContain(room.id);
  await grant("active", "pms");expect(await readablePmsSource(db, f.owner, room.id)).toMatchObject({ id: room.id });expect(await ids(f.owner)).toContain(room.id);
  await grant("draining", "pms");expect(await readablePmsSource(db, f.owner, room.id)).toMatchObject({ id: room.id });expect(await ids(f.owner)).toContain(room.id);
  const receptionist = await f.member("source-receptionist", "pms_frontdesk"); receptionist.modules = ["pms"]; receptionist.branchId = null; receptionist.departmentId = null;
  await expect(readablePmsSource(db, receptionist, room.id)).rejects.toMatchObject({ status: 404 }); expect(await ids(receptionist)).not.toContain(room.id);
  const narrowed = { ...f.owner, departmentId: f.department }; expect(await ids(narrowed)).toEqual([]);
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), f.owner.assignmentId).run();
  await expect(readablePmsSource(db, f.owner, room.id)).rejects.toMatchObject({ status: 404 }); expect(await ids(f.owner)).toEqual([]);
});
