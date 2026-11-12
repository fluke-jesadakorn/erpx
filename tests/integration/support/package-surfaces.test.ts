import {withNativeFieldFixture} from '../../support/files/field-files-native-parity';
import {seedExpenseCompany} from '../../../fixtures/expenses/company';
import type {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import { crmDetail, createCRMLead, getCRMLead } from "@erpx/server-crm/public/crm";
import { createHash } from "node:crypto";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { changeCRMIdentity } from "@erpx/server-crm/public/crm-identities";
import { readExecutiveMetricProjection } from "@erpx/server-support/public/executive-metric-projections";
import { executiveMainEvidenceSources as supportMetricEvidence } from "@erpx/server-support/public/executive-main-evidence-sources";
import { metricCatalogFor, queryMetric } from "@erpx/server-runtime/public/executive/metrics";
import { pmsCommand, loadPms } from "@erpx/server-pms/public/pms";
import { readablePmsSource, readablePmsSourceIds } from "@erpx/server-pms/public/pms/source";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import {supportWorkspace,supportDetail,getSupportTicket,supportWork} from '@erpx/server-support/public/support';
import {seedSupportTicket,seedSupportTicketState} from '../../support/helpers/support-ticket-seed';
import {seedSupportIncident} from '../../support/helpers/support-incident-seed';
import { supportEvidence } from "@erpx/server-support/public/support-evidence";
import {incidentWorkspace} from '@erpx/server-support/public/support-incidents';
import { supportSlaWorkspace } from "@erpx/server-support/public/support-sla";
import { supportSessionWorkspace } from "@erpx/server-support/public/support-sessions";
import { packageGrantStore } from "@erpx/server-commercial/adapters/package-grants";
import { readCompanyPackageGrants, packageGrantRevision } from "@erpx/server-commercial/public/package-grants";
import { ownerPackageObligations, withOwnerPackageObligations } from "@erpx/server-commercial/public/owner-package-obligations";
import type { CompanyPackageGrant } from "@erpx/server-commercial/public/packages";
import { attachmentPolicy, authorizeAttachmentTarget } from "@erpx/server-files/public/attachments/policy";
import { createAttachmentSet } from "@erpx/server-files/public/attachments/service";
import {saveMaster,saveOrder,orderSchema} from '@erpx/server-commerce/public/commerce';
import { seedSupportSession } from "../../support/helpers/support-session-seed";
import { startSupportClock } from "@erpx/server-support/public/rules/support-clock";
import { seedSupportClock, seedSupportPolicy } from "../../support/helpers/support-sla-seed";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { POST as reserveFile } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/uploads/route";
import { PUT as uploadFile } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/uploads/[id]/content/route";
import { GET as downloadFile } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/attachments/[id]/content/route";
import { holdingResources } from "@erpx/server-runtime/public/runtime";
import { customerTimeline } from "@erpx/server-customer_360/public/customer-timeline";
import { salesCustomersAccess, customerSummaries, customerWorkspaceProfile } from "@erpx/server-sales/public/sales-customers";
import { calendarSupportSupportFacts } from "@erpx/server-support/public/calendar-support-facts";
import { executiveMainEvidenceSources, type CustomerActivityTransport } from "@erpx/server-customer_360/public/executive-main-evidence-sources";

// Owner rows the native Assets register command persists; the TypeScript writer
// is retired and the native Rust suites own command behavior.
async function seedAsset(actor:{companyId:string;userId:string},input:{branchId:string;departmentId?:string|null;code:string;name:string;category:string;currency?:string;costMinor?:number;acquiredDate:string;warrantyEnd?:string|null}){
 const id=`asset-${key()}`,at=Date.now();
 await db.batch([
  db.prepare("INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,input.branchId,input.departmentId??null,input.code,input.name,input.category,input.currency??'THB',input.costMinor??0,input.acquiredDate,input.warrantyEnd??null,actor.userId,at,at),
  db.prepare("INSERT INTO operational_asset_events VALUES(?,?,1,'register',?,'Asset registered','good','registered',NULL,NULL,?)").bind(`asset-event-${key()}`,id,actor.userId,at),
 ]);
 return {id,version:1};
}
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
  ticketId = String((await seedSupportTicket(db, f.claimant, { title: "Private office report", issue: "The shared printer stopped responding." })).id);
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
  return seedSupportTicketState(db, ticketId, { action: "triage", status: "open", ownerId: f.head.userId, dueDate: "2026-10-20", actorId: f.manager.userId, reason: "Assign the existing report" });
}
const fileInput = () => ({ targetType: "support_ticket", targetId: ticketId, componentKind: "action" as const,
  componentRef: "support-evidence:support_evidence", purposeKey: "support_evidence", expectedTargetVersion: 2 });
const policy = { targetMinutes: 60, pauseWhileWaiting: true,
  calendar: { timezone: "Asia/Bangkok", weekdays: [1, 2, 3, 4, 5], opensAtMinute: 540, closesAtMinute: 1020, holidays: [] } };
const metricFilters = { from: "2026-10-01", to: "2026-10-31", currency: "THB" };
async function orderTicket() {
  const customer = await saveMaster(f.owner, "parties", null, { kind: "customer", code: key(), name: "Fictional service customer" }, key());
  const warehouse = await saveMaster(f.owner, "warehouses", null, { branchId: f.branch, code: key(), name: "Service source warehouse" }, key());
  const product = await saveMaster(f.owner, "products", null, { sku: key(), name: "Service source item", price: "10" }, key());
  const order = await saveOrder(f.owner, null, orderSchema.parse({ kind: "quote", partyId: customer.id, warehouseId: warehouse.id,
    orderDate: "2026-10-15", dueDate: "2026-10-20", lines: [{ productId: product.id, quantity: "1", price: "10", taxBps: 0 }] }), key());
  return seedSupportTicket(db, f.owner, { orderId: String(order.id), title: "Private source ticket", issue: "Installation details are private.", ownerId: f.head.userId, dueDate: "2026-10-20" });
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
  await seedSupportTicketState(db, ticketId, { action: "cancel", status: "cancelled", actorId: f.manager.userId, reason: "Retain the completed Company record" });
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
  expect((await customerTimeline(f.owner, customerId)).items.filter(item => item.source_type === "support")).toEqual([]);
  await grant("active");
  expect((await salesCustomersAccess(f.owner)).support).toBe(true);
  expect((await customerSummaries(f.owner, [customerId])).get(customerId)?.openSupport).toBe(1);
  const purchased = await customerWorkspaceProfile(f.owner, customerId);
  expect(purchased.supportSource).not.toBeNull();
  expect((await customerTimeline(f.owner, customerId)).items.filter(item => item.source_type === "support").map(item => item.source_id)).toContain(ticket.id);
  await grant("draining");
  expect((await salesCustomersAccess(f.owner)).support).toBe(true);
  expect(await customerWorkspaceProfile(f.owner, customerId)).toMatchObject({ supportSource: null });
  expect((await customerSummaries(f.owner, [customerId])).get(customerId)?.openSupport).toBe(1);
});

