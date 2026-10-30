import {historicalServiceCount} from '../../support/field_service/legacy-count-oracle';
import {attachmentToolEnvironment} from '../../../scripts/testing/runtime-tooling';
import { packageGrantStore, readCompanyPackageGrants } from "@erpx/server-commercial/public/package-grants";
import { fieldServiceWorkspace } from "@erpx/server-field_service/public/field-service";
import {getSupportTicket,supportWorkspace,supportDetail,supportWork,changeSupportTicket} from '@erpx/server-support/public/support';
import { supportEvidence } from "@erpx/server-support/public/support-evidence";
import {requiredFact,recordField} from "../../support/helpers/required-fact";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {orderSchema,saveMaster,saveOrder} from '@erpx/server-commerce/public/commerce';
import {createSupportTicket} from '@erpx/server-support/public/support';
import {
  changeAgreement,
  changeServiceOrder,
  createAgreement,
  createServiceOrder,
  createServiceRequest,
  fieldServiceDetail,
  fieldServiceWork,
} from "@erpx/server-field_service/public/field-service";
import {createParty,partySchema,postInvoice,postJournal} from '@erpx/server-finance/public/finance';
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import { escalateInventoryReservations } from "@erpx/server-inventory/public/inventory-reservation-alerts";
import { queryMetric } from "@erpx/server-runtime/public/executive/metrics";
import { readExecutiveEvidence, searchExecutiveEvidence } from "@erpx/server-runtime/public/executive/evidence";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
let serviceDatabase:SqlDatabase|undefined;
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: serviceDatabase??env.H001_DB, files: env.H001_FILES }),
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
  await db.batch([
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'asset','THB',1,1)").bind(`${f.company}-parts-stock`,f.chart,f.company,'1300','Service spare inventory'),
    db.prepare('UPDATE commerce_settings SET inventory_account_id=? WHERE company_id=?').bind(`${f.company}-parts-stock`,f.company),
  ]);
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
async function serviceOrder(options: { assetId?: string; warrantyDiscountBps?: number; scheduledAt?: number } = {}) {
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
          scheduledAt: options.scheduledAt ?? Date.now() + 60000,
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
it("reconciles scheduled field visit completion and uses bounded authorized service-order evidence", async () => {
  const completed = await serviceOrder(), pending = await serviceOrder(), cancelled = await serviceOrder(), outside = await serviceOrder({ scheduledAt: Date.parse("2027-01-05T12:00:00.000Z") });
  await act(f.claimant, completed, { action: "dispatch", expectedVersion: 1, reason: "Technician received the confirmed route." });
  await act(f.claimant, completed, { action: "start", expectedVersion: 2, reason: "Customer granted site access." });
  await act(f.claimant, completed, { action: "complete", expectedVersion: 3, laborMinutes: 45, evidence: "Replaced the controller and verified the safe restart.", customerConfirmation: "Customer confirmed the controller is operating.", reason: "Service work was completed and confirmed." });
  await act(f.claimant, cancelled, { action: "cancel", expectedVersion: 1, reason: "Customer canceled the appointment before dispatch." });
  const scheduledAt = (await db.prepare("SELECT scheduled_at FROM field_service_orders WHERE id=?").bind(completed).first<{ scheduled_at: number }>())!.scheduled_at;
  const from = new Date(Number(scheduledAt)).toISOString().slice(0, 10), filters = { from, to: from, currency: "THB" };
  const total = await queryMetric(db, f.owner, "field-visit-completion", filters);
  const byMonth = await queryMetric(db, f.owner, "field-visit-completion", filters, "month");
  const byStatus = await queryMetric(db, f.owner, "field-visit-completion", filters, "status");
  expect(total.total).toBe("5000");
  expect(total.rows).toEqual([{ label: "Field visits", value: "5000" }]);
  expect(byMonth.rows).toEqual([{ label: from.slice(0, 7), value: "5000" }]);
  expect(byStatus.rows).toEqual([{ label: "completed", value: "10000" }, { label: "scheduled", value: "0" }]);
  await expect(queryMetric(db, { ...f.owner, modules: ["support"] }, "field-visit-completion", filters)).rejects.toMatchObject({ code: "METRIC_FORBIDDEN" });
  const first = await searchExecutiveEvidence(db, f.owner, { metric: "field-visit-completion", filters, limit: 1 });
  expect(first.provider).toBe("field-service-orders");
  expect(first.items).toHaveLength(1); expect(first.hasMore).toBe(true); expect(first.nextCursor).toBeTruthy();
  const second = await searchExecutiveEvidence(db, f.owner, { metric: "field-visit-completion", filters, limit: 1, cursor: first.nextCursor! });
  expect(second.items).toHaveLength(1); expect(second.hasMore).toBe(false);
  expect([first.items[0].id, second.items[0].id]).toContain(pending);
  const match = await searchExecutiveEvidence(db, f.owner, { metric: "field-visit-completion", filters, query: completed });
  expect(match.items.map((item) => item.id)).toEqual([completed]);
  const detail = await readExecutiveEvidence(db, f.owner, "field-visit-completion", completed, filters);
  expect(detail.record).toMatchObject({ id: completed, status: "completed", version: 4, href: `/field-service/${completed}` });
  expect(Number(recordField(detail.record,"visit_completed_at"))).toBeGreaterThan(0);
  expect(detail.fields).toEqual(["scheduled_at", "visit_completed_at", "status", "version"]);
  await expect(readExecutiveEvidence(db, f.owner, "field-visit-completion", cancelled, filters)).rejects.toMatchObject({ status: 404, code: "EVIDENCE_NOT_FOUND" });
  await expect(readExecutiveEvidence(db, f.owner, "field-visit-completion", outside, filters)).rejects.toMatchObject({ status: 404, code: "EVIDENCE_NOT_FOUND" });
  const audit = (await db.prepare("SELECT action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? AND metric_id=? ORDER BY created_at,id")
    .bind(f.company, "field-service-orders", "field-visit-completion")
    .all<{ action: string; record_id: string | null; record_version: number | null; fields: string }>()).results;
  expect(audit.map((row) => row.action).sort()).toEqual(["open", "search", "search", "search"]);
  expect(audit.some((row) => row.action === "open" && row.record_id === completed && row.record_version === 4)).toBe(true);
  expect(audit.every((row) => JSON.parse(row.fields).every((field: string) => ["scheduled_at", "visit_completed_at", "status", "version"].includes(field)))).toBe(true);
});
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
  const cost = (await fieldServiceDetail(f.owner,id)).consumptions[0];
  expect(cost).toMatchObject({ source_kind:'field_service',quantity_milli:1000,value_minor:8000,journal_status:'draft' });
  await expect(act(f.claimant,id,{ action:'complete',expectedVersion:7,laborMinutes:90,evidence:'Startup checks passed.',customerConfirmation:'Customer confirmed operation.',reason:'Cost must be reviewed before closing the visit' })).rejects.toMatchObject({code:'CONFLICT'});
  const costJournal = String(cost.journal_id), flow = await resolveFlow(db,f.owner,'journal',costJournal);
  await assignFlowPrimary(db,f.owner,'journal',costJournal,{stepKey:'draft',userId:f.manager.userId,reason:'Independent review of consumed spare stock and service cost',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  await postJournal(f.manager,costJournal,key(),{expectedVersion:1});
  expect((await db.prepare('SELECT account_id,base_debit_minor,base_credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal').bind(costJournal).all()).results).toEqual([
    {account_id:f.accounts.expense,base_debit_minor:8000,base_credit_minor:0},
    {account_id:`${f.company}-parts-stock`,base_debit_minor:0,base_credit_minor:8000},
  ]);
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM inventory_ledger WHERE source_type='field_service' AND source_id=?").bind(id).first()).toEqual({quantity_milli:-1000,value_minor:-8000});
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
  expect(String(release?.pricing_date)).toBe(new Date().toISOString().slice(0, 10));
  const invoice = await db
    .prepare("SELECT id,total_minor,outstanding_minor FROM finance_invoices WHERE source_type='field_service' AND source_id=?")
    .bind(id)
    .first();
  expect(invoice).toMatchObject({ total_minor: 75000, outstanding_minor: 75000 });
  await postInvoice(
    f.manager,
    String(requiredFact(invoice).id),
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
it("rolls Finance, money, release evidence and command receipts back together and safely replays the retry", async () => {
  const id = await serviceOrder();
  await act(f.claimant,id,{action:"dispatch",expectedVersion:1,reason:"Confirmed site appointment"});
  await act(f.claimant,id,{action:"start",expectedVersion:2,reason:"Customer granted service access"});
  await act(f.claimant,id,{action:"complete",expectedVersion:3,laborMinutes:30,evidence:"Recorded completed safety checks",customerConfirmation:"Customer confirmed completion",reason:"Visit completed for independent billing review"});
  const commandKey=key(),input={action:"release-billing",expectedVersion:4,reason:"Independent reviewer checked retained visit evidence"},fault=`field_service_failure_${crypto.randomUUID().replaceAll("-","").slice(0,12)}`;
  // Fail after Finance and money statements, before the final owner event. The
  // fault is restricted to this case's order and is removed even on failure.
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'FIELD_SERVICE_FINAL_EVENT_FAILURE' USING ERRCODE='23514';END;$$;
    CREATE TRIGGER ${fault} BEFORE INSERT ON field_service_order_events FOR EACH ROW WHEN (NEW.order_id='${id}' AND NEW.action='release-billing') EXECUTE FUNCTION ${fault}();`);
  try {
    await expect(changeServiceOrder(f.owner,id,input,commandKey)).rejects.toMatchObject({code:"23514"});
    expect(await db.prepare("SELECT status,version FROM field_service_orders WHERE id=?").bind(id).first()).toEqual({status:"completed",version:4});
    expect(await db.prepare("SELECT count(*) n FROM finance_invoices WHERE source_type='field_service' AND source_id=?").bind(id).first("n")).toBe(0);
    expect(await db.prepare("SELECT count(*) n FROM money_transaction_snapshots WHERE source_type='field_service_billing_release' AND source_id=?").bind(`field-billing-${commandKey}`).first("n")).toBe(0);
    expect(await db.prepare("SELECT count(*) n FROM field_service_billing_releases WHERE order_id=?").bind(id).first("n")).toBe(0);
    for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(commandKey).first("n")).toBe(0);
  } finally { await db.exec(`DROP TRIGGER IF EXISTS ${fault} ON field_service_order_events;DROP FUNCTION IF EXISTS ${fault}();`); }
  const result=await changeServiceOrder(f.owner,id,input,commandKey);
  expect(result).toMatchObject({id,version:5,status:"billed"});
  expect(await changeServiceOrder(f.owner,id,input,commandKey)).toEqual(result);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(commandKey).first("n")).toBe(1);
  expect(await db.prepare("SELECT count(*) n FROM finance_invoices WHERE source_type='field_service' AND source_id=?").bind(id).first("n")).toBe(1);
  expect(await db.prepare("SELECT count(*) n FROM money_transaction_snapshots WHERE source_type='field_service_billing_release' AND source_id=?").bind(`field-billing-${commandKey}`).first("n")).toBe(1);
  expect(await db.prepare("SELECT count(*) n FROM staff_notifications WHERE record_id=? AND record_version=5").bind(id).first("n")).toBe(1);
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
it("returns only current Organization scope and minimal customer/currency references without buying Finance or People", async () => {
  const usd = String((await createParty(f.owner,partySchema.parse({kind:"customer",code:key(),name:"Service dollar customer",currency:"USD"}),key())).id);
  const blocked = String((await createParty(f.owner,partySchema.parse({kind:"customer",code:key(),name:"Blocked Service customer",currency:"THB"}),key())).id);
  await db.prepare("UPDATE finance_parties SET status='blocked' WHERE id=?").bind(blocked).run();
  const data = await fieldServiceWorkspace({...f.owner,modules:["field_service"],branchId:f.branch,departmentId:f.department});
  expect(data.referenceScope).toEqual({branchId:f.branch,departmentId:f.department});
  expect(data.nodes.map(row=>row.id).sort()).toEqual([f.branch,f.department].sort());
  expect(data.customers.find(row=>row.id===usd)).toEqual({id:usd,name:"Service dollar customer",currency:"USD"});
  expect(data.customers.some(row=>row.id===blocked)).toBe(false);
  expect(data.customers.every(row=>Object.keys(row).sort().join(',')==='currency,id,name')).toBe(true);
  expect(data.assets.some(row=>row.id===warrantyAsset)).toBe(false);
  expect(data.people.every(row=>row.branch_id===f.branch&&row.department_id===f.department)).toBe(true);
  const draft=await createAgreement({...f.owner,modules:["field_service"],branchId:f.branch,departmentId:f.department},
    {branchId:f.branch,departmentId:f.department,customerId:usd,assetId:null,title:"Dollar customer agreement",startDate:"2026-01-01",endDate:"2027-12-31",currency:"USD",visitRateMinor:125075,warrantyDiscountBps:0},key());
  expect(draft).toMatchObject({status:"draft",version:1});
  expect(await db.prepare("SELECT currency,visit_rate_minor,department_id FROM field_service_agreements WHERE id=?").bind(draft.id).first()).toEqual({currency:"USD",visit_rate_minor:125075,department_id:f.department});
});
it("rejects a new link to an asset outside the current Department as well as omitting its reference", async () => {
  const actor={...f.owner,branchId:f.branch,departmentId:f.department},commandKey=key();
  expect((await fieldServiceWorkspace(actor)).assets.some(row=>row.id===warrantyAsset)).toBe(false);
  await expect(createAgreement(actor,{branchId:f.branch,departmentId:f.department,customerId:customer,assetId:warrantyAsset,title:"Cross department asset agreement",startDate:"2026-01-01",endDate:"2027-12-31",currency:"THB",visitRateMinor:150000,warrantyDiscountBps:0},commandKey)).rejects.toMatchObject({code:"CONFLICT"});
  expect(await db.prepare("SELECT count(*) n FROM field_service_agreements WHERE id=?").bind(`service-agreement-${commandKey}`).first('n')).toBe(0);
  expect(await db.prepare("SELECT count(*) n FROM command_receipts WHERE command_id=?").bind(commandKey).first('n')).toBe(0);
});
it("keeps eligible technician choices aligned with actual department assignment and request rejection", async () => {
  const agreementId = await agreement();
  await db.prepare("UPDATE assignments SET scope_kind='department',scope_node_id=? WHERE id=?").bind(f.department,f.claimant.assignmentId).run();
  const people = (await fieldServiceWorkspace(f.owner)).people.filter(row=>row.id===f.claimant.userId);
  expect(people).toHaveLength(1);
  expect(people[0]).toMatchObject({branch_id:f.branch,department_id:f.department});
  const commandKey=key();
  await expect(createServiceRequest(f.owner,{agreementId,agreementVersion:2,title:"Out of scope visit",issue:"Inspect the equipment",site:"Customer reception",technicianId:f.claimant.userId,scheduledAt:null},commandKey)).rejects.toMatchObject({code:"CONFLICT"});
  expect(await db.prepare("SELECT count(*) n FROM field_service_orders WHERE id=?").bind(`service-order-${commandKey}`).first('n')).toBe(0);
  expect(await db.prepare("SELECT count(*) n FROM support_tickets WHERE service_order_id=?").bind(`service-order-${commandKey}`).first('n')).toBe(0);
});
it("removes technicians with an ended engagement and revoked assignments from fresh reference snapshots", async () => {
  expect((await fieldServiceWorkspace(f.owner)).people.some(row=>row.id===f.claimant.userId)).toBe(true);
  await db.prepare("UPDATE hr_engagements SET status='ended' WHERE person_id=?").bind(`person-${f.claimant.userId}`).run();
  expect((await fieldServiceWorkspace(f.owner)).people.some(row=>row.id===f.claimant.userId)).toBe(false);
  expect((await fieldServiceWorkspace(f.owner)).people.some(row=>row.id===f.tie.userId)).toBe(true);
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE user_id=? AND company_id=?").bind(Date.now(),f.tie.userId,f.company).run();
  expect((await fieldServiceWorkspace(f.owner)).people.some(row=>row.id===f.tie.userId)).toBe(false);
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

afterEach(() => { serviceDatabase=undefined;vi.unstubAllEnvs(); vi.restoreAllMocks(); });
async function serviceGrant(state: "active" | "draining", packageId = "service", endsAt: number | null = null) {
  const current = (await readCompanyPackageGrants(db, f.company)).find(row => row.packageId === packageId);
  return packageGrantStore(db, { authorize: async () => ({ userId: "isolated-provider-service-reviewer" }), obligations: async () => { throw new Error("Concrete Service lifecycle owner sources remain required"); } }).apply({
    grant: { companyId: f.company, packageId, packageVersion: 1, state, startsAt: 0, endsAt, version: (current?.version ?? 0) + 1, reason: "Native Service package-only review", evidence: "test:normal-commercial-apply" },
    expectedVersion: current?.version ?? 0, idempotencyKey: key(),
  });
}
const enforceService = () => { vi.stubEnv("ERPX_SERVICE_PACKAGE_ENFORCEMENT", "enforced"); vi.stubEnv("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "enforced"); };
async function freshIntake() {
  f=await expenseFixture();
  for(const actor of [f.owner,f.tie,f.claimant])actor.modules=["field_service"];
  customer=String((await createParty(f.owner,partySchema.parse({kind:"customer",code:key(),name:"Fresh Service customer",currency:"THB"}),key())).id);
  enforceService();await serviceGrant("active");
  const agreementId=await agreement();
  return {agreementId,agreementVersion:2,title:"Customer visit request",issue:"Inspect the equipment at the customer site",site:"Customer reception",technicianId:f.claimant.userId,scheduledAt:Date.now()+60000};
}
it("refuses real Service reads/commands without purchase and omits its Work items", async () => {
  enforceService(); await expect(fieldServiceWorkspace(f.owner)).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" });
  await expect(agreement()).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" }); expect(await fieldServiceWork(f.owner)).toEqual([]);
  expect(await db.prepare("SELECT count(*) total FROM field_service_agreements WHERE company_id=?").bind(f.company).first()).toEqual({ total: 0 });
});
it("completes a Service-only visit through its actual draft invoice handoff without opening Helpdesk", async () => {
  enforceService(); await serviceGrant("active");
  await expect(getSupportTicket(f.owner, ticket)).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" });
  const active = await fieldServiceWorkspace(f.owner);
  expect(active.tickets.map(row => row.id)).toContain(ticket);
  expect(active).toMatchObject({ canCreateAgreement:true,canCreateOrder:true,canUseParts:false,canUseAssetReferences:false,assets:[] });
  const id = await serviceOrder();
  await serviceGrant("draining");
  expect(await fieldServiceWorkspace(f.owner)).toMatchObject({ canCreateAgreement:false,canCreateOrder:false,canUseParts:false,tickets:[],customers:[],nodes:[] });
  await expect(agreement()).rejects.toMatchObject({ code: "PACKAGE_CAPABILITY_CHANGED" });
  await act(f.claimant, id, { action: "dispatch", expectedVersion: 1, reason: "Complete an existing contractual obligation" });
  await act(f.claimant, id, { action: "start", expectedVersion: 2, reason: "Customer granted site access" });
  await act(f.claimant, id, { action: "complete", expectedVersion: 3, laborMinutes: 45, evidence: "Diagnosed the fixture safely", customerConfirmation: "Customer confirmed restored operation", reason: "Existing visit completed" });
  await act(f.tie, id, { action: "release-billing", expectedVersion: 4, reason: "Independent reviewer accepted visit evidence" });
  const detail = await fieldServiceDetail(f.owner, id); expect(detail.order).toMatchObject({ status: "billed", version: 5 });
  expect(await db.prepare("SELECT status,source_type,source_id FROM finance_invoices WHERE source_type='field_service' AND source_id=?").bind(id).first()).toEqual({ status: "draft", source_type: "field_service", source_id: id });
  expect(await db.prepare("SELECT count(*) total FROM command_assertions WHERE command_id LIKE '%:service-purchase:%'").first()).toEqual({ total: 0 });
  // Draft handoff is deliberately not acceptance of invoice issue/collection.
});
it("requires the add-on for actual spare reservations and cleans every composite purchase guard", async () => {
  enforceService(); await serviceGrant("active"); const id = await serviceOrder();
  const input = { action: "reserve-part", expectedVersion: 1, warehouseId: warehouse, productId: product, quantityMilli: 1000, expiresAt: Date.now() + 60000, reason: "Reserve the contractual replacement part" };
  await expect(act(f.claimant, id, input)).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" });
  expect(await db.prepare("SELECT count(*) total FROM field_service_part_reservations WHERE order_id=?").bind(id).first()).toEqual({ total: 0 });
  await serviceGrant("active", "service-parts-assets"); await act(f.claimant, id, input);
  expect(await db.prepare("SELECT count(*) total FROM field_service_part_reservations WHERE order_id=?").bind(id).first()).toEqual({ total: 1 });
  expect(await db.prepare("SELECT count(*) total FROM command_assertions WHERE command_id LIKE '%:service-purchase:%'").first()).toEqual({ total: 0 });
});

it("requires the asset add-on and cannot omit an agreement's bound asset", async () => {
  enforceService(); await serviceGrant("active");
  await expect(agreement({ assetId: warrantyAsset })).rejects.toMatchObject({ code: "PACKAGE_REQUIRED" });
  await serviceGrant("active", "service-parts-assets"); const agreementId = await agreement({ assetId: warrantyAsset });
  const input = { ticketId:ticket,ticketVersion:1,agreementId,agreementVersion:2,issue:"Inspect the bound warranty asset",site:"Customer plant A",technicianId:f.claimant.userId,scheduledAt:Date.now()+60000 };
  await expect(createServiceOrder(f.owner,input,key())).rejects.toMatchObject({ code:"CONFLICT" });
  expect(await db.prepare("SELECT count(*) total FROM field_service_orders WHERE company_id=?").bind(f.company).first()).toEqual({total:0});
  const order = await createServiceOrder(f.owner,{...input,assetId:warrantyAsset},key());
  await serviceGrant("draining", "service-parts-assets");
  expect(await fieldServiceWorkspace(f.owner)).toMatchObject({canUseParts:true,canUseAssetReferences:false,assets:[]});
  await expect(createServiceOrder(f.owner,{...input,assetId:warrantyAsset},key())).rejects.toMatchObject({code:"PACKAGE_CAPABILITY_CHANGED"});
  await act(f.claimant,String(order.id),{action:"cancel",expectedVersion:1,reason:"Finish the existing asset-related obligation"});
});
it("does not serialize a legacy asset reference or offer its actions without add-on history", async () => {
  const id=await serviceOrder({assetId:warrantyAsset});
  enforceService();await serviceGrant("active");
  const workspace=await fieldServiceWorkspace(f.owner),detail=await fieldServiceDetail(f.owner,id);
  expect(JSON.stringify(workspace)).not.toContain(warrantyAsset);
  expect(JSON.stringify(detail)).not.toContain(warrantyAsset);
  expect(workspace.agreements[0]).toMatchObject({asset_id:null,asset_code:null,requires_asset_capability:1});
  expect(detail.order).toMatchObject({asset_id:null,asset_code:null,can_work:0,can_release_billing:0});
  expect((await fieldServiceWork(f.owner)).find(row=>row.id===id)).toMatchObject({canAct:false,queue:"waiting"});
  await serviceGrant("active","service-parts-assets");
  expect((await fieldServiceDetail(f.owner,id)).order).toMatchObject({asset_id:warrantyAsset,can_work:1});
});
it("rejects revoked Service readers and drops their Work instead of returning reference lists", async () => {
  enforceService(); await serviceGrant("active"); const id=await serviceOrder();
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.claimant.assignmentId).run();
  await expect(fieldServiceWorkspace(f.claimant)).rejects.toMatchObject({code:"WORK_ROLE_UNAVAILABLE"});
  await expect(fieldServiceDetail(f.claimant,id)).rejects.toMatchObject({code:"WORK_ROLE_UNAVAILABLE"});
  expect(await fieldServiceWork(f.claimant)).toEqual([]);
  expect((await fieldServiceWork(f.owner)).find(row=>row.id===id)?.blocker).toContain("no longer eligible");
});

it("fails closed for invalid or non-development Service enforcement configuration", async () => {
  vi.stubEnv("ERPX_SERVICE_PACKAGE_ENFORCEMENT","invalid");
  await expect(fieldServiceWorkspace(f.owner)).rejects.toMatchObject({code:"PACKAGE_ENFORCEMENT_CONFIGURATION"});
  const previous=env.ERPX_ENVIRONMENT;
  try { env.ERPX_ENVIRONMENT="production";vi.stubEnv("ERPX_SERVICE_PACKAGE_ENFORCEMENT","enforced");
    await expect(fieldServiceWorkspace(f.owner)).rejects.toMatchObject({code:"PACKAGE_ENFORCEMENT_ENVIRONMENT"});
  } finally { env.ERPX_ENVIRONMENT=previous; }
});
it("observes both Service and parts denial in shadow while the real reservation commits once", async () => {
  vi.stubEnv("ERPX_SERVICE_PACKAGE_ENFORCEMENT","shadow"); const messages:string[]=[];
  vi.spyOn(console,"info").mockImplementation(message=>messages.push(String(message)));
  const id=await serviceOrder(); await act(f.claimant,id,{action:"reserve-part",expectedVersion:1,warehouseId:warehouse,productId:product,quantityMilli:1000,expiresAt:Date.now()+60000,reason:"Declared shadow reservation"});
  expect(await db.prepare("SELECT count(*) total FROM field_service_part_reservations WHERE order_id=?").bind(id).first()).toEqual({total:1});
  const observed=messages.map(message=>JSON.parse(message)).filter(row=>row.event==="commercial.purchase.shadow");
  for(const capability of ["service.orders","service.parts-assets"])
    expect(observed.some(row=>row.phase==="checked"&&row.capability===capability&&row.code==="PACKAGE_REQUIRED")).toBe(true);
  expect(observed.every(row=>Object.keys(row).sort().join(",")==="capability,code,companyId,event,phase,purchaseRevision,purpose")).toBe(true);
  expect(await db.prepare("SELECT count(*) total FROM command_assertions WHERE command_id LIKE '%:service-purchase:%'").first()).toEqual({total:0});
});

it("accepts a fresh Service-only request without a sales document or pre-existing ticket and closes its Support case atomically", async () => {
  f=await expenseFixture();for(const actor of [f.owner,f.tie,f.claimant])actor.modules=["field_service"];
  customer=String((await createParty(f.owner,partySchema.parse({kind:"customer",code:key(),name:"Direct Service intake customer",currency:"THB"}),key())).id);
  await db.batch([
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,control_kind,created_at,updated_at) VALUES(?,?,?,'FS-AR','Service receivables','asset','THB','receivable',1,1)").bind(key(),f.chart,f.company),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'FS-REV','Service revenue','revenue','THB',1,1)").bind(key(),f.chart,f.company),
  ]);
  enforceService();await serviceGrant("active");
  expect(await db.prepare("SELECT count(*) total FROM commerce_orders WHERE company_id=?").bind(f.company).first()).toEqual({total:0});
  expect(await db.prepare("SELECT count(*) total FROM support_tickets WHERE company_id=?").bind(f.company).first()).toEqual({total:0});
  const agreementId=await agreement(),requestKey=key(),input={agreementId,agreementVersion:2,title:"Direct customer visit request",issue:"Inspect the customer equipment on site",site:"Customer reception",technicianId:f.claimant.userId,scheduledAt:Date.now()+60000};
  const result=await createServiceRequest(f.owner,input,requestKey),id=String(result.id),ticketId=String(result.ticketId);
  expect(await createServiceRequest(f.owner,input,requestKey)).toEqual(result);
  expect(await db.prepare("SELECT source_kind,order_id,service_agreement_id,service_order_id,status FROM support_tickets WHERE id=?").bind(ticketId).first()).toEqual({source_kind:"field_service",order_id:null,service_agreement_id:agreementId,service_order_id:id,status:"open"});
  expect(await db.prepare("SELECT count(*) total FROM commerce_orders WHERE company_id=?").bind(f.company).first()).toEqual({total:0});
  expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(["service"]);
  await serviceGrant("draining");
  await act(f.claimant,id,{action:"dispatch",expectedVersion:1,reason:"Complete the contracted visit"});
  await act(f.claimant,id,{action:"start",expectedVersion:2,reason:"Customer provided access"});
  await act(f.claimant,id,{action:"complete",expectedVersion:3,laborMinutes:30,evidence:"Inspected the equipment safely",customerConfirmation:"Customer confirmed restored operation",reason:"Visit completed"});
  await expect(changeAgreement(f.tie,agreementId,{action:"retire",expectedVersion:2,reason:"The completed visit still needs its billing handoff"},key())).rejects.toMatchObject({code:"CONFLICT"});
  await act(f.tie,id,{action:"release-billing",expectedVersion:4,reason:"Independent reviewer accepted the completed visit"});
  expect(await db.prepare("SELECT status,confirmed_by,service_order_id FROM support_tickets WHERE id=?").bind(ticketId).first()).toEqual({status:"closed",confirmed_by:f.tie.userId,service_order_id:id});
  expect(await db.prepare("SELECT count(*) total FROM support_tickets WHERE company_id=?").bind(f.company).first()).toEqual({total:1});
  await changeAgreement(f.tie,agreementId,{action:"retire",expectedVersion:2,reason:"Every visit has completed its billing handoff"},key());
});

it("keeps linked accounts from independently approving an agreement or their own visit completion", async () => {
  const input=await freshIntake(),result=await createServiceRequest(f.owner,input,key()),id=String(result.id);
  await act(f.claimant,id,{action:"dispatch",expectedVersion:1,reason:"Confirmed customer visit"});
  await act(f.claimant,id,{action:"start",expectedVersion:2,reason:"Customer granted site access"});
  await act(f.claimant,id,{action:"complete",expectedVersion:3,laborMinutes:30,evidence:"Inspected the customer equipment",customerConfirmation:"Customer accepted the visit",reason:"Awaiting independent review"});
  // Explicit isolated directory-link fixture: two authorized logins for one
  // person do not become two independent business reviewers.
  await db.batch([f.tie.userId,f.claimant.userId].map(userId=>db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES(?,?,?,?)").bind(f.company,userId,userId,`person-${f.claimant.userId}`)));
  expect((await fieldServiceDetail(f.tie,id)).order.can_release_billing).toBe(0);
  expect((await fieldServiceWork(f.tie)).find(row=>row.id===id)).toMatchObject({canAct:false,queue:"waiting"});
  await expect(act(f.tie,id,{action:"release-billing",expectedVersion:4,reason:"A linked login is the same person who completed the visit"})).rejects.toMatchObject({code:"INDEPENDENT_BILLING_RELEASE_REQUIRED"});
  expect(await db.prepare("SELECT count(*) total FROM finance_invoices WHERE source_type='field_service' AND source_id=?").bind(id).first()).toEqual({total:0});
  await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES(?,?,?,?)").bind(f.company,f.owner.userId,f.owner.userId,`person-${f.claimant.userId}`).run();
  await expect(changeAgreement(f.tie,input.agreementId,{action:"retire",expectedVersion:2,reason:"A linked login cannot review its own agreement"},key())).rejects.toMatchObject({code:"CONFLICT"});
});

it("consumes concurrent direct intake retries once and keeps the case source immutable", async () => {
  const input=await freshIntake(),commandKey=key(),results=await Promise.all([
    createServiceRequest(f.owner,input,commandKey),createServiceRequest(f.owner,input,commandKey),
  ]),id=String(results[0].id),ticketId=String(results[0].ticketId);
  expect(results[1]).toEqual(results[0]);
  for(const [table,column,value] of [["field_service_orders","id",id],["support_tickets","id",ticketId],["support_ticket_events","ticket_id",ticketId],["field_service_order_events","order_id",id]])
    expect(await db.prepare(`SELECT count(*) total FROM ${table} WHERE ${column}=?`).bind(value).first()).toEqual({total:1});
  for(const table of ["command_receipts","audit_events","outbox_events"])
    expect(await db.prepare(`SELECT count(*) total FROM ${table} WHERE command_id=?`).bind(commandKey).first()).toEqual({total:1});
  await expect(createServiceRequest(f.owner,{...input,site:"Different command content"},commandKey)).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
  await expect(db.prepare("UPDATE support_tickets SET service_agreement_id=? WHERE id=?").bind(key(),ticketId).run()).rejects.toThrow("SUPPORT_SOURCE_IMMUTABLE");
  await expect(db.prepare("UPDATE support_tickets SET status='cancelled',version=version+1 WHERE id=?").bind(ticketId).run()).rejects.toThrow("SERVICE_CASE_MANAGED_BY_ORDER");
});

it("rejects inaccessible, stale and ineligible intake sources without orphaning a case or command effects", async () => {
  const input=await freshIntake();
  const attempts: Array<[typeof f.owner,typeof input,string]> = [
    [{...f.owner,modules:[]},input,"MODULE_FORBIDDEN"],
    [{...f.owner,branchId:"foreign-branch"},input,"FIELD_SERVICE_AGREEMENT_NOT_FOUND"],
    [f.owner,{...input,agreementVersion:1},"FIELD_SERVICE_AGREEMENT_CHANGED"],
    [f.owner,{...input,technicianId:"unavailable-technician"},"CONFLICT"],
  ];
  for(const [actor,request,code] of attempts){
    const commandKey=key();
    await expect(createServiceRequest(actor,request,commandKey)).rejects.toMatchObject({code});
    for(const table of ["command_receipts","audit_events","outbox_events"])
      expect(await db.prepare(`SELECT count(*) total FROM ${table} WHERE command_id=?`).bind(commandKey).first()).toEqual({total:0});
  }
  expect(await db.prepare("SELECT count(*) total FROM support_tickets WHERE company_id=?").bind(f.company).first()).toEqual({total:0});
  expect(await db.prepare("SELECT count(*) total FROM field_service_orders WHERE company_id=?").bind(f.company).first()).toEqual({total:0});
});

it("projects reviewed reassignment and cancellation without creating a second Helpdesk task", async () => {
  const input=await freshIntake(),result=await createServiceRequest(f.owner,input,key()),id=String(result.id),ticketId=String(result.ticketId);
  await act(f.owner,id,{action:"schedule",expectedVersion:1,technicianId:f.owner.userId,scheduledAt:Date.now()+120000,reason:"Reviewed technician reassignment"});
  expect(await db.prepare("SELECT owner_id,status,service_order_version,version FROM support_tickets WHERE id=?").bind(ticketId).first()).toEqual({owner_id:f.owner.userId,status:"open",service_order_version:2,version:2});
  await serviceGrant("active","support");
  const helpdesk={...f.owner,modules:["field_service","support"]};
  expect((await supportWorkspace(helpdesk)).items.some(row=>row.id===ticketId)).toBe(true);
  expect(await supportDetail(helpdesk,ticketId)).toMatchObject({managedInFieldService:true,canWork:false,canManage:false,canReport:false,canConfirm:false,canReopen:false,slaPolicies:[]});
  expect((await supportWork(helpdesk)).some(row=>row.id===ticketId)).toBe(false);
  expect(await supportEvidence(helpdesk,ticketId)).toMatchObject({canChange:false,canShare:false});
  await expect(changeSupportTicket(helpdesk,ticketId,{action:"cancel",expectedVersion:2,reason:"Use the Service source to cancel"},key())).rejects.toMatchObject({code:"SOURCE_WORKSPACE_REQUIRED"});
  await serviceGrant("draining");
  await act(f.owner,id,{action:"cancel",expectedVersion:2,reason:"Customer cancelled the existing appointment"});
  expect(await db.prepare("SELECT status,service_order_version,version FROM support_tickets WHERE id=?").bind(ticketId).first()).toEqual({status:"cancelled",service_order_version:3,version:3});
  expect((await db.prepare("SELECT action FROM support_ticket_events WHERE ticket_id=? ORDER BY version").bind(ticketId).all()).results).toEqual([{action:"service-intake"},{action:"service-schedule"},{action:"service-cancel"}]);
});

it("rechecks new-work expiry after outbox without orphaning intake and preserves settlement of an existing visit", async () => {
  const input=await freshIntake(),result=await createServiceRequest(f.owner,input,key()),id=String(result.id),ticketId=String(result.ticketId),commandKey=key(),fault=`service_expiry_${crypto.randomUUID().replaceAll("-","").slice(0,12)}`;
  // The non-transactional sequence proves that the outbox stage was reached,
  // even though every business statement and receipt must roll back.
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
    PERFORM nextval('${fault}');PERFORM pg_sleep(2.25);RETURN NEW;END;$$;
    CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.command_id='${commandKey}') EXECUTE FUNCTION ${fault}();`);
  try{
    await serviceGrant("active","service",Date.now()+2000);
    await expect(createServiceRequest(f.owner,input,commandKey)).rejects.toMatchObject({code:"PACKAGE_CAPABILITY_CHANGED"});
    expect(await db.prepare(`SELECT is_called FROM ${fault}`).first("is_called")).toBe(true);
    expect(await db.prepare("SELECT status,version FROM field_service_orders WHERE id=?").bind(id).first()).toEqual({status:"scheduled",version:1});
    expect(await db.prepare("SELECT status,service_order_version,version FROM support_tickets WHERE id=?").bind(ticketId).first()).toEqual({status:"open",service_order_version:1,version:1});
    expect(await db.prepare("SELECT count(*) total FROM support_tickets WHERE id=?").bind(`ticket-${commandKey}`).first()).toEqual({total:0});
    expect(await db.prepare("SELECT count(*) total FROM field_service_orders WHERE id=?").bind(`service-order-${commandKey}`).first()).toEqual({total:0});
    expect(await db.prepare("SELECT count(*) total FROM staff_notifications WHERE record_id=?").bind(`service-order-${commandKey}`).first()).toEqual({total:0});
    for(const table of ["command_receipts","audit_events","outbox_events"])
      expect(await db.prepare(`SELECT count(*) total FROM ${table} WHERE command_id=?`).bind(commandKey).first()).toEqual({total:0});
  }finally{await db.exec(`DROP TRIGGER IF EXISTS ${fault} ON outbox_events;DROP FUNCTION IF EXISTS ${fault}();DROP SEQUENCE IF EXISTS ${fault};`);}
  expect(await fieldServiceWorkspace(f.owner)).toMatchObject({canCreateOrder:false});
  await act(f.claimant,id,{action:"dispatch",expectedVersion:1,reason:"Expiry drains new work while the existing contractual visit can settle"});
  expect(await db.prepare("SELECT status,version FROM field_service_orders WHERE id=?").bind(id).first()).toEqual({status:"dispatched",version:2});
});

it("rolls case, order, notifications and command effects back when the deferred case projection fails at commit", async () => {
  const input=await freshIntake(),result=await createServiceRequest(f.owner,input,key()),id=String(result.id),ticketId=String(result.ticketId),commandKey=key(),fault=`service_case_failure_${crypto.randomUUID().replaceAll("-","").slice(0,12)}`;
  const before=(await db.prepare("SELECT * FROM support_tickets WHERE id=?").bind(ticketId).first())!;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SERVICE_CASE_PROJECTION_FAILURE' USING ERRCODE='23514';END;$$;
    CREATE TRIGGER ${fault} BEFORE INSERT ON support_ticket_events FOR EACH ROW WHEN (NEW.ticket_id='${ticketId}' AND NEW.action='service-dispatch') EXECUTE FUNCTION ${fault}();`);
  try{
    await expect(changeServiceOrder(f.claimant,id,{action:"dispatch",expectedVersion:1,reason:"Dispatch the agreed customer visit"},commandKey)).rejects.toThrow("SERVICE_CASE_PROJECTION_FAILURE");
    expect(await db.prepare("SELECT * FROM support_tickets WHERE id=?").bind(ticketId).first()).toEqual(before);
    expect(await db.prepare("SELECT status,version FROM field_service_orders WHERE id=?").bind(id).first()).toEqual({status:"scheduled",version:1});
    for(const table of ["command_receipts","audit_events","outbox_events"])
      expect(await db.prepare(`SELECT count(*) total FROM ${table} WHERE command_id=?`).bind(commandKey).first()).toEqual({total:0});
    expect(await db.prepare("SELECT count(*) total FROM staff_notifications WHERE record_id=? AND record_version=2").bind(id).first()).toEqual({total:0});
    expect(await db.prepare("SELECT count(*) total FROM field_service_order_events WHERE order_id=?").bind(id).first()).toEqual({total:1});
  }finally{await db.exec(`DROP TRIGGER IF EXISTS ${fault} ON support_ticket_events;DROP FUNCTION IF EXISTS ${fault}();`);}
  const retry=await changeServiceOrder(f.claimant,id,{action:"dispatch",expectedVersion:1,reason:"Dispatch the agreed customer visit"},commandKey);
  expect(retry).toMatchObject({id,status:"dispatched",version:2});
  expect(await db.prepare("SELECT status,service_order_version,version FROM support_tickets WHERE id=?").bind(ticketId).first()).toEqual({status:"open",service_order_version:2,version:2});
});


it("Service owned source leases observe an actual concurrently retired agreement before the first intake snapshot",async()=>{
 const input=await freshIntake();let ready:()=>void=()=>{},failed:(error:unknown)=>void=()=>{},release:()=>void=()=>{};
 const sourceReady=new Promise<void>((resolve,reject)=>{ready=resolve;failed=reject;}),finish=new Promise<void>(resolve=>{release=resolve;});
 const writer=db.transaction(async transaction=>{serviceDatabase=transaction;vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','disabled');try{await changeAgreement(f.tie,input.agreementId,{action:'retire',expectedVersion:2,reason:'The actual original owner retires the agreement before a queued intake can read'},key());}finally{serviceDatabase=undefined;vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','enforced');}ready();await finish;}).catch(error=>{failed(error);throw error;});
 const command=key();try{await sourceReady;let complete=false;const intake=createServiceRequest(f.owner,input,command).then(()=>null,error=>error).finally(()=>{complete=true;});await new Promise(resolve=>setTimeout(resolve,100));expect(complete).toBe(false);release();await writer;expect(await intake).toMatchObject({code:'FIELD_SERVICE_AGREEMENT_CHANGED'});}finally{release();serviceDatabase=undefined;await writer.catch(()=>{});}
 expect(await db.prepare('SELECT status,version FROM field_service_agreements WHERE id=?').bind(input.agreementId).first()).toEqual({status:'retired',version:3});
 expect(await db.prepare('SELECT id FROM field_service_orders WHERE id=?').bind(`service-order-${command}`).first()).toBeNull();expect(await db.prepare('SELECT id FROM support_tickets WHERE service_order_id=?').bind(`service-order-${command}`).first()).toBeNull();
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
});

it("Service final owner fences roll deferred agreement source and selected authority changes back with the real Support intake and exact retry",async()=>{
 const input=await freshIntake(),command=key(),functionName=`service_owner_final_${key().replaceAll('-','')}`;
 for(const fault of ['source','authority'] as const){const before=await db.prepare('SELECT * FROM field_service_agreements WHERE id=?').bind(input.agreementId).first(),assignment=await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.owner.assignmentId).first();await db.exec(`CREATE SEQUENCE ${functionName}_attempt;CREATE FUNCTION ${functionName}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${functionName}_attempt');${fault==='source'?`UPDATE field_service_agreements SET end_date='2026-01-02' WHERE id='${input.agreementId}';`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}';`}RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${functionName} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${functionName}()`);
  try{await expect(createServiceRequest(f.owner,input,command)).rejects.toMatchObject({code:fault==='source'?'FIELD_SERVICE_SOURCE_CHANGED':'WORK_ROLE_UNAVAILABLE'});expect(await db.prepare(`SELECT is_called FROM ${functionName}_attempt`).first('is_called')).toBe(true);expect(await db.prepare('SELECT * FROM field_service_agreements WHERE id=?').bind(input.agreementId).first()).toEqual(before);expect(await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.owner.assignmentId).first()).toEqual(assignment);for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect(await db.prepare('SELECT id FROM support_tickets WHERE service_order_id=?').bind(`service-order-${command}`).first()).toBeNull();expect(await db.prepare('SELECT id FROM field_service_orders WHERE id=?').bind(`service-order-${command}`).first()).toBeNull();}
  finally{await db.exec(`DROP TRIGGER ${functionName} ON outbox_events;DROP FUNCTION ${functionName}();DROP SEQUENCE ${functionName}_attempt`);}
 }
 const result=await createServiceRequest(f.owner,input,command);await serviceGrant('draining');expect(await createServiceRequest(f.owner,input,command)).toEqual(result);await db.prepare('UPDATE assignments SET revoked_at=1 WHERE id=?').bind(f.owner.assignmentId).run();await expect(createServiceRequest(f.owner,input,command)).rejects.toMatchObject({code:'WORK_ROLE_UNAVAILABLE'});expect(await db.prepare('SELECT count(*)::int count FROM command_receipts WHERE command_id=?').bind(command).first('count')).toBe(1);
});

