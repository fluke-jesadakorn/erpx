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
  cancelPlanningScenario,
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
import { confirmManualRate } from "../src/server/company-money";
import { createManufacturingAccountingProfile, reviewManufacturingAccountingProfile } from "../src/server/production-accounting";
import { saveProductionResource, saveProductionResourceCalendar } from "../src/server/production-resources";
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
let productionAccounts: { inventory: string; wip: string; labor: string; overhead: string };
beforeAll(async () => {
  enableAttachmentV2ForTest();
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
it("posts material, conversion cost and independently released output atomically, clearing WIP and freezing each money source", async () => {
  // Foreign sales prices cannot relabel the independently reviewed THB stock map.
  await db.prepare("UPDATE commerce_settings SET currency='USD' WHERE company_id=?").bind(f.company).run();
  const { id } = await order(), reserveKey = key(), issueKey = key(), reportKey = key(), qcKey = key();
  const issue = { action: "issue", expectedVersion: 2, entryDate: "2026-10-02", reason: "Issue the source materials under the approved account map" };
  await changeManufacturingOrder(owner, id, { action: "reserve", expectedVersion: 1, expiresAt: Date.now() + 3600000, reason: "Reserve required material quantities" }, reserveKey);
  const issued = await changeManufacturingOrder(owner, id, issue, issueKey);
  expect(await changeManufacturingOrder(owner, id, issue, issueKey)).toEqual(issued);
  expect(await db.prepare(`SELECT COALESCE(sum(l.debit_minor-l.credit_minor),0)::text balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
    WHERE j.company_id=? AND j.status='posted' AND l.account_id=?`).bind(f.company, productionAccounts.wip).first("balance")).toBe("2000");
  await changeManufacturingOrder(owner, id, { action: "report", expectedVersion: 3, outputQuantity: "1.8", scrapQuantity: "0.2", laborCost: "10", overheadCost: "2", entryDate: "2026-10-02", reason: "Record actual conversion costs and normal scrap" }, reportKey);
  const qc = { action: "pass-qc", expectedVersion: 4, entryDate: "2026-10-02", reason: "Independent output inspection accepts the finished quantity" };
  const completed = await changeManufacturingOrder(reviewer, id, qc, qcKey);
  expect(await changeManufacturingOrder(reviewer, id, qc, qcKey)).toEqual(completed);
  const detail = await productionDetail(owner, id);
  expect(detail.postings.map(p => [p.phase, p.amount_minor, p.journal_status])).toEqual([["issue", 2000, "posted"], ["report", 1200, "posted"], ["output", 3200, "posted"]]);
  const balances = (await db.prepare(`SELECT l.account_id, sum(l.debit_minor-l.credit_minor)::text balance FROM finance_journal_lines l
    JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted' GROUP BY l.account_id`).bind(f.company).all<{ account_id: string; balance: string }>()).results;
  expect(balances.find(b => b.account_id === productionAccounts.wip)?.balance).toBe("0");
  expect(balances.find(b => b.account_id === productionAccounts.inventory)?.balance).toBe("1200");
  expect(await db.prepare("SELECT sum(value_minor)::text value FROM commerce_stock WHERE warehouse_id=?").bind(warehouse).first("value")).toBe("6200");
  const snapshots = (await db.prepare(`SELECT s.amount_kind,s.amount_units::text,s.currency,s.currency_scale,s.fx_rate_id
    FROM money_transaction_snapshots s JOIN manufacturing_cost_movements m ON m.id=s.source_id WHERE m.order_id=? AND s.source_type='manufacturing_cost_movement' ORDER BY s.amount_kind`)
    .bind(id).all<{ amount_kind: string; amount_units: string; currency: string; currency_scale: number; fx_rate_id: string }>()).results;
  expect(snapshots.map(s => [s.amount_kind, s.amount_units])).toEqual([["labor", "1000"], ["material_issue", "2000"], ["output", "3200"], ["overhead", "200"]]);
  expect(snapshots.every(s => s.currency === "THB" && s.currency_scale === 2 && Boolean(s.fx_rate_id))).toBe(true);
  await confirmManualRate(f.manager, { currency: "THB", effectiveDate: "2026-10-02", currencyPerUsd: "40", note: "A later quote cannot rewrite recorded production" }, key());
  expect((await db.prepare(`SELECT s.fx_rate_id FROM money_transaction_snapshots s JOIN manufacturing_cost_movements m ON m.id=s.source_id WHERE m.order_id=?`).bind(id).all<{ fx_rate_id: string }>()).results.map(s => s.fx_rate_id).sort()).toEqual(snapshots.map(s => s.fx_rate_id).sort());
  await expect(db.prepare("UPDATE manufacturing_cost_movements SET amount_minor=1 WHERE order_id=?").bind(id).run()).rejects.toMatchObject({ code: "23514" });
});

it("keeps a failed run in WIP through rework, accumulates actual labor and independently scraps unusable output", async () => {
  const { id } = await order();
  await changeManufacturingOrder(owner, id, { action: "reserve", expectedVersion: 1, expiresAt: Date.now() + 3600000, reason: "Reserve material for failed production" }, key());
  await changeManufacturingOrder(owner, id, { action: "issue", expectedVersion: 2, reason: "Issue materials to work in progress" }, key());
  await changeManufacturingOrder(owner, id, { action: "report", expectedVersion: 3, outputQuantity: "1.8", scrapQuantity: "0.2", laborCost: "10", overheadCost: "2", reason: "Report first production costs" }, key());
  await expect(changeManufacturingOrder(owner, id, { action: "fail-qc", expectedVersion: 4, reason: "Reporter cannot fail own output" }, key())).rejects.toMatchObject({ code: "CONFLICT" });
  await changeManufacturingOrder(reviewer, id, { action: "fail-qc", expectedVersion: 4, reason: "Independent measurement fails the batch" }, key());
  await changeManufacturingOrder(owner, id, { action: "rework", expectedVersion: 5, reason: "Attempt controlled rework of failed output" }, key());
  await changeManufacturingOrder(owner, id, { action: "report", expectedVersion: 6, outputQuantity: "1.7", scrapQuantity: "0.3", laborCost: "5", overheadCost: "1", reason: "Additional rework conversion costs" }, key());
  expect((await productionDetail(owner, id)).order).toMatchObject({ labor_cost_minor: 1500, overhead_cost_minor: 300, material_cost_minor: 2000 });
  await changeManufacturingOrder(reviewer, id, { action: "fail-qc", expectedVersion: 7, reason: "Reworked batch remains outside the tolerance" }, key());
  await expect(changeManufacturingOrder(owner, id, { action: "scrap", expectedVersion: 8, reason: "Reporter cannot approve own loss" }, key())).rejects.toMatchObject({ code: "CONFLICT" });
  await changeManufacturingOrder(reviewer, id, { action: "scrap", expectedVersion: 8, reason: "Independent disposal records the production loss" }, key());
  const detail = await productionDetail(owner, id);
  expect(detail.order.status).toBe("cancelled");
  expect(detail.postings.at(-1)).toMatchObject({ phase: "scrap", amount_minor: 3800, journal_status: "posted" });
  expect(await db.prepare("SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(warehouse, output).first()).toBeNull();
  expect(await db.prepare(`SELECT sum(l.debit_minor-l.credit_minor)::text balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
    WHERE j.company_id=? AND j.status='posted' AND l.account_id=?`).bind(f.company, productionAccounts.wip).first("balance")).toBe("0");
});

async function routedSpecification() {
  const resource = await saveProductionResource(reviewer, null, { branchId: f.branch, code: key(), name: "Reviewed assembly cell", dailyMinutes: 20 }, key());
  await saveProductionResourceCalendar(reviewer, String(resource.id), { expectedVersion: 0, resourceVersion: 1, workDate: "2026-10-05", availableMinutes: 0, reason: "Planned equipment shutdown" }, key());
  const spec = await createSpecification(owner, { name: "Routed assembly", mode: "bom", outputProductId: output, batchQuantity: "1", expectedYieldBps: 10000,
    lines: [{ productId: material, quantity: "2" }], operations: [
      { resourceId: resource.id, name: "Prepare and assemble", setupMinutes: 5, runMinutesPerBatch: 10 },
      { resourceId: resource.id, name: "Package and inspect", setupMinutes: 0, runMinutesPerBatch: 5 },
    ] }, key());
  await reviewSpecification(reviewer, String(spec.id), { action: "approve", expectedVersion: 1, reason: "Independent BOM and routing review" }, key());
  return { resourceId: String(resource.id), specificationId: String(spec.id) };
}

it("uses effective resource calendars, compares immutable plans and atomically rejects competing capacity releases", async () => {
  const { resourceId, specificationId } = await routedSpecification(), input = { warehouseId: warehouse, specificationId, specificationVersion: 2,
    demandQuantity: "2", startDate: "2026-10-05", dueDate: "2026-10-06", capacityMinutes: 9999 };
  const first = await createPlanningScenario(owner, input, key()), alternative = await createPlanningScenario(owner, { ...input, comparisonScenarioId: first.id }, key());
  expect(first).toMatchObject({ requiredMinutes: 35, capacityMinutes: 20, feasibleDate: "2026-10-07", context: { capacityBasis: "approved_routing_resource_calendar" } });
  expect(first.context.allocations?.map(x => [x.workDate, x.minutes])).toEqual([["2026-10-06", 20], ["2026-10-07", 15]]);
  expect(alternative.context.comparison).toMatchObject({ scenarioId: first.id, shortageDeltaMilli: 0, requiredMinutesDelta: 0, currentFeasibleDate: "2026-10-07" });
  const outcomes = await Promise.allSettled([first, alternative].map(s => reviewPlanningRecommendation(reviewer, String(s.recommendationId), { action: "approve", expectedVersion: 1, reason: "Independently review a single available routing slot" }, key())));
  expect(outcomes.filter(o => o.status === "fulfilled")).toHaveLength(1);
  expect(outcomes.filter(o => o.status === "rejected")).toHaveLength(1);
  expect(await db.prepare("SELECT sum(minutes)::text minutes FROM planning_resource_allocations WHERE resource_id=? AND released_at IS NULL").bind(resourceId).first("minutes")).toBe("35");
  await expect(saveProductionResourceCalendar(reviewer, resourceId, { expectedVersion: 0, resourceVersion: 1, workDate: "2026-10-06", availableMinutes: 0, reason: "Cannot remove capacity from an approved run" }, key())).rejects.toMatchObject({ code: "CONFLICT" });
  const approved = outcomes.find(o => o.status === "fulfilled")! as PromiseFulfilledResult<Record<string, unknown>>, orderId = String(approved.value.orderId);
  await changeManufacturingOrder(owner, orderId, { action: "reserve", expectedVersion: 1, expiresAt: Date.now() + 3600000, reason: "Reserve the routed material requirements" }, key());
  await changeManufacturingOrder(owner, orderId, { action: "issue", expectedVersion: 2, reason: "Issue materials for the first operation" }, key());
  const detail = await productionDetail(owner, orderId), secondOperation = String(detail.operations[1].id);
  await expect(changeManufacturingOrder(owner, orderId, { action: "complete-operation", expectedVersion: 3, operationId: secondOperation, laborMinutes: 5, machineMinutes: 10, reason: "Do not skip the preceding assembly operation" }, key())).rejects.toMatchObject({ code: "CONFLICT" });
  await expect(changeManufacturingOrder(owner, orderId, { action: "report", expectedVersion: 3, outputQuantity: "2", scrapQuantity: "0", laborCost: "2", overheadCost: "1", reason: "Routing must finish before output release" }, key())).rejects.toMatchObject({ code: "CONFLICT" });
  await changeManufacturingOrder(owner, orderId, { action: "complete-operation", expectedVersion: 3, operationId: String(detail.operations[0].id), laborMinutes: 20, machineMinutes: 25, reason: "Assembly operation completed with machine evidence" }, key());
  await changeManufacturingOrder(owner, orderId, { action: "complete-operation", expectedVersion: 4, operationId: secondOperation, laborMinutes: 5, machineMinutes: 10, reason: "Packaging operation completed with inspection evidence" }, key());
  await changeManufacturingOrder(owner, orderId, { action: "report", expectedVersion: 5, outputQuantity: "2", scrapQuantity: "0", laborCost: "2", overheadCost: "1", reason: "Report actual routed output and costs" }, key());
  await changeManufacturingOrder(reviewer, orderId, { action: "pass-qc", expectedVersion: 6, reason: "Independent inspection completes the routed batch" }, key());
  expect(await db.prepare("SELECT count(*)::text remaining FROM planning_resource_allocations WHERE resource_id=? AND released_at IS NULL").bind(resourceId).first("remaining")).toBe("0");
  const unapproved = outcomes[0].status === "fulfilled" ? alternative : first, cancelKey = key(), cancellation = { action: "cancel", expectedVersion: 1, reason: "Retire the unused comparison plan" };
  const cancelled = await cancelPlanningScenario(owner, String(unapproved.id), cancellation, cancelKey);
  expect(await cancelPlanningScenario(owner, String(unapproved.id), cancellation, cancelKey)).toEqual(cancelled);
  expect(await db.prepare("SELECT action FROM planning_scenario_events WHERE scenario_id=? ORDER BY version").bind(unapproved.id).all()).toMatchObject({ results: [{ action: "calculate" }, { action: "cancel" }] });
});

it("issues exact material lot cost and retains balanced serial output genealogy in the shared inventory ledger", async () => {
  const lotId = `material-lot-${key()}`;
  await db.batch([
    db.prepare("DELETE FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(warehouse, material),
    db.prepare("UPDATE commerce_products SET tracking_mode='lot' WHERE id=?").bind(material),
    db.prepare("UPDATE commerce_products SET tracking_mode='serial' WHERE id=?").bind(output),
    db.prepare("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,manufactured_date,expiry_date,created_by,created_at) VALUES(?,?,?,'lot','RAW-LOT-A','2026-09-01','2026-12-31',?,1)")
      .bind(lotId, f.company, material, owner.userId),
    db.prepare("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES(?,?,10000,5000)").bind(lotId, warehouse),
    db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES(?,?,10000,5000)").bind(warehouse, material),
  ]);
  const { id } = await order();
  await changeManufacturingOrder(owner, id, { action: "reserve", expectedVersion: 1, expiresAt: Date.now() + 3600000, reason: "Reserve tracked materials for finished serials" }, key());
  await expect(changeManufacturingOrder(owner, id, { action: "issue", expectedVersion: 2, reason: "Unallocated lot issues are rejected" }, key())).rejects.toMatchObject({ code: "MANUFACTURING_LOT_ALLOCATION_REQUIRED" });
  const issueKey = key(), issue = { action: "issue", expectedVersion: 2, entryDate: "2026-10-02", reason: "Use the selected source lot with exact issued cost",
    materialLotAllocations: [{ productId: material, lotId, quantity: "4", balanceVersion: 1 }] };
  await changeManufacturingOrder(owner, id, issue, issueKey);
  await changeManufacturingOrder(owner, id, issue, issueKey);
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?").bind(lotId, warehouse).first()).toEqual({ quantity_milli: 6000, value_minor: 3000 });
  await changeManufacturingOrder(owner, id, { action: "report", expectedVersion: 3, outputQuantity: "2", scrapQuantity: "0", laborCost: "10.01", overheadCost: "2", entryDate: "2026-10-02", reason: "Report exact conversion cost for two serial units" }, key());
  await expect(changeManufacturingOrder(reviewer, id, { action: "pass-qc", expectedVersion: 4, reason: "Serial identities are required before output can be accepted" }, key())).rejects.toMatchObject({ code: "MANUFACTURING_OUTPUT_LOTS_REQUIRED" });
  const accepted = { action: "pass-qc", expectedVersion: 4, entryDate: "2026-10-02", reason: "Independent inspection accepts both serialized units",
    outputLotAllocations: [{ serialNumber: "FINISHED-SERIAL-A", quantity: "1" }, { serialNumber: "FINISHED-SERIAL-B", quantity: "1" }] }, qcKey = key();
  await changeManufacturingOrder(reviewer, id, accepted, qcKey);
  await changeManufacturingOrder(reviewer, id, accepted, qcKey);
  const detail = await productionDetail(owner, id);
  expect(detail.lotGenealogy.map(g => [g.input_lot_id, g.output_serial_number, g.input_quantity_milli])).toEqual([[lotId, "FINISHED-SERIAL-A", 2000], [lotId, "FINISHED-SERIAL-B", 2000]]);
  expect((await db.prepare("SELECT b.quantity_milli,b.value_minor FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE l.product_id=? ORDER BY l.serial_number")
    .bind(output).all()).results).toEqual([{ quantity_milli: 1000, value_minor: 1601 }, { quantity_milli: 1000, value_minor: 1600 }]);
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(warehouse, output).first()).toEqual({ quantity_milli: 2000, value_minor: 3201 });
  expect((await db.prepare("SELECT source_type,quantity_milli,value_minor FROM inventory_ledger WHERE source_id=? ORDER BY quantity_milli").bind(id).all()).results)
    .toEqual([{ source_type: "manufacturing_issue", quantity_milli: -4000, value_minor: -2000 }, { source_type: "manufacturing_output", quantity_milli: 2000, value_minor: 3201 }]);
  await expect(db.prepare("DELETE FROM manufacturing_lot_genealogy WHERE order_id=?").bind(id).run()).rejects.toMatchObject({ code: "23514" });
});
beforeEach(async () => {
  f = await expenseFixture();
  owner = { ...f.owner, modules: ["planning", "manufacturing"] };
  reviewer = { ...f.tie, role: "holding_owner", assignmentId: await f.grant(f.tie, "holding_owner"), modules: ["planning", "manufacturing"] };
  await db.batch([owner,reviewer].map(actor=>db.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify(actor.modules),actor.assignmentId)));
  productionAccounts = Object.fromEntries(["inventory", "wip", "labor", "overhead"].map(name => [name, `${f.company}-production-${name}`])) as typeof productionAccounts;
  await db.batch(Object.entries(productionAccounts).map(([name, id]) => db.prepare(`INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at)
    VALUES(?,?,?,?,?,?,'THB',1,1)`).bind(id, f.chart, f.company, `production-${name}`, name, ["inventory", "wip"].includes(name) ? "asset" : "liability")));
  await db.prepare("UPDATE commerce_settings SET inventory_account_id=? WHERE company_id=?").bind(productionAccounts.inventory, f.company).run();
  const profile = await createManufacturingAccountingProfile(f.reviewer, { chartId: f.chart, inventoryAccountId: productionAccounts.inventory,
    wipAccountId: productionAccounts.wip, laborAccountId: productionAccounts.labor, overheadAccountId: productionAccounts.overhead, scrapAccountId: f.accounts.expense }, key());
  await reviewManufacturingAccountingProfile(f.manager, String(profile.id), { action: "approve", expectedVersion: 1, reason: "Review controlled production account mapping" }, key());
  await confirmManualRate(f.manager, { currency: "THB", effectiveDate: "2026-01-01", currencyPerUsd: "35", note: "Isolated test manual rate" }, key());
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
  const postingClock = vi.spyOn(Date, "now").mockReturnValue(Date.parse("2026-10-02T12:00:00Z"));
  try {
    await changeManufacturingOrder(owner, orderId, { action: "issue", expectedVersion: 2, entryDate: "2026-10-02", reason: "Issue reserved material to WIP" }, key());
    await changeManufacturingOrder(owner, orderId, { action: "report", expectedVersion: 3, entryDate: "2026-10-02", outputQuantity: "1.8", scrapQuantity: "0.2", laborCost: "10.00", overheadCost: "2.00", reason: "Record actual production output" }, key());
    await changeManufacturingOrder(reviewer, orderId, { action: "pass-qc", expectedVersion: 4, entryDate: "2026-10-02", reason: "Independent inspection passed" }, key());
  } finally {
    postingClock.mockRestore();
  }

  const now = Date.parse("2026-10-06T12:00:00Z"), postedJournal = `mfg-journal-${key()}`, draftJournal = `mfg-draft-${key()}`;
  const insertJournal = async (id: string, number: string, status: "posted" | "draft", postedAt: number | null) => db.prepare(`INSERT INTO finance_journals
    (id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,is_opening,version,created_by,posted_by,posted_at,created_at,updated_at)
    VALUES(?,?,?,?,?,'2026-10-06','THB',1000000,'manufacturing_cost',?,'English test production cost',?,0,1,?,?,?, ?,?)`)
    .bind(id, f.company, f.chart, f.period, number, id, status, owner.userId, status === "posted" ? reviewer.userId : null, postedAt, now, now).run();
  await insertJournal(postedJournal, `EX-MFG-${postedJournal.slice(-8)}`, "draft", null);
  await insertJournal(draftJournal, `EX-MFG-${draftJournal.slice(-8)}`, "draft", null);
  await db.prepare(`INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal)
    VALUES(?,?,?,'English test posted manufacturing costs',2750,0,2750,0,'{}',0),(?,?,?,'English test balancing line',0,2750,0,2750,'{}',1)`)
    .bind(key(), postedJournal, f.accounts.expense, key(), postedJournal, f.accounts.cash).run();
  await db.prepare("UPDATE finance_journals SET status='posted',posted_by=?,posted_at=? WHERE id=?")
    .bind(reviewer.userId,now,postedJournal).run();

  const movementIds = { issue: key(), returned: key(), labor: key(), overhead: key(), draft: key() };
  const movement = (id: string, kind: string, amount: number, journalId: string, source: string) => db.prepare(`INSERT INTO manufacturing_cost_movements
    (id,company_id,order_id,movement_kind,quantity_milli,amount_minor,source_type,source_id,calculation_run_id,journal_id,occurred_on,version,created_by,created_at)
    VALUES(?,?,?, ?,0,?,?,?,NULL,?,'2026-10-06',1,?,?)`).bind(id, f.company, orderId, kind, amount, source, id, journalId, owner.userId, now).run();
  await movement(movementIds.issue, "material_issue", 2000, postedJournal, "inventory_issue");
  await movement(movementIds.returned, "material_return", -250, postedJournal, "inventory_return");
  await movement(movementIds.labor, "labor", 1000, postedJournal, "labor_entry");
  await movement(movementIds.overhead, "overhead", 8000, postedJournal, "overhead_estimate");
  await movement(movementIds.draft, "labor", 9000, draftJournal, "unposted_labor");

  const filters = { from: "2026-10-06", to: "2026-10-06", currency: "THB" };
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

it('rolls back independently released Planning capacity and Manufacturing order on late outbox failure, then retries and replays',async()=>{
 const {resourceId,specificationId}=await routedSpecification(),plan=await createPlanningScenario(owner,{warehouseId:warehouse,specificationId,specificationVersion:2,demandQuantity:'2',startDate:'2026-10-05',dueDate:'2026-10-06'},key());
 const command=key(),input={action:'approve',expectedVersion:1,reason:'Independent reviewer authorizes protected routing capacity'},suffix=key().replace(/[^a-zA-Z0-9]/g,''),fault=`planning_release_fault_${suffix}`,trigger=`planning_release_outbox_${suffix}`;
 await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN RAISE EXCEPTION 'PLANNING_RELEASE_LATE_FAILURE'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${trigger} AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
 try{
  await expect(reviewPlanningRecommendation(reviewer,plan.recommendationId,input,command)).rejects.toThrow('PLANNING_RELEASE_LATE_FAILURE');
  expect(await db.prepare('SELECT status,version FROM planning_scenarios WHERE id=?').bind(plan.id).first()).toEqual({status:'calculated',version:1});
  expect(await db.prepare('SELECT status,version FROM planning_recommendations WHERE id=?').bind(plan.recommendationId).first()).toEqual({status:'proposed',version:1});
  expect(await db.prepare('SELECT count(*)::int count FROM planning_resource_allocations WHERE resource_id=?').bind(resourceId).first('count')).toBe(0);
  expect(await db.prepare('SELECT count(*)::int count FROM manufacturing_orders WHERE recommendation_id=?').bind(plan.recommendationId).first('count')).toBe(0);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*)::int count FROM ${table} WHERE command_id=?`).bind(command).first('count')).toBe(0);
 }finally{await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 const receipt=await reviewPlanningRecommendation(reviewer,plan.recommendationId,input,command);expect(await reviewPlanningRecommendation(reviewer,plan.recommendationId,input,command)).toEqual(receipt);
 expect(await db.prepare('SELECT sum(minutes)::int minutes FROM planning_resource_allocations WHERE resource_id=? AND released_at IS NULL').bind(resourceId).first('minutes')).toBe(35);
 expect(await db.prepare('SELECT count(*)::int count FROM manufacturing_orders WHERE recommendation_id=?').bind(plan.recommendationId).first('count')).toBe(1);
});

it('rolls back released output stock, WIP journal and Files binding on late Manufacturing outbox failure, then retries and replays',async()=>{
 const {id}=await order();await changeManufacturingOrder(owner,id,{action:'reserve',expectedVersion:1,expiresAt:Date.now()+3600000,reason:'Reserve material for atomic output proof'},key());
 await changeManufacturingOrder(owner,id,{action:'issue',expectedVersion:2,reason:'Issue reserved material under the approved map',entryDate:'2026-10-02'},key());
 await changeManufacturingOrder(owner,id,{action:'report',expectedVersion:3,outputQuantity:'2',scrapQuantity:'0',laborCost:'10',overheadCost:'2',reason:'Report exact costs before independent output review',entryDate:'2026-10-02'},key());
 const evidence=await seedAttachmentSet(db,owner,{targetType:'production_run',targetId:id,purposeKey:'qc_evidence',componentRef:'qc-review',expectedTargetVersion:4});
 const command=key(),input={action:'pass-qc',expectedVersion:4,entryDate:'2026-10-02',reason:'Independent inspector releases accepted output with retained scanned evidence',attachmentSetIds:[evidence.setId]},suffix=key().replace(/[^a-zA-Z0-9]/g,''),fault=`manufacturing_issue_fault_${suffix}`,trigger=`manufacturing_issue_outbox_${suffix}`;
 await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN RAISE EXCEPTION 'MANUFACTURING_ISSUE_LATE_FAILURE'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${trigger} AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
 try{
  await expect(changeManufacturingOrder(reviewer,id,input,command)).rejects.toThrow('MANUFACTURING_ISSUE_LATE_FAILURE');
  expect(await db.prepare('SELECT status,version,material_cost_minor FROM manufacturing_orders WHERE id=?').bind(id).first()).toEqual({status:'awaiting_qc',version:4,material_cost_minor:2000});
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,output).first()).toBeNull();
  expect(await db.prepare('SELECT consumed_milli FROM manufacturing_material_reservations WHERE order_id=?').bind(id).first('consumed_milli')).toBe(4000);
  expect(await db.prepare("SELECT count(*)::int count FROM finance_journals WHERE source_type='manufacturing_output' AND source_id=?").bind(id).first('count')).toBe(0);
  expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*)::int count FROM ${table} WHERE command_id=?`).bind(command).first('count')).toBe(0);
 }finally{await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 const receipt=await changeManufacturingOrder(reviewer,id,input,command);expect(await changeManufacturingOrder(reviewer,id,input,command)).toEqual(receipt);
 expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:'bound',link_state:'bound'});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,output).first()).toEqual({quantity_milli:2000,value_minor:3200});
 expect(await db.prepare("SELECT count(*)::int count FROM manufacturing_order_postings WHERE order_id=? AND phase='output'").bind(id).first('count')).toBe(1);
});
