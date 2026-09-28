import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import { saveMaster } from "../src/server/commerce";
import {
  changeManufacturingOrder,
  createPlanningScenario,
  createSpecification,
  productionDetail,
  reviewPlanningRecommendation,
  reviewSpecification,
} from "../src/server/production";
import type { WorkContext } from "../src/server/authorization";
import { escalateInventoryReservations } from "../src/server/inventory-reservation-alerts";
import { queryMetric } from "../src/server/executive/metrics";
import { readExecutiveEvidence, searchExecutiveEvidence } from "../src/server/executive/evidence";
import { attachmentBindingState, enableAttachmentV2ForTest, seedAttachmentSet } from "./helpers/attachment-v2";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  owner: WorkContext,
  reviewer: WorkContext,
  warehouse: string,
  material: string,
  output: string;
beforeAll(async () => {
  enableAttachmentV2ForTest();
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  owner = { ...f.owner, modules: ["planning", "manufacturing"] };
  reviewer = { ...f.tie, modules: ["planning", "manufacturing"] };
  warehouse = String(
    (
      await saveMaster(
        f.owner,
        "warehouses",
        null,
        { branchId: f.branch, code: key(), name: "Production warehouse" },
        key(),
      )
    ).id,
  );
  material = String(
    (
      await saveMaster(
        f.owner,
        "products",
        null,
        { sku: key(), name: "Raw material", price: "5" },
        key(),
      )
    ).id,
  );
  output = String(
    (
      await saveMaster(
        f.owner,
        "products",
        null,
        { sku: key(), name: "Finished item", price: "20" },
        key(),
      )
    ).id,
  );
  await db
    .prepare(
      "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,10000,5000,1)",
    )
    .bind(warehouse, material)
    .run();
});
async function order(mode: "bom" | "formula" = "bom", demand = "2") {
  const spec = await createSpecification(
    owner,
    {
      name: `${mode} specification`,
      mode,
      outputProductId: output,
      batchQuantity: "1",
      expectedYieldBps: 9500,
      lines: [{ productId: material, quantity: "2", substitutionGroup: null }],
    },
    key(),
  );
  await expect(
    reviewSpecification(
      owner,
      String(spec.id),
      {
        action: "approve",
        expectedVersion: 1,
        reason: "Self approval forbidden",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await reviewSpecification(
    reviewer,
    String(spec.id),
    {
      action: "approve",
      expectedVersion: 1,
      reason: "Material and yield reviewed independently",
    },
    key(),
  );
  const plan = await createPlanningScenario(
    owner,
    {
      warehouseId: warehouse,
      specificationId: spec.id,
      specificationVersion: 2,
      demandQuantity: demand,
      dueDate: "2026-10-01",
      capacityMinutes: 10,
    },
    key(),
  );
  const release = await reviewPlanningRecommendation(
    reviewer,
    String(plan.recommendationId),
    {
      action: "approve",
      expectedVersion: 1,
      reason: "Demand, shortage and capacity reviewed",
    },
    key(),
  );
  return { spec, plan, id: String(release.orderId) };
}
it("reconciles posted production cost by month and opens only authorized movement evidence", async () => {
  const { id: orderId } = await order();
  await changeManufacturingOrder(owner, orderId, { action: "reserve", expectedVersion: 1, expiresAt: Date.now() + 3600000, reason: "Reserve approved plan materials" }, key());
  await changeManufacturingOrder(owner, orderId, { action: "issue", expectedVersion: 2, reason: "Issue reserved material to WIP" }, key());
  await changeManufacturingOrder(owner, orderId, { action: "report", expectedVersion: 3, outputQuantity: "1.8", scrapQuantity: "0.2", laborCost: "10.00", overheadCost: "2.00", reason: "Record actual production output" }, key());
  await changeManufacturingOrder(reviewer, orderId, { action: "pass-qc", expectedVersion: 4, reason: "Independent inspection passed" }, key());

  const now = Date.parse("2026-10-06T12:00:00Z"), postedJournal = `mfg-journal-${key()}`, draftJournal = `mfg-draft-${key()}`;
  const insertJournal = async (id: string, number: string, status: "posted" | "draft", postedAt: number | null) => db.prepare(`INSERT INTO finance_journals
    (id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,is_opening,version,created_by,posted_by,posted_at,created_at,updated_at)
    VALUES(?,?,?,?,?,'2026-10-06','THB',1000000,'manufacturing_cost',?,'English test production cost',?,0,1,?,?,?, ?,?)`)
    .bind(id, f.company, f.chart, f.period, number, id, status, owner.userId, status === "posted" ? reviewer.userId : null, postedAt, now, now).run();
  await insertJournal(postedJournal, `EX-MFG-${postedJournal.slice(-8)}`, "posted", now);
  await insertJournal(draftJournal, `EX-MFG-${draftJournal.slice(-8)}`, "draft", null);
  await db.prepare(`INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal)
    VALUES(?,?,?,'English test posted manufacturing costs',2750,0,2750,0,'{}',0),(?,?,?,'English test balancing line',0,2750,0,2750,'{}',1)`)
    .bind(key(), postedJournal, f.accounts.expense, key(), postedJournal, f.accounts.cash).run();

  const movementIds = { issue: key(), returned: key(), labor: key(), overhead: key(), draft: key() };
  const movement = (id: string, kind: string, amount: number, journalId: string, source: string) => db.prepare(`INSERT INTO manufacturing_cost_movements
    (id,company_id,order_id,movement_kind,quantity_milli,amount_minor,source_type,source_id,calculation_run_id,journal_id,occurred_on,version,created_by,created_at)
    VALUES(?,?,?, ?,0,?,?,?,NULL,?,'2026-10-06',1,?,?)`).bind(id, f.company, orderId, kind, amount, source, id, journalId, owner.userId, now).run();
  await movement(movementIds.issue, "material_issue", 2000, postedJournal, "inventory_issue");
  await movement(movementIds.returned, "material_return", -250, postedJournal, "inventory_return");
  await movement(movementIds.labor, "labor", 1000, postedJournal, "labor_entry");
  await movement(movementIds.overhead, "overhead", 8000, postedJournal, "overhead_estimate");
  await movement(movementIds.draft, "labor", 9000, draftJournal, "unposted_labor");

  const filters = { from: "2026-10-01", to: "2026-10-31", currency: "THB" };
  const actor = { ...owner, modules: ["planning", "manufacturing"] };
  const total = await queryMetric(db, actor, "production-cost", filters);
  const byMonth = await queryMetric(db, actor, "production-cost", filters, "month");
  const byProduct = await queryMetric(db, actor, "production-cost", filters, "product");
  expect(total).toMatchObject({ total: "2750", unit: "money", currency: "THB", status: "ready" });
  expect(byMonth.rows).toEqual([{ label: "2026-10", value: "2750" }]);
  expect(byProduct.rows).toEqual([{ label: "Finished item", value: "2750" }]);
  expect((await queryMetric(db, actor, "production-cost", { ...filters, from: "2026-11-01", to: "2026-11-30" })).status).toBe("empty");

  const found = await searchExecutiveEvidence(db, actor, { metric: "production-cost", filters, limit: 2 });
  expect(found.provider).toBe("manufacturing-cost-movements");
  expect(found.items).toHaveLength(2);
  expect(found.hasMore).toBe(true);
  const searched = await searchExecutiveEvidence(db, actor, { metric: "production-cost", filters, query: movementIds.labor, limit: 5 });
  expect(searched.items.map((item) => item.id)).toEqual([movementIds.labor]);
  const detail = await readExecutiveEvidence(db, actor, "production-cost", movementIds.issue, filters);
  expect(detail.record).toMatchObject({ order_id: orderId, movement_kind: "material_issue", amount_minor: "2000", journal_status: "posted", order_status: "completed", product_name: "Finished item" });
  const audit = (await db.prepare("SELECT action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? ORDER BY created_at,id")
    .bind(f.company, "manufacturing-cost-movements").all<{ action: string; record_id: string | null; record_version: number | null; fields: string }>()).results;
  expect(audit.some((row) => row.action === "open" && row.record_id === movementIds.issue && row.record_version === 1)).toBe(true);
  expect(JSON.stringify(audit)).not.toContain("English test production cost");
  await expect(queryMetric(db, { ...actor, modules: [] }, "production-cost", filters)).rejects.toMatchObject({ code: "METRIC_FORBIDDEN" });
  await expect(searchExecutiveEvidence(db, { ...actor, modules: [] }, { metric: "production-cost", filters })).rejects.toMatchObject({ code: "EVIDENCE_FORBIDDEN" });
  await expect(queryMetric(db, { ...actor, departmentId: f.department }, "production-cost", filters)).rejects.toMatchObject({ code: "METRIC_FORBIDDEN" });
});
it("runs BOM planning through reservation, actual issue, output, independent QC, cost and genealogy", async () => {
  const { id } = await order();
  const reportFiles = await seedAttachmentSet(db, owner, {
    targetType: "production_run",
    targetId: id,
    purposeKey: "run_report_evidence",
    componentRef: "output-report",
    expectedTargetVersion: 3,
  });
  const reportCommand = key();
  await changeManufacturingOrder(
    owner,
    id,
    {
      action: "reserve",
      expectedVersion: 1,
      expiresAt: Date.now() + 3600000,
      reason: "Reserve approved plan materials",
    },
    key(),
  );
  await changeManufacturingOrder(
    owner,
    id,
    {
      action: "issue",
      expectedVersion: 2,
      reason: "Issue reserved material to WIP",
    },
    key(),
  );
  expect(
    await db
      .prepare(
        "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?",
      )
      .bind(warehouse, material)
      .first(),
  ).toEqual({ quantity_milli: 6000, value_minor: 3000 });
  const wrongReportFiles = await seedAttachmentSet(db, owner, {
    targetType: "production_run",
    targetId: id,
    purposeKey: "run_sheet",
    componentKind: "record",
    componentRef: "output-report",
    expectedTargetVersion: 3,
  });
  await expect(
    changeManufacturingOrder(
      owner,
      id,
      {
        action: "report",
        expectedVersion: 3,
        outputQuantity: "1.8",
        scrapQuantity: "0.2",
        laborCost: "10.00",
        overheadCost: "2.00",
        reason: "Reject a record document submitted as output evidence",
        attachmentSetIds: [wrongReportFiles.setId],
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "MANUFACTURING_ATTACHMENT_PURPOSE" });
  expect(await attachmentBindingState(db, wrongReportFiles.setId)).toMatchObject({
    set_state: "draft",
    link_state: "draft",
  });
  await changeManufacturingOrder(
    owner,
    id,
    {
      action: "report",
      expectedVersion: 3,
      outputQuantity: "1.8",
      scrapQuantity: "0.2",
      laborCost: "10.00",
      overheadCost: "2.00",
      reason: "Actual output and scrap recorded",
      attachmentSetIds: [reportFiles.setId],
    },
    reportCommand,
  );
  expect(await attachmentBindingState(db, reportFiles.setId)).toMatchObject({
    set_state: "bound",
    link_state: "bound",
    binding_type: "manufacturing_report",
    binding_id: `mfg-event-${reportCommand}`,
  });
  await expect(
    changeManufacturingOrder(
      owner,
      id,
      {
        action: "pass-qc",
        expectedVersion: 4,
        reason: "Reporter cannot self-review",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  const qcFiles = await seedAttachmentSet(db, reviewer, {
    targetType: "production_run",
    targetId: id,
    purposeKey: "qc_evidence",
    componentRef: "qc-review",
    expectedTargetVersion: 4,
  });
  const qcCommand = key();
  await changeManufacturingOrder(
    reviewer,
    id,
    {
      action: "pass-qc",
      expectedVersion: 4,
      reason: "Independent inspection passed",
      attachmentSetIds: [qcFiles.setId],
    },
    qcCommand,
  );
  expect(await attachmentBindingState(db, qcFiles.setId)).toMatchObject({
    set_state: "bound",
    link_state: "bound",
    binding_type: "manufacturing_pass-qc",
    binding_id: `mfg-event-${qcCommand}`,
  });
  const d = await productionDetail(owner, id);
  expect(d.order).toMatchObject({
    status: "completed",
    material_cost_minor: 2000,
    labor_cost_minor: 1000,
    overhead_cost_minor: 200,
  });
  expect(d.cost).toMatchObject({
    total_minor: 3200,
    output_quantity_milli: 1800,
    source_order_version: 5,
  });
  expect(d.genealogy).toHaveLength(1);
  expect(
    await db
      .prepare(
        "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?",
      )
      .bind(warehouse, output)
      .first(),
  ).toEqual({ quantity_milli: 1800, value_minor: 3200 });
});
it("binds failed-QC and rework evidence to their immutable production events", async () => {
  const { id } = await order();
  await changeManufacturingOrder(owner, id, { action: "reserve", expectedVersion: 1, expiresAt: Date.now() + 3600000, reason: "Reserve materials for a controlled rework test" }, key());
  await changeManufacturingOrder(owner, id, { action: "issue", expectedVersion: 2, reason: "Issue the reserved materials" }, key());
  await changeManufacturingOrder(owner, id, { action: "report", expectedVersion: 3, outputQuantity: "1.8", scrapQuantity: "0.2", laborCost: "10.00", overheadCost: "2.00", reason: "Report output for independent failed-QC review" }, key());
  const qcFiles = await seedAttachmentSet(db, reviewer, { targetType: "production_run", targetId: id, purposeKey: "qc_evidence", componentRef: "qc-review", expectedTargetVersion: 4 });
  const qcCommand = key();
  await changeManufacturingOrder(reviewer, id, { action: "fail-qc", expectedVersion: 4, reason: "Measured output is outside the approved tolerance", attachmentSetIds: [qcFiles.setId] }, qcCommand);
  expect(await attachmentBindingState(db, qcFiles.setId)).toMatchObject({ set_state: "bound", link_state: "bound", binding_type: "manufacturing_fail-qc", binding_id: `mfg-event-${qcCommand}` });
  const reworkFiles = await seedAttachmentSet(db, owner, { targetType: "production_run", targetId: id, purposeKey: "rework_action_evidence", componentRef: "rework", expectedTargetVersion: 5 });
  const reworkCommand = key();
  await changeManufacturingOrder(owner, id, { action: "rework", expectedVersion: 5, reason: "Approved rework instruction returns this run to production", attachmentSetIds: [reworkFiles.setId] }, reworkCommand);
  expect(await attachmentBindingState(db, reworkFiles.setId)).toMatchObject({ set_state: "bound", link_state: "bound", binding_type: "manufacturing_rework", binding_id: `mfg-event-${reworkCommand}` });
  expect((await productionDetail(owner, id)).order.status).toBe("in_progress");
});
it("calculates shortages and a capacity-delayed feasible date without changing stock", async () => {
  const { plan } = await order("formula", "4");
  expect(plan.shortages).toEqual([
    expect.objectContaining({
      requiredMilli: 8000,
      availableMilli: 10000,
      shortageMilli: 0,
    }),
  ]);
  expect(plan.feasibleDate).toBe("2026-10-02");
  expect(
    await db
      .prepare(
        "SELECT quantity_milli FROM commerce_stock WHERE warehouse_id=? AND product_id=?",
      )
      .bind(warehouse, material)
      .first("quantity_milli"),
  ).toBe(10000);
});
it("serializes last-material reservations across competing released orders", async () => {
  const a = await order("bom", "3"),
    b = await order("bom", "3"),
    input = {
      action: "reserve" as const,
      expectedVersion: 1,
      expiresAt: Date.now() + 3600000,
      reason: "Competing material reservation",
    };
  const results = await Promise.allSettled([
    changeManufacturingOrder(owner, a.id, input, key()),
    changeManufacturingOrder(owner, b.id, input, key()),
  ]);
  expect(results.filter((x) => x.status === "fulfilled")).toHaveLength(1);
  expect(
    Number(
      await db
        .prepare(
          "SELECT sum(quantity_milli-consumed_milli-released_milli) n FROM manufacturing_material_reservations",
        )
        .first("n"),
    ),
  ).toBe(6000);
});
it("warns the manufacturing owner before material protection expires and escalates the released availability", async () => {
  const now = Date.now(), { id } = await order();
  await changeManufacturingOrder(owner, id, { action: "reserve", expectedVersion: 1, expiresAt: now + 3600000, reason: "Reserve materials for approved production" }, key());
  expect((await escalateInventoryReservations(db, now)).expiring).toBeGreaterThanOrEqual(1);
  expect(await db.prepare("SELECT record_type,message FROM staff_notifications WHERE record_id=? AND user_id=? AND message LIKE 'Stock reservation%'").bind(id,owner.userId).first()).toMatchObject({ record_type: "manufacturing_order", message: expect.stringContaining("expires within 24 hours") });
  await db.prepare("UPDATE manufacturing_material_reservations SET expires_at=? WHERE order_id=?").bind(now - 1, id).run();
  expect((await escalateInventoryReservations(db, now)).expired).toBeGreaterThanOrEqual(1);
  expect(await escalateInventoryReservations(db, now)).toEqual({ expiring: 0, expired: 0 });
});