it("Service owned source writer leases serialize actual intake in two Companies without hidden upgrade deadlocks or crossed case receipts",async()=>{
 const firstInput=await freshIntake(),firstActor=f.owner,firstKey=key(),secondInput=await freshIntake(),secondActor=f.owner,secondKey=key(),functionName=`service_parallel_${key().replaceAll('-','')}`;
 const {measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics');await db.exec(`CREATE FUNCTION ${functionName}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1);RETURN NEW;END $$;CREATE TRIGGER ${functionName} BEFORE INSERT ON field_service_orders FOR EACH ROW WHEN(NEW.id IN ('service-order-${firstKey}','service-order-${secondKey}')) EXECUTE FUNCTION ${functionName}()`);
 let results:Awaited<ReturnType<typeof createServiceRequest>>[]=[];try{const measured=await measureReads(()=>Promise.all([createServiceRequest(firstActor,firstInput,firstKey),createServiceRequest(secondActor,secondInput,secondKey)]));results=measured.result;expect(measured.metrics.deadlockFailures??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${functionName} ON field_service_orders;DROP FUNCTION ${functionName}()`);}
 for(const [index,actor,input,command] of [[0,firstActor,firstInput,firstKey],[1,secondActor,secondInput,secondKey]] as const){const result=results[index];expect(await db.prepare('SELECT company_id,agreement_id,version FROM field_service_orders WHERE id=?').bind(result.id).first()).toEqual({company_id:actor.companyId,agreement_id:input.agreementId,version:1});expect(await db.prepare('SELECT company_id,service_order_id,service_agreement_id FROM support_tickets WHERE id=?').bind(result.ticketId).first()).toEqual({company_id:actor.companyId,service_order_id:result.id,service_agreement_id:input.agreementId});for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*)::int count FROM ${table} actual JOIN command_receipts receipt ON receipt.command_id=actual.command_id WHERE actual.command_id=? AND receipt.actor_id=? AND receipt.authority_json::jsonb->>'companyId'=?`).bind(command,actor.userId,actor.companyId).first('count')).toBe(1);}
});


it("Service-only parts and billing settle through actual independently reviewed source-bound Finance journals and invoices while both owners enforce purchases",async()=>{
 enforceService();await serviceGrant('active');await serviceGrant('active','service-parts-assets');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const id=await serviceOrder();await serviceGrant('draining');await serviceGrant('draining','service-parts-assets');
 await act(f.claimant,id,{action:'reserve-part',expectedVersion:1,warehouseId:warehouse,productId:product,quantityMilli:1000,expiresAt:Date.now()+3600000,reason:'Reserve the actual existing Service spare obligation'});await act(f.claimant,id,{action:'dispatch',expectedVersion:2,reason:'Complete the original contractual dispatch'});await act(f.claimant,id,{action:'start',expectedVersion:3,reason:'Customer granted safe access to the existing visit'});const reservation=String(await db.prepare('SELECT id FROM field_service_part_reservations WHERE order_id=?').bind(id).first('id')),issueKey=key(),issue={action:'issue-part',expectedVersion:4,reservationId:reservation,quantityMilli:1000,reason:'Use the reserved spare with original immutable valuation'};
 const issued=await changeServiceOrder(f.claimant,id,issue,issueKey);expect(await changeServiceOrder(f.claimant,id,issue,issueKey)).toEqual(issued);const consumption=(await fieldServiceDetail(f.owner,id)).consumptions[0],journal=String(consumption.journal_id);
 const {journalDetail,ledgerWorkspace}=await import('@erpx/server-finance/public/ledger'),{financeDocumentPostAccess,financeProductDocumentEntry}=await import('@erpx/server-finance/public/document-purchases'),{getInvoice}=await import('@erpx/server-finance/public/finance-invoice-read');expect((await journalDetail(f.manager,journal)).journal).toMatchObject({id:journal,status:'draft',source_type:'field_service_parts'});expect(await financeDocumentPostAccess(f.manager,'journal',journal)).toEqual({canPost:true});await expect(ledgerWorkspace(f.manager,f.chart)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const assignmentVersion=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.manager.assignmentId).first('version'));expect(await financeProductDocumentEntry(db,{companyId:f.company,branchId:f.manager.branchId,departmentId:f.manager.departmentId,userId:f.manager.userId,assignmentId:f.manager.assignmentId,assignmentVersion,role:f.manager.role},{kind:'journal',id:journal})).toEqual({capability:'finance.service-invoice',purchasePurpose:'history'});
 const complete={action:'complete',expectedVersion:5,laborMinutes:45,evidence:'Customer equipment was tested with the actual replacement spare',customerConfirmation:'Customer confirmed restored original operation',reason:'Complete the actual purchased source after independent cost review'};await expect(act(f.claimant,id,complete)).rejects.toMatchObject({code:'CONFLICT'});const flow=await resolveFlow(db,f.owner,'journal',journal);await assignFlowPrimary(db,f.owner,'journal',journal,{stepKey:'draft',userId:f.manager.userId,reason:'Independent review of the exact Service spare cost',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postJournal(f.manager,journal,key(),{expectedVersion:1});await act(f.claimant,id,complete);await act(f.tie,id,{action:'release-billing',expectedVersion:6,reason:'Independent original Service billing release'});
 const invoice=String(await db.prepare("SELECT id FROM finance_invoices WHERE source_type='field_service' AND source_id=?").bind(id).first('id'));expect((await fieldServiceDetail(f.owner,id)).billing?.invoice_id).toBe(invoice);expect((await getInvoice(f.manager,invoice)).invoice).toMatchObject({id:invoice,status:'draft',total_minor:150000});expect(await financeDocumentPostAccess(f.manager,'invoice',invoice)).toEqual({canPost:true});const invoiceFlow=await resolveFlow(db,f.owner,'invoice',invoice);await assignFlowPrimary(db,f.owner,'invoice',invoice,{stepKey:'draft',userId:f.manager.userId,reason:'Independent original receivable issue review',saveDefault:false,expectedVersion:invoiceFlow.version,recordVersion:invoiceFlow.recordVersion},key());const postKey=key(),postInput={periodId:f.period,entryDate:new Date().toISOString().slice(0,10),expectedVersion:1};const posted=await postInvoice(f.manager,invoice,postInput,postKey);expect(await postInvoice(f.manager,invoice,postInput,postKey)).toEqual(posted);expect((await fieldServiceDetail(f.owner,id)).order).toMatchObject({status:'billed',version:7});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:0,value_minor:0});
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service');enableAttachmentV2ForTest();const version=Number(await db.prepare('SELECT version FROM finance_journals WHERE id=?').bind(journal).first('version')),file=await seedAttachmentSet(db,f.manager,{targetType:'finance_journal',targetId:journal,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:version});await bindRecordAttachmentSet(db,f.manager,file.setId,version,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Exact original Service cost review evidence',{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();expect(await (await attachmentContent(db,env.H001_FILES,f.manager,file.linkId,'original')).object.text()).toBe('Exact original Service cost review evidence');
});