it("filters unpurchased Support Calendar sources while retaining draining owner work", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled"); await triage();
  const source = await orderTicket(), incident = await seedSupportIncident(db, f.manager, { tickets: [{ ticketId: String(source.id), linkedVersion: 1 }],
    title: "Calendar fixture incident", impact: "Scoped service work", severity: "normal", ownerId: f.head.userId, dueDate: "2026-10-20", branchId: f.branch, status: "open" });
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

it("forwards Executive customer evidence through the native transport without a second connection", async () => {
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "disabled"); const ticket = await orderTicket();
  const event = await db.prepare("SELECT id FROM support_ticket_events WHERE ticket_id=? ORDER BY version LIMIT 1").bind(ticket.id).first<{ id: string }>();
  const filters = { from: "2026-01-01", to: "2027-01-01", currency: "THB" }, recordId = `support:${event!.id}`;
  vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced");
  let purchased = false;
  const row = { id: recordId, source_id: String(ticket.id), source_type: "support", title: "Synthetic support case", status: "open", version: 1, at: 105, href: `/support/${ticket.id}`, customer_id: "customer", customer_name: "Acme Customer" };
  const transport: CustomerActivityTransport = {
    search: async (_incoming, actor) => purchased && actor.assignmentId !== "stale-fixture-assignment" ? [row] : [],
    read: async (_incoming, actor) => purchased && actor.assignmentId !== "stale-fixture-assignment" ? row : null,
  };
  const options = { incoming: new Headers({ host: "acme.jesadakorn.com" }), transport };
  expect((await executiveMainEvidenceSources(db, options).factsReadCustomerActivityEvidence(f.owner, recordId, filters)).row).toBeNull();
  expect((await executiveMainEvidenceSources(db, options).factsSearchCustomerActivityEvidence(f.owner, { filters, query: "Private source ticket", limit: 100, metric: "recent-customer-activity" }, null)).rows).toEqual([]);
  purchased = true; vi.mocked(holdingResources).mockClear();
  await db.transaction(async tx => {
    expect((await executiveMainEvidenceSources(tx, options).factsReadCustomerActivityEvidence(f.owner, recordId, filters)).row).toMatchObject({ source_id: ticket.id, source_type: "support" });
    expect((await executiveMainEvidenceSources(tx, options).factsReadCustomerActivityEvidence({ ...f.owner, assignmentId: "stale-fixture-assignment" }, recordId, filters)).row).toBeNull();
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
  await seedSupportTicketState(db, ticketId, { action: "cancel", status: "cancelled", actorId: f.manager.userId, reason: "Close the existing report" });
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
  await seedSupportTicketState(db, ticketId, { action: "cancel", status: "cancelled", actorId: f.manager.userId, reason: "Freeze the retained file history" });
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



it("reads incident file authority through the supplied owner transaction without checking out another Holding connection", async () => {
  await grant("active"); const ticket = await orderTicket();
  const incident = await seedSupportIncident(db, f.manager, { tickets: [{ ticketId: String(ticket.id), linkedVersion: 1 }], title: "Shared fixture incident",
    impact: "Installation investigation", severity: "normal", ownerId: f.head.userId, dueDate: "2026-10-20", branchId: f.branch, status: "open" });
  vi.mocked(holdingResources).mockClear();
  await db.transaction(async transaction => {
    expect(await authorizeAttachmentTarget(transaction, f.manager, attachmentPolicy("support_incident", "incident_evidence", "action"), String(incident.id), "read"))
      .toMatchObject({ companyId: f.company, editable: false });
  });
  expect(holdingResources).not.toHaveBeenCalled();
});

it("keeps rejection/revocation while draining, hides access expansion and expires pending sessions in Work", async () => {
  await grant("active"); const ticket = await orderTicket();
  const expiresAt = Date.now() + 60000;
  const approved = await seedSupportSession(db, f.manager, { ticketId: ticket.id, requestedFor: f.head.userId, createdBy: f.head.userId, reviewedBy: f.manager.userId, status: "approved", expiresAt });
  const pending = await seedSupportSession(db, f.head, { ticketId: ticket.id, requestedFor: f.head.userId, createdBy: f.head.userId, status: "pending", expiresAt });
  await grant("draining");
  const data = await supportSessionWorkspace(f.manager);
  expect(data.tickets).toEqual([]);
  expect(data.rows.find(row => row.id === pending.id)).toMatchObject({ canReview: true, canApprove: false });
  expect(data.rows.find(row => row.id === approved.id)).toMatchObject({ canManage: true, canIssueCapability: false });
  vi.spyOn(Date, "now").mockReturnValue(expiresAt + 1);
  const expired = await supportSessionWorkspace(f.manager);
  expect(expired.rows.find(row => row.id === pending.id)).toMatchObject({ effective_status: "expired", canReview: false, canApprove: false });
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
  await seedSupportTicketState(db, ticketId, { action: "cancel", status: "cancelled", actorId: f.manager.userId, reason: "Retain immutable closed evidence" });
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
  const source = await orderTicket(), incident = await seedSupportIncident(db, f.manager, { tickets: [{ ticketId: String(source.id), linkedVersion: 1 }],
    title: "Metric incident fixture", impact: "Declared synthetic service outage", severity: "normal", ownerId: f.head.userId, dueDate: "2026-10-20", branchId: f.branch, status: "open" });
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
  const asset = await seedAsset(f.owner, { branchId: f.branch, departmentId: f.department, code: key(), name: "Private source fixture", category: "Display", currency: "THB", costMinor: 0, acquiredDate: "2026-10-01" });
  const ticket = await seedSupportTicket(db, f.owner, { sourceKind: "asset", sourceRecord: { kind: "asset", id: asset.id, version: 1, companyId: f.company, href: `/assets/${asset.id}` }, title: "Asset source authority fixture", issue: "Do not disclose this source after access changes" });
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


it('reads a native managed Field case through retained Support consumers after the ticket writer retirement',async()=>{
 let prepared:{db:PostgresDatabase;f:Awaited<ReturnType<typeof seedExpenseCompany>>;customer:string}|undefined;
 try{await withNativeFieldFixture(async native=>{
  if(!prepared)throw new Error('NATIVE_SUPPORT_FIXTURE_REQUIRED');
  const {db,f,customer}=prepared,root='/api/v1/holdings/acme/field-service';
  vi.mocked(holdingResources).mockResolvedValue({write:db,read:db,files:env.H001_FILES});
  const agreement=await native.requestAs<{id:string}>('owner','POST',root,{entity:'agreement',branchId:f.branch,departmentId:f.department,customerId:customer,title:'Native Support handoff',startDate:'2026-01-01',endDate:'2027-12-31',currency:'THB',visitRateMinor:2500,warrantyDiscountBps:0},key());expect(agreement.status,JSON.stringify(agreement.json)).toBe(200);
  const review=await native.requestAs('reviewer','POST',root,{entity:'agreement',id:agreement.json.data.id,action:'activate',expectedVersion:1,reason:'Independent manager approved the native Support fixture'},key());expect(review.status,JSON.stringify(review.json)).toBe(200);
  const created=await native.requestAs<{id:string;ticketId:string}>('owner','POST',root,{entity:'request',agreementId:agreement.json.data.id,agreementVersion:2,title:'Original managed intake',issue:'Customer needs an on-site visit',site:'Customer site',technicianId:f.claimant.userId,scheduledAt:Date.now()+60000},key());expect(created.status,JSON.stringify(created.json)).toBe(200);
  const id=created.json.data.id,ticket=created.json.data.ticketId;
  const schedule=await native.requestAs('owner','POST',root,{id,action:'schedule',expectedVersion:1,technicianId:f.owner.userId,scheduledAt:Date.now()+120000,reason:'Reviewed technician reassignment'},key());expect(schedule.status,JSON.stringify(schedule.json)).toBe(200);
  expect(await db.prepare('SELECT owner_id,status,service_order_version,version FROM support_tickets WHERE id=?').bind(ticket).first()).toEqual({owner_id:f.owner.userId,status:'open',service_order_version:2,version:2});
  const now=Date.now();await db.transaction(async tx=>{
   await tx.prepare("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES(?,'support',1,'active',0,1,'Native managed-case readback','Public synthetic fixture',?)").bind(f.company,now).run();
   await tx.prepare("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT ?,company_id,package_id,?,'native-support-consumer',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),? FROM company_package_grants WHERE company_id=? AND package_id='support'").bind(key(),f.owner.userId,now,f.company).run();
  });
  const helpdesk={...f.owner,modules:['field_service','support']};
  expect((await supportWorkspace(helpdesk)).items.some(row=>row.id===ticket)).toBe(true);
  expect(await supportDetail(helpdesk,ticket)).toMatchObject({managedInFieldService:true,canWork:false,canManage:false,canReport:false,canConfirm:false,canReopen:false,slaPolicies:[]});
  expect((await supportWork(helpdesk)).some(row=>row.id===ticket)).toBe(false);
  expect(await supportEvidence(helpdesk,ticket)).toMatchObject({canChange:false,canShare:false});
  // Explicit fixture package transition, not a Commercial Apply qualification.
  await db.transaction(async tx=>{
   const before=await tx.prepare("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) image FROM company_package_grants WHERE company_id=? AND package_id='service'").bind(f.company).first('image');
   await tx.prepare("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id=? AND package_id='service'").bind(f.company).run();
   await tx.prepare("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT ?,company_id,package_id,?,'native-support-consumer-draining',?::jsonb,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),? FROM company_package_grants WHERE company_id=? AND package_id='service'").bind(key(),f.owner.userId,typeof before==='string'?before:JSON.stringify(before),Date.now(),f.company).run();
  });
  const cancel=await native.requestAs('owner','POST',root,{id,action:'cancel',expectedVersion:2,reason:'Customer cancelled the existing appointment'},key());expect(cancel.status,JSON.stringify(cancel.json)).toBe(200);
  expect(await db.prepare('SELECT status,service_order_version,version FROM support_tickets WHERE id=?').bind(ticket).first()).toEqual({status:'cancelled',service_order_version:3,version:3});
  expect((await db.prepare('SELECT action FROM support_ticket_events WHERE ticket_id=? ORDER BY version').bind(ticket).all()).results).toEqual([{action:'service-intake'},{action:'service-schedule'},{action:'service-cancel'}]);
 },async db=>{
  vi.mocked(holdingResources).mockResolvedValue({write:db,read:db,files:env.H001_FILES});
  const f=await seedExpenseCompany(db,env.H001_FILES);
  const customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Native Support customer'},key())).id);
  await db.exec(`INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_field_parts',2,'["records.read","records.write","records.review","module.field_service.read"]','[]','["holding","branch","department"]','synthetic-support-consumer',0);UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_field_parts'`);
  const actors=[];for(const [label,actor]of [['owner',f.owner],['reviewer',f.tie],['technician',f.claimant]]as const){const assignmentId=await f.grant(actor,'custom_field_parts');await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(['field_service']),assignmentId).run();const version=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(assignmentId).first<number>('version');if(version===null)throw new Error('NATIVE_SUPPORT_ASSIGNMENT_REQUIRED');actors.push({label,userId:actor.userId,assignmentId,assignmentVersion:Number(version),role:'custom_field_parts',modules:['field_service'],branchId:null,departmentId:null});}
  prepared={db,f,customer};return{companyId:f.company,actors};
 },'service');}finally{vi.mocked(holdingResources).mockResolvedValue({write:env.H001_DB,read:env.H001_DB,files:env.H001_FILES});}
},120000);