it("Service included Finance source access rejects legacy parts without the actual addon and forged same-label or foreign invoice and journal links",async()=>{
 const id=await serviceOrder();await act(f.claimant,id,{action:'reserve-part',expectedVersion:1,warehouseId:warehouse,productId:product,quantityMilli:1000,expiresAt:Date.now()+3600000,reason:'Original legacy compatible spare reservation'});await act(f.claimant,id,{action:'dispatch',expectedVersion:2,reason:'Actual pre-migration dispatch'});await act(f.claimant,id,{action:'start',expectedVersion:3,reason:'Actual pre-migration visit started'});await act(f.claimant,id,{action:'issue-part',expectedVersion:4,reservationId:String(await db.prepare('SELECT id FROM field_service_part_reservations WHERE order_id=?').bind(id).first('id')),quantityMilli:1000,reason:'Actual original immutable source cost before enforcement'});const actual=(await fieldServiceDetail(f.owner,id)).consumptions[0],journal=String(actual.journal_id);enforceService();await serviceGrant('active');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const {journalDetail}=await import('@erpx/server-finance/public/ledger');await expect(journalDetail(f.manager,journal)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await serviceGrant('active','service-parts-assets');expect((await journalDetail(f.manager,journal)).journal.id).toBe(journal);
 const fakeJournal=`fake-service-${key()}`;await db.prepare("INSERT INTO finance_journals(id,journal_number,chart_id,period_id,company_id,entry_date,currency,memo,source_type,source_id,status,created_by,created_at,updated_at) SELECT ?,journal_number||'-forged',chart_id,period_id,company_id,entry_date,currency,'Forged source label only',source_type,source_id,'draft',created_by,created_at,updated_at FROM finance_journals WHERE id=?").bind(fakeJournal,journal).run();await expect(journalDetail(f.manager,fakeJournal)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(postJournal(f.manager,fakeJournal,key(),{expectedVersion:1})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const foreign=await expenseFixture();await expect(journalDetail({...foreign.manager,modules:['finance']},journal)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
});


it("Service source-bound independent Finance posting rechecks actual reservation lineage and current Finance authority after deferred effects with exact rollback and retry",async()=>{
 const otherProduct=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Another actual spare source',price:'80'},key())).id);await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,1000,8000,1)').bind(warehouse,otherProduct).run();enforceService();await serviceGrant('active');await serviceGrant('active','service-parts-assets');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const id=await serviceOrder();await act(f.claimant,id,{action:'reserve-part',expectedVersion:1,warehouseId:warehouse,productId:product,quantityMilli:1000,expiresAt:Date.now()+3600000,reason:'Original reservation for final source witness'});await act(f.claimant,id,{action:'dispatch',expectedVersion:2,reason:'Actual original dispatch'});await act(f.claimant,id,{action:'start',expectedVersion:3,reason:'Original customer access'});const reservation=String(await db.prepare('SELECT id FROM field_service_part_reservations WHERE order_id=?').bind(id).first('id'));await act(f.claimant,id,{action:'issue-part',expectedVersion:4,reservationId:reservation,quantityMilli:1000,reason:'Original exact cost source for independent journal review'});const journal=String((await fieldServiceDetail(f.owner,id)).consumptions[0].journal_id),flow=await resolveFlow(db,f.owner,'journal',journal);await assignFlowPrimary(db,f.owner,'journal',journal,{stepKey:'draft',userId:f.manager.userId,reason:'Actual independent final Service cost reviewer',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await serviceGrant('draining');await serviceGrant('draining','service-parts-assets');const command=key(),functionName=`service_finance_final_${key().replaceAll('-','')}`,beforeJournal=await db.prepare('SELECT * FROM finance_journals WHERE id=?').bind(journal).first(),beforeReservation=await db.prepare('SELECT * FROM field_service_part_reservations WHERE id=?').bind(reservation).first(),beforeAuthority=await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.manager.assignmentId).first();
 for(const fault of ['source','authority'] as const){await db.exec(`CREATE SEQUENCE ${functionName}_attempt;CREATE FUNCTION ${functionName}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${functionName}_attempt');${fault==='source'?`UPDATE field_service_part_reservations SET product_id='${otherProduct}' WHERE id='${reservation}';`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}';`}RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${functionName} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${functionName}()`);try{await expect(postJournal(f.manager,journal,command,{expectedVersion:1})).rejects.toMatchObject({code:fault==='source'?'FINANCE_DOCUMENT_SOURCE_CHANGED':'FINANCE_ACCESS_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${functionName}_attempt`).first('is_called')).toBe(true);expect(await db.prepare('SELECT * FROM finance_journals WHERE id=?').bind(journal).first()).toEqual(beforeJournal);expect(await db.prepare('SELECT * FROM field_service_part_reservations WHERE id=?').bind(reservation).first()).toEqual(beforeReservation);expect(await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.manager.assignmentId).first()).toEqual(beforeAuthority);for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${functionName} ON outbox_events;DROP FUNCTION ${functionName}();DROP SEQUENCE ${functionName}_attempt`);}}
 const result=await postJournal(f.manager,journal,command,{expectedVersion:1});expect(await postJournal(f.manager,journal,command,{expectedVersion:1})).toEqual(result);expect(await db.prepare('SELECT status FROM finance_journals WHERE id=?').bind(journal).first('status')).toBe('posted');expect(await db.prepare('SELECT count(*)::int count FROM command_receipts WHERE command_id=?').bind(command).first('count')).toBe(1);
});


it("Service billing copies its immutable original quote to the invoice draft and reviewed secondary posting without rerating after a later manual FX confirmation",async()=>{
 const {prepareBookRule,decideBookRule,bookRuleSchema}=await import('@erpx/server-finance/public/finance-books'),{confirmManualRate}=await import('@erpx/server-finance/public/company-money'),{journalDetail}=await import('@erpx/server-finance/public/ledger'),{financeProductDocumentEntry}=await import('@erpx/server-finance/public/document-purchases'),{fieldServiceInvoiceMoneySource}=await import('@erpx/server-field_service/public/finance-record-sources');const secondary=key();await db.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,?,'THB',?,1,1)").bind(secondary,f.company,'Reviewed original Service secondary',f.owner.userId).run();const accounts=(await db.prepare('SELECT id FROM finance_accounts WHERE chart_id=? ORDER BY code').bind(f.chart).all<{id:string}>()).results,mappings:{primaryAccountId:string;secondaryAccountId:string}[]=[];for(const account of accounts){const target=key();await db.prepare('INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) SELECT ?,?,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at FROM finance_accounts WHERE id=?').bind(target,secondary,account.id).run();mappings.push({primaryAccountId:account.id,secondaryAccountId:target});}const date=new Date().toISOString().slice(0,10),rule=await prepareBookRule(f.owner,bookRuleSchema.parse({primaryChartId:f.chart,secondaryChartId:secondary,effectiveDate:date,openingPeriodId:f.period,reason:'Original independent Service reporting book mapping',mappings}),key());await decideBookRule(f.manager,String(rule.id),{expectedVersion:1,reason:'Independent review of the real Service secondary book'},'approve',key());
 enforceService();await serviceGrant('active');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const id=await serviceOrder();await act(f.claimant,id,{action:'dispatch',expectedVersion:1,reason:'Original Service source dispatch'});await act(f.claimant,id,{action:'start',expectedVersion:2,reason:'Actual source customer access'});await act(f.claimant,id,{action:'complete',expectedVersion:3,laborMinutes:45,evidence:'Exact retained original source visit',customerConfirmation:'Actual customer accepted this completed visit',reason:'Original sign-off before independent bill release'});const releaseKey=key();await changeServiceOrder(f.tie,id,{action:'release-billing',expectedVersion:4,reason:'Independent original price and customer sign-off review'},releaseKey);const invoice=`field-invoice-${releaseKey}`,release=`field-billing-${releaseKey}`,quoteSql='SELECT currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback FROM money_transaction_snapshots WHERE company_id=? AND source_type=? AND source_id=? AND amount_kind=?',frozen=await db.prepare(quoteSql).bind(f.company,'field_service_billing_release',release,'invoice_total').first();expect(await db.prepare(quoteSql).bind(f.company,'finance_invoice',invoice,'draft:v1').first()).toEqual(frozen);expect(await fieldServiceInvoiceMoneySource(db,f.company,invoice)).toEqual({sourceType:'field_service_billing_release',sourceId:release,amountKind:'invoice_total'});
 const later=await confirmManualRate(f.manager,{currency:'THB',effectiveDate:date,currencyPerUsd:'72',note:'Actual independently confirmed later quote cannot rewrite the released Service price'},key());expect(later).toBeTruthy();const newer=String(await db.prepare("SELECT id FROM company_fx_rates WHERE company_id=? AND currency='THB' ORDER BY effective_date DESC,confirmed_at DESC,id DESC LIMIT 1").bind(f.company).first('id'));expect(newer).not.toBe(frozen?.fx_rate_id);await serviceGrant('draining');const flow=await resolveFlow(db,f.owner,'invoice',invoice);await assignFlowPrimary(db,f.owner,'invoice',invoice,{stepKey:'draft',userId:f.manager.userId,reason:'Independent issue of the exact retained Service amount',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());const posted=await postInvoice(f.manager,invoice,{periodId:f.period,entryDate:date,expectedVersion:1},key());expect(await db.prepare(quoteSql).bind(f.company,'finance_invoice',invoice,'total').first()).toEqual(frozen);expect(await db.prepare("SELECT carried_source_type,carried_source_id,carried_amount_kind FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind='total'").bind(f.company,invoice).first()).toEqual({carried_source_type:'finance_invoice',carried_source_id:invoice,carried_amount_kind:'draft:v1'});const projected=String(await db.prepare('SELECT secondary_journal_id FROM finance_book_postings WHERE primary_journal_id=? AND rule_id=?').bind(posted.journalId,rule.id).first('secondary_journal_id'));expect(projected).not.toBe('null');expect((await journalDetail(f.manager,projected)).journal).toMatchObject({id:projected,status:'posted',chart_id:secondary});expect(await db.prepare('SELECT sum(debit_minor)::text debit,sum(credit_minor)::text credit FROM finance_journal_lines WHERE journal_id=?').bind(projected).first()).toEqual({debit:'150000',credit:'150000'});const version=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.manager.assignmentId).first('version'));expect(await financeProductDocumentEntry(db,{companyId:f.company,branchId:f.manager.branchId,departmentId:f.manager.departmentId,userId:f.manager.userId,assignmentId:f.manager.assignmentId,assignmentVersion:version,role:f.manager.role},{kind:'journal',id:projected})).toEqual({capability:'finance.service-invoice',purchasePurpose:'history'});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['service']);
});

it("Service original fully covered warranty can retain its genuine zero billing draft and exact zero reporting evidence",async()=>{
 enforceService();await serviceGrant('active');await serviceGrant('active','service-parts-assets');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const id=await serviceOrder({assetId:warrantyAsset,warrantyDiscountBps:10000});await act(f.claimant,id,{action:'dispatch',expectedVersion:1,reason:'Original covered warranty dispatch'});await act(f.claimant,id,{action:'start',expectedVersion:2,reason:'Actual customer covered equipment access'});await act(f.claimant,id,{action:'complete',expectedVersion:3,laborMinutes:30,evidence:'Warranty inspection retained',customerConfirmation:'Customer accepted the covered original visit',reason:'Original full warranty completion'});const command=key();await changeServiceOrder(f.tie,id,{action:'release-billing',expectedVersion:4,reason:'Independent reviewer verified full original warranty coverage'},command);expect(await db.prepare('SELECT total_minor,status FROM finance_invoices WHERE id=?').bind(`field-invoice-${command}`).first()).toEqual({total_minor:0,status:'draft'});expect(await db.prepare("SELECT amount_units::text,usd_units::text FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind='draft:v1'").bind(f.company,`field-invoice-${command}`).first()).toEqual({amount_units:'0',usd_units:'0'});
});


it("Service retained pre-draft-snapshot billing command shape carries the actual original release quote when issued after a later manual rate",async()=>{
 const id=await serviceOrder();await act(f.claimant,id,{action:'dispatch',expectedVersion:1,reason:'Original legacy compatible dispatch'});await act(f.claimant,id,{action:'start',expectedVersion:2,reason:'Actual legacy compatible customer access'});await act(f.claimant,id,{action:'complete',expectedVersion:3,laborMinutes:30,evidence:'Original retained visit evidence',customerConfirmation:'Customer accepted the retained original visit',reason:'Completed original visit before the old billing writer'});
 // Retain the exact previously shipped billing statement shape through the
 // original command runner: no new invoice-draft money snapshot existed yet.
 // Original immutable history, independent-review and command guards all run.
 const {executeCommand}=await import('@erpx/server-workflow/public/commands'),{assertion}=await import('@erpx/server-workflow/public/command-assertions'),{snapshotMoney,confirmManualRate}=await import('@erpx/server-finance/public/company-money'),releaseKey=key(),release=`field-billing-${releaseKey}`,invoice=`field-invoice-${releaseKey}`,now=Date.now(),date=new Date(now).toISOString().slice(0,10),dueDate=new Date(now+30*86400000).toISOString().slice(0,10),order=(await db.prepare('SELECT * FROM field_service_orders WHERE id=?').bind(id).first())!,input={action:'release-billing',expectedVersion:4,reason:'Original independent billing writer predating invoice draft snapshots'},money=await snapshotMoney(db,f.tie,{sourceType:'field_service_billing_release',sourceId:release,amountKind:'invoice_total',transactionDate:date,currency:'THB',currencyScale:2,amountUnits:'150000'});
 await executeCommand(db,f.tie,releaseKey,'field.order.release-billing',input,id,5,{id,version:5,status:'billed'},[
  assertion(db,releaseKey,'legacy-completion',"EXISTS(SELECT 1 FROM field_service_orders WHERE id=? AND version=4 AND status='completed')",[id]),money.statement,
  db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,created_by,created_at,updated_at) VALUES(?,?,?,'receivable',?,?,?,'THB',150000,0,150000,150000,?,?,'field_service',?,'draft',?,?,?)").bind(invoice,f.company,customer,`FS-${id}`,date,dueDate,`${f.company}-receivable`,f.accounts.revenue,id,f.tie.userId,now,now),
  db.prepare("INSERT INTO field_service_billing_releases(id,order_id,agreement_id,customer_id,currency,amount_minor,warranty_applied,warranty_discount_bps,warranty_end_snapshot,pricing_date,source_order_version,released_by,reason,created_at) VALUES(?,?,?,?,'THB',150000,false,0,NULL,?,5,?,?,?)").bind(release,id,order.agreement_id,customer,date,f.tie.userId,input.reason,now),
  db.prepare("UPDATE field_service_orders SET status='billed',version=5,updated_at=? WHERE id=?").bind(now,id),
  db.prepare("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES(?,?,5,'release-billing',?,?,'billed',?,?,'{}',?)").bind(`service-order-event-${releaseKey}`,id,f.tie.userId,input.reason,order.technician_id,order.scheduled_at,now),
 ],f.tie);
 const quoteSql='SELECT currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback FROM money_transaction_snapshots WHERE company_id=? AND source_type=? AND source_id=? AND amount_kind=?',frozen=await db.prepare(quoteSql).bind(f.company,'field_service_billing_release',release,'invoice_total').first();expect(await db.prepare(quoteSql).bind(f.company,'finance_invoice',invoice,'draft:v1').first()).toBeNull();enforceService();await serviceGrant('active');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');await confirmManualRate(f.manager,{currency:'THB',effectiveDate:date,currencyPerUsd:'72',note:'Later actual manual rate after the legacy immutable release'},key());await serviceGrant('draining');const flow=await resolveFlow(db,f.owner,'invoice',invoice);await assignFlowPrimary(db,f.owner,'invoice',invoice,{stepKey:'draft',userId:f.manager.userId,reason:'Original independent review of retained pre-snapshot billing',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());const command=key(),post={periodId:f.period,entryDate:date,expectedVersion:1},result=await postInvoice(f.manager,invoice,post,command);expect(await postInvoice(f.manager,invoice,post,command)).toEqual(result);expect(await db.prepare(quoteSql).bind(f.company,'finance_invoice',invoice,'total').first()).toEqual(frozen);expect(await db.prepare(quoteSql).bind(f.company,'field_service_billing_release',release,'invoice_total').first()).toEqual(frozen);expect(await db.prepare("SELECT carried_source_type,carried_source_id,carried_amount_kind FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind='total'").bind(f.company,invoice).first()).toEqual({carried_source_type:'field_service_billing_release',carried_source_id:release,carried_amount_kind:'invoice_total'});
});


it("Service registered visit Files require the actual purchase addon current module and original private source while existing draining visits can intake evidence",async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{createAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service');enableAttachmentV2ForTest();const id=await serviceOrder({assetId:warrantyAsset}),file=await seedAttachmentSet(db,f.claimant,{targetType:'field_service_visit',targetId:id,componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:1}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),body='Actual private Service technician evidence';await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();enforceService();const intake={targetType:'field_service_visit',targetId:id,componentKind:'record' as const,purposeKey:'technician_report',expectedTargetVersion:1};await expect(createAttachmentSet(db,f.claimant,intake,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await serviceGrant('active');await expect(attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await serviceGrant('active','service-parts-assets');expect(await(await attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).object.text()).toBe(body);await expect(attachmentContent(db,env.H001_FILES,{...f.claimant,modules:['finance']},file.linkId,'original')).rejects.toMatchObject({code:'MODULE_FORBIDDEN'});await serviceGrant('draining');await serviceGrant('draining','service-parts-assets');expect(await createAttachmentSet(db,f.claimant,{...intake,componentRef:'Existing visit settlement'},key())).toMatchObject({targetType:'field_service_visit',targetId:id});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.claimant.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_TARGET_NOT_FOUND'});
});

it("Service record and compatibility Files binding preserve the actual immutable visit event and deferred Support case version in disabled and enforced modes",async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest,attachmentBindingState}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,bindCompatibilityAttachmentSet,attachmentContent,createAttachmentSet}=await import('@erpx/server-files/public/attachments/service'),{fieldServiceAttachmentRecordVersionStep}=await import('@erpx/server-field_service/public/attachment-purchases');enableAttachmentV2ForTest();for(const mode of ['disabled','enforced'] as const)for(const compatibility of [false,true]){const input=await freshIntake(),result=await createServiceRequest(f.owner,input,key()),id=String(result.id),file=await seedAttachmentSet(db,f.claimant,{targetType:'field_service_visit',targetId:id,componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:1,compatibilityAdapter:compatibility}),command=key();vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT',mode);expect(()=>fieldServiceAttachmentRecordVersionStep(db,'field_service_visit',id,1,Date.now())).toThrow('original Files command context');const bind=compatibility?bindCompatibilityAttachmentSet:bindRecordAttachmentSet,receipt=await bind(db,f.claimant,file.setId,1,command);expect(await bind(db,f.claimant,file.setId,1,command)).toEqual(receipt);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:'bound',link_state:'bound'});expect((await fieldServiceDetail(f.claimant,id)).canEditVisitFiles).toBe(true);expect(await db.prepare('SELECT version,status FROM field_service_orders WHERE id=?').bind(id).first()).toEqual({version:2,status:'scheduled'});expect(await db.prepare("SELECT version,action,actor_id,status,details_json->>'attachmentCommandId' attachment_command FROM field_service_order_events WHERE id=?").bind(`service-file-event-${command}`).first()).toEqual({version:2,action:'attach-evidence',actor_id:f.claimant.userId,status:'scheduled',attachment_command:command});expect(await db.prepare('SELECT service_order_version,version,status FROM support_tickets WHERE id=?').bind(result.ticketId).first()).toEqual({service_order_version:2,version:2,status:'open'});expect(await db.prepare('SELECT action,actor_id FROM support_ticket_events WHERE ticket_id=? AND version=2').bind(result.ticketId).first()).toEqual({action:'service-attach-evidence',actor_id:f.claimant.userId});expect(await db.prepare('SELECT count(*)::int count FROM command_receipts WHERE command_id=?').bind(command).first('count')).toBe(1);
  vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','enforced');await serviceGrant('draining');expect(await bind(db,f.claimant,file.setId,1,command)).toEqual(receipt);const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),body=`Exact ${mode} ${compatibility?'compatibility':'record'} Service evidence`;await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();await act(f.owner,id,{action:'cancel',expectedVersion:2,reason:'Original customer cancelled this documented existing visit'});expect((await fieldServiceDetail(f.claimant,id)).canEditVisitFiles).toBe(false);expect(await(await attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).object.text()).toBe(body);await expect(createAttachmentSet(db,f.claimant,{targetType:'field_service_visit',targetId:id,componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:3},key())).rejects.toMatchObject({code:'SERVICE_ATTACHMENT_HISTORY_ONLY'});expect(await bind(db,f.claimant,file.setId,1,command)).toEqual(receipt);expect(await db.prepare('SELECT version,status FROM field_service_orders WHERE id=?').bind(id).first()).toEqual({version:3,status:'cancelled'});
 }
});

it("Service Files final source and selected authority fences roll original visit event case projection sensitive set and command effects back before exact retry",async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest,attachmentBindingState}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet}=await import('@erpx/server-files/public/attachments/service');enableAttachmentV2ForTest();const input=await freshIntake(),visit=await createServiceRequest(f.owner,input,key()),id=String(visit.id),file=await seedAttachmentSet(db,f.claimant,{targetType:'field_service_visit',targetId:id,componentKind:'record',purposeKey:'signed_job_sheet',expectedTargetVersion:1}),command=key(),functionName=`service_file_final_${key().replaceAll('-','')}`,before=await db.prepare('SELECT * FROM field_service_orders WHERE id=?').bind(id).first(),caseBefore=await db.prepare('SELECT * FROM support_tickets WHERE id=?').bind(visit.ticketId).first(),authorityBefore=await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first();for(const fault of ['source','authority'] as const){await db.exec(`CREATE SEQUENCE ${functionName}_attempt;CREATE FUNCTION ${functionName}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${functionName}_attempt');${fault==='source'?`UPDATE field_service_orders SET site=site||' changed late' WHERE id='${id}';`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}';`}RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${functionName} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${functionName}()`);try{await expect(bindRecordAttachmentSet(db,f.claimant,file.setId,1,command)).rejects.toMatchObject({code:fault==='source'?'SERVICE_ATTACHMENT_CHANGED':'CONFLICT'});expect(await db.prepare(`SELECT is_called FROM ${functionName}_attempt`).first('is_called')).toBe(true);expect(await db.prepare('SELECT * FROM field_service_orders WHERE id=?').bind(id).first()).toEqual(before);expect(await db.prepare('SELECT * FROM support_tickets WHERE id=?').bind(visit.ticketId).first()).toEqual(caseBefore);expect(await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first()).toEqual(authorityBefore);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:'draft',link_state:'draft'});expect(await db.prepare('SELECT id FROM field_service_order_events WHERE id=?').bind(`service-file-event-${command}`).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${functionName} ON outbox_events;DROP FUNCTION ${functionName}();DROP SEQUENCE ${functionName}_attempt`);}}
 const result=await bindRecordAttachmentSet(db,f.claimant,file.setId,1,command);expect(await bindRecordAttachmentSet(db,f.claimant,file.setId,1,command)).toEqual(result);expect(await db.prepare('SELECT service_order_version,version FROM support_tickets WHERE id=?').bind(visit.ticketId).first()).toEqual({service_order_version:2,version:2});
});

it("Service visit Files bind in two actual Companies with zero hidden writer lease deadlocks and isolated source event and receipt identities",async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet}=await import('@erpx/server-files/public/attachments/service'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics');enableAttachmentV2ForTest();const firstInput=await freshIntake(),firstVisit=await createServiceRequest(f.owner,firstInput,key()),firstActor=f.claimant,firstFile=await seedAttachmentSet(db,firstActor,{targetType:'field_service_visit',targetId:String(firstVisit.id),componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:1}),secondInput=await freshIntake(),secondVisit=await createServiceRequest(f.owner,secondInput,key()),secondActor=f.claimant,secondFile=await seedAttachmentSet(db,secondActor,{targetType:'field_service_visit',targetId:String(secondVisit.id),componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:1}),firstKey=key(),secondKey=key(),functionName=`service_file_parallel_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${functionName}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1);RETURN NEW;END $$;CREATE TRIGGER ${functionName} BEFORE UPDATE ON field_service_orders FOR EACH ROW WHEN(NEW.id IN ('${firstVisit.id}','${secondVisit.id}') AND NEW.version=2 AND OLD.version=1) EXECUTE FUNCTION ${functionName}()`);try{const measured=await measureReads(()=>Promise.all([bindRecordAttachmentSet(db,firstActor,firstFile.setId,1,firstKey),bindRecordAttachmentSet(db,secondActor,secondFile.setId,1,secondKey)]));expect(measured.result).toHaveLength(2);expect(measured.metrics.deadlockFailures??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${functionName} ON field_service_orders;DROP FUNCTION ${functionName}()`);}for(const [actor,visit,command] of [[firstActor,firstVisit,firstKey],[secondActor,secondVisit,secondKey]] as const){expect(await db.prepare('SELECT company_id,version FROM field_service_orders WHERE id=?').bind(visit.id).first()).toEqual({company_id:actor.companyId,version:2});expect(await db.prepare('SELECT actor_id FROM field_service_order_events WHERE id=? AND order_id=?').bind(`service-file-event-${command}`,visit.id).first('actor_id')).toBe(actor.userId);expect(await db.prepare("SELECT actor_id,authority_json::jsonb->>'companyId' company FROM command_receipts WHERE command_id=?").bind(command).first()).toEqual({actor_id:actor.userId,company:actor.companyId});}
});

async function serviceCashInvoice(){
 const id=await serviceOrder();await act(f.claimant,id,{action:'dispatch',expectedVersion:1,reason:'Original customer cash collection dispatch'});await act(f.claimant,id,{action:'start',expectedVersion:2,reason:'Original customer access before cash collection'});await act(f.claimant,id,{action:'complete',expectedVersion:3,laborMinutes:35,evidence:'Actual completed Service visit before receivable',customerConfirmation:'Customer accepted the original visit and charge',reason:'Original Service completion proof'});const release=key();await changeServiceOrder(f.tie,id,{action:'release-billing',expectedVersion:4,reason:'Independent actual billing release for cash collection'},release);const invoice=`field-invoice-${release}`,flow=await resolveFlow(db,f.owner,'invoice',invoice);await assignFlowPrimary(db,f.owner,'invoice',invoice,{stepKey:'draft',userId:f.manager.userId,reason:'Independent review of original Service receivable',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());const date=new Date().toISOString().slice(0,10);await postInvoice(f.manager,invoice,{periodId:f.period,entryDate:date,expectedVersion:1},key());await db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='bank' WHERE id=?").bind(f.accounts.cash).run();return {id,invoice,date};
}
it('Service-only controlled cash receipt uses its exact immutable invoice and independent bank evidence while draining',async()=>{
 const {createPayment,approvePayment,releasePayment,confirmPayment,importBankStatement,getPayment,bankWorkspace}=await import('@erpx/server-finance/public/finance'),{getInvoice}=await import('@erpx/server-finance/public/finance-invoice-read'),{journalDetail}=await import('@erpx/server-finance/public/ledger'),{financeProductDocumentEntry}=await import('@erpx/server-finance/public/document-purchases');enforceService();await serviceGrant('active');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const {invoice,date}=await serviceCashInvoice();await serviceGrant('draining');const input={partyId:customer,kind:'receipt' as const,paymentNumber:key(),paymentDate:date,currency:'THB',amount:'1500',bankAccount:f.accounts.cash,settlementInvoiceId:invoice},create=key(),payment=await createPayment(f.reviewer,input,create);expect(await createPayment(f.reviewer,input,create)).toEqual(payment);
 await expect(approvePayment(f.reviewer,String(payment.id),{expectedVersion:1,proof:'Original self approval must still fail'},key())).rejects.toMatchObject({code:'FINANCE_APPROVAL_REQUIRED'});await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:'Independent review of exact Service invoice and receipt'},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:'Third person checked the real bank receipt'},key());
 const bankInput={settlementPaymentId:String(payment.id),bankAccount:f.accounts.cash,statementNumber:key(),statementDate:date,opening:'0',closing:'1500',sourceHash:key(),rows:[{externalId:key(),transactionDate:date,description:'Actual customer payment for this Service invoice',amount:'1500',currency:'THB'}]},bankKey=key(),statement=await importBankStatement(f.reviewer,bankInput,bankKey);expect(await importBankStatement(f.reviewer,bankInput,bankKey)).toEqual(statement);const bank=String(await db.prepare('SELECT id FROM finance_bank_transactions WHERE statement_id=?').bind(statement.id).first('id')),workspace=await getPayment(f.payer,String(payment.id));expect(workspace.invoices.map(row=>row.id)).toEqual([invoice]);expect(workspace.bankTransactions.map(row=>row.id)).toEqual([bank]);expect(workspace.sourcePurchase).toMatchObject({capability:'finance.service-invoice',settleExisting:true,history:true});
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{attachmentContent}=await import('@erpx/server-files/public/attachments/service');enableAttachmentV2ForTest();const evidence=await seedAttachmentSet(db,f.payer,{targetType:'finance_payment',targetId:String(payment.id),purposeKey:'payment_proof',componentKind:'action',componentRef:'bank-confirmation',expectedTargetVersion:3}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Original private Service bank receipt retained under Legal hold',{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(evidence.sourceId).run();
 const confirm={expectedVersion:3,attachmentSetIds:[evidence.setId],periodId:f.period,bankTransactionId:bank,bankVersion:1,bankReference:'ACTUAL-SERVICE-CUSTOMER-RECEIPT',proof:'Treasury confirmed exact original customer bank evidence',allocations:[{invoiceId:invoice,amount:'1500'}]},keyConfirm=key(),settled=await confirmPayment(f.payer,String(payment.id),confirm,keyConfirm);expect(await confirmPayment(f.payer,String(payment.id),confirm,keyConfirm)).toEqual(settled);expect((await getInvoice(f.manager,invoice)).invoice).toMatchObject({status:'paid',outstanding_minor:0});expect((await journalDetail(f.manager,String(settled.journalId))).journal).toMatchObject({status:'posted',source_type:'payment'});const version=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.manager.assignmentId).first('version'));expect(await financeProductDocumentEntry(db,{companyId:f.company,branchId:f.manager.branchId,departmentId:f.manager.departmentId,userId:f.manager.userId,assignmentId:f.manager.assignmentId,assignmentVersion:version,role:f.manager.role},{kind:'payment',id:String(payment.id)})).toEqual({capability:'finance.service-invoice',purchasePurpose:'history'});await expect(bankWorkspace(f.owner,{})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['service']);
 const observed=await serviceSettlement();expect(observed.observations.find(row=>row.moduleId==='finance')?.outstanding).toBe(0);expect(observed.total).toBeGreaterThan(0);const agreementId=String(await db.prepare('SELECT agreement_id FROM field_service_orders WHERE id=?').bind((await db.prepare('SELECT source_id FROM finance_invoices WHERE id=?').bind(invoice).first('source_id'))).first('agreement_id'));await changeAgreement(f.tie,agreementId,{action:'retire',expectedVersion:2,reason:'Original fulfilled agreement ends after actual cash collection'},key());expect((await serviceSettlement()).total).toBe(0);const withdrawal=await serviceWithdrawal();await serviceWithdrawalReview(withdrawal,true);expect(await confirmPayment(f.payer,String(payment.id),confirm,keyConfirm)).toEqual(settled);expect((await getPayment(f.manager,String(payment.id))).sourcePurchase).toMatchObject({history:true,settleExisting:false});expect(await (await attachmentContent(db,env.H001_FILES,f.manager,evidence.linkId,'original')).object.text()).toBe('Original private Service bank receipt retained under Legal hold');
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.payer.assignmentId).run();await expect(confirmPayment(f.payer,String(payment.id),confirm,keyConfirm)).rejects.toMatchObject({code:'FINANCE_ACCESS_CHANGED'});
});
it('Service cash rejects generic or forged invoice/payment sources and mismatched or retargeted bank evidence',async()=>{
 const {createPayment,approvePayment,releasePayment,confirmPayment,importBankStatement}=await import('@erpx/server-finance/public/finance');enforceService();await serviceGrant('active');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const {invoice,date}=await serviceCashInvoice(),input={partyId:customer,kind:'receipt' as const,paymentNumber:key(),paymentDate:date,currency:'THB',amount:'1500',bankAccount:f.accounts.cash,settlementInvoiceId:invoice};for(const settlementInvoiceId of [undefined,'missing-service-invoice'])await expect(createPayment(f.reviewer,{...input,settlementInvoiceId},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const payment=await createPayment(f.reviewer,input,key());await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:'Original independent invoice review'},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:'Original independent beneficiary review'},key());const bank={settlementPaymentId:String(payment.id),bankAccount:f.accounts.cash,statementNumber:key(),statementDate:date,opening:'0',closing:'1500',sourceHash:key(),rows:[{externalId:key(),transactionDate:date,amount:'1500',currency:'THB'}]};await expect(importBankStatement(f.reviewer,{...bank,rows:[{...bank.rows[0],amount:'1499'}]},key())).rejects.toMatchObject({code:'SERVICE_PAYMENT_BANK_SOURCE_INVALID'});await expect(importBankStatement(f.reviewer,{...bank,settlementPaymentId:'not-an-actual-service-receipt'},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(confirmPayment(f.payer,String(payment.id),{expectedVersion:3,periodId:f.period,bankTransactionId:'unrelated-bank-row',bankVersion:1,bankReference:'NO-RETARGET',proof:'A generic matching bank label must not become Service evidence',allocations:[{invoiceId:invoice,amount:'1500'}]},key())).rejects.toMatchObject({code:'SERVICE_PAYMENT_BANK_SOURCE_INVALID'});expect(await db.prepare('SELECT status,version FROM finance_payments WHERE id=?').bind(payment.id).first()).toEqual({status:'released',version:3});
});
it('Service cash final source and current Treasury authority reject deferred changes and retain exact receipt retry',async()=>{
 const {createPayment,approvePayment}=await import('@erpx/server-finance/public/finance');const foreign=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Another real same-Company customer'},key())).id);enforceService();await serviceGrant('active');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const {invoice,date}=await serviceCashInvoice(),payment=await createPayment(f.reviewer,{partyId:customer,kind:'receipt',paymentNumber:key(),paymentDate:date,currency:'THB',amount:'1500',bankAccount:f.accounts.cash,settlementInvoiceId:invoice},key()),command=key(),input={expectedVersion:1,proof:'Original independent review must survive the final owner fence'},fault=`service_cash_final_${key().replaceAll('-','')}`;
 for(const kind of ['source','authority'] as const){const before=await db.prepare('SELECT * FROM finance_payments WHERE id=?').bind(payment.id).first(),authority=await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.manager.assignmentId).first();await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${kind==='source'?`UPDATE finance_payments SET party_id='${foreign}' WHERE id='${payment.id}';`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}';`}RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try{await expect(approvePayment(f.manager,String(payment.id),input,command)).rejects.toMatchObject({code:kind==='source'?'SERVICE_PAYMENT_SOURCE_CHANGED':'FINANCE_ACCESS_CHANGED'});expect(await db.prepare('SELECT * FROM finance_payments WHERE id=?').bind(payment.id).first()).toEqual(before);expect(await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.manager.assignmentId).first()).toEqual(authority);for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}()`);}
 }
 const result=await approvePayment(f.manager,String(payment.id),input,command);await serviceGrant('draining');expect(await approvePayment(f.manager,String(payment.id),input,command)).toEqual(result);expect(await db.prepare('SELECT count(*)::int count FROM command_receipts WHERE command_id=?').bind(command).first('count')).toBe(1);
});

const serviceSettlementInput=(packageId='service')=>({companyId:f.company,packageId,packageVersion:1});
async function serviceSettlement(packageId='service'){const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');return withOwnerPackageObligations(db,serviceSettlementInput(packageId),tx=>readOwnerPackageObligations(db,tx,{...serviceSettlementInput(packageId),now:Date.now()}));}
async function serviceWithdrawal(packageId='service'){const {packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId===packageId)!;return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Original Service obligations actually discharged and reviewed',evidence:'test:service-owner-native-settlement'}}]};}
async function serviceWithdrawalReview(request:Awaited<ReturnType<typeof serviceWithdrawal>>,apply=false,after?:()=>Promise<void>){const {withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-Service-lifecycle-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});return withOwnerPackageObligations(db,serviceSettlementInput(request.changes[0].grant.packageId),async tx=>{const result=apply?await store.batch.applyIn(tx,request,async()=>{}):await store.batch.previewIn(tx,request);if(after){serviceDatabase=tx;try{await after();}finally{serviceDatabase=undefined;}}return result;});}
it('Service v1 withdrawal uses five raw owners and preserves cancelled visit held bytes, original history and exact replay',async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service');enableAttachmentV2ForTest();enforceService();await serviceGrant('active');await serviceGrant('active','service-parts-assets');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const id=await serviceOrder(),agreementId=String(await db.prepare('SELECT agreement_id FROM field_service_orders WHERE id=?').bind(id).first('agreement_id')),document=await seedAttachmentSet(db,f.owner,{targetType:'field_service_visit',targetId:id,componentKind:'record',purposeKey:'site_photo',expectedTargetVersion:1});await bindRecordAttachmentSet(db,f.owner,document.setId,1,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Original private Service visit photo held independently',{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(document.sourceId).run();await serviceGrant('draining');await serviceGrant('draining','service-parts-assets');const observation=await serviceSettlement();expect(observation.observations.map(row=>row.moduleId)).toEqual(['field_service','support','inventory','assets','finance']);expect(observation.total).toBeGreaterThan(0);expect(JSON.stringify(observation)).not.toMatch(/Customer plant|Service visit photo|source_id|currency|customer_id/);const request=await serviceWithdrawal();await expect(serviceWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(serviceWithdrawalReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 // Fully unrelated base work cannot block an addon with no asset/parts duty.
 expect((await serviceSettlement('service-parts-assets')).total).toBe(0);const addon=await serviceWithdrawal('service-parts-assets');await serviceWithdrawalReview(addon,true);
 const cancel={action:'cancel',expectedVersion:2,reason:'Customer cancelled the retained original visit'},cancelKey=key(),cancelled=await changeServiceOrder(f.owner,id,cancel,cancelKey),retire={action:'retire' as const,expectedVersion:2,reason:'Original provider explicitly ends the active Service agreement'},retireKey=key(),retired=await changeAgreement(f.tie,agreementId,retire,retireKey);expect((await serviceSettlement()).total).toBe(0);const final=await serviceWithdrawal(),before=(await db.prepare('SELECT row_to_json(s)::text row FROM field_service_order_events s WHERE order_id=? ORDER BY version').bind(id).all()).results;expect((await serviceWithdrawalReview(final)).after.find(row=>row.packageId==='service')?.state).toBe('inactive');const applied=await serviceWithdrawalReview(final,true);expect(await serviceWithdrawalReview(final,true)).toEqual(applied);expect(await changeServiceOrder(f.owner,id,cancel,cancelKey)).toEqual(cancelled);expect(await changeAgreement(f.tie,agreementId,retire,retireKey)).toEqual(retired);expect((await fieldServiceDetail(f.owner,id)).order.status).toBe('cancelled');expect(await (await attachmentContent(db,env.H001_FILES,f.owner,document.linkId,'original')).object.text()).toBe('Original private Service visit photo held independently');expect((await db.prepare('SELECT row_to_json(s)::text row FROM field_service_order_events s WHERE order_id=? ORDER BY version').bind(id).all()).results).toEqual(before);await expect(createAgreement(f.owner,{branchId:f.branch,departmentId:null,customerId:customer,title:'Inactive cannot accept new contractual obligation',startDate:'2026-01-01',endDate:'2027-12-31',currency:'THB',visitRateMinor:150000},key())).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.owner.assignmentId).run();await expect(changeServiceOrder(f.owner,id,cancel,cancelKey)).rejects.toMatchObject({code:'WORK_ROLE_UNAVAILABLE'});expect((await serviceSettlement()).total).toBe(0);
});
it('Service raw withdrawal rejects unregistered snapshots and rolls late original owner intake back after normal Apply',async()=>{
 const {servicePackageObligations}=await import('@erpx/server-field_service/public/package-obligations');await expect(db.transaction(tx=>servicePackageObligations(db,tx,{...serviceSettlementInput(),moduleId:'field_service',now:Date.now()}))).rejects.toThrow('SERVICE_PACKAGE_OBLIGATION_SOURCE_REQUIRED');enforceService();await serviceGrant('active');await serviceGrant('draining');expect((await serviceSettlement()).total).toBe(0);const request=await serviceWithdrawal(),command=key(),input={branchId:f.branch,departmentId:null,customerId:customer,title:'Late original legacy acceptance must not escape final settlement review',startDate:'2026-01-01',endDate:'2027-12-31',currency:'THB',visitRateMinor:150000};await expect(serviceWithdrawalReview(request,true,async()=>{vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','disabled');try{await createAgreement(f.owner,input,command);}finally{vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','enforced');}})).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect((await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='service')).toMatchObject({state:'draining',version:2});expect(await db.prepare('SELECT id FROM field_service_agreements WHERE id=?').bind(`service-agreement-${command}`).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect((await serviceSettlement()).total).toBe(0);await serviceWithdrawalReview(request,true);
});
it('Service parts addon discharges only actual reservation and reviewed cost duties without closing unrelated live base work',async()=>{
 const {journalDetail}=await import('@erpx/server-finance/public/ledger');enforceService();await serviceGrant('active');await serviceGrant('active','service-parts-assets');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const id=await serviceOrder();await act(f.claimant,id,{action:'reserve-part',expectedVersion:1,warehouseId:warehouse,productId:product,quantityMilli:1000,expiresAt:Date.now()+3600000,reason:'Original spare obligation before addon withdrawal'});await serviceGrant('draining','service-parts-assets');expect((await serviceSettlement('service-parts-assets')).observations.find(row=>row.moduleId==='inventory')?.outstanding).toBeGreaterThan(0);await expect(serviceWithdrawalReview(await serviceWithdrawal('service-parts-assets'),true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await act(f.claimant,id,{action:'dispatch',expectedVersion:2,reason:'Original retained work dispatch'});await act(f.claimant,id,{action:'start',expectedVersion:3,reason:'Original customer access to retained spare job'});const reservation=String(await db.prepare('SELECT id FROM field_service_part_reservations WHERE order_id=?').bind(id).first('id'));await act(f.claimant,id,{action:'issue-part',expectedVersion:4,reservationId:reservation,quantityMilli:1000,reason:'Exact original reserved spare consumed while draining'});expect((await serviceSettlement('service-parts-assets')).observations.find(row=>row.moduleId==='finance')?.outstanding).toBeGreaterThan(0);const journal=String((await fieldServiceDetail(f.owner,id)).consumptions[0].journal_id),flow=await resolveFlow(db,f.owner,'journal',journal);await assignFlowPrimary(db,f.owner,'journal',journal,{stepKey:'draft',userId:f.manager.userId,reason:'Original independent reviewed Service stock cost',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postJournal(f.manager,journal,key(),{expectedVersion:1});expect((await serviceSettlement('service-parts-assets')).total).toBe(0);expect((await serviceSettlement()).total).toBeGreaterThan(0);const request=await serviceWithdrawal('service-parts-assets');await serviceWithdrawalReview(request,true);expect((await journalDetail(f.manager,journal)).journal).toMatchObject({status:'posted'});expect((await fieldServiceDetail(f.owner,id)).order).toMatchObject({status:'in_visit',version:5});await act(f.claimant,id,{action:'complete',expectedVersion:5,laborMinutes:20,evidence:'Completed original base visit after actual spare duties ended',customerConfirmation:'Customer accepted original retained base work',reason:'Ending software addon does not force unrelated base work cancellation'});expect((await fieldServiceDetail(f.owner,id)).order.status).toBe('completed');expect((await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='service')).toMatchObject({state:'active'});expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:0,value_minor:0});
});
it('Service withdrawal takes source leases before its first snapshot and sees a concurrent original agreement draft',async()=>{
 enforceService();await serviceGrant('active');await serviceGrant('draining');const command=key(),input={branchId:f.branch,departmentId:null,customerId:customer,title:'Concurrent original legacy contractual acceptance',startDate:'2026-01-01',endDate:'2027-12-31',currency:'THB',visitRateMinor:150000};let ready:()=>void=()=>{},release:()=>void=()=>{},failed:(error:unknown)=>void=()=>{};const acquired=new Promise<void>((resolve,reject)=>{ready=resolve;failed=reject;}),finish=new Promise<void>(resolve=>{release=resolve;});const writer=db.transaction(async tx=>{serviceDatabase=tx;vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','disabled');try{await createAgreement(f.owner,input,command);}finally{serviceDatabase=undefined;vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','enforced');}ready();await finish;}).catch(error=>{failed(error);throw error;});try{await acquired;let complete=false;const observed=serviceSettlement().finally(()=>{complete=true;});await new Promise(resolve=>setTimeout(resolve,100));expect(complete).toBe(false);release();await writer;expect((await observed).observations.find(row=>row.moduleId==='field_service')?.outstanding).toBeGreaterThan(0);}finally{release();serviceDatabase=undefined;await writer.catch(()=>{});}expect(await db.prepare('SELECT status FROM field_service_agreements WHERE id=?').bind(`service-agreement-${command}`).first('status')).toBe('draft');
});

it('filters actual Service Records/search, Inbox and Staff sources before their limits and rechecks selected context and owner references',async()=>{
 const {getRecord,recordsPage,searchReferences}=await import('@erpx/server-records/public/records'),{publishInboxEvent,inboxPage}=await import('@erpx/server-collaboration/public/inbox'),{listStaffNotifications,notificationsPage}=await import('@erpx/server-collaboration/public/staff-notifications'),{enrichFlowWork}=await import('@erpx/server-workflow/public/flow-work');
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);for(const actor of [f.owner,f.tie,f.claimant])actor.modules=['field_service'];
 const valid=await serviceOrder(),privateIds:string[]=[],generalAgreement=String(await db.prepare('SELECT agreement_id FROM field_service_orders WHERE id=?').bind(valid).first('agreement_id'));
 // The original owner allows a general agreement to name the concrete asset
 // on an individual visit; source enforcement retains that valid relationship.
 const specific=await createServiceOrder(f.owner,{ticketId:ticket,ticketVersion:1,agreementId:generalAgreement,agreementVersion:2,assetId:warrantyAsset,issue:'Inspect the concrete asset under the original general agreement',site:'Customer plant A',technicianId:f.claimant.userId,scheduledAt:Date.now()+60000},key());privateIds.push(String(specific.id));
 for(let index=0;index<13;index++)privateIds.push(await serviceOrder({assetId:warrantyAsset}));const now=Date.now();
 for(const [index,id] of [valid,...privateIds].entries())await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,created_by,updated_by,created_at,updated_at) VALUES('field_service',?,'service-orders','Service projection private source','','draft','[]',?,?,?,?,?,?)").bind(id,f.company,f.branch,f.owner.userId,f.owner.userId,now+index,now+index).run();
 for(let index=0;index<106;index++){const id=index===0?valid:index%2?privateIds[0]:key(),message=index===0?'Actual base Service visit':'Private or missing Service source must not consume the cursor';await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'field_service_order',?,?,?,?)").bind(key(),f.company,f.owner.userId,id,index+100,message,now+index).run();await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:'field_service_order',sourceId:id,sourceVersion:index+100,category:'work',preview:message,recipientIds:[f.owner.userId]});}
 const staleWork=await fieldServiceWork(f.owner);enforceService();
 expect((await recordsPage(f.owner,'field_service','service-orders',{})).items).toEqual([]);expect((await searchReferences(f.owner,'Service projection private source')).flatMap(row=>'id' in row?[row.id]:[])).toEqual([]);await expect(getRecord(f.owner,'field_service',valid)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect((await listStaffNotifications(db,f.owner)).filter(row=>row.record_type==='field_service_order')).toEqual([]);expect((await enrichFlowWork(f.owner,staleWork)).filter(row=>row.type==='field_service')).toEqual([]);
 await serviceGrant('active');await serviceGrant('draining');
 expect((await recordsPage(f.owner,'field_service','service-orders',{})).items.map(row=>row.id)).toEqual([valid]);expect((await searchReferences(f.owner,'Service projection private source')).flatMap(row=>'id' in row?[row.id]:[])).toEqual([valid]);expect((await getRecord(f.owner,'field_service',valid)).id).toBe(valid);await expect(getRecord(f.owner,'field_service',privateIds[0])).rejects.toMatchObject({status:404});
 const notices=await listStaffNotifications(db,f.owner);expect(notices.filter(row=>row.record_type==='field_service_order').map(row=>row.record_id)).toEqual(expect.arrayContaining([valid]));expect(notices.some(row=>row.message.startsWith('Private or missing'))).toBe(false);expect((await notificationsPage(db,f.owner,{view:'all'})).items.some(row=>row.record_id===valid)).toBe(true);const inbox=await inboxPage(db,f.owner,{view:'all'});expect(inbox.items.some(row=>row.sourceId===valid)).toBe(true);expect(inbox.items.some(row=>row.message.startsWith('Private or missing'))).toBe(false);
 expect((await enrichFlowWork(f.owner,staleWork)).filter(row=>row.type==='field_service').every(row=>!privateIds.includes(row.id))).toBe(true);
 await serviceGrant('active');await serviceGrant('active','service-parts-assets');await serviceGrant('draining','service-parts-assets');await serviceGrant('draining');expect((await getRecord(f.owner,'field_service',privateIds[0])).id).toBe(privateIds[0]);
 const assignmentVersion=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first('version')),session=`service-read-${key()}`;await db.prepare('INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,revision,assignment_version,updated_at,expires_at) VALUES(?,?,?,?,1,?,?,?)').bind(session,f.owner.userId,f.company,f.owner.assignmentId,assignmentVersion,Date.now(),Date.now()+60000).run();const captured={...f.owner,authority:{sessionId:session,revision:1,assignmentVersion}};
 expect((await listStaffNotifications(db,captured)).some(row=>row.record_id===valid)).toBe(true);await db.prepare('UPDATE active_work_contexts SET revision=revision+1 WHERE session_id=?').bind(session).run();expect((await listStaffNotifications(db,captured)).filter(row=>row.record_type==='field_service_order')).toEqual([]);expect((await inboxPage(db,captured,{view:'all'})).items.filter(row=>row.sourceType==='field_service_order')).toEqual([]);expect((await enrichFlowWork(captured,staleWork)).filter(row=>row.type==='field_service')).toEqual([]);
 // The original immutable visit source independently prevents retargeting;
 // this is retained original protection, not a new projection witness.
 await expect(db.prepare('UPDATE field_service_orders SET agreement_id=? WHERE id=?').bind(String(await db.prepare('SELECT agreement_id FROM field_service_orders WHERE id=?').bind(privateIds[1]).first('agreement_id')),valid).run()).rejects.toThrow('FIELD_SERVICE_ORDER_SOURCE_IMMUTABLE');expect((await getRecord(f.owner,'field_service',valid)).id).toBe(valid);
 // A genuinely changed current customer reference must still fail closed.
 await db.prepare("UPDATE finance_parties SET kind='supplier' WHERE id=?").bind(customer).run();await expect(getRecord(f.owner,'field_service',valid)).rejects.toMatchObject({status:404});expect((await listStaffNotifications(db,f.owner)).some(row=>row.record_id===valid)).toBe(false);
});
it('keeps Service metric catalogues, current source drilldowns and visit evidence inside the purchased original Company and optional asset scope',async()=>{
 const {metricCatalogFor}=await import('@erpx/server-runtime/public/executive/metrics'),{readExecutiveMetricProjection}=await import('@erpx/server-field_service/public/executive-metric-projections');
 for(const actor of [f.owner,f.tie,f.claimant])actor.modules=['field_service'];const valid=await serviceOrder(),privateId=await serviceOrder({assetId:warrantyAsset});await act(f.claimant,valid,{action:'dispatch',expectedVersion:1,reason:'Original base source visit dispatch'});await act(f.claimant,valid,{action:'start',expectedVersion:2,reason:'Original customer source access'});await act(f.claimant,valid,{action:'complete',expectedVersion:3,laborMinutes:20,evidence:'Original base customer service completed',customerConfirmation:'Customer accepted retained service',reason:'Original Company work completion'});
 const day=new Date().toISOString().slice(0,10),filters={from:day,to:day,currency:'THB'};enforceService();expect((await metricCatalogFor(db,f.owner)).find(row=>row.id==='field-visit-completion')?.available).toBe(false);await expect(queryMetric(db,f.owner,'field-visit-completion',filters)).rejects.toMatchObject({code:'METRIC_SOURCE_UNAVAILABLE'});
 await serviceGrant('active');await serviceGrant('draining');expect((await metricCatalogFor(db,f.owner)).find(row=>row.id==='field-visit-completion')?.available).toBe(true);expect((await queryMetric(db,f.owner,'field-visit-completion',filters)).total).toBe('10000');expect((await searchExecutiveEvidence(db,f.owner,{metric:'field-visit-completion',filters,limit:1})).items.map(row=>row.id)).toEqual([valid]);await expect(readExecutiveEvidence(db,f.owner,'field-visit-completion',privateId,filters)).rejects.toMatchObject({status:404});
 const projection=await readExecutiveMetricProjection(db,f.owner,'field-visit-completion',filters,'default');expect((await projection.source({label:'Field visits',page:1},'10000')).rows.some(row=>row.id===privateId)).toBe(false);
 await serviceGrant('active');await serviceGrant('active','service-parts-assets');await serviceGrant('draining','service-parts-assets');await serviceGrant('draining');expect((await queryMetric(db,f.owner,'field-visit-completion',filters)).total).toBe('5000');const evidence=await searchExecutiveEvidence(db,f.owner,{metric:'field-visit-completion',filters,limit:1});expect(evidence.items).toHaveLength(1);expect(evidence.hasMore).toBe(true);
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.owner.assignmentId).run();expect((await metricCatalogFor(db,f.owner)).find(row=>row.id==='field-visit-completion')?.available).toBe(false);await expect(projection.source({label:'Field visits',page:1},'10000')).rejects.toMatchObject({code:'METRIC_SOURCE_UNAVAILABLE'});await expect(readExecutiveEvidence(db,f.owner,'field-visit-completion',valid,filters)).rejects.toMatchObject({status:403});
});


it('Service private byte read consumes lazy bytes inside its fresh actual source unit and rolls access audits back after late customer or selected IAM changes',async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{attachmentContent}=await import('@erpx/server-files/public/attachments/service');enableAttachmentV2ForTest();const id=await serviceOrder(),file=await seedAttachmentSet(db,f.claimant,{targetType:'field_service_visit',targetId:id,componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:1}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),body='Exact private technician evidence under independent Legal hold';await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();enforceService();await serviceGrant('active');await serviceGrant('draining');let consumed=0;const files:import("@erpx/server-runtime/public/runtime/types").FileStore={get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;},put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
 expect(await(await attachmentContent(db,files,f.claimant,file.linkId,'original')).object.text()).toBe(body);expect(consumed).toBe(1);await expect(db.transaction(tx=>attachmentContent(tx,files,f.claimant,file.linkId,'original'))).rejects.toMatchObject({code:'SERVICE_FILE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(1);
 for(const fault of ['customer','iam'] as const){const trigger=`service_byte_${key().replaceAll('-','')}`,effect=fault==='customer'?`UPDATE finance_parties SET status='blocked' WHERE id='${customer}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{await expect(attachmentContent(db,files,f.claimant,file.linkId,'original')).rejects.toMatchObject({code:fault==='customer'?'SERVICE_FILE_READ_CHANGED':'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(await db.prepare('SELECT status FROM finance_parties WHERE id=?').bind(customer).first('status')).toBe('active');expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first()).toEqual({revoked_at:null,version:1});expect(Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(file.linkId).first('n'))).toBe(1);}
 expect(consumed).toBe(3);expect(await(await attachmentContent(db,files,f.claimant,file.linkId,'original')).object.text()).toBe(body);expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});
 vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','disabled');await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.claimant.assignmentId).run();await expect(attachmentContent(db,files,f.claimant,file.linkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(consumed).toBe(4);expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied'").bind(file.linkId).first('n'))).toBe(1);
});
it('Service private byte leases observe a committed original withdrawn-link writer before the first owned metadata snapshot without retrying or opening the blob',async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{attachmentContent}=await import('@erpx/server-files/public/attachments/service'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility');enableAttachmentV2ForTest();const id=await serviceOrder(),file=await seedAttachmentSet(db,f.claimant,{targetType:'field_service_visit',targetId:id,componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:1});let opened=0;const files:import("@erpx/server-runtime/public/runtime/types").FileStore={get:async()=>{opened++;return null;},put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
 const result=await measureReads(()=>committedSourceVisibility(db,tx=>tx.prepare("UPDATE attachment_links SET state='withdrawn',withdrawn_by=created_by,withdrawn_at=1,withdrawal_reason='Actual committed original evidence withdrawal' WHERE id=? AND state='draft'").bind(file.linkId).run(),async()=>{try{await attachmentContent(db,files,f.claimant,file.linkId,'original');return null;}catch(error){return error;}}));expect(result.result).toMatchObject({status:404,code:'ATTACHMENT_LINK_NOT_FOUND'});expect(opened).toBe(0);expect(result.metrics.deadlockFailures??0).toBe(0);expect(result.metrics.serializationFailures??0).toBe(0);expect(result.metrics.transactionRetries??0).toBe(0);expect((await db.prepare('SELECT outcome,reason FROM attachment_access_events WHERE link_id=?').bind(file.linkId).all()).results).toEqual([{outcome:'denied',reason:'ATTACHMENT_LINK_NOT_FOUND'}]);
});
it('productSourceHandoff enters the actual Service-only Invoice on Finance while all legacy enforcement modes remain disabled',async()=>{
 const {financeProductDocumentEntry}=await import('@erpx/server-finance/public/document-purchases'),{financeDocumentPresentationAccess,financeSourceDocumentBackdrop}=await import('@erpx/server-finance/public/record-presentation');const {invoice}=await serviceCashInvoice(),scope={companyId:f.company,branchId:f.manager.branchId,departmentId:f.manager.departmentId,userId:f.manager.userId,assignmentId:f.manager.assignmentId,assignmentVersion:Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.manager.assignmentId).first('version')),role:f.manager.role};
 expect(await financeProductDocumentEntry(db,scope,{kind:'invoice',id:invoice})).toBeNull();expect(await financeDocumentPresentationAccess(f.manager,'invoice',invoice)).toEqual({sourceOnly:false,historyOnly:false});await expect(financeDocumentPresentationAccess(f.manager,'invoice',invoice,{strictProduct:true})).rejects.toMatchObject({code:'FINANCE_RECORD_NOT_FOUND'});await serviceGrant('active');
 for(const mode of ['disabled','shadow','enforced'] as const){vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT',mode);expect(await financeDocumentPresentationAccess(f.manager,'invoice',invoice,{strictProduct:true})).toEqual({sourceOnly:true,historyOnly:true});expect(await financeSourceDocumentBackdrop(f.manager,'invoice',invoice,{strictProduct:true})).toEqual({kind:'invoice',id:invoice,number:expect.any(String),date:expect.any(String),status:'posted',version:expect.any(Number)});}
 expect(await financeProductDocumentEntry(db,scope,{kind:'invoice',id:invoice})).toEqual({capability:'finance.service-invoice',purchasePurpose:'history'});expect(await financeProductDocumentEntry(db,{...scope,assignmentVersion:scope.assignmentVersion+1},{kind:'invoice',id:invoice})).toBeNull();expect(await financeProductDocumentEntry(db,{...scope,companyId:'different-company'},{kind:'invoice',id:invoice})).toBeNull();expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['service']);await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.manager.assignmentId).run();await expect(financeSourceDocumentBackdrop(f.manager,'invoice',invoice,{strictProduct:true})).rejects.toMatchObject({code:'FINANCE_ACCESS_CHANGED'});
});

it('Service No-Charge Confirmation closes a real fully warranted source through normal independent review and retained withdrawal history', async()=>{
 const {voidDraftInvoice}=await import('@erpx/server-finance/public/finance');
 enforceService();await serviceGrant('active');await serviceGrant('active','service-parts-assets');
 vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 for(const actor of [f.owner,f.tie,f.claimant])actor.modules=['field_service'];
 const agreementId=await agreement({assetId:warrantyAsset,warrantyDiscountBps:10000}),requestKey=key(),request={agreementId,agreementVersion:2,title:'Actual full warranty no-charge visit',issue:'Inspect the originally covered controller',site:'Covered customer plant',technicianId:f.claimant.userId,scheduledAt:Date.now()+60000};
 const intake=await createServiceRequest(f.owner,request,requestKey),id=String(intake.id);
 expect(await createServiceRequest(f.owner,request,requestKey)).toEqual(intake);
 expect(await db.prepare('SELECT source_kind,order_id,service_order_id FROM support_tickets WHERE id=?').bind(String(intake.ticketId)).first()).toEqual({source_kind:'field_service',order_id:null,service_order_id:id});
 await serviceGrant('draining');await serviceGrant('draining','service-parts-assets');
 await act(f.claimant,id,{action:'dispatch',expectedVersion:1,reason:'Finish the original contracted warranty obligation'});
 await act(f.claimant,id,{action:'start',expectedVersion:2,reason:'Actual customer granted site access'});
 await act(f.claimant,id,{action:'complete',expectedVersion:3,laborMinutes:30,evidence:'Completed real covered visit',customerConfirmation:'Customer accepted the covered inspection',reason:'Completed original full warranty work'});
 const releaseKey=key(),releaseBody={action:'release-billing',expectedVersion:4,reason:'Independent original reviewer confirmed full warranty coverage'};
 const released=await changeServiceOrder(f.tie,id,releaseBody,releaseKey),invoice=`field-invoice-${releaseKey}`;
 expect(await changeServiceOrder(f.tie,id,releaseBody,releaseKey)).toEqual(released);
 expect(await db.prepare('SELECT company_id,status,version,total_minor,outstanding_minor,journal_id,source_id FROM finance_invoices WHERE id=?').bind(invoice).first()).toEqual({company_id:f.company,status:'draft',version:1,total_minor:0,outstanding_minor:0,journal_id:null,source_id:id});
 expect(await db.prepare("SELECT amount_units::text,usd_units::text FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind='draft:v1'").bind(f.company,invoice).first()).toEqual({amount_units:'0',usd_units:'0'});
 const date=new Date().toISOString().slice(0,10),flow=await resolveFlow(db,f.owner,'invoice',invoice);
 await assignFlowPrimary(db,f.owner,'invoice',invoice,{stepKey:'draft',userId:f.manager.userId,reason:'Independent actual Finance review of captured zero demand',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 const image=()=>db.prepare("SELECT jsonb_build_object('invoice',(SELECT to_jsonb(i) FROM finance_invoices i WHERE i.id=?),'release',(SELECT to_jsonb(r) FROM field_service_billing_releases r WHERE r.id=?),'money',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.source_type,m.amount_kind) FROM money_transaction_snapshots m WHERE m.company_id=? AND m.source_id IN (?,?)))::text image").bind(invoice,`field-billing-${releaseKey}`,f.company,invoice,`field-billing-${releaseKey}`).first('image');
 const before=await image(),postKey=key();
 await expect(postInvoice(f.manager,invoice,{periodId:f.period,entryDate:date,expectedVersion:1},postKey)).rejects.toMatchObject({status:422,code:'INVALID_JOURNAL_LINE'});
 expect(await image()).toEqual(before);
 // Original generic void's source restriction must remain. Disable only the
 // isolated test switch to distinguish native source refusal from a missing
 // generic Finance purchase; this does not grant a terminal Service workflow.
 const voidKey=key();vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');
 try{await expect(voidDraftInvoice(f.manager,invoice,1,voidKey)).rejects.toMatchObject({code:'CONFLICT'});}finally{vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');}
 expect(await image()).toEqual(before);
 expect(await db.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id IN (?,?)').bind(postKey,voidKey).first('n')).toBe(0);
 expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE source_type='invoice' AND source_id=?").bind(invoice).first('n')).toBe(0);
 expect(await db.prepare('SELECT count(*)::int n FROM finance_allocations WHERE invoice_id=?').bind(invoice).first('n')).toBe(0);
 await changeAgreement(f.tie,agreementId,{action:'retire',expectedVersion:2,reason:'Original service agreement has no remaining contracted visit'},key());
 const observed=await serviceSettlement();expect(observed.observations.map(row=>row.moduleId)).toEqual(['field_service','support','inventory','assets','finance']);
 expect(observed.observations.filter(row=>row.moduleId!=='finance').every(row=>row.outstanding===0)).toBe(true);
 expect(observed.observations.find(row=>row.moduleId==='finance')?.outstanding).toBeGreaterThan(0);
 const withdrawal=await serviceWithdrawal();await expect(serviceWithdrawalReview(withdrawal)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 expect(await image()).toEqual(before);
 // RED: all original owner actions leave a genuine covered visit stranded.
 // A new explicit nonmonetary review must discharge this one source, without
 // creating a payment/journal or weakening other owner obligations.
 const {confirmServiceNoCharge,serviceNoChargeControls}=await import('@erpx/server-finance/public/service-no-charge');
 const controls=await serviceNoChargeControls(f.manager,invoice);expect(controls).toMatchObject({invoiceId:invoice,version:1,canConfirm:true,confirmed:false});
 const {withPackageSettlementSources}=await import('@erpx/server-commercial/public/package-settlement-sources'),{financePackageObligations,lockFinancePackageObligationSources}=await import('@erpx/server-finance/public/package-obligations'),rawFinance=()=>withPackageSettlementSources(db,f.company,[lock=>lockFinancePackageObligationSources(db,lock)],tx=>financePackageObligations(db,tx,{companyId:f.company,packageId:'finance',packageVersion:1,moduleId:'finance',now:Date.now()})),financeBefore=await rawFinance();
 const confirmationKey=key(),confirmationBody={expectedVersion:1,contextKey:controls!.contextKey,reason:'Independent Finance confirmed original full warranty creates no customer monetary demand'};
 const confirmed=await confirmServiceNoCharge(f.manager,invoice,confirmationBody,confirmationKey);expect(confirmed).toEqual({id:invoice,status:'no_charge_confirmed',version:2,confirmationId:`service-no-charge-${confirmationKey}`});
 expect(await confirmServiceNoCharge(f.manager,invoice,confirmationBody,confirmationKey)).toEqual(confirmed);
 expect(await db.prepare('SELECT status,version,total_minor,outstanding_minor,journal_id,posted_at FROM finance_invoices WHERE id=?').bind(invoice).first()).toEqual({status:'no_charge_confirmed',version:2,total_minor:0,outstanding_minor:0,journal_id:null,posted_at:null});
 const after=JSON.parse(String(await image())),original=JSON.parse(String(before));expect(after.release).toEqual(original.release);expect(after.money).toEqual(original.money);
 expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE source_type='invoice' AND source_id=?").bind(invoice).first('n')).toBe(0);expect(await db.prepare('SELECT count(*)::int n FROM finance_allocations WHERE invoice_id=?').bind(invoice).first('n')).toBe(0);
 expect((await rawFinance()).outstanding).toBe(financeBefore.outstanding-1);expect((await serviceSettlement()).total).toBe(0);expect(await resolveFlow(db,f.manager,'invoice',invoice)).toMatchObject({state:'no_charge_confirmed',current:null,canAct:false});
 const addon=await serviceWithdrawal('service-parts-assets');await serviceWithdrawalReview(addon,true);const final=await serviceWithdrawal();expect((await serviceWithdrawalReview(final)).after.find(row=>row.packageId==='service')?.state).toBe('inactive');const applied=await serviceWithdrawalReview(final,true);expect(await serviceWithdrawalReview(final,true)).toEqual(applied);
 expect(await confirmServiceNoCharge(f.manager,invoice,confirmationBody,confirmationKey)).toEqual(confirmed);
 expect((await serviceNoChargeControls(f.manager,invoice))?.confirmed).toBe(true);
 await expect(confirmServiceNoCharge(f.manager,invoice,{...confirmationBody,reason:'A different receipt body cannot replace the original review'},confirmationKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['service','service-parts-assets']);
});

async function noChargeReviewSource(discountBps=10000){
 enforceService();await serviceGrant('active');await serviceGrant('active','service-parts-assets');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const agreementId=await agreement({assetId:warrantyAsset,warrantyDiscountBps:discountBps}),visit=await createServiceRequest(f.owner,{agreementId,agreementVersion:2,title:'Original no-charge review boundary',issue:'Inspect actual warranty coverage',site:'Actual covered site',technicianId:f.claimant.userId,scheduledAt:Date.now()+60000},key()),id=String(visit.id);
 await act(f.claimant,id,{action:'dispatch',expectedVersion:1,reason:'Original current contracted visit'});await act(f.claimant,id,{action:'start',expectedVersion:2,reason:'Actual permitted customer site access'});await act(f.claimant,id,{action:'complete',expectedVersion:3,laborMinutes:35,evidence:'Original completed warranty inspection',customerConfirmation:'Original customer accepted completed inspection',reason:'Completed actual retained contractual visit'});const releaseKey=key();await changeServiceOrder(f.tie,id,{action:'release-billing',expectedVersion:4,reason:'Independent original billing release review'},releaseKey);const invoice=`field-invoice-${releaseKey}`,flow=await resolveFlow(db,f.owner,'invoice',invoice);
 await assignFlowPrimary(db,f.owner,'invoice',invoice,{stepKey:'draft',userId:f.manager.userId,reason:'Independent Finance original covered demand review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());return {id,invoice,agreementId,releaseKey};
}
it('Service no-charge confirmation rejects nonzero and partial warranty, stale reviews, foreign Company and original non-independent or unauthorized actors',async()=>{
 const {confirmServiceNoCharge,serviceNoChargeControls}=await import('@erpx/server-finance/public/service-no-charge'),actual=await noChargeReviewSource(),controls=await serviceNoChargeControls(f.manager,actual.invoice);expect(controls?.canConfirm).toBe(true);const input={expectedVersion:1,contextKey:controls!.contextKey,reason:'Independent actual original contractual zero-demand review'};
 await expect(confirmServiceNoCharge(f.manager,actual.invoice,{...input,expectedVersion:2},key())).rejects.toMatchObject({code:'SERVICE_NO_CHARGE_CONTEXT_CHANGED'});await expect(confirmServiceNoCharge(f.manager,actual.invoice,{...input,contextKey:'f'.repeat(64)},key())).rejects.toMatchObject({code:'SERVICE_NO_CHARGE_CONTEXT_CHANGED'});
 await expect(serviceNoChargeControls(f.tie,actual.invoice)).rejects.toMatchObject({status:403});await expect(confirmServiceNoCharge(f.tie,actual.invoice,input,key())).rejects.toMatchObject({status:403});await expect(confirmServiceNoCharge(f.reviewer,actual.invoice,input,key())).rejects.toMatchObject({status:403});
 for(const discount of [5000,0]){const monetary=await noChargeReviewSource(discount);expect(await serviceNoChargeControls(f.manager,monetary.invoice)).toBeNull();await expect(confirmServiceNoCharge(f.manager,monetary.invoice,input,key())).rejects.toMatchObject({code:'SERVICE_NO_CHARGE_SOURCE_CHANGED'});expect(await db.prepare('SELECT total_minor,status FROM finance_invoices WHERE id=?').bind(monetary.invoice).first()).toEqual({total_minor:discount===5000?75000:150000,status:'draft'});}
 const tiedRole=f.tie.role,tiedAssignment=f.tie.assignmentId;try{f.tie.role='finance_approver';f.tie.assignmentId=await f.grant(f.tie,'finance_approver');const ownControls=await serviceNoChargeControls(f.tie,actual.invoice);expect(ownControls?.canConfirm).toBe(false);await expect(confirmServiceNoCharge(f.tie,actual.invoice,{...input,contextKey:ownControls!.contextKey},key())).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});}finally{f.tie.role=tiedRole;f.tie.assignmentId=tiedAssignment;}
 const originalFixture=f,foreign=await expenseFixture();try{f=foreign;await serviceGrant('active');}finally{f=originalFixture;}await expect(confirmServiceNoCharge(foreign.manager,actual.invoice,input,key())).rejects.toMatchObject({code:'SERVICE_NO_CHARGE_SOURCE_CHANGED'});expect(await db.prepare('SELECT count(*)::int n FROM finance_service_no_charge_confirmations WHERE invoice_id=?').bind(actual.invoice).first('n')).toBe(0);
});
it('Service no-charge confirmation rolls back actual deferred signoff and current IAM changes, then preserves its immutable decision through lawful later identity linking',async()=>{
 const {confirmServiceNoCharge,serviceNoChargeControls}=await import('@erpx/server-finance/public/service-no-charge'),actual=await noChargeReviewSource(),controls=await serviceNoChargeControls(f.manager,actual.invoice),input={expectedVersion:1,contextKey:controls!.contextKey,reason:'Independent Finance review of the actual zero covered source'},command=key(),beforeInvoice=await db.prepare('SELECT * FROM finance_invoices WHERE id=?').bind(actual.invoice).first(),beforeOrder=await db.prepare('SELECT * FROM field_service_orders WHERE id=?').bind(actual.id).first(),beforeIam=await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.manager.assignmentId).first();
 for(const fault of ['source','iam'] as const){const trigger=`no_charge_final_${key().replaceAll('-','')}`;await db.exec(`CREATE SEQUENCE ${trigger}_attempt;CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${trigger}_attempt');${fault==='source'?`UPDATE field_service_orders SET customer_confirmation='Contradictory late signoff' WHERE id='${actual.id}';`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}';`}RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${trigger}()`);
  try{await expect(confirmServiceNoCharge(f.manager,actual.invoice,input,command)).rejects.toMatchObject(fault==='source'?{status:409,code:'SERVICE_NO_CHARGE_PROOF_REQUIRED'}:{status:403,code:'FINANCE_ACCESS_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${trigger}_attempt`).first('is_called')).toBe(true);expect(await db.prepare('SELECT * FROM finance_invoices WHERE id=?').bind(actual.invoice).first()).toEqual(beforeInvoice);expect(await db.prepare('SELECT * FROM field_service_orders WHERE id=?').bind(actual.id).first()).toEqual(beforeOrder);expect(await db.prepare('SELECT * FROM assignments WHERE id=?').bind(f.manager.assignmentId).first()).toEqual(beforeIam);for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM finance_service_no_charge_confirmations WHERE invoice_id=?').bind(actual.invoice).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${trigger} ON outbox_events;DROP FUNCTION ${trigger}();DROP SEQUENCE ${trigger}_attempt`);}
 }
 const confirmed=await confirmServiceNoCharge(f.manager,actual.invoice,input,command);await changeAgreement(f.tie,actual.agreementId,{action:'retire',expectedVersion:2,reason:'Original completed agreement retained after nonmonetary confirmation'},key());
 // Linking identities after an independent decision changes current authority,
 // not the historical fact that the original decision was independent.
 const person=`later-identity-${key()}`;for(const actor of [f.manager,f.tie])await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES(?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=EXCLUDED.person_id").bind(f.company,actor.userId,actor.userId,person).run();expect(await db.prepare('SELECT iam_same_person(?,?,?) same').bind(f.manager.userId,f.tie.userId,f.company).first('same')).toBe(true);expect(await db.prepare('SELECT id FROM finance_service_no_charge_completed WHERE id=?').bind(actual.invoice).first('id')).toBe(actual.invoice);expect(await confirmServiceNoCharge(f.manager,actual.invoice,input,command)).toEqual(confirmed);
 await expect(db.prepare("UPDATE field_service_orders SET completion_evidence='Contradictory post-confirmation evidence' WHERE id=?").bind(actual.id).run()).rejects.toMatchObject({code:'23514'});expect(await db.prepare('SELECT completion_evidence FROM field_service_orders WHERE id=?').bind(actual.id).first('completion_evidence')).toBe(beforeOrder?.completion_evidence);
});
it('Service no-charge confirmation serializes exact competing decisions without hidden transaction retries or zero ledger artifacts',async()=>{
 const {confirmServiceNoCharge,serviceNoChargeControls}=await import('@erpx/server-finance/public/service-no-charge'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),actual=await noChargeReviewSource(),controls=await serviceNoChargeControls(f.manager,actual.invoice),input={expectedVersion:1,contextKey:controls!.contextKey,reason:'Independent actual original zero-demand concurrent decision'},command=key();const measured=await measureReads(()=>Promise.all([confirmServiceNoCharge(f.manager,actual.invoice,input,command),confirmServiceNoCharge(f.manager,actual.invoice,input,command)]));expect(measured.result[0]).toEqual(measured.result[1]);for(const metric of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(measured.metrics[metric]??0).toBe(0);expect(await db.prepare('SELECT count(*)::int n FROM finance_service_no_charge_confirmations WHERE invoice_id=?').bind(actual.invoice).first('n')).toBe(1);await expect(confirmServiceNoCharge(f.manager,actual.invoice,input,key())).rejects.toMatchObject({code:'SERVICE_NO_CHARGE_CONTEXT_CHANGED'});expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE source_type='invoice' AND source_id=?").bind(actual.invoice).first('n')).toBe(0);expect(await db.prepare('SELECT count(*)::int n FROM finance_allocations WHERE invoice_id=?').bind(actual.invoice).first('n')).toBe(0);
});
it('Service no-charge confirmation retains independently posted real spare cost and held visit evidence through draining and inactive history',async()=>{
 const {confirmServiceNoCharge,serviceNoChargeControls}=await import('@erpx/server-finance/public/service-no-charge'),{seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service'),{invoicePaymentOwner}=await import('@erpx/server-finance/public/invoice-payment-evidence');enforceService();await serviceGrant('active');await serviceGrant('active','service-parts-assets');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');enableAttachmentV2ForTest();
 const agreementId=await agreement({assetId:warrantyAsset,warrantyDiscountBps:10000}),visit=await createServiceRequest(f.owner,{agreementId,agreementVersion:2,title:'Covered visit with actual valued spare',issue:'Replace the covered controller',site:'Original customer equipment',technicianId:f.claimant.userId,scheduledAt:Date.now()+60000},key()),id=String(visit.id);
 await act(f.claimant,id,{action:'reserve-part',expectedVersion:1,warehouseId:warehouse,productId:product,quantityMilli:1000,expiresAt:Date.now()+3600000,reason:'Reserve actual original covered spare'});await act(f.claimant,id,{action:'dispatch',expectedVersion:2,reason:'Original covered dispatch'});await act(f.claimant,id,{action:'start',expectedVersion:3,reason:'Customer provided access to actual equipment'});const reservation=String(await db.prepare('SELECT id FROM field_service_part_reservations WHERE order_id=?').bind(id).first('id'));await act(f.claimant,id,{action:'issue-part',expectedVersion:4,reservationId:reservation,quantityMilli:1000,reason:'Actual immutable stock cost belongs to the covered visit'});
 const completion={action:'complete',expectedVersion:5,laborMinutes:45,evidence:'Actual covered replacement tested',customerConfirmation:'Customer accepted the original replacement',reason:'Complete original warranty obligation after independent stock cost'};await expect(act(f.claimant,id,completion)).rejects.toMatchObject({code:'CONFLICT'});const journal=String((await fieldServiceDetail(f.owner,id)).consumptions[0].journal_id),costFlow=await resolveFlow(db,f.owner,'journal',journal);await assignFlowPrimary(db,f.owner,'journal',journal,{stepKey:'draft',userId:f.manager.userId,reason:'Independent review of the real covered spare cost',saveDefault:false,expectedVersion:costFlow.version,recordVersion:costFlow.recordVersion},key());await postJournal(f.manager,journal,key(),{expectedVersion:1});
 await serviceGrant('draining');await serviceGrant('draining','service-parts-assets');const file=await seedAttachmentSet(db,f.claimant,{targetType:'field_service_visit',targetId:id,componentKind:'record',purposeKey:'signed_job_sheet',expectedTargetVersion:5}),fileKey=key();await bindRecordAttachmentSet(db,f.claimant,file.setId,5,fileKey);const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes='Original signed covered visit evidence';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();
 await act(f.claimant,id,{...completion,expectedVersion:6});const releaseKey=key();await changeServiceOrder(f.tie,id,{action:'release-billing',expectedVersion:7,reason:'Independent original covered source release with retained evidence'},releaseKey);const invoice=`field-invoice-${releaseKey}`,flow=await resolveFlow(db,f.owner,'invoice',invoice);await assignFlowPrimary(db,f.owner,'invoice',invoice,{stepKey:'draft',userId:f.manager.userId,reason:'Independent no-charge customer demand review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());const controls=await serviceNoChargeControls(f.manager,invoice),input={expectedVersion:1,contextKey:controls!.contextKey,reason:'Confirmed contractual full warranty with separately posted original spare cost'},command=key(),confirmed=await confirmServiceNoCharge(f.manager,invoice,input,command);
 expect(await db.prepare('SELECT status FROM finance_journals WHERE id=?').bind(journal).first('status')).toBe('posted');expect(await db.prepare('SELECT sum(debit_minor)::int debit,sum(credit_minor)::int credit FROM finance_journal_lines WHERE journal_id=?').bind(journal).first()).toEqual({debit:8000,credit:8000});expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:0,value_minor:0});await expect(invoicePaymentOwner(db,f.manager,invoice,true)).rejects.toMatchObject({status:422,code:'PAYABLE_REQUIRED'});
 await changeAgreement(f.tie,agreementId,{action:'retire',expectedVersion:2,reason:'Original contractual work completed without customer monetary demand'},key());expect((await serviceSettlement()).total).toBe(0);await serviceWithdrawalReview(await serviceWithdrawal('service-parts-assets'),true);await serviceWithdrawalReview(await serviceWithdrawal(),true);expect(await confirmServiceNoCharge(f.manager,invoice,input,command)).toEqual(confirmed);expect(await (await attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).object.text()).toBe(bytes);expect(await db.prepare('SELECT legal_hold FROM attachments WHERE id=?').bind(file.sourceId).first('legal_hold')).toBe(1);
 const late=await seedAttachmentSet(db,f.claimant,{targetType:'field_service_visit',targetId:id,componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:8});await expect(bindRecordAttachmentSet(db,f.claimant,late.setId,8,key())).rejects.toMatchObject({code:'SERVICE_ATTACHMENT_HISTORY_ONLY'});expect(await db.prepare('SELECT version,status FROM field_service_orders WHERE id=?').bind(id).first()).toEqual({version:8,status:'billed'});
});
it('Service no-charge raw Finance source leases remain query-free until every later package owner is locked',async()=>{
 const {createTalentCandidate}=await import('@erpx/server-talent/public/talent'),{withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics');await serviceGrant('active','finance');await serviceGrant('active','recruit');const actor={...f.owner,branchId:f.branch,departmentId:f.department,modules:['talent']},candidate=await createTalentCandidate(actor,{legalName:'Actual later package source',email:`candidate-${key()}@example.test`,consentEvidence:'Original consent for the isolated source lease proof',retentionUntil:'2027-09-30'},key());
 const measured=await measureReads(()=>committedSourceVisibility(db,tx=>tx.prepare("UPDATE talent_candidates SET retention_until='2028-09-30',version=version+1 WHERE id=?").bind(candidate.id).run(),()=>withOwnerPackageObligations(db,[{companyId:f.company,packageId:'finance',packageVersion:1},{companyId:f.company,packageId:'recruit',packageVersion:1}],async tx=>({retention:await tx.prepare('SELECT retention_until FROM talent_candidates WHERE id=?').bind(candidate.id).first('retention_until'),facts:await ownerPackageObligations(db,tx,f.company,'recruit')}))));expect(measured.result.retention).toBe('2028-09-30');expect(measured.result.facts).toBe(1);for(const metric of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(measured.metrics[metric]??0).toBe(0);
});
it('Service no-charge confirmation keeps two actual Company decisions isolated with zero hidden lock retries',async()=>{
 const {confirmServiceNoCharge,serviceNoChargeControls}=await import('@erpx/server-finance/public/service-no-charge'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),original={f,customer,warrantyAsset},first=await noChargeReviewSource(),firstActor=f.manager,firstControls=await serviceNoChargeControls(firstActor,first.invoice);let second:Awaited<ReturnType<typeof noChargeReviewSource>>,secondActor:typeof firstActor,secondControls:Awaited<ReturnType<typeof serviceNoChargeControls>>;
 try{f=await expenseFixture();for(const actor of [f.owner,f.tie,f.claimant])actor.modules=['field_service'];vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');try{customer=String((await createParty(f.owner,partySchema.parse({kind:'customer',code:key(),name:'Second real covered Company customer',currency:'THB'}),key())).id);}finally{vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');}warrantyAsset=`asset-${key()}`;await db.prepare("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at) VALUES(?,?,?,?,'Second actual covered equipment','equipment','THB',8000,'2026-01-01','2099-12-31',?,?,?)").bind(warrantyAsset,f.company,f.branch,key(),f.owner.userId,Date.now(),Date.now()).run();await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'asset','THB',1,'receivable',1,1)").bind(`${f.company}-receivable`,f.chart,f.company,'1200','Second original trade receivable').run();second=await noChargeReviewSource();secondActor=f.manager;secondControls=await serviceNoChargeControls(secondActor,second.invoice);}finally{({f,customer,warrantyAsset}=original);}
 const firstKey=key(),secondKey=key(),reason='Independent original covered Company source review';const measured=await measureReads(()=>Promise.all([confirmServiceNoCharge(firstActor,first.invoice,{expectedVersion:1,contextKey:firstControls!.contextKey,reason},firstKey),confirmServiceNoCharge(secondActor,second.invoice,{expectedVersion:1,contextKey:secondControls!.contextKey,reason},secondKey)]));for(const metric of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(measured.metrics[metric]??0).toBe(0);for(const [actor,source,command,result] of [[firstActor,first,firstKey,measured.result[0]],[secondActor,second,secondKey,measured.result[1]]] as const){expect(result).toMatchObject({id:source.invoice,status:'no_charge_confirmed',version:2});expect(await db.prepare('SELECT company_id,invoice_id,service_order_id,command_id FROM finance_service_no_charge_confirmations WHERE invoice_id=?').bind(source.invoice).first()).toEqual({company_id:actor.companyId,invoice_id:source.invoice,service_order_id:source.id,command_id:command});expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE company_id=? AND source_type='invoice' AND source_id=?").bind(actor.companyId,source.invoice).first('n')).toBe(0);}
});


it('Service no-charge routing preserves normal reassignment authority after a reviewer captured controls',async()=>{
 const {confirmServiceNoCharge,serviceNoChargeControls}=await import('@erpx/server-finance/public/service-no-charge'),actual=await noChargeReviewSource(),captured=await serviceNoChargeControls(f.manager,actual.invoice),body={expectedVersion:1,contextKey:captured!.contextKey,reason:'Independent original full warranty review before current responsibility changed'},originalKey=key();
 const next={...f.payer,role:'finance_approver'};next.assignmentId=await f.grant(next,'finance_approver');
 const flow=await resolveFlow(db,f.owner,'invoice',actual.invoice);expect(flow.current?.primary?.userId).toBe(f.manager.userId);
 await assignFlowPrimary(db,f.owner,'invoice',actual.invoice,{stepKey:'draft',userId:next.userId,reason:'Normal Company administrator assigned the other independent current reviewer',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 expect((await resolveFlow(db,f.manager,'invoice',actual.invoice)).canAct).toBe(false);expect((await resolveFlow(db,next,'invoice',actual.invoice)).canAct).toBe(true);
 expect((await serviceNoChargeControls(f.manager,actual.invoice))?.canConfirm).toBe(false);expect((await serviceNoChargeControls(next,actual.invoice))?.canConfirm).toBe(true);
 await expect(confirmServiceNoCharge(f.manager,actual.invoice,body,originalKey)).rejects.toMatchObject({status:403,code:'FLOW_PRIMARY_REQUIRED'});
 expect(await db.prepare('SELECT status,version FROM finance_invoices WHERE id=?').bind(actual.invoice).first()).toEqual({status:'draft',version:1});
 expect(await db.prepare('SELECT 1 FROM finance_service_no_charge_confirmations WHERE invoice_id=?').bind(actual.invoice).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(originalKey).first()).toBeNull();
 const controls=await serviceNoChargeControls(next,actual.invoice),input={...body,contextKey:controls!.contextKey},command=key(),result=await confirmServiceNoCharge(next,actual.invoice,input,command);
 expect(result).toMatchObject({id:actual.invoice,status:'no_charge_confirmed',version:2});expect(await confirmServiceNoCharge(next,actual.invoice,input,command)).toEqual(result);
 expect(await db.prepare("SELECT count(*)::int n FROM flow_events WHERE company_id=? AND kind='invoice' AND record_id=? AND command_id=? AND action='post' AND actor_user_id=?").bind(f.company,actual.invoice,command,next.userId).first('n')).toBe(1);
 expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE company_id=? AND source_type='invoice' AND source_id=?").bind(f.company,actual.invoice).first('n')).toBe(0);expect((await readCompanyPackageGrants(db,f.company)).map(g=>g.packageId)).toEqual(['service','service-parts-assets']);
});

it('Service no-charge routing rolls back a real late deferred route revision and permits exact same-key recovery',async()=>{
 const {confirmServiceNoCharge,serviceNoChargeControls}=await import('@erpx/server-finance/public/service-no-charge'),actual=await noChargeReviewSource(),controls=await serviceNoChargeControls(f.manager,actual.invoice),input={expectedVersion:1,contextKey:controls!.contextKey,reason:'Independent actual full warranty review must retain the original current routing authority'},command=key();
 const beforeInvoice=await db.prepare('SELECT * FROM finance_invoices WHERE id=?').bind(actual.invoice).first(),beforeRoute=await db.prepare("SELECT * FROM flow_routes WHERE company_id=? AND kind='invoice' AND record_id=?").bind(f.company,actual.invoice).first(),trigger=`no_charge_route_${key().replaceAll('-','')}`;
 await db.exec(`CREATE SEQUENCE ${trigger}_attempt;CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${trigger}_attempt');UPDATE flow_routes SET steps_json=jsonb_set(steps_json::jsonb,'{draft,primaryUserId}',to_jsonb('${f.payer.userId}'::text))::text,version=version+1 WHERE company_id='${f.company}' AND kind='invoice' AND record_id='${actual.invoice}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${trigger}()`);
 try{
  await expect(confirmServiceNoCharge(f.manager,actual.invoice,input,command)).rejects.toMatchObject({status:409});
  expect(await db.prepare(`SELECT is_called FROM ${trigger}_attempt`).first('is_called')).toBe(true);
  expect(await db.prepare('SELECT * FROM finance_invoices WHERE id=?').bind(actual.invoice).first()).toEqual(beforeInvoice);expect(await db.prepare("SELECT * FROM flow_routes WHERE company_id=? AND kind='invoice' AND record_id=?").bind(f.company,actual.invoice).first()).toEqual(beforeRoute);
  for(const table of ['finance_service_no_charge_confirmations','command_receipts','audit_events','outbox_events','flow_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
 }finally{await db.exec(`DROP TRIGGER ${trigger} ON outbox_events;DROP FUNCTION ${trigger}();DROP SEQUENCE ${trigger}_attempt`);}
 const result=await confirmServiceNoCharge(f.manager,actual.invoice,input,command);expect(result).toMatchObject({id:actual.invoice,status:'no_charge_confirmed',version:2});expect(await confirmServiceNoCharge(f.manager,actual.invoice,input,command)).toEqual(result);
 expect(await db.prepare("SELECT count(*)::int n FROM flow_events WHERE command_id=? AND kind='invoice' AND record_id=? AND action='post'").bind(command,actual.invoice).first('n')).toBe(1);expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE source_type='invoice' AND source_id=?").bind(actual.invoice).first('n')).toBe(0);
});

it('Service no-charge routing protects the actual captured default while permitting normal automatic routes and future-only defaults',async()=>{
 const {confirmServiceNoCharge,serviceNoChargeControls}=await import('@erpx/server-finance/public/service-no-charge'),seed=await noChargeReviewSource(),next={...f.payer,role:'finance_approver'};next.assignmentId=await f.grant(next,'finance_approver');
 let seedFlow=await resolveFlow(db,f.owner,'invoice',seed.invoice);
 await assignFlowPrimary(db,f.owner,'invoice',seed.invoice,{stepKey:'draft',userId:f.manager.userId,reason:'Normal original Company default for future independent covered invoice reviews',saveDefault:true,expectedVersion:seedFlow.version,recordVersion:seedFlow.recordVersion},key());
 const originalDefault=await db.prepare("SELECT * FROM flow_defaults WHERE company_id=? AND kind='invoice' ORDER BY revision DESC LIMIT 1").bind(f.company).first<{revision:number;primary_user_id:string}>();expect(originalDefault?.primary_user_id).toBe(f.manager.userId);
 // This second invoice is created entirely through the normal Service owner;
 // no per-record assignment is made, so its first command must snapshot the default.
 const agreementId=await agreement({assetId:warrantyAsset,warrantyDiscountBps:10000}),visit=await createServiceRequest(f.owner,{agreementId,agreementVersion:2,title:'Normal covered source with inherited Finance default',issue:'Actual default routing before the first invoice command',site:'Original covered customer site',technicianId:f.claimant.userId,scheduledAt:Date.now()+60000},key()),id=String(visit.id);
 await act(f.claimant,id,{action:'dispatch',expectedVersion:1,reason:'Original covered dispatch for default routing'});await act(f.claimant,id,{action:'start',expectedVersion:2,reason:'Actual customer access for the covered inspection'});await act(f.claimant,id,{action:'complete',expectedVersion:3,laborMinutes:35,evidence:'Completed actual covered inspection with inherited reviewer',customerConfirmation:'Customer accepted the original contractual inspection',reason:'Original completion before independent zero-demand decision'});const release=key();await changeServiceOrder(f.tie,id,{action:'release-billing',expectedVersion:4,reason:'Independent actual original covered release before default review'},release);const invoice=`field-invoice-${release}`;
 expect(await db.prepare("SELECT 1 FROM flow_routes WHERE company_id=? AND kind='invoice' AND record_id=?").bind(f.company,invoice).first()).toBeNull();const flow=await resolveFlow(db,f.manager,'invoice',invoice);expect(flow).toMatchObject({version:0,defaultsRevision:originalDefault!.revision,canAct:true});expect(flow.current?.primary?.userId).toBe(f.manager.userId);
 const controls=await serviceNoChargeControls(f.manager,invoice),body={expectedVersion:1,contextKey:controls!.contextKey,reason:'Independent actual full warranty review preserves the default captured before its first route'},command=key(),beforeInvoice=await db.prepare('SELECT * FROM finance_invoices WHERE id=?').bind(invoice).first(),trigger=`no_charge_default_${key().replaceAll('-','')}`;
 await db.exec(`CREATE SEQUENCE ${trigger}_attempt;CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${trigger}_attempt');UPDATE flow_defaults SET primary_user_id='${next.userId}' WHERE company_id='${f.company}' AND revision=${originalDefault!.revision};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${trigger}()`);
 try{
  await expect(confirmServiceNoCharge(f.manager,invoice,body,command)).rejects.toMatchObject({status:409,code:'FLOW_ROUTING_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${trigger}_attempt`).first('is_called')).toBe(true);
  expect(await db.prepare('SELECT * FROM finance_invoices WHERE id=?').bind(invoice).first()).toEqual(beforeInvoice);expect(await db.prepare('SELECT * FROM flow_defaults WHERE company_id=? AND revision=?').bind(f.company,originalDefault!.revision).first()).toEqual(originalDefault);expect(await db.prepare("SELECT 1 FROM flow_routes WHERE company_id=? AND kind='invoice' AND record_id=?").bind(f.company,invoice).first()).toBeNull();
  for(const table of ['finance_service_no_charge_confirmations','command_receipts','audit_events','outbox_events','flow_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
 }finally{await db.exec(`DROP TRIGGER ${trigger} ON outbox_events;DROP FUNCTION ${trigger}();DROP SEQUENCE ${trigger}_attempt`);}
 const result=await confirmServiceNoCharge(f.manager,invoice,body,command);expect(result).toMatchObject({id:invoice,status:'no_charge_confirmed',version:2});expect(await db.prepare("SELECT version,defaults_revision FROM flow_routes WHERE company_id=? AND kind='invoice' AND record_id=?").bind(f.company,invoice).first()).toEqual({version:1,defaults_revision:originalDefault!.revision});expect(await db.prepare("SELECT count(*)::int n FROM flow_events WHERE command_id=? AND kind='invoice' AND record_id=? AND action='post' AND actor_user_id=?").bind(command,invoice,f.manager.userId).first('n')).toBe(1);
 // A later normal default applies to future records; it must not invalidate
 // the original terminal invoice or rewrite the exact stored confirmation.
 seedFlow=await resolveFlow(db,f.owner,'invoice',seed.invoice);await assignFlowPrimary(db,f.owner,'invoice',seed.invoice,{stepKey:'draft',userId:next.userId,reason:'Normal new default applies only to future invoice routes',saveDefault:true,expectedVersion:seedFlow.version,recordVersion:seedFlow.recordVersion},key());expect(Number(await db.prepare("SELECT max(revision) revision FROM flow_defaults WHERE company_id=? AND kind='invoice'").bind(f.company).first('revision'))).toBeGreaterThan(originalDefault!.revision);expect(await confirmServiceNoCharge(f.manager,invoice,body,command)).toEqual(result);expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE source_type='invoice' AND source_id=?").bind(invoice).first('n')).toBe(0);
});


// Additive Service-only native authority fixture; all prior assertions stay byte exact.
type ServicePartsFixtureActor=import("@erpx/server-identity_access/public/authorization").WorkContext;
async function zeroLedgerServicePartsCompany() {
  // Seed organization and actual native worker/assignment authority only.
  // This Company has NEVER had a chart/account/period/quote/settings row.
  // It is a disposable Company fixture, not normal Company onboarding proof.
  const company=`zero-service-parts-${key()}`,branch=`${company}-branch`,department=`${company}-department`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero ledger Service parts fixture',?)").bind(company,company,company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Native services branch',?,?,?)").bind(branch,branch,company,company,branch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Native Service parts team',?,?,?)").bind(department,department,branch,company,branch),
  ]);
  async function actor(name:string,role:string,scopeKind:'holding'|'branch'|'department'='holding'):Promise<ServicePartsFixtureActor>{
    const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`;
    await db.batch([
      db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,branch_id,scope_node_id) VALUES(?,?,?,?,?,0,9999999999999,?,?,?)").bind(assignmentId,userId,role,JSON.stringify(['field_service']),company,scopeKind,scopeKind==='holding'?null:branch,scopeKind==='department'?department:scopeKind==='branch'?branch:null),
      db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${userId}`,userId,company,name),
      db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Native worker','THB','fixture',1,1)").bind(`eng-${userId}`,`person-${userId}`,company,branch,department),
    ]);
    const scope:ServicePartsFixtureActor['scope']=scopeKind==='department'?{kind:'department',holdingId:'h001',holdingSlug:'service-parts-test',holdingName:'Test',companyId:company,branchId:branch,departmentId:department,departmentSlug:department,departmentName:'Native Service parts team'}:scopeKind==='branch'?{kind:'branch',holdingId:'h001',holdingSlug:'service-parts-test',holdingName:'Test',companyId:company,branchId:branch,branchSlug:branch,branchName:'Native service branch'}:{kind:'holding',holdingId:'h001',holdingSlug:'service-parts-test',holdingName:'Test',companyId:company};
    const context:ServicePartsFixtureActor={userId,assignmentId,role,companyId:company,branchId:scopeKind==='holding'?null:branch,departmentId:scopeKind==='department'?department:null,holdingId:'h001',holdingSlug:'service-parts-test',holdingName:'Test',name,email:`${name}@example.test`,assignmentScope:scopeKind,inherited:false,modules:['field_service'],assignments:[],workspaces:[],scope};
    const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');
    context.iam=await loadIamAuthority(db,company,userId,role);
    return context;
  }
  const owner=await actor('owner','holding_owner'),reviewer=await actor('reviewer','finance_preparer'),manager=await actor('manager','finance_approver'),serviceReviewer=await actor('service-reviewer','holding_owner'),technician=await actor('technician','employee');
  const {installStaffLevels,savePosition,positionSchema}=await import('@erpx/server-organization/public/organization');
  await installStaffLevels(db,owner,key());
  const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Fixture accountable owner',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Native organizational authority fixture'}),key())).id);
  for(const [member,rank] of [[owner,1],[reviewer,5],[manager,4],[serviceReviewer,3],[technician,4]] as const){
    const position=rank===1?ceo:String((await savePosition(db,owner,null,positionSchema.parse({title:`Fixture ${member.role}`,levelId:`org-level-${company}-${rank}`,branchId:null,departmentId:null,reportsToPositionId:ceo,expectedVersion:0,reason:'Native selected worker authority fixture'}),key())).id);
    await db.prepare('UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=?').bind(`org-level-${company}-${rank}`,position,rank===1?null:`person-${owner.userId}`,`eng-${member.userId}`).run();
  }
  await db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,'ceo',?,1,?,?,1) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=1,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=1").bind(company,company,ceo,`person-${owner.userId}`).run();
  const counts=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','commerce_orders','finance_journals','finance_invoices','commerce_warehouses','commerce_products'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')));
  expect(await counts()).toEqual([0,0,0,0,0,0,0,0,0,0]);
  // Normal packageStore is an explicit isolated provider fixture port, never
  // installed Commercial authority or normal Company onboarding acceptance.
  const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-Service-parts-provider'}),obligations:async()=>{throw Error('Service parts fixture cannot withdraw');}});
  async function purchase(){for(const packageId of ['service','service-parts-assets'])await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Declared native Service-only parts startup',evidence:'test:Service-parts-fresh-Company'}});}
  vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_SUPPORT_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
  for(const person of [owner,reviewer,manager,serviceReviewer,technician]){const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');person.iam=await loadIamAuthority(db,company,person.userId,person.role);}
  return {company,branch,department,owner,reviewer,manager,serviceReviewer,technician,counts,purchase,store,actor};
}

async function zeroLedgerServicePartsSource(){
 const z=await zeroLedgerServicePartsCompany(),finance=await import('@erpx/server-finance/public/service-parts-prerequisites'),setup=await import('@erpx/server-commerce/public/service-parts-posting-setup'),masters=await import('@erpx/server-commerce/public/service-initial-physical-stock-masters');
 const empty=await z.counts(),deniedKey=key();expect(await finance.servicePartsPrerequisites(z.reviewer)).toMatchObject({available:false,unavailableReason:'purchase-required',charts:[],accounts:[]});await expect(finance.createServicePartsChart(z.reviewer,{expectedVersion:0,contextKey:'a'.repeat(64),input:{name:'No purchase cannot create references',baseCurrency:'THB',accountingBasis:'TFRS'}},deniedKey)).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await z.counts()).toEqual(empty);for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(deniedKey).first()).toBeNull();await z.purchase();
 const envelope=async()=>{const view=await finance.servicePartsPrerequisites(z.reviewer);expect(view.available).toBe(true);if(!view.contextKey)throw Error('Real current Finance prerequisite context required');return {expectedVersion:0 as const,contextKey:view.contextKey};};
 const chartInput={...await envelope(),input:{name:'Actual Service parts chart',baseCurrency:'THB',accountingBasis:'TFRS'}},chartKey=key(),chart=await finance.createServicePartsChart(z.reviewer,chartInput,chartKey);expect(await finance.createServicePartsChart(z.reviewer,chartInput,chartKey)).toEqual(chart);const accounts:Record<string,string>={};
 for(const [name,code,category,controlKind] of [['ar','1100','asset','receivable'],['ap','2100','liability','payable'],['revenue','4100','revenue','none'],['inventory','1300','asset','none'],['clearing','2150','liability','none'],['cogs','5100','expense','none'],['inputTax','1150','asset','none'],['outputTax','2151','liability','none'],['cash','1001','asset','bank'],['variance','5400','expense','none']] as const){const created=await finance.createServicePartsAccount(z.reviewer,{...await envelope(),input:{chartId:chart.id,code,name:`Actual Service parts ${name}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key());accounts[name]=created.id;}
 const period=await finance.createServicePartsPeriod(z.reviewer,{...await envelope(),input:{name:'Actual Service current period',startDate:'2020-01-01',endDate:'2035-12-31'}},key()),rateSeed=await finance.servicePartsPrerequisites(z.manager);if(!rateSeed.contextKey)throw Error('Actual Company rate authority required');await expect(finance.confirmServicePartsRate(z.reviewer,{expectedVersion:0,contextKey:rateSeed.contextKey,input:{currency:'THB',effectiveDate:'2020-01-01',currencyPerUsd:'35',note:'Preparer cannot confirm own authority'}},key())).rejects.toMatchObject({status:403});await finance.confirmServicePartsRate(z.manager,{expectedVersion:0,contextKey:rateSeed.contextKey,input:{currency:'THB',effectiveDate:'2020-01-01',currencyPerUsd:'35',note:'Independent actual Company Finance rate confirmation'}},key());
 const view=await setup.servicePartsPostingSetup(z.reviewer);expect(view).toMatchObject({available:true,configured:false,sourceReady:false,schemaAvailable:true,canConfigure:true});if(!view.contextKey)throw Error('Actual Service parts mapping context required');const mappingKey=key(),mapping={expectedVersion:0 as const,contextKey:view.contextKey,chartId:chart.id,currency:'THB',receivableAccountId:accounts.ar,payableAccountId:accounts.ap,revenueAccountId:accounts.revenue,inventoryAccountId:accounts.inventory,clearingAccountId:accounts.clearing,cogsAccountId:accounts.cogs,inputTaxAccountId:accounts.inputTax,outputTaxAccountId:accounts.outputTax,cashAccountId:accounts.cash,reason:'Deliberately select all nine actual Service-only posting roles'},configured=await setup.initializeServicePartsPostingSetup(z.reviewer,mapping,mappingKey);expect(configured).toEqual({id:z.company,status:'configured',version:1});expect(await setup.initializeServicePartsPostingSetup(z.reviewer,mapping,mappingKey)).toEqual(configured);
 const warehouse=await masters.createServiceInitialPhysicalStockWarehouse(z.owner,{branchId:z.branch,code:'PARTS',name:'Actual first Service parts warehouse',status:'active'},key()),product=await masters.createServiceInitialPhysicalStockProduct(z.owner,{sku:'FIRST-PART',name:'Actual physically observed Service spare',itemType:'stock',trackingMode:'none',unit:'piece',price:'100',taxBps:0,status:'active'},key());
 const snapshot=async()=>Object.fromEntries(await Promise.all(['commerce_settings','commerce_service_parts_posting_sources','inventory_service_initial_physical_sources','inventory_counts','inventory_count_lines','inventory_count_lot_lines','inventory_count_decisions','inventory_adjustment_moves','inventory_ledger','inventory_lots','inventory_lot_balances','inventory_lot_moves','commerce_stock','commerce_stock_moves','finance_journals','finance_journal_lines','finance_book_postings','finance_financial_events','money_transaction_snapshots','command_receipts','command_assertions','audit_events','outbox_events','commerce_products','assignments'].map(async table=>[table,await db.prepare(`SELECT COALESCE(jsonb_agg(to_jsonb(actual) ORDER BY to_jsonb(actual)::text),'[]'::jsonb)::text image FROM ${table} actual`).first<string>('image')])));
 expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first('n')).toBe(0);expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);
 return {z,finance,setup,masters,chart,accounts,period,mapping,mappingKey,warehouse,product,snapshot,input:{warehouseId:warehouse.id,countDate:new Date().toISOString().slice(0,10),reason:'Actual first physical Service spare observation, current-period variance only',lines:[{productId:product.id,countedQuantity:'2',unitCost:'6',allocations:[]}]}};
}
it('HISTORICAL COMPATIBILITY: Service-only fresh parts setup independently posts the first physical spare and consumes it through original reviewed cost and billing owners',async()=>{
 const s=await zeroLedgerServicePartsSource(),initial=await historicalServiceCount(),counts=await import('@erpx/server-inventory/public/inventory-counts'),before=await s.snapshot();
 expect(counts.canPrepareCounts(s.z.owner)).toBe(true);expect(counts.canApproveCounts(s.z.manager)).toBe(true);await expect(counts.saveCount(s.z.owner,null,s.input,key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await s.snapshot()).toEqual(before);
 const selection={warehouseId:s.warehouse.id,productId:s.product.id,countDate:s.input.countDate},seed=await initial.serviceInitialPhysicalStockCountWorkspace(s.z.owner,selection);expect(seed).toMatchObject({available:true,canPrepare:true,readiness:{schemaAvailable:true,postingConfigured:true,varianceAccountAvailable:true,postingPeriodAvailable:true}});if(!seed.contextKey)throw Error('Actual Service first Count context required');const command=key(),body={expectedVersion:0 as const,contextKey:seed.contextKey,input:s.input},created=await initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command),id=String(created.id);expect(created).toEqual({id:`count-${command}`,status:'draft',version:1});expect(await initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command)).toEqual(created);
 const {enableAttachmentV2ForTest,seedAttachmentSet}=await import('../../support/helpers/attachment-v2');enableAttachmentV2ForTest();const sheet=await seedAttachmentSet(db,s.z.owner,{targetType:'stock_count',targetId:id,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1});await initial.transitionServiceInitialPhysicalStockCount(s.z.owner,id,'submit',{expectedVersion:1,attachmentSetIds:[sheet.setId]},key());
 for(const [step,version] of [['submitted',2],['approved',3]] as const){const flow=await initial.serviceInitialPhysicalStockCountFlow(s.z.owner,id);await initial.assignServiceInitialPhysicalStockCountFlow(s.z.owner,id,{stepKey:step,userId:s.z.manager.userId,expectedVersion:flow.version,recordVersion:version,reason:'Original Company Flow authority chooses independent real Stock reviewer',saveDefault:false},key());if(step==='submitted')await initial.transitionServiceInitialPhysicalStockCount(s.z.manager,id,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Independently verify the actual physical spare quantity and unit cost'},key());}
 const postKey=key(),postBody={expectedVersion:3,attachmentSetIds:[],reason:'Current-period physical variance, not opening equity or a purchase'},posted=await initial.transitionServiceInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey);expect(posted).toMatchObject({id,status:'posted',version:4});expect(await initial.transitionServiceInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey)).toEqual(posted);const detail=await initial.serviceInitialPhysicalStockCountWorkspace(s.z.reviewer,{countId:id}),journal=detail.record?.journalId;if(!journal)throw Error('Actual Count posted journal required');expect(detail.record).toMatchObject({id,sourceKind:'service-initial-physical-stock-count',status:'posted',version:4});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(s.warehouse.id,s.product.id).first()).toEqual({quantity_milli:2000,value_minor:1200});expect(await db.prepare('SELECT source_type,source_id,status,is_opening FROM finance_journals WHERE id=? AND company_id=?').bind(journal,s.z.company).first()).toEqual({source_type:'stock_count',source_id:id,status:'posted',is_opening:0});expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=?').bind(journal).all()).results).toEqual(expect.arrayContaining([{account_id:s.accounts.inventory,debit_minor:1200,credit_minor:0},{account_id:s.accounts.variance,debit_minor:0,credit_minor:1200}]));
 const {journalDetail,ledgerWorkspace}=await import('@erpx/server-finance/public/ledger');expect((await journalDetail(s.z.reviewer,journal)).journal.id).toBe(journal);await expect(ledgerWorkspace(s.z.reviewer,s.chart.id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const customer=await createParty(s.z.reviewer,partySchema.parse({kind:'customer',code:key(),name:'Original Service parts customer',currency:'THB'}),key()),agreement=await createAgreement(s.z.owner,{branchId:s.z.branch,departmentId:null,customerId:customer.id,title:'Actual Service-only spare agreement',startDate:'2020-01-01',endDate:'2035-12-31',currency:'THB',visitRateMinor:10000,warrantyDiscountBps:0},key());await changeAgreement(s.z.serviceReviewer,String(agreement.id),{action:'activate',expectedVersion:1,reason:'Different original Company reviewer confirms the actual agreement'},key());
 const request=await createServiceRequest(s.z.owner,{agreementId:String(agreement.id),agreementVersion:2,title:'First actual spare visit',issue:'Inspect and replace the physically counted spare',site:'Actual customer reception',technicianId:s.z.technician.userId,scheduledAt:Date.now()+60000},key()),orderId=String(request.id);await changeServiceOrder(s.z.technician,orderId,{action:'reserve-part',expectedVersion:1,warehouseId:s.warehouse.id,productId:s.product.id,quantityMilli:1000,expiresAt:Date.now()+3600000,reason:'Reserve one actual first physically counted spare'},key());await changeServiceOrder(s.z.technician,orderId,{action:'dispatch',expectedVersion:2,reason:'Original customer agreed visit dispatch'},key());await changeServiceOrder(s.z.technician,orderId,{action:'start',expectedVersion:3,reason:'Original customer grants actual site access'},key());const reservation=String(await db.prepare('SELECT id FROM field_service_part_reservations WHERE order_id=?').bind(orderId).first('id'));await changeServiceOrder(s.z.technician,orderId,{action:'issue-part',expectedVersion:4,reservationId:reservation,quantityMilli:1000,entryDate:s.input.countDate,reason:'Use exactly one real reserved spare at original actual unit valuation'},key());
 const consumption=(await fieldServiceDetail(s.z.owner,orderId)).consumptions[0];if(!consumption)throw Error('Original immutable part consumption required');const costJournal=String(consumption.journal_id);expect(consumption).toMatchObject({quantity_milli:1000,value_minor:600});const complete={action:'complete' as const,expectedVersion:5,laborMinutes:30,evidence:'The actual replacement spare passed original customer operation checks',customerConfirmation:'Customer accepted the repaired original equipment',reason:'Original visit completion requires independently posted cost'};await expect(changeServiceOrder(s.z.technician,orderId,complete,key())).rejects.toMatchObject({code:'CONFLICT'});const costFlow=await resolveFlow(db,s.z.owner,'journal',costJournal);await assignFlowPrimary(db,s.z.owner,'journal',costJournal,{stepKey:'draft',userId:s.z.manager.userId,expectedVersion:costFlow.version,recordVersion:costFlow.recordVersion,reason:'Independent Finance review of actual original spare consumption',saveDefault:false},key());await postJournal(s.z.manager,costJournal,key(),{expectedVersion:1});await changeServiceOrder(s.z.technician,orderId,complete,key());const release=key();await changeServiceOrder(s.z.serviceReviewer,orderId,{action:'release-billing',expectedVersion:6,reason:'Different original eligible Service reviewer releases actual customer billing'},release);const invoice=`field-invoice-${release}`,invoiceFlow=await resolveFlow(db,s.z.owner,'invoice',invoice);await assignFlowPrimary(db,s.z.owner,'invoice',invoice,{stepKey:'draft',userId:s.z.manager.userId,expectedVersion:invoiceFlow.version,recordVersion:invoiceFlow.recordVersion,reason:'Independent Finance approval of actual original Service receivable',saveDefault:false},key());await postInvoice(s.z.manager,invoice,{periodId:s.period.id,entryDate:s.input.countDate,expectedVersion:1},key());expect((await fieldServiceDetail(s.z.owner,orderId)).order).toMatchObject({id:orderId,status:'billed',version:7});expect(await db.prepare('SELECT status,total_minor FROM finance_invoices WHERE id=?').bind(invoice).first()).toEqual({status:'posted',total_minor:10000});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(s.warehouse.id,s.product.id).first()).toEqual({quantity_milli:1000,value_minor:600});const final=await s.snapshot();expect(await initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command)).toEqual(created);expect(await initial.transitionServiceInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey)).toEqual(posted);expect(await s.snapshot()).toEqual(final);expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);
},120000);

it('HISTORICAL COMPATIBILITY: Service first physical Count refuses stale and foreign selections and rolls a deferred real master-source fault back before an exact-key retry',async()=>{
 const s=await zeroLedgerServicePartsSource(),initial=await historicalServiceCount(),selection={warehouseId:s.warehouse.id,productId:s.product.id,countDate:s.input.countDate},seed=await initial.serviceInitialPhysicalStockCountWorkspace(s.z.owner,selection);if(!seed.contextKey)throw Error('Real selected Service Count source required');const command=key(),body={expectedVersion:0 as const,contextKey:seed.contextKey,input:s.input},before=await s.snapshot();
 await expect(initial.prepareServiceInitialPhysicalStockCount(s.z.owner,{...body,contextKey:'a'.repeat(64)},key())).rejects.toMatchObject({status:409,code:'SERVICE_INITIAL_PHYSICAL_STOCK_CONTEXT_CHANGED'});expect(await s.snapshot()).toEqual(before);
 // The old beforeEach warehouse belongs to another real disposable Company;
 // no changed WorkContext, package grant, source label or positive Stock is used.
 await expect(initial.prepareServiceInitialPhysicalStockCount(s.z.owner,{...body,input:{...s.input,warehouseId:warehouse}},key())).rejects.toMatchObject({status:404,code:'SERVICE_INITIAL_PHYSICAL_STOCK_SELECTION_UNAVAILABLE'});expect(await s.snapshot()).toEqual(before);
 const fault=`service_first_count_${key().replaceAll('-','')}`;
 // Native fault only: a real master row changes after command effects, inside
 // the owned transaction. The complete rollback restores that row as well.
 await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${s.z.company}' THEN UPDATE commerce_products SET name=name||' late source drift' WHERE id='${s.product.id}'; END IF;RETURN NULL;END $$`).run();
 await db.prepare(`CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON inventory_service_initial_physical_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
 try{await expect(initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command)).rejects.toMatchObject({status:409,code:'SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});expect(await s.snapshot()).toEqual(before);}finally{await db.prepare(`DROP TRIGGER ${fault} ON inventory_service_initial_physical_sources`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 const created=await initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command);expect(created).toEqual({id:`count-${command}`,status:'draft',version:1});expect(await initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command)).toEqual(created);
 const cancelled=await initial.transitionServiceInitialPhysicalStockCount(s.z.owner,String(created.id),'cancel',{expectedVersion:1,attachmentSetIds:[],reason:'Normal cancellation leaves no synthetic positive Stock'},key());expect(cancelled).toMatchObject({status:'cancelled',version:2,journalId:null});expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(s.warehouse.id,s.product.id).first('n')).toBe(0);
 const after=await s.snapshot();await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(s.z.owner.assignmentId).run();await expect(initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command)).rejects.toMatchObject({status:403,code:'QUEUED_AUTHORITY_CHANGED'});expect(await db.prepare('SELECT status,version FROM inventory_counts WHERE id=?').bind(created.id).first()).toEqual({status:'cancelled',version:2});expect(await db.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(1);const {assignments:beforeAuthority,...beforeUnchanged}=after,{assignments:revokedAuthority,...afterUnchanged}=await s.snapshot();expect(afterUnchanged).toEqual(beforeUnchanged);expect(revokedAuthority).not.toBe(beforeAuthority);
},120000);

it('HISTORICAL COMPATIBILITY: Service confirms an unchanged historical Supply mapping after normal Supply withdrawal and owns only a new absent-part physical Count',async()=>{
 const z=await zeroLedgerServicePartsCompany(),finance=await import('@erpx/server-finance/public/finance'),money=await import('@erpx/server-finance/public/company-money'),supply=await import('@erpx/server-commerce/public/posting-setup'),commerce=await import('@erpx/server-commerce/public/commerce'),inventory=await import('@erpx/server-inventory/public/inventory-counts'),setup=await import('@erpx/server-commerce/public/service-parts-posting-setup'),initial=await historicalServiceCount(),masters=await import('@erpx/server-commerce/public/service-initial-physical-stock-masters');
 const {packageGrantRevision,readCompanyPackageHistoryRights}=await import('@erpx/server-commercial/public/package-grants'),{withOwnerPackageObligations,ownerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');
 // Explicit historical fixture scope: this Company really bought Supply first.
 // Its old owner creates all business rows; it ends through actual owner-registry
 // review before Service confirmation. This is not a never-Supply Company claim.
 const grant={companyId:z.company,packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Actual original Supply mapping and physical observation before Service adoption',evidence:'test:historical-Service-parts-map'},provider=packageGrantStore(db,{authorize:async()=>({userId:'isolated-Service-existing-map-provider'}),obligations:(transaction,companyId,packageId)=>ownerPackageObligations(db,transaction,companyId,packageId)});await provider.apply({expectedVersion:0,idempotencyKey:key(),grant});
 const chart=await finance.createChart(z.reviewer,{name:'Actual retained original Supply chart',baseCurrency:'THB',accountingBasis:'TFRS'},key()),accounts:Record<string,string>={};
 for(const [field,code,category,controlKind] of [['receivableAccountId','1100','asset','receivable'],['payableAccountId','2100','liability','payable'],['revenueAccountId','4100','revenue','none'],['inventoryAccountId','1300','asset','none'],['clearingAccountId','2150','liability','none'],['cogsAccountId','5100','expense','none'],['inputTaxAccountId','1150','asset','none'],['outputTaxAccountId','2151','liability','none'],['cashAccountId','1001','asset','bank'],['varianceAccountId','5400','expense','none']] as const)accounts[field]=String((await finance.createAccount(z.reviewer,{chartId:chart.id,code,name:`Actual original retained ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind},key())).id);
 await finance.createPeriod(z.reviewer,{name:'Actual original Supply observation year',startDate:'2020-01-01',endDate:'2035-12-31'},key());await money.confirmManualRate(z.manager,{currency:'THB',effectiveDate:'2020-01-01',currencyPerUsd:'35',note:'Actual independent Company quote before Service source confirmation'},key());
 const preview=await supply.commercePostingSetup(z.reviewer);if(!preview.contextKey)throw Error('Actual original shared Commerce mapping context required');const nine=Object.fromEntries(Object.entries(accounts).filter(([field])=>field!=='varianceAccountId')),mappingBody={expectedVersion:0 as const,contextKey:preview.contextKey,chartId:chart.id,currency:'THB',reason:'Original Company owner selects the immutable shared nine-role mapping',...nine},mappingKey=key();expect(await supply.initializeCommercePostingSetup(z.reviewer,mappingBody,mappingKey)).toEqual({id:z.company,status:'configured',version:1});
 const warehouse=await commerce.saveMaster(z.owner,'warehouses',null,{branchId:z.branch,code:key(),name:'Actual retained original Supply warehouse'},key()),oldProduct=await commerce.saveMaster(z.owner,'products',null,{sku:key(),name:'Actual original independently observed spare',itemType:'stock',price:'6',taxBps:0},key()),countDate=new Date().toISOString().slice(0,10),oldKey=key(),oldInput=inventory.countSchema.parse({warehouseId:String(warehouse.id),countDate,reason:'Original Supply current-period physical observation, never opening equity',lines:[{productId:String(oldProduct.id),countedQuantity:'2',unitCost:'6'}]}),oldCount=await inventory.saveCount(z.owner,null,oldInput,oldKey),oldId=String(oldCount.id);
 const {enableAttachmentV2ForTest,seedAttachmentSet}=await import('../../support/helpers/attachment-v2');enableAttachmentV2ForTest();const oldSheet=await seedAttachmentSet(db,z.owner,{targetType:'stock_count',targetId:oldId,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1});await inventory.transitionCount(z.owner,oldId,'submit',{expectedVersion:1,attachmentSetIds:[oldSheet.setId]},key());
 for(const [step,version] of [['submitted',2],['approved',3]] as const){const flow=await resolveFlow(db,z.owner,'stock_count',oldId);await assignFlowPrimary(db,z.owner,'stock_count',oldId,{stepKey:step,userId:z.manager.userId,expectedVersion:flow.version,recordVersion:version,reason:'Original Company Flow chooses the real different Stock reviewer',saveDefault:false},key());if(step==='submitted')await inventory.transitionCount(z.manager,oldId,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Original independent observation confirms two real physical units'},key());}
 await inventory.transitionCount(z.manager,oldId,'post',{expectedVersion:3,attachmentSetIds:[],reason:'Original Supply posts real current-period inventory variance'},key());expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,oldProduct.id).first()).toEqual({quantity_milli:2000,value_minor:1200});const originalCreate=JSON.parse(String(await db.prepare('SELECT result_json FROM command_receipts WHERE command_id=?').bind(oldKey).first('result_json')));expect(originalCreate).not.toHaveProperty('_serviceInitialPhysicalStockCount');
 const oldJournal=String(await db.prepare('SELECT journal_id FROM inventory_counts WHERE id=?').bind(oldId).first('journal_id')),retained=async()=>({mapping:await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).first<string>('image'),chart:await db.prepare('SELECT to_jsonb(c)::text image FROM finance_charts c WHERE id=?').bind(chart.id).first<string>('image'),accounts:(await db.prepare('SELECT to_jsonb(a)::text image FROM finance_accounts a WHERE company_id=? ORDER BY id').bind(z.company).all()).results,defaults:(await db.prepare('SELECT to_jsonb(d)::text image FROM finance_sales_defaults d WHERE company_id=? ORDER BY id').bind(z.company).all()).results,count:await db.prepare('SELECT to_jsonb(c)::text image FROM inventory_counts c WHERE id=?').bind(oldId).first<string>('image'),lines:(await db.prepare('SELECT to_jsonb(l)::text image FROM inventory_count_lines l WHERE count_id=? ORDER BY id').bind(oldId).all()).results,decisions:(await db.prepare('SELECT to_jsonb(d)::text image FROM inventory_count_decisions d WHERE count_id=? ORDER BY id').bind(oldId).all()).results,journal:await db.prepare('SELECT to_jsonb(j)::text image FROM finance_journals j WHERE id=?').bind(oldJournal).first<string>('image'),journalLines:(await db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l WHERE journal_id=? ORDER BY id').bind(oldJournal).all()).results,stock:await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_stock s WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,oldProduct.id).first<string>('image'),receipt:await db.prepare('SELECT to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?').bind(oldKey).first<string>('image')}),original=await retained();
 // Real Preview/Apply, current versions and actual all-owner settlement readers.
 // Positive terminal Stock is retained as a revision fact, not deleted or seeded.
 const settlement={companyId:z.company,packageId:'supply',packageVersion:1};for(const [state,expectedVersion] of [['draining',1],['inactive',2]] as const){const grants=await readCompanyPackageGrants(db,z.company),request={companyId:z.company,expectedRevision:packageGrantRevision(z.company,grants),idempotencyKey:key(),changes:[{expectedVersion,grant:{...grant,state,version:expectedVersion+1,reason:state==='draining'?'Stop new Supply work before normal original owner settlement':'Actual Supply obligations are terminal; retain original stock and history',evidence:'test:real-Service-existing-map-Supply-settlement'}}]},decision=await withOwnerPackageObligations(db,settlement,async transaction=>{expect((await readOwnerPackageObligations(db,transaction,{...settlement,now:Date.now()})).total).toBe(0);return provider.batch.previewIn(transaction,request);});expect(await withOwnerPackageObligations(db,settlement,transaction=>provider.batch.applyIn(transaction,request,async current=>{expect(current).toEqual(decision);}))).toEqual(decision);}
 await z.purchase();expect((await readCompanyPackageGrants(db,z.company)).filter(row=>row.state==='active').map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);expect((await readCompanyPackageHistoryRights(db,z.company)).some(row=>row.packageId==='supply')).toBe(true);expect(await retained()).toEqual(original);
 const seed=await setup.servicePartsPostingSetup(z.reviewer);expect(seed).toMatchObject({available:true,schemaAvailable:true,configured:true,sourceReady:false,version:0,canConfigure:false,canConfirmExisting:true});if(!seed.contextKey)throw Error('Actual unchanged existing mapping confirmation context required');const body={expectedVersion:0 as const,contextKey:seed.contextKey,reason:'Current Company Finance deliberately confirms the actual historical nine-role mapping'},confirmKey=key(),noEffects=async()=>({old:await retained(),sources:(await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_service_parts_posting_sources s WHERE company_id=?').bind(z.company).all()).results,commands:await Promise.all(['command_receipts','audit_events','outbox_events','command_assertions'].map(async table=>(await db.prepare(`SELECT to_jsonb(c)::text image FROM ${table} c WHERE command_id=? ORDER BY image`).bind(confirmKey).all()).results))}),before=await noEffects();
 await expect(setup.initializeServicePartsPostingSetup(z.reviewer,{...mappingBody,contextKey:seed.contextKey},key())).rejects.toMatchObject({status:409,code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});expect(await noEffects()).toEqual(before);await expect(setup.confirmExistingServicePartsPostingSetup(z.reviewer,{...body,contextKey:'a'.repeat(64)},confirmKey)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_CONTEXT_CHANGED'});expect(await noEffects()).toEqual(before);
 const confirmed=await setup.confirmExistingServicePartsPostingSetup(z.reviewer,body,confirmKey);expect(confirmed).toEqual({id:z.company,status:'confirmed-existing',version:1});expect(Object.keys(confirmed).sort()).toEqual(['id','status','version']);expect(await retained()).toEqual(original);const after=await noEffects();expect(await setup.confirmExistingServicePartsPostingSetup(z.reviewer,body,confirmKey)).toEqual(confirmed);expect(await noEffects()).toEqual(after);await expect(setup.confirmExistingServicePartsPostingSetup(z.reviewer,{...body,reason:'Changed input cannot replace the committed actual confirmation'},confirmKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});await expect(setup.confirmExistingServicePartsPostingSetup(z.reviewer,body,key())).rejects.toMatchObject({code:'SERVICE_PARTS_ALREADY_CONFIGURED'});expect(await noEffects()).toEqual(after);
 expect(await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE record_id=? AND operation='commerce.service-parts-posting-setup.initialize'").bind(z.company).first('n')).toBe(0);const proof=await db.prepare('SELECT mode,proof_json::text proof,mapping_json::text mapping FROM commerce_service_parts_posting_sources WHERE company_id=?').bind(z.company).first<{mode:string;proof:string;mapping:string}>();if(!proof)throw Error('Actual distinct existing confirmation provenance required');expect(proof.mode).toBe('confirm-existing');expect(JSON.parse(proof.mapping)).toEqual(JSON.parse(original.mapping!));expect(JSON.parse(proof.proof)).toMatchObject({kind:'service-parts-posting-setup',commandId:confirmKey,mode:'confirm-existing'});
 const nextProduct=await masters.createServiceInitialPhysicalStockProduct(z.owner,{sku:'SERVICE-ABSENT',name:'Actual new absent Service spare',itemType:'stock',trackingMode:'none',unit:'piece',price:'100',taxBps:0,status:'active'},key()),input={warehouseId:String(warehouse.id),countDate,reason:'New absent Service spare follows original physical Count source',lines:[{productId:nextProduct.id,countedQuantity:'1',unitCost:'7',allocations:[]}]},selection={warehouseId:String(warehouse.id),productId:nextProduct.id,countDate};expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,nextProduct.id).first('n')).toBe(0);const countSeed=await initial.serviceInitialPhysicalStockCountWorkspace(z.owner,selection);expect(countSeed.canPrepare).toBe(true);if(!countSeed.contextKey)throw Error('Actual new Service Count selected context required');const nextKey=key(),nextBody={expectedVersion:0 as const,contextKey:countSeed.contextKey,input},next=await initial.prepareServiceInitialPhysicalStockCount(z.owner,nextBody,nextKey);expect(next).toEqual({id:`count-${nextKey}`,status:'draft',version:1});const nativeProof=JSON.parse(String(await db.prepare('SELECT proof_json FROM inventory_service_initial_physical_sources WHERE count_id=?').bind(next.id).first('proof_json')));expect(nativeProof).toMatchObject({kind:'service-initial-physical-stock-count',serviceSetupCommandId:confirmKey,chartId:chart.id,inventoryAccountId:accounts.inventoryAccountId});
 await initial.transitionServiceInitialPhysicalStockCount(z.owner,String(next.id),'cancel',{expectedVersion:1,attachmentSetIds:[],reason:'Normal retained draft cancellation leaves the old physical stock untouched'},key());expect(await retained()).toEqual(original);expect(await setup.confirmExistingServicePartsPostingSetup(z.reviewer,body,confirmKey)).toEqual(confirmed);expect(await initial.prepareServiceInitialPhysicalStockCount(z.owner,nextBody,nextKey)).toEqual(next);expect((await initial.serviceInitialPhysicalStockCountWorkspace(z.owner,{countId:String(next.id)})).record).toMatchObject({status:'cancelled',version:2});expect((await readCompanyPackageGrants(db,z.company)).filter(row=>row.state==='active').map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);
},120000);

it('Service mapping rolls a deferred actual Finance reference change back before an exact-body and key retry',async()=>{
 const z=await zeroLedgerServicePartsCompany(),finance=await import('@erpx/server-finance/public/service-parts-prerequisites'),setup=await import('@erpx/server-commerce/public/service-parts-posting-setup');await z.purchase();
 const envelope=async()=>{const view=await finance.servicePartsPrerequisites(z.reviewer);expect(view.available).toBe(true);if(!view.contextKey)throw Error('Actual Service Finance prerequisite context required');return {expectedVersion:0 as const,contextKey:view.contextKey};};
 const chart=await finance.createServicePartsChart(z.reviewer,{...await envelope(),input:{name:'Actual Service mapping reference finality chart',baseCurrency:'THB',accountingBasis:'TFRS'}},key()),accounts:Record<string,string>={};
 for(const [name,code,category,controlKind] of [['ar','1100','asset','receivable'],['ap','2100','liability','payable'],['revenue','4100','revenue','none'],['inventory','1300','asset','none'],['clearing','2150','liability','none'],['cogs','5100','expense','none'],['inputTax','1150','asset','none'],['outputTax','2151','liability','none'],['cash','1001','asset','bank']] as const){const account=await finance.createServicePartsAccount(z.reviewer,{...await envelope(),input:{chartId:chart.id,code,name:`Actual Service mapping ${name}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key());accounts[name]=account.id;}
 const seed=await setup.servicePartsPostingSetup(z.reviewer);expect(seed).toMatchObject({available:true,schemaAvailable:true,configured:false,sourceReady:false,canConfigure:true});if(!seed.contextKey)throw Error('Actual fresh Service mapping context required');
 const command=key(),body={expectedVersion:0 as const,contextKey:seed.contextKey,chartId:chart.id,currency:'THB',receivableAccountId:accounts.ar,payableAccountId:accounts.ap,revenueAccountId:accounts.revenue,inventoryAccountId:accounts.inventory,clearingAccountId:accounts.clearing,cogsAccountId:accounts.cogs,inputTaxAccountId:accounts.inputTax,outputTaxAccountId:accounts.outputTax,cashAccountId:accounts.cash,reason:'Current Finance explicitly selects the actual nine original Service posting roles'};
 const image=async()=>Object.fromEntries(await Promise.all(['finance_charts','finance_accounts','company_money_settings','company_currency_scales','company_fx_rates','commerce_settings','commerce_service_parts_posting_sources','finance_journals','finance_journal_lines','finance_book_postings','finance_financial_events','money_transaction_snapshots','command_receipts','command_assertions','audit_events','outbox_events'].map(async table=>[table,await db.prepare(`SELECT COALESCE(jsonb_agg(to_jsonb(actual) ORDER BY to_jsonb(actual)::text),'[]'::jsonb)::text image FROM ${table} actual`).first<string>('image')]))),before=await image(),fault=`service_mapping_finance_${key().replaceAll('-','')}`;
 // Native diagnostic fault only: the real chart changes after source effects
 // inside the owned command. No positive Finance or Stock row is SQL-seeded.
 await db.exec(`CREATE SEQUENCE ${fault}_attempt;CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${z.company}' THEN PERFORM nextval('${fault}_attempt');UPDATE finance_charts SET name=name||' deferred reference drift' WHERE id='${chart.id}' AND company_id='${z.company}';END IF;RETURN NULL;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON commerce_service_parts_posting_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
 try{await expect(setup.initializeServicePartsPostingSetup(z.reviewer,body,command)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}_attempt`).first('is_called')).toBe(true);expect(await image()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON commerce_service_parts_posting_sources;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}_attempt`);}
 const configured=await setup.initializeServicePartsPostingSetup(z.reviewer,body,command);expect(configured).toEqual({id:z.company,status:'configured',version:1});expect(await db.prepare('SELECT name FROM finance_charts WHERE id=?').bind(chart.id).first('name')).toBe('Actual Service mapping reference finality chart');
 const committed=await image();expect(await setup.initializeServicePartsPostingSetup(z.reviewer,body,command)).toEqual(configured);expect(await image()).toEqual(committed);expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);
},120000);


it('HISTORICAL COMPATIBILITY: Service product entry uses the actual current Finance or Stock assignment and named Count source without generic package authority',async()=>{
 const fixture=await zeroLedgerServicePartsSource(),{z}=fixture;
 const setup=await import('@erpx/server-commerce/public/service-parts-posting-setup'),counts=await historicalServiceCount();
 async function scope(actor:ServicePartsFixtureActor){const version=await db.prepare('SELECT version FROM assignments WHERE id=? AND company_id=? AND user_id=?').bind(actor.assignmentId,actor.companyId,actor.userId).first<number>('version');if(!version)throw Error('Original native assignment version required');return {companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:version,role:actor.role,sessionId:'isolated-native-entry-session'};}
 const finance=await scope(z.reviewer),stock=await scope(z.owner),employee=await scope(z.technician);
 expect(await setup.servicePartsPostingSetupProductEntry(db,finance)).toMatchObject({revision:expect.stringMatching(/^[a-f0-9]{64}$/),purchasePurpose:'history'});
 expect(await setup.servicePartsPostingSetupProductEntry(db,stock)).toMatchObject({purchasePurpose:'history'});
 expect(await setup.servicePartsPostingSetupProductEntry(db,employee)).toBeNull();
 for(const change of [{assignmentVersion:finance.assignmentVersion+1},{sessionId:''},{departmentId:z.department},{branchId:z.branch}])expect(await setup.servicePartsPostingSetupProductEntry(db,{...finance,...change})).toBeNull();
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,stock,{})).toEqual({capability:'service.parts-assets',purchasePurpose:'history'});
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,finance,{})).toBeNull();
 const input=fixture.input,view=await counts.serviceInitialPhysicalStockCountWorkspace(z.owner,{warehouseId:input.warehouseId,productId:fixture.product.id,countDate:input.countDate});if(!view.contextKey)throw Error('Actual Count source review required');
 const created=await counts.prepareServiceInitialPhysicalStockCount(z.owner,{expectedVersion:0,contextKey:view.contextKey,input},key());if(typeof created.id!=='string')throw Error('Original Count receipt id required');const countId=created.id;
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,stock,{countId})).toEqual({capability:'service.parts-assets',purchasePurpose:'history'});
 for(const target of [{countId:'count-foreign-source'},{countId:fixture.product.id}])expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,stock,target)).toBeNull();
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,{...stock,assignmentVersion:stock.assignmentVersion+1},{countId})).toBeNull();
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,{...stock,departmentId:z.department},{countId})).toBeNull();
 expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);
});


it('Service exact mapping replay rolls a late current mapping drift back without pinning lawful historical references',async()=>{
 const fixture=await zeroLedgerServicePartsSource(),{z}=fixture,setup=await import('@erpx/server-commerce/public/service-parts-posting-setup');
 const committed=await fixture.snapshot(),fault=`aaa_service_replay_mapping_${key().replaceAll('-','')}`;
 // Labelled native negative: only this unit's assertion cleanup may alter the
 // current mapping. The original completed receipt is never recreated.
 await db.exec(`CREATE SEQUENCE ${fault}_attempt;CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.command_id LIKE 'service-parts-posting:%' THEN PERFORM nextval('${fault}_attempt');UPDATE commerce_settings SET currency='USD' WHERE company_id='${z.company}';END IF;RETURN OLD;END $$;CREATE TRIGGER ${fault} INSTEAD OF DELETE ON command_assertions FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
 try{await expect(setup.initializeServicePartsPostingSetup(z.reviewer,fixture.mapping,fixture.mappingKey)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}_attempt`).first('is_called')).toBe(true);expect(await fixture.snapshot()).toEqual(committed);}finally{await db.exec(`DROP TRIGGER ${fault} ON command_assertions;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}_attempt`);}
 expect(await setup.initializeServicePartsPostingSetup(z.reviewer,fixture.mapping,fixture.mappingKey)).toEqual({id:z.company,status:'configured',version:1});expect(await fixture.snapshot()).toEqual(committed);
},120000);

it('Service fresh mapping preserves independently approved Sales defaults including omitted VAT and rejects late edition changes',async()=>{
 const service=await import('@erpx/server-finance/public/service-parts-prerequisites'),setup=await import('@erpx/server-commerce/public/service-parts-posting-setup'),sales=await import('@erpx/server-finance/public/sales'),{commerceFinanceJoinedFacts}=await import('@erpx/server-finance/public/commerce-joined-facts');
 for(const withVat of [true,false]){
  const z=await zeroLedgerServicePartsCompany();await z.purchase();
  const envelope=async()=>{const view=await service.servicePartsPrerequisites(z.reviewer);expect(view.available).toBe(true);if(!view.contextKey)throw Error('Actual current Service Finance reference context required');return {expectedVersion:0 as const,contextKey:view.contextKey};};
  const refs=async(label:string)=>{
   const chart=await service.createServicePartsChart(z.reviewer,{...await envelope(),input:{name:`Actual ${label} Company chart`,baseCurrency:'THB',accountingBasis:'TFRS'}},key()),accounts:Record<string,string>={};
   for(const [name,code,category,controlKind] of [['ar','1100','asset','receivable'],['ap','2100','liability','payable'],['revenue','4100','revenue','none'],['inventory','1300','asset','none'],['clearing','2150','liability','none'],['cogs','5100','expense','none'],['inputTax','1150','asset','none'],['outputTax','2151','liability','none'],['cash','1001','asset','bank'],['otherAr','1101','asset','receivable'],['otherRevenue','4101','revenue','none'],['otherVat','2152','liability','none']] as const)accounts[name]=(await service.createServicePartsAccount(z.reviewer,{...await envelope(),input:{chartId:chart.id,code,name:`Actual ${label} ${name}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key())).id;
   return {chart,accounts};
  };
  const reviewed=await refs('reviewed Sales intent'),other=await refs('different valid Service intent');
  // Normal original pre-enforcement Finance preparation and independent review
  // model retained approved intent. No Sales/Finance package or business SQL is
  // added to the actual Service-only Company; no economic document exists yet.
  async function approveDefaults(reason:string){
   vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');
   try{const draft=await sales.prepareSalesDefaults(z.owner,{chartId:reviewed.chart.id,currency:'THB',receivableAccountId:reviewed.accounts.ar,revenueAccountId:reviewed.accounts.revenue,...withVat?{taxAccountId:reviewed.accounts.outputTax}:{},reason},key());await expect(sales.decideSalesDefaults(z.owner,draft.id,{expectedVersion:1,action:'approve',reason:'Original same-person approval remains forbidden'},key())).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});const decision=await sales.decideSalesDefaults(z.manager,draft.id,{expectedVersion:1,action:'approve',reason:'Actual independent Finance review of retained Sales intent'},key());expect(decision).toMatchObject({id:draft.id,status:'approved',version:2});return {id:draft.id,version:2};}finally{vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');}
  }
  const edition=await approveDefaults('Actual independently reviewed pre-existing Sales defaults');
  for(const table of ['commerce_settings','commerce_orders','finance_invoices','finance_payments','finance_journals'])expect(await db.prepare(`SELECT count(*)::int n FROM ${table} WHERE company_id=?`).bind(z.company).first('n')).toBe(0);
  const effective=async()=>{const row=await commerceFinanceJoinedFacts(db).readCommerceSettings1({companyId:z.company});return row?Object.fromEntries(['chart_id','currency','receivable_account_id','revenue_account_id','output_tax_account_id'].map(field=>[field,row[field]??null])):null;},originalEffective=await effective();
  expect(originalEffective).toEqual({chart_id:reviewed.chart.id,currency:'THB',receivable_account_id:reviewed.accounts.ar,revenue_account_id:reviewed.accounts.revenue,output_tax_account_id:withVat?reviewed.accounts.outputTax:null});
  const seed=await setup.servicePartsPostingSetup(z.reviewer);expect(seed).toMatchObject({available:true,configured:false,sourceReady:false,canConfigure:true});if(!seed.contextKey)throw Error('Current Service mapping context required');
  // On the original source, absent metadata remains absent in the original
  // strict schema. The expected rejection therefore reaches the real policy,
  // rather than failing because the diagnostic added an unknown request field.
  const identity='expectedSalesDefaults' in seed?{expectedSalesDefaults:seed.expectedSalesDefaults}:{},body=(selected:typeof reviewed)=>({expectedVersion:0 as const,contextKey:seed.contextKey,...identity,chartId:selected.chart.id,currency:'THB',receivableAccountId:selected.accounts.ar,payableAccountId:selected.accounts.ap,revenueAccountId:selected.accounts.revenue,inventoryAccountId:selected.accounts.inventory,clearingAccountId:selected.accounts.clearing,cogsAccountId:selected.accounts.cogs,inputTaxAccountId:selected.accounts.inputTax,outputTaxAccountId:selected.accounts.outputTax,cashAccountId:selected.accounts.cash,reason:'Current Finance deliberately preserves reviewed Sales intent while adding Service parts roles'});
  const image=async()=>Object.fromEntries(await Promise.all(['finance_charts','finance_accounts','finance_sales_defaults','commerce_settings','commerce_service_parts_posting_sources','finance_journals','finance_journal_lines','finance_invoices','finance_payments','command_receipts','command_assertions','audit_events','outbox_events'].map(async table=>[table,await db.prepare(`SELECT COALESCE(jsonb_agg(to_jsonb(actual) ORDER BY to_jsonb(actual)::text),'[]'::jsonb)::text image FROM ${table} actual`).first<string>('image')]))),before=await image();
  await expect(setup.initializeServicePartsPostingSetup(z.reviewer,body(other),key())).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_DEFAULTS_CHANGED'});expect(await image()).toEqual(before);expect(await effective()).toEqual(originalEffective);
  expect(seed).toMatchObject({expectedSalesDefaults:edition,approvedSalesDefaults:{...edition,chartId:reviewed.chart.id,currency:'THB',receivableAccountId:reviewed.accounts.ar,revenueAccountId:reviewed.accounts.revenue,taxAccountId:withVat?reviewed.accounts.outputTax:null}});
  for(const change of [{receivableAccountId:reviewed.accounts.otherAr},{revenueAccountId:reviewed.accounts.otherRevenue},...withVat?[{outputTaxAccountId:reviewed.accounts.otherVat}]:[]]){await expect(setup.initializeServicePartsPostingSetup(z.reviewer,{...body(reviewed),...change},key())).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_DEFAULTS_CHANGED'});expect(await image()).toEqual(before);}
  const command=key(),input=body(reviewed),fault=`service_defaults_final_${key().replaceAll('-','')}`;
  // Adverse native-only fault changes the actual reviewed edition after the
  // new mapping effects. Both the edition and all command effects must abort.
  await db.exec(`CREATE SEQUENCE ${fault}_attempt;CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${z.company}' THEN PERFORM nextval('${fault}_attempt');UPDATE finance_sales_defaults SET reason=reason||' deferred contradiction',version=version+1 WHERE id='${edition.id}';END IF;RETURN NULL;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON commerce_service_parts_posting_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
  try{await expect(setup.initializeServicePartsPostingSetup(z.reviewer,input,command)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}_attempt`).first('is_called')).toBe(true);expect(await image()).toEqual(before);expect(await effective()).toEqual(originalEffective);}finally{await db.exec(`DROP TRIGGER ${fault} ON commerce_service_parts_posting_sources;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}_attempt`);}
  const configured=await setup.initializeServicePartsPostingSetup(z.reviewer,input,command);expect(configured).toEqual({id:z.company,status:'configured',version:1});expect(await effective()).toEqual(originalEffective);expect(await db.prepare('SELECT output_tax_account_id FROM commerce_settings WHERE company_id=?').bind(z.company).first('output_tax_account_id')).toBe(reviewed.accounts.outputTax);
  const committed=await image();expect(await setup.initializeServicePartsPostingSetup(z.reviewer,input,command)).toEqual(configured);expect(await image()).toEqual(committed);
  const later=await approveDefaults('A lawful later independent edition preserves the same reviewed roles');expect(later.id).not.toBe(edition.id);const current=await image();expect(await setup.initializeServicePartsPostingSetup(z.reviewer,input,command)).toEqual(configured);expect(await image()).toEqual(current);expect(await effective()).toEqual(originalEffective);
  expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);
 }
},120000);

// Service customer preparation starts from honest imported Org/HR authority
// only. No Finance customer/chart/account/period/quote/economics are seeded.
async function serviceCustomerCompany(purchased=true){const z=await zeroLedgerServicePartsCompany();if(purchased)await z.store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:z.company,packageId:'service',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Native Service-only customer startup',evidence:'test:Service-customer-no-Finance-or-parts-package'}});const customers=await import('@erpx/server-finance/public/service-customer-preparation');return{z,customers};}
async function serviceCustomerImage(){const tables=['finance_parties','finance_charts','finance_accounts','finance_periods','company_money_settings','company_currency_scales','company_fx_rates','money_transaction_snapshots','command_receipts','command_assertions','audit_events','outbox_events','assignments','company_package_grants'];return Object.fromEntries(await Promise.all(tables.map(async table=>[table,await db.prepare(`SELECT COALESCE(jsonb_agg(to_jsonb(actual) ORDER BY to_jsonb(actual)::text),'[]'::jsonb)::text image FROM ${table} actual`).first<string>('image')])));}
async function serviceCustomerBody(s:Awaited<ReturnType<typeof serviceCustomerCompany>>,code=key()){const view=await s.customers.serviceCustomerPreparation(s.z.owner);expect(view).toMatchObject({available:true,canCreate:true,customers:[]});if(!view.contextKey)throw Error('Actual current Service customer context required');return{expectedVersion:0 as const,contextKey:view.contextKey,input:{kind:'customer' as const,code,name:'First actual Service customer',currency:'USD',paymentTermsDays:30,creditLimit:'0'}};}
it('Service-only Company creates its first native customer and independently active agreement through the closed customer owner without Finance or parts grants',async()=>{
 const s=await serviceCustomerCompany(),{z,customers}=s;expect(await db.prepare('SELECT count(*)::int n FROM finance_parties WHERE company_id=?').bind(z.company).first('n')).toBe(0);const input=await serviceCustomerBody(s),command=key(),created=await customers.createServiceCustomer(z.owner,input,command);expect(created).toMatchObject({id:expect.stringMatching(/^party-/),...input.input,creditLimitMinor:0});expect('version'in created).toBe(false);
 expect(await customers.serviceCustomerConfirmation(z.owner,input,command)).toEqual({commandId:command,companyId:z.company,id:created.id,kind:'customer',status:'active',code:input.input.code,name:input.input.name,currency:'USD'});const settled=await serviceCustomerImage();expect(await customers.createServiceCustomer(z.owner,input,command)).toEqual(created);expect(await serviceCustomerImage()).toEqual(settled);
 const next=await customers.serviceCustomerPreparation(z.owner);if(!next.contextKey)throw Error('Current customer choice context required');await expect(customers.createServiceCustomer(z.owner,{...input,contextKey:next.contextKey},key())).rejects.toMatchObject({status:409,code:'SERVICE_CUSTOMER_CODE_EXISTS'});expect(await serviceCustomerImage()).toEqual(settled);
 const workspace=await fieldServiceWorkspace(z.owner);expect(workspace.customers).toEqual([{id:created.id,name:input.input.name,currency:'USD'}]);const agreement=await createAgreement(z.owner,{branchId:z.branch,departmentId:null,customerId:created.id,assetId:null,title:'First native customer agreement',startDate:'2026-10-01',endDate:'2027-10-01',currency:'USD',visitRateMinor:12345,warrantyDiscountBps:0},key());expect(agreement).toMatchObject({status:'draft',version:1});if(typeof agreement.id!=='string')throw Error('Original agreement ID required');
 await expect(changeAgreement(z.owner,agreement.id,{action:'activate',expectedVersion:1,reason:'Creator cannot review its own first agreement'},key())).rejects.toMatchObject({code:'CONFLICT'});expect(await changeAgreement(z.serviceReviewer,agreement.id,{action:'activate',expectedVersion:1,reason:'Independent review of actual first Service agreement'},key())).toMatchObject({id:agreement.id,status:'active',version:2});
 expect(await db.prepare("SELECT currency,amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='field_service_agreement' AND source_id=? AND amount_kind='visit_rate'").bind(z.company,agreement.id).first()).toEqual({currency:'USD',amount_units:'12345',usd_units:'12345',fx_rate_id:null});expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId)).toEqual(['service']);
 for(const table of ['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','finance_invoices','finance_payments','finance_journals'])expect(await db.prepare(`SELECT count(*)::int n FROM ${table} WHERE company_id=?`).bind(z.company).first('n')).toBe(0);
});
it('Service customer constructor preserves original Company Finance authority, current source and exact input while denying no purchase, other scopes and suppliers',async()=>{
 const s=await serviceCustomerCompany(false),{z,customers}=s,raw={expectedVersion:0 as const,contextKey:'a'.repeat(64),input:{kind:'customer' as const,code:key(),name:'Denied not purchased',currency:'USD',paymentTermsDays:30,creditLimit:'0'}};const before=await serviceCustomerImage();expect(await customers.serviceCustomerPreparation(z.owner)).toMatchObject({available:false,unavailableReason:'purchase-required'});await expect(customers.createServiceCustomer(z.owner,raw,key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await serviceCustomerImage()).toEqual(before);
 await z.store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:z.company,packageId:'service',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Original Service-only customer capability',evidence:'test:closed-customer-authority'}});const input=await serviceCustomerBody(s),baseline=await serviceCustomerImage();
 for(const actor of [z.technician,{...z.owner,companyId:'foreign-company'},await z.actor('customer-branch','finance_preparer','branch'),await z.actor('customer-department','finance_preparer','department')]){const scoped=await serviceCustomerImage();expect(await customers.serviceCustomerPreparation(actor)).toMatchObject({available:false,unavailableReason:'authority-required'});await expect(customers.createServiceCustomer(actor,input,key())).rejects.toMatchObject({status:403});expect(await serviceCustomerImage()).toEqual(scoped);}
 const current=await serviceCustomerImage();await expect(customers.createServiceCustomer(z.owner,{...input,contextKey:'0'.repeat(64)},key())).rejects.toMatchObject({status:409,code:'SERVICE_CUSTOMER_CONTEXT_CHANGED'});expect(await serviceCustomerImage()).toEqual(current);await expect(customers.createServiceCustomer(z.owner,{...input,input:{...input.input,kind:'supplier'}},key())).rejects.toBeDefined();expect(await serviceCustomerImage()).toEqual(current);expect(baseline.finance_parties).toEqual(current.finance_parties);
});
it('Service customer closes born receipt poisoning and deferred customer drift with complete native rollback before exact retry',async()=>{
 const s=await serviceCustomerCompany(),{z,customers}=s,input=await serviceCustomerBody(s),command=key(),before=await serviceCustomerImage(),fault=`service_customer_${key().replaceAll('-','')}`;
 // Labelled robustness-only native trigger: not a normal user mutation path.
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN NEW.request_hash=repeat('0',64);END IF;RETURN NEW;END $$;CREATE TRIGGER ${fault} BEFORE INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
 try{await expect(customers.createServiceCustomer(z.owner,input,command)).rejects.toMatchObject({status:409,code:'IDEMPOTENCY_CONFLICT'});expect(await serviceCustomerImage()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${z.company}' THEN UPDATE finance_parties SET name=name||' deferred drift' WHERE id=NEW.id;END IF;RETURN NULL;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON finance_parties DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
 try{await expect(customers.createServiceCustomer(z.owner,input,command)).rejects.toMatchObject({status:409,code:'SERVICE_CUSTOMER_SOURCE_CHANGED'});expect(await serviceCustomerImage()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON finance_parties;DROP FUNCTION ${fault}()`);}
 const created=await customers.createServiceCustomer(z.owner,input,command),committed=await serviceCustomerImage();expect(await customers.createServiceCustomer(z.owner,input,command)).toEqual(created);expect(await customers.serviceCustomerConfirmation(z.owner,input,command)).toMatchObject({id:created.id,commandId:command});expect(await serviceCustomerImage()).toEqual(committed);expect(await db.prepare("SELECT count(*)::int n FROM audit_events WHERE command_id=? AND action='finance.party.create'").bind(command).first('n')).toBe(1);
});
it('Service customer exact history replay survives addon-free Service expiry while a fresh customer remains denied',async()=>{
 const s=await serviceCustomerCompany(),{z,customers}=s,input=await serviceCustomerBody(s),command=key(),created=await customers.createServiceCustomer(z.owner,input,command),grant=(await readCompanyPackageGrants(db,z.company)).find(row=>row.packageId==='service');if(!grant)throw Error('Original Service grant required');
 await z.store.apply({expectedVersion:grant.version,idempotencyKey:key(),grant:{...grant,endsAt:Date.now()-1,version:grant.version+1,reason:'Native Service-only capability expiry',evidence:'test:customer-history-after-expiry'}});const before=await serviceCustomerImage();expect(await customers.serviceCustomerPreparation(z.owner)).toMatchObject({available:true,canCreate:false});expect(await customers.createServiceCustomer(z.owner,input,command)).toEqual(created);expect(await customers.serviceCustomerConfirmation(z.owner,input,command)).toMatchObject({id:created.id,commandId:command,status:'active'});await expect(customers.createServiceCustomer(z.owner,{...input,input:{...input.input,code:key()}},key())).rejects.toMatchObject({status:403});expect(await serviceCustomerImage()).toEqual(before);
});
it('Service customer late native IAM or purchase edition changes cannot commit a customer or its receipt',async()=>{
 for(const target of ['IAM','purchase'] as const){const s=await serviceCustomerCompany(),{z,customers}=s,input=await serviceCustomerBody(s),command=key(),before=await serviceCustomerImage(),fault=`service_customer_final_${key().replaceAll('-','')}`,mutation=target==='IAM'?`UPDATE assignments SET version=version+1,revoked_at=1 WHERE id='${z.owner.assignmentId}'`:`UPDATE company_package_grants SET version=version+1 WHERE company_id='${z.company}' AND package_id='service'`;
 // Labelled robustness-only deferred fault on the actual selected source.
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${z.company}' THEN ${mutation};END IF;RETURN NULL;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON finance_parties DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
 try{await expect(customers.createServiceCustomer(z.owner,input,command)).rejects.toMatchObject(target==='IAM'?{status:403,code:'QUEUED_AUTHORITY_CHANGED'}:{status:409,code:'SERVICE_CUSTOMER_SOURCE_CHANGED'});expect(await serviceCustomerImage()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON finance_parties;DROP FUNCTION ${fault}()`);}
 expect(await customers.createServiceCustomer(z.owner,input,command)).toMatchObject({kind:'customer',currency:'USD'});
 }
});


// This suite's native Files intake uses the existing actual development ClamAV
// socket/definitions. No simulated readiness, scan result or installed setting.
const serviceScannerEnvironment=attachmentToolEnvironment(process.cwd());
beforeEach(()=>{
 vi.stubEnv('ERPX_CLAMD_SOCKET',serviceScannerEnvironment.ERPX_CLAMD_SOCKET);
 vi.stubEnv('ERPX_CLAMAV_DATABASE_DIR',serviceScannerEnvironment.ERPX_CLAMAV_DATABASE_DIR);
});
