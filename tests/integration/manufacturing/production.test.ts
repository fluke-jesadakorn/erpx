import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import { saveMaster } from "../../../src/server/modules/commerce/public/commerce";
import {
  changeManufacturingOrder,
  createPlanningScenario,
  cancelPlanningScenario,
  createSpecification,
  productionDetail,
  reviewPlanningRecommendation,
  reviewSpecification,
} from "../../../src/server/modules/manufacturing/public/production";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import type {FileStore} from '../../../src/server/modules/runtime/public/runtime/types';
import { escalateInventoryReservations } from "../../../src/server/modules/inventory/public/inventory-reservation-alerts";
import { queryMetric } from "../../../src/server/modules/runtime/public/executive/metrics";
import { readExecutiveEvidence, searchExecutiveEvidence } from "../../../src/server/modules/runtime/public/executive/evidence";
import { attachmentBindingState, enableAttachmentV2ForTest, seedAttachmentSet } from "../../support/helpers/attachment-v2";
import { confirmManualRate } from "../../../src/server/modules/finance/public/company-money";
import { createManufacturingAccountingProfile, reviewManufacturingAccountingProfile } from "../../../src/server/modules/manufacturing/public/production-accounting";
import { saveProductionResource, saveProductionResourceCalendar } from "../../../src/server/modules/manufacturing/public/production-resources";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: manufacturingDatabase ?? env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let manufacturingDatabase:import("../../../src/server/modules/runtime/public/runtime/types").SqlDatabase|undefined;
afterEach(()=>{manufacturingDatabase=undefined;vi.unstubAllEnvs();});
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

async function purchasedManufacturing(state:'active'|'draining'='active',endsAt:number|null=null,packageId:'manufacturing'|'manufacturing-mrp'='manufacturing',companyId=f.company){
 const {packageGrantStore,readCompanyPackageGrants}=await import('../../../src/server/modules/commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId===packageId);return packageGrantStore(db,{authorize:async()=>({userId:'isolated-manufacturing-provider'}),obligations:async()=>{throw Error('MANUFACTURING_FIXTURE_CANNOT_INACTIVATE');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(prior?.version??0)+1,reason:'Isolated original manufacturing owner purchase',evidence:'test:manufacturing-purchase'}});
}
it.each(['disabled','enforced'] as const)('buffers the three original Manufacturing and Planning private document targets through current native sources (%s)',async mode=>{
 const {attachmentContent,bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service');await purchasedManufacturing();vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT',mode);vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const actual=await order();
 for(const source of [{targetType:'manufacturing_spec',targetId:String(actual.spec.id),purposeKey:'drawing',table:'manufacturing_specifications'},{targetType:'planning_scenario',targetId:String(actual.plan.id),purposeKey:'source_document',table:'planning_scenarios'},{targetType:'production_run',targetId:actual.id,purposeKey:'run_sheet',table:'manufacturing_orders'}] as const){
  const version=Number(await db.prepare(`SELECT version FROM ${source.table} WHERE id=?`).bind(source.targetId).first('version')),document=await seedAttachmentSet(db,owner,{...source,componentKind:'record',expectedTargetVersion:version});await bindRecordAttachmentSet(db,owner,document.setId,version,key());const objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_NATIVE_PRODUCTION_DOCUMENT_REQUIRED');const bytes=`Original retained ${source.targetType} document`;await env.H001_FILES.put(objectKey,bytes);let consumed=0;
  const files:FileStore={put:(...args)=>env.H001_FILES.put(...args),get:async(...args)=>{const value=await env.H001_FILES.get(...args);return value?{...value,arrayBuffer:async()=>{consumed++;return value.arrayBuffer();}}:null;},head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
  for(const kind of ['record','lineage','file','iam'] as const){
   const fault=`production_private_${key().replaceAll('-','')}`,effect=kind==='record'?`UPDATE ${source.table} SET version=version+1 WHERE id='${source.targetId}'`:kind==='lineage'?`UPDATE manufacturing_specifications SET name='Late native controlled specification' WHERE id='${actual.spec.id}'`:kind==='file'?`UPDATE attachments SET legal_hold=1 WHERE id='${document.sourceId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${owner.assignmentId}'`;
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${document.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
   try{await expect(attachmentContent(db,files,owner,document.linkId,'original')).rejects.toMatchObject({code:'MANUFACTURING_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
   expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(document.linkId).first('n')).toBe(0);
  }
  expect(consumed).toBe(4);await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(document.sourceId).run();expect(await(await attachmentContent(db,files,owner,document.linkId,'original')).object.text()).toBe(bytes);expect(consumed).toBe(5);
  await expect(db.transaction(tx=>attachmentContent(tx,files,owner,document.linkId,'original'))).rejects.toMatchObject({code:'MANUFACTURING_FILE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(5);
 }
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(f.company).all<{package_id:string}>()).results).toEqual([{package_id:'manufacturing'}]);
});
it('observes a committed native production cancellation before its byte snapshot and isolates two Company source-writing document reads',async()=>{
 const {attachmentContent,bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{companyPurchaseReadLocks}=await import('../../../src/server/modules/commercial/public/purchase-command-boundary'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics');vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','disabled');const first=await order(),runFile=await seedAttachmentSet(db,owner,{targetType:'production_run',targetId:first.id,componentKind:'record',purposeKey:'run_sheet',expectedTargetVersion:1});await bindRecordAttachmentSet(db,owner,runFile.setId,1,key());const runKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(runFile.sourceId).first<string>('object_key');if(!runKey)throw Error('ACTUAL_RETAINED_RUN_SHEET_REQUIRED');await env.H001_FILES.put(runKey,'Original planned run retained after native cancellation');const fault=`production_bytes_visible_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF (SELECT status FROM manufacturing_orders WHERE id='${first.id}')<>'cancelled' THEN RAISE EXCEPTION 'STALE_NATIVE_PRODUCTION_FILE_SOURCE';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${runFile.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{const observed=await committedSourceVisibility(db,async tx=>{await tx.batch(companyPurchaseReadLocks(tx,f.company));manufacturingDatabase=tx;try{await changeManufacturingOrder(owner,first.id,{action:'cancel',expectedVersion:2,reason:'Actual native source closes before the private byte snapshot'},key());}finally{manufacturingDatabase=undefined;}},()=>measureReads(()=>attachmentContent(db,env.H001_FILES,owner,runFile.linkId,'original')));expect(await observed.result.object.text()).toBe('Original planned run retained after native cancellation');expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{manufacturingDatabase=undefined;await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
 const otherFixture=await expenseFixture(),other={...otherFixture.owner,modules:['planning','manufacturing']};await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(other.modules),other.assignmentId).run();const otherOutput=await saveMaster(other,'products',null,{sku:key(),name:'Separate Company native output',price:'10'},key()),otherMaterial=await saveMaster(other,'products',null,{sku:key(),name:'Separate Company native material',price:'5'},key()),second=await createSpecification(other,{name:'Separate Company private specification',mode:'bom',outputProductId:String(otherOutput.id),batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:String(otherMaterial.id),quantity:'1'}]},key()),rows:{actor:WorkContext;id:string;version:number;file:Awaited<ReturnType<typeof seedAttachmentSet>>;bytes:string}[]=[];
 for(const source of [{actor:owner,id:String(first.spec.id),version:2},{actor:other,id:String(second.id),version:1}]){const file=await seedAttachmentSet(db,source.actor,{targetType:'manufacturing_spec',targetId:source.id,componentKind:'record',purposeKey:'drawing',expectedTargetVersion:source.version});await bindRecordAttachmentSet(db,source.actor,file.setId,source.version,key());const objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_PRIVATE_SPECIFICATION_REQUIRED');const bytes=`Native specification ${source.actor.companyId}`;await env.H001_FILES.put(objectKey,bytes);rows.push({...source,file,bytes});}
 const parallel=`production_bytes_parallel_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${parallel}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.08);UPDATE manufacturing_specifications SET name=name WHERE id=CASE WHEN NEW.company_id='${f.company}' THEN '${rows[0].id}' ELSE '${rows[1].id}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${parallel} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN ('${rows[0].file.linkId}','${rows[1].file.linkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${parallel}()`);
 try{const observed=await measureReads(()=>Promise.all(rows.map(row=>attachmentContent(db,env.H001_FILES,row.actor,row.file.linkId,'original'))));for(let index=0;index<rows.length;index++)expect(await observed.result[index].object.text()).toBe(rows[index].bytes);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${parallel} ON attachment_access_events;DROP FUNCTION ${parallel}()`);}
});
it('runs Manufacturing alone through retained material issue independent QC and original WIP while included Finance remains readonly',async()=>{
 vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 await expect(createSpecification(owner,{name:'Unpurchased specification',mode:'bom',outputProductId:output,batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:material,quantity:'2'}]},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await purchasedManufacturing();
 const {id,spec}=await order(),other=await order(),reserveKey=key(),reserve={action:'reserve',expectedVersion:1,expiresAt:Date.now()+3600000,reason:'Protect purchased production material'};
 const reserved=await changeManufacturingOrder(owner,id,reserve,reserveKey);await purchasedManufacturing('draining');expect(await changeManufacturingOrder(owner,id,reserve,reserveKey)).toEqual(reserved);await expect(changeManufacturingOrder(owner,other.id,reserve,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await expect(createPlanningScenario(owner,{warehouseId:warehouse,specificationId:String(spec.id),specificationVersion:2,demandQuantity:'1',dueDate:'2026-10-02'},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 const issueKey=key(),issue={action:'issue',expectedVersion:2,entryDate:'2026-10-02',reason:'Issue already reserved material to original WIP'};const issued=await changeManufacturingOrder(owner,id,issue,issueKey);expect(await changeManufacturingOrder(owner,id,issue,issueKey)).toEqual(issued);
 await changeManufacturingOrder(owner,id,{action:'report',expectedVersion:3,outputQuantity:'1.8',scrapQuantity:'0.2',laborCost:'10',overheadCost:'2',entryDate:'2026-10-02',reason:'Record original retained conversion costs'},key());
 await expect(changeManufacturingOrder(owner,id,{action:'pass-qc',expectedVersion:4,entryDate:'2026-10-02',reason:'Reporter cannot inspect own purchased output'},key())).rejects.toMatchObject({code:'CONFLICT'});
 await changeManufacturingOrder(reviewer,id,{action:'pass-qc',expectedVersion:4,entryDate:'2026-10-02',reason:'Independent purchased output inspection'},key());const detail=await productionDetail(owner,id);expect(detail.order).toMatchObject({status:'completed',version:5});expect(detail.postings.map(row=>[row.phase,row.amount_minor,row.journal_status])).toEqual([['issue',2000,'posted'],['report',1200,'posted'],['output',3200,'posted']]);
 expect(await db.prepare(`SELECT sum(line.debit_minor-line.credit_minor)::text balance FROM finance_journal_lines line JOIN finance_journals journal ON journal.id=line.journal_id WHERE journal.company_id=? AND journal.status='posted' AND line.account_id=?`).bind(f.company,productionAccounts.wip).first('balance')).toBe('0');
 const {readCompanyPackageGrants}=await import('../../../src/server/modules/commercial/public/package-grants');expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['manufacturing']);const currentFilters={from:'2026-01-01',to:'2026-12-31',currency:'THB'};expect(await queryMetric(db,owner,'completed-manufacturing-orders',currentFilters)).toMatchObject({status:'ready',total:'1'});expect(await queryMetric(db,owner,'production-cost',currentFilters)).toMatchObject({status:'ready',total:'3000'});
 const {journalDetail,ledgerWorkspace}=await import('../../../src/server/modules/finance/public/ledger'),{financeDocumentPostAccess,financeProductDocumentEntry}=await import('../../../src/server/modules/finance/public/document-purchases'),{financeDocumentPresentationAccess,financeDocumentFlow}=await import('../../../src/server/modules/finance/public/record-presentation');const journalId=String(detail.postings[0].journal_id);
 expect((await journalDetail(owner,journalId,db)).journal).toMatchObject({id:journalId,status:'posted',source_type:'manufacturing_issue',source_id:id});expect(await financeDocumentPresentationAccess(owner,'journal',journalId)).toEqual({sourceOnly:true,historyOnly:true});expect(await financeDocumentPostAccess(owner,'journal',journalId)).toEqual({canPost:false});expect(await financeDocumentFlow(owner,'journal',journalId)).toMatchObject({canAct:false,canManage:false});const assignmentVersion=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(owner.assignmentId).first('version'));expect(await financeProductDocumentEntry(db,{companyId:f.company,branchId:owner.branchId,departmentId:owner.departmentId,userId:owner.userId,assignmentId:owner.assignmentId,assignmentVersion,role:owner.role},{kind:'journal',id:journalId})).toEqual({capability:'finance.production-costing',purchasePurpose:'history'});await expect(ledgerWorkspace(owner,f.chart)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const {postJournal}=await import('../../../src/server/modules/finance/public/finance');await expect(postJournal(owner,journalId,key(),{expectedVersion:1})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const {bindRecordAttachmentSet,attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),files=await seedAttachmentSet(db,owner,{targetType:'finance_journal',targetId:journalId,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:1});await bindRecordAttachmentSet(db,owner,files.setId,1,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(files.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Original production journal evidence',{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(files.sourceId).run();expect(await (await attachmentContent(db,env.H001_FILES,owner,files.linkId,'original')).object.text()).toBe('Original production journal evidence');
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),owner.assignmentId).run();await expect(changeManufacturingOrder(owner,id,issue,issueKey)).rejects.toMatchObject({code:'MANUFACTURING_ACCESS_CHANGED'});
});
it('requires the advanced addon only for actual scenario comparison and retains that actual source while draining',async()=>{
 vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');await purchasedManufacturing();const {spec,plan}=await order(),input={warehouseId:warehouse,specificationId:String(spec.id),specificationVersion:2,demandQuantity:'1',dueDate:'2026-10-03',capacityMinutes:100,comparisonScenarioId:String(plan.id)};
 await expect(createPlanningScenario(owner,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await purchasedManufacturing('active',null,'manufacturing-mrp');const command=key(),compared=await createPlanningScenario(owner,input,command);expect(compared.context.comparison).toMatchObject({scenarioId:plan.id,sourceVersion:2});await purchasedManufacturing('draining',null,'manufacturing-mrp');expect(await createPlanningScenario(owner,input,command)).toEqual(compared);await expect(createPlanningScenario(owner,input,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 const {productionWorkspace}=await import('../../../src/server/modules/manufacturing/public/production');expect((await productionWorkspace(owner,'planning')).plans.map(row=>row.id)).toContain(compared.id);await cancelPlanningScenario(owner,compared.id,{action:'cancel',expectedVersion:1,reason:'Close the retained actual advanced comparison'},key());
 const fake={...input,comparisonScenarioId:key()};await purchasedManufacturing('active',null,'manufacturing-mrp');await expect(createPlanningScenario(owner,fake,key())).rejects.toMatchObject({code:'PRODUCTION_SOURCE_NOT_FOUND'});
});
it('rolls back a late active production expiry and retries the same original specification and receipt',async()=>{
 vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');await purchasedManufacturing('active',Date.now()+3000);const command=key(),id=`mfg-spec-${command}`,input={name:'Expiry controlled original specification',mode:'bom',outputProductId:output,batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:material,quantity:'2'}]},fault=`manufacturing_expiry_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(3.15);RETURN NEW;END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(createSpecification(owner,input,command)).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();`);}for(const [table,column,value] of [['manufacturing_specifications','id',id],['manufacturing_specification_lines','specification_id',id],['command_receipts','command_id',command],['audit_events','command_id',command],['outbox_events','command_id',command]])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE ${column}=?`).bind(value).first()).toBeNull();await purchasedManufacturing();const created=await createSpecification(owner,input,command);expect(await createSpecification(owner,input,command)).toEqual(created);
});
it('rolls back original material stock WIP and posting evidence when current production authority changes after the receipt',async()=>{
 vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');await purchasedManufacturing();const {id}=await order();await changeManufacturingOrder(owner,id,{action:'reserve',expectedVersion:1,expiresAt:Date.now()+3600000,reason:'Reserve material for authority rollback'},key());await purchasedManufacturing('draining');const command=key(),input={action:'issue',expectedVersion:2,entryDate:'2026-10-02',reason:'Only current original authority may issue retained material'},fault=`manufacturing_authority_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint,version=version+1 WHERE id='${owner.assignmentId}';RETURN NEW;END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(changeManufacturingOrder(owner,id,input,command)).rejects.toMatchObject({code:'CONFLICT'});}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();`);}expect((await productionDetail(owner,id)).order).toMatchObject({status:'reserved',version:2});expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,material).first()).toEqual({quantity_milli:10000,value_minor:5000});for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM manufacturing_order_postings WHERE order_id=?').bind(id).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM manufacturing_cost_movements WHERE order_id=?').bind(id).first()).toBeNull();const issued=await changeManufacturingOrder(owner,id,input,command);expect(await changeManufacturingOrder(owner,id,input,command)).toEqual(issued);
});

it('checks actual Manufacturing source history before shared Records Work Inbox and Files limits without granting another package',async()=>{
 const {id,spec,plan}=await order(),now=Date.now();for(const [recordId,module,section,title] of [[id,'manufacturing','orders','Original manufacturing order'],[String(spec.id),'manufacturing','specifications','Original controlled BOM'],[String(plan.id),'planning','scenarios','Original planning source']] as const)await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES(?,?,?,?,'','draft','[]',?,?,?,?,?)").bind(module,recordId,section,title,f.company,owner.userId,owner.userId,now,now).run();
 const {listRecords,getRecord}=await import('../../../src/server/modules/records/public/records'),{listStaffNotifications}=await import('../../../src/server/modules/collaboration/public/staff-notifications'),{publishInboxEvent,inboxPage}=await import('../../../src/server/modules/collaboration/public/inbox'),{manufacturingWork,productionWorkspace}=await import('../../../src/server/modules/manufacturing/public/production'),{listAttachmentSets,bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service');
 await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'manufacturing_order',?,1,'Original production work',?)").bind(key(),f.company,owner.userId,id,now).run();await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:'manufacturing_order',sourceId:id,sourceVersion:1,category:'work',preview:'Original production work',recipientIds:[owner.userId]});const document=await seedAttachmentSet(db,owner,{targetType:'manufacturing_spec',targetId:String(spec.id),componentKind:'record',purposeKey:'drawing',expectedTargetVersion:2});
 vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');const {metricCatalogFor}=await import('../../../src/server/modules/runtime/public/executive/metrics');const deniedCatalog=await metricCatalogFor(db,owner);expect(['completed-manufacturing-orders','production-cost','planning-shortage-scenarios'].map(id=>deniedCatalog.find(metric=>metric.id===id)?.available)).toEqual([false,false,false]);for(const metric of ['completed-manufacturing-orders','production-cost','planning-shortage-scenarios'] as const)await expect(queryMetric(db,owner,metric,{from:'2026-01-01',to:'2026-12-31',currency:'THB'})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await listRecords(owner,'manufacturing','orders')).toEqual([]);await expect(getRecord(owner,'manufacturing',id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect((await listStaffNotifications(db,owner)).some(row=>row.record_type==='manufacturing_order'&&row.record_id===id)).toBe(false);expect((await inboxPage(db,owner,{view:'all'})).items.some(item=>item.sourceId===id)).toBe(false);expect(await manufacturingWork(owner)).toEqual([]);await expect(listAttachmentSets(db,owner,{targetType:'manufacturing_spec',targetId:String(spec.id)})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await purchasedManufacturing();const {measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),small=await measureReads(()=>productionWorkspace(owner,'manufacturing'));await order();const larger=await measureReads(()=>productionWorkspace(owner,'manufacturing'));expect(larger.result.orders.length).toBe(small.result.orders.length+1);expect(larger.metrics.queries).toBe(small.metrics.queries);const purchasedCatalog=await metricCatalogFor(db,owner);expect(['completed-manufacturing-orders','production-cost','planning-shortage-scenarios'].map(id=>purchasedCatalog.find(metric=>metric.id===id)?.available)).toEqual([true,true,true]);expect((await listRecords(owner,'manufacturing','orders')).map(row=>row.id)).toEqual([id]);expect((await getRecord(owner,'planning',String(plan.id))).id).toBe(plan.id);expect((await listStaffNotifications(db,owner)).some(row=>row.record_id===id)).toBe(true);expect((await inboxPage(db,owner,{view:'all'})).items.some(item=>item.sourceId===id)).toBe(true);expect((await manufacturingWork(owner)).find(item=>item.id===id)).toMatchObject({canAct:true});await purchasedManufacturing('draining');expect((await manufacturingWork(owner)).find(item=>item.id===id)).toMatchObject({canAct:false});await bindRecordAttachmentSet(db,owner,document.setId,2,key());expect(await attachmentBindingState(db,document.setId)).toMatchObject({set_state:'bound',link_state:'bound'});expect((await productionWorkspace(owner,'manufacturing')).specifications.find(row=>row.id===spec.id)?.version).toBe(3);
});
it('binds two Companies original production files concurrently without upgrading shared source leases after their first snapshot',async()=>{
 const first=await order(),second=await expenseFixture(),other={...second.owner,modules:['planning','manufacturing']};await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(other.modules),other.assignmentId).run();const material2=String((await saveMaster(other,'products',null,{sku:key(),name:'Second Company original material',price:'5'},key())).id),output2=String((await saveMaster(other,'products',null,{sku:key(),name:'Second Company output',price:'10'},key())).id),spec2=await createSpecification(other,{name:'Second Company controlled specification',mode:'bom',outputProductId:output2,batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:material2,quantity:'1'}]},key()),one=await seedAttachmentSet(db,owner,{targetType:'manufacturing_spec',targetId:String(first.spec.id),componentKind:'record',purposeKey:'drawing',expectedTargetVersion:2}),two=await seedAttachmentSet(db,other,{targetType:'manufacturing_spec',targetId:String(spec2.id),componentKind:'record',purposeKey:'drawing',expectedTargetVersion:1});
 vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');await purchasedManufacturing();await purchasedManufacturing('active',null,'manufacturing',second.company);const {bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service');const {measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),{manufacturingAttachmentPurchaseScope,manufacturingAttachmentSourceReadLocks}=await import('../../../src/server/modules/manufacturing/public/attachment-purchases'),source=await manufacturingAttachmentPurchaseScope(db,owner,'manufacturing_spec',String(first.spec.id)),otherSource=await manufacturingAttachmentPurchaseScope(db,other,'manufacturing_spec',String(spec2.id));expect(source).not.toBeNull();expect(otherSource).not.toBeNull();const fault=`manufacturing_parallel_files_${key().replaceAll('-','')}`;await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id LIKE 'manufacturing-file-before:%' THEN PERFORM pg_sleep(0.1); END IF; RETURN NEW; END $$`).run();await db.prepare(`CREATE TRIGGER ${fault} INSTEAD OF INSERT ON command_assertions FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();try{const measured=await measureReads(()=>Promise.all([db.transaction(async tx=>{await tx.batch(await manufacturingAttachmentSourceReadLocks(tx,[source!]));return bindRecordAttachmentSet(tx,owner,one.setId,2,key());}),db.transaction(async tx=>{await tx.batch(await manufacturingAttachmentSourceReadLocks(tx,[otherSource!]));return bindRecordAttachmentSet(tx,other,two.setId,1,key());})]));expect(measured.result).toHaveLength(2);expect(measured.metrics.deadlockFailures??0).toBe(0);const directOne=await seedAttachmentSet(db,owner,{targetType:'manufacturing_spec',targetId:String(first.spec.id),componentKind:'record',purposeKey:'sop',expectedTargetVersion:3}),directTwo=await seedAttachmentSet(db,other,{targetType:'manufacturing_spec',targetId:String(spec2.id),componentKind:'record',purposeKey:'sop',expectedTargetVersion:2}),direct=await measureReads(()=>Promise.all([bindRecordAttachmentSet(db,owner,directOne.setId,3,key()),bindRecordAttachmentSet(db,other,directTwo.setId,2,key())]));expect(direct.result).toHaveLength(2);expect(direct.metrics.deadlockFailures??0).toBe(0);expect(await attachmentBindingState(db,directOne.setId)).toMatchObject({set_state:'bound',link_state:'bound'});expect(await attachmentBindingState(db,directTwo.setId)).toMatchObject({set_state:'bound',link_state:'bound'});}finally{await db.prepare(`DROP TRIGGER ${fault} ON command_assertions`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}expect(await attachmentBindingState(db,one.setId)).toMatchObject({set_state:'bound',link_state:'bound'});expect(await attachmentBindingState(db,two.setId)).toMatchObject({set_state:'bound',link_state:'bound'});
});
it('rolls back a production file command when its original specification source changes after the effect then retries the captured set',async()=>{
 const {spec}=await order(),id=String(spec.id),document=await seedAttachmentSet(db,owner,{targetType:'manufacturing_spec',targetId:id,componentKind:'record',purposeKey:'drawing',expectedTargetVersion:2});vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');await purchasedManufacturing();await purchasedManufacturing('draining');const {withdrawAttachmentLink}=await import('../../../src/server/modules/files/public/attachments/service'),fault=`manufacturing_file_source_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE manufacturing_specifications SET name='Changed during original file command' WHERE id='${id}';RETURN NEW;END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON attachment_events FOR EACH ROW WHEN(NEW.link_id='${document.linkId}' AND NEW.action='link.withdrawn') EXECUTE FUNCTION ${fault}();`);
 try{await expect(withdrawAttachmentLink(db,owner,document.linkId,'Withdraw superseded drawing draft',key())).rejects.toMatchObject({code:'MANUFACTURING_ATTACHMENT_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_events;DROP FUNCTION ${fault}();`);}expect(await attachmentBindingState(db,document.setId)).toMatchObject({set_state:'draft',link_state:'draft'});expect(await db.prepare('SELECT name FROM manufacturing_specifications WHERE id=?').bind(id).first('name')).toBe('bom specification');await withdrawAttachmentLink(db,owner,document.linkId,'Withdraw superseded drawing draft',key());
});

const manufacturingSettlementInput=(packageId='manufacturing')=>({companyId:f.company,packageId,packageVersion:1});
async function manufacturingSettlement(packageId='manufacturing'){
 const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations');return withOwnerPackageObligations(db,manufacturingSettlementInput(packageId),tx=>readOwnerPackageObligations(db,tx,{...manufacturingSettlementInput(packageId),now:Date.now()}));
}
async function manufacturingWithdrawal(packageId='manufacturing'){
 const {readCompanyPackageGrants,packageGrantRevision}=await import('../../../src/server/modules/commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,f.company),current=grants.find(grant=>grant.packageId===packageId)!;return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Native Manufacturing owner settlement reviewed',evidence:'test:manufacturing-concrete-settlement'}}]};
}
async function manufacturingWithdrawalReview(request:Awaited<ReturnType<typeof manufacturingWithdrawal>>,apply=false,after?:()=>Promise<void>){
 const {withOwnerPackageObligations,ownerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),{packageGrantStore}=await import('../../../src/server/modules/commercial/public/package-grants'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-manufacturing-settlement-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});
 return withOwnerPackageObligations(db,manufacturingSettlementInput(request.changes[0].grant.packageId),async tx=>{const result=apply?await store.batch.applyIn(tx,request,async()=>{}):await store.batch.previewIn(tx,request);if(after){manufacturingDatabase=tx;try{await after();}finally{manufacturingDatabase=undefined;}}return result;});
}
it('withdraws Manufacturing only after its five actual owners settle and retains original held history and exact command replay',async()=>{
 vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');await purchasedManufacturing();const {id}=await order();await changeManufacturingOrder(owner,id,{action:'reserve',expectedVersion:1,expiresAt:Date.now()+3600000,reason:'Reserve real Manufacturing withdrawal material'},key());const opened=await manufacturingSettlement();expect(opened.observations.map(row=>row.moduleId)).toEqual(['manufacturing','planning','quality','inventory','finance']);expect(opened.observations.find(row=>row.moduleId==='manufacturing')?.outstanding).toBeGreaterThan(0);expect(opened.observations.find(row=>row.moduleId==='inventory')?.outstanding).toBeGreaterThan(0);expect(JSON.stringify(opened)).not.toMatch(/bom specification|wip_account_id|authority_json|context_json/);await purchasedManufacturing('draining');const request=await manufacturingWithdrawal();await expect(manufacturingWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(manufacturingWithdrawalReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await changeManufacturingOrder(owner,id,{action:'issue',expectedVersion:2,entryDate:'2026-10-02',reason:'Issue original retained material into source WIP'},key());expect((await manufacturingSettlement()).observations.find(row=>row.moduleId==='finance')?.outstanding).toBeGreaterThan(0);await changeManufacturingOrder(owner,id,{action:'report',expectedVersion:3,outputQuantity:'1.8',scrapQuantity:'0.2',laborCost:'10',overheadCost:'2',entryDate:'2026-10-02',reason:'Report actual retained conversion costs'},key());const command=key(),qc={action:'pass-qc',expectedVersion:4,entryDate:'2026-10-02',reason:'Independent final source inspection before withdrawal'},completed=await changeManufacturingOrder(reviewer,id,qc,command);
 const {bindRecordAttachmentSet,attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),document=await seedAttachmentSet(db,owner,{targetType:'production_run',targetId:id,componentKind:'record',purposeKey:'run_sheet',expectedTargetVersion:5});await bindRecordAttachmentSet(db,owner,document.setId,5,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Held original completed production run',{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(document.sourceId).run();
 expect((await manufacturingSettlement()).total).toBe(0);const tables=['manufacturing_orders','manufacturing_order_events','manufacturing_material_reservations','manufacturing_order_postings','manufacturing_cost_movements','manufacturing_cost_snapshots','manufacturing_genealogy','finance_journals','finance_journal_lines','money_transaction_snapshots','attachments','attachment_links'],history=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await history();expect((await manufacturingWithdrawalReview(request)).after.find(row=>row.packageId==='manufacturing')?.state).toBe('inactive');const applied=await manufacturingWithdrawalReview(request,true);expect(await manufacturingWithdrawalReview(request,true)).toEqual(applied);expect(await history()).toEqual(before);expect((await productionDetail(owner,id)).order).toMatchObject({status:'completed',version:6});expect(await changeManufacturingOrder(reviewer,id,qc,command)).toEqual(completed);expect(await (await attachmentContent(db,env.H001_FILES,owner,document.linkId,'original')).object.text()).toBe('Held original completed production run');await expect(changeManufacturingOrder(owner,id,{action:'cancel',expectedVersion:6,reason:'Inactive cannot change retained owner work'},key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});
});
it('withdraws only the actual MRP comparison scope while an unrelated basic Manufacturing order remains open',async()=>{
 vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');await purchasedManufacturing();const basic=await order();await purchasedManufacturing('active',null,'manufacturing-mrp');const scenario=await createPlanningScenario(owner,{warehouseId:warehouse,specificationId:String(basic.spec.id),specificationVersion:2,demandQuantity:'1',dueDate:'2026-10-03',capacityMinutes:100,comparisonScenarioId:String(basic.plan.id)},key()),released=await reviewPlanningRecommendation(reviewer,String(scenario.recommendationId),{action:'approve',expectedVersion:1,reason:'Independent actual advanced comparison release'},key());await changeManufacturingOrder(owner,String(released.orderId),{action:'reserve',expectedVersion:1,expiresAt:Date.now()+3600000,reason:'Protect the actual advanced source material before withdrawal'},key());await purchasedManufacturing('draining',null,'manufacturing-mrp');const request=await manufacturingWithdrawal('manufacturing-mrp');await expect(manufacturingWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await changeManufacturingOrder(owner,String(released.orderId),{action:'cancel',expectedVersion:2,reason:'Close the retained actual advanced production plan'},key());expect(await db.prepare('SELECT quantity_milli,consumed_milli,released_milli FROM manufacturing_material_reservations WHERE order_id=?').bind(String(released.orderId)).first()).toEqual({quantity_milli:2000,consumed_milli:0,released_milli:2000});expect((await manufacturingSettlement('manufacturing-mrp')).total).toBe(0);await manufacturingWithdrawalReview(request);await manufacturingWithdrawalReview(request,true);expect((await productionDetail(owner,String(released.orderId))).order.status).toBe('cancelled');expect((await productionDetail(owner,basic.id)).order.status).toBe('planned');expect((await manufacturingSettlement()).total).toBeGreaterThan(0);
});
it('rolls back withdrawal and a late normal owner source command together before the full Commercial transaction commits',async()=>{
 await purchasedManufacturing();await purchasedManufacturing('draining');const request=await manufacturingWithdrawal(),command=key(),id=`mfg-spec-${command}`;await expect(manufacturingWithdrawalReview(request,true,async()=>{await createSpecification(owner,{name:'Late original owner settlement obligation',mode:'bom',outputProductId:output,batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:material,quantity:'2'}]},command);})).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='manufacturing'").bind(f.company).first()).toEqual({state:'draining',version:2});expect(await db.prepare('SELECT 1 FROM manufacturing_specifications WHERE id=?').bind(id).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect((await manufacturingSettlement()).total).toBe(0);
});
it('requires exact minted source leases and v1 owner scope before any Manufacturing withdrawal observation',async()=>{
 const {manufacturingPackageObligations}=await import('../../../src/server/modules/manufacturing/public/package-obligations'),{withOwnerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),input={...manufacturingSettlementInput(),moduleId:'manufacturing',now:Date.now()};await expect(db.transaction(tx=>manufacturingPackageObligations(db,tx,input))).rejects.toThrow('MANUFACTURING_PACKAGE_OBLIGATION_SOURCE_REQUIRED');expect(()=>withOwnerPackageObligations(db,{...manufacturingSettlementInput(),packageVersion:2},tx=>manufacturingPackageObligations(db,tx,{...input,packageVersion:2}))).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');await expect(withOwnerPackageObligations(db,manufacturingSettlementInput(),tx=>manufacturingPackageObligations(db,tx,{...input,moduleId:'planning'}))).rejects.toThrow('MANUFACTURING_PACKAGE_OBLIGATION_SCOPE_REQUIRED');await expect(withOwnerPackageObligations(db,manufacturingSettlementInput(),tx=>manufacturingPackageObligations(db,tx,{...input,companyId:'other-company'}))).rejects.toThrow('MANUFACTURING_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
});
it('acquires Manufacturing withdrawal source leases before its first snapshot and sees a concurrent committed native obligation',async()=>{
 await purchasedManufacturing();await purchasedManufacturing('draining');const request=await manufacturingWithdrawal(),command=key();let ready:()=>void=()=>{},failed:(cause:unknown)=>void=()=>{},release:()=>void=()=>{};const sourceReady=new Promise<void>((resolve,reject)=>{ready=resolve;failed=reject;}),finish=new Promise<void>(resolve=>{release=resolve;});
 const writer=db.transaction(async tx=>{manufacturingDatabase=tx;try{await createSpecification(owner,{name:'Concurrent original Manufacturing obligation',mode:'bom',outputProductId:output,batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:material,quantity:'2'}]},command);}finally{manufacturingDatabase=undefined;}ready();await finish;}).catch(cause=>{failed(cause);throw cause;});
 try{await sourceReady;let completed=false;const observation=manufacturingWithdrawalReview(request).then(()=>null,cause=>cause).finally(()=>{completed=true;});await new Promise(resolve=>setTimeout(resolve,100));expect(completed).toBe(false);release();await writer;expect(await observation).toMatchObject({message:'PACKAGE_OBLIGATIONS_REMAIN'});}finally{release();manufacturingDatabase=undefined;await writer.catch(()=>{});}
 expect(await db.prepare('SELECT status FROM manufacturing_specifications WHERE id=?').bind(`mfg-spec-${command}`).first('status')).toBe('draft');expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='manufacturing'").bind(f.company).first()).toEqual({state:'draining',version:2});
});

it('retains withdrawal while actual legacy cost pools or unposted native cost movements require owner settlement',async()=>{
 const {id}=await order();await changeManufacturingOrder(owner,id,{action:'reserve',expectedVersion:1,expiresAt:Date.now()+3600000,reason:'Reserve the real cost-source materials'},key());await changeManufacturingOrder(owner,id,{action:'issue',expectedVersion:2,entryDate:'2026-10-02',reason:'Issue into the actual source WIP'},key());await changeManufacturingOrder(owner,id,{action:'report',expectedVersion:3,outputQuantity:'2',scrapQuantity:'0',laborCost:'0',overheadCost:'0',entryDate:'2026-10-02',reason:'Record the original closed batch'},key());await changeManufacturingOrder(reviewer,id,{action:'pass-qc',expectedVersion:4,entryDate:'2026-10-02',reason:'Independent acceptance closes native output'},key());const now=Date.now(),movement=key(),pool=key();await db.prepare("INSERT INTO manufacturing_cost_movements(id,company_id,order_id,movement_kind,quantity_milli,amount_minor,source_type,source_id,calculation_run_id,journal_id,occurred_on,version,created_by,created_at) VALUES(?,?,?,'adjustment',0,500,'legacy_cost_review',?,NULL,NULL,'2026-10-02',1,?,?)").bind(movement,f.company,id,key(),owner.userId,now).run();await db.prepare("INSERT INTO manufacturing_cost_pools(id,company_id,period_id,name,allocation_basis,amount_minor,source_hash,status,version,created_by,created_at,updated_at) VALUES(?,?,?,'Original pending legacy cost pool','manual',500,?,'draft',1,?,?,?)").bind(pool,f.company,f.period,key(),owner.userId,now,now).run();await purchasedManufacturing();await purchasedManufacturing('draining');const observed=await manufacturingSettlement();expect(observed.observations.find(row=>row.moduleId==='manufacturing')?.outstanding).toBeGreaterThan(0);expect(observed.observations.find(row=>row.moduleId==='finance')?.outstanding).toBeGreaterThan(0);const unrelatedPosted=key(),realJournal=String(await db.prepare("SELECT journal_id FROM manufacturing_order_postings WHERE order_id=? AND phase='issue'").bind(id).first('journal_id'));await db.prepare("INSERT INTO manufacturing_cost_movements(id,company_id,order_id,movement_kind,quantity_milli,amount_minor,source_type,source_id,calculation_run_id,journal_id,occurred_on,version,created_by,created_at) VALUES(?,?,?,'adjustment',0,500,'legacy_cost_review',?,NULL,?,'2026-10-02',1,?,?)").bind(unrelatedPosted,f.company,id,key(),realJournal,owner.userId,now).run();expect((await manufacturingSettlement()).observations.find(row=>row.moduleId==='finance')?.outstanding).toBe((observed.observations.find(row=>row.moduleId==='finance')?.outstanding??0)+1);const request=await manufacturingWithdrawal();await expect(manufacturingWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await db.prepare('SELECT journal_id FROM manufacturing_cost_movements WHERE id=?').bind(movement).first('journal_id')).toBeNull();expect(await db.prepare('SELECT status FROM manufacturing_cost_pools WHERE id=?').bind(pool).first('status')).toBe('draft');
});
it('productSourceHandoff enters the actual Manufacturing-only cost Journal on Finance while all legacy enforcement modes remain disabled',async()=>{
 const {financeProductDocumentEntry}=await import('../../../src/server/modules/finance/public/document-purchases'),{financeDocumentPresentationAccess,financeSourceDocumentBackdrop}=await import('../../../src/server/modules/finance/public/record-presentation');await purchasedManufacturing();const {id}=await order();await changeManufacturingOrder(owner,id,{action:'reserve',expectedVersion:1,expiresAt:Date.now()+3600000,reason:'Actual product handoff reserves original material'},key());await changeManufacturingOrder(owner,id,{action:'issue',expectedVersion:2,entryDate:'2026-10-02',reason:'Actual original Manufacturing cost journal source'},key());const detail=await productionDetail(owner,id),journal=String(detail.postings[0].journal_id),scope={companyId:f.company,branchId:owner.branchId,departmentId:owner.departmentId,userId:owner.userId,assignmentId:owner.assignmentId,assignmentVersion:Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(owner.assignmentId).first('version')),role:owner.role};
 expect(await financeProductDocumentEntry(db,scope,{kind:'journal',id:journal})).toEqual({capability:'finance.production-costing',purchasePurpose:'history'});expect(await financeDocumentPresentationAccess(owner,'journal',journal)).toEqual({sourceOnly:false,historyOnly:false});expect(await financeDocumentPresentationAccess(owner,'journal',journal,{strictProduct:true})).toEqual({sourceOnly:true,historyOnly:true});expect(await financeSourceDocumentBackdrop(owner,'journal',journal,{strictProduct:true})).toEqual({kind:'journal',id:journal,number:expect.any(String),date:'2026-10-02',status:'posted',version:expect.any(Number)});expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(f.company).all()).results).toEqual([{package_id:'manufacturing'}]);
 const {packageGrantStore}=await import('../../../src/server/modules/commercial/public/package-grants');await packageGrantStore(db,{authorize:async()=>({userId:'isolated-full-finance-provider'}),obligations:async()=>{throw Error('No withdrawal');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'finance',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual full Finance keeps its original presentation',evidence:'test:full-finance-presentation'}});expect(await financeDocumentPresentationAccess(owner,'journal',journal,{strictProduct:true})).toEqual({sourceOnly:false,historyOnly:false});expect(await financeSourceDocumentBackdrop(owner,'journal',journal,{strictProduct:true})).toBeNull();
});

// Ignored original-boundary RED candidate for existing manufacturing.production.integration.
// Append after the exact original production.test.ts bytes. Its unchanged
// beforeEach prepares an unrelated legacy Company; THIS Company's only SQL
// fixture writes are native organization/worker/assignment facts. All of this
// Company's chart/accounts/period/rate are original Finance owner commands.
async function zeroLedgerManufacturingCompany() {
  // Seed organization and actual native worker/assignment authority only.
  // This Company has NEVER had a chart/account/period/quote/settings row.
  // It is a disposable Company fixture, not normal Company onboarding proof.
  const company=`zero-manufacturing-${key()}`,branch=`${company}-branch`,department=`${company}-department`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero ledger Manufacturing fixture',?)").bind(company,company,company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Native services branch',?,?,?)").bind(branch,branch,company,company,branch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Native Manufacturing team',?,?,?)").bind(department,department,branch,company,branch),
  ]);
  async function actor(name:string,role:string,scopeKind:'holding'|'branch'|'department'='holding'):Promise<WorkContext>{
    const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`;
    await db.batch([
      db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,branch_id,scope_node_id) VALUES(?,?,?,?,?,0,9999999999999,?,?,?)").bind(assignmentId,userId,role,JSON.stringify(['manufacturing','planning']),company,scopeKind,scopeKind==='holding'?null:branch,scopeKind==='department'?department:scopeKind==='branch'?branch:null),
      db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${userId}`,userId,company,name),
      db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Native worker','THB','fixture',1,1)").bind(`eng-${userId}`,`person-${userId}`,company,branch,department),
    ]);
    const scope:WorkContext['scope']=scopeKind==='department'?{kind:'department',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company,branchId:branch,departmentId:department,departmentSlug:department,departmentName:'Native Manufacturing team'}:scopeKind==='branch'?{kind:'branch',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company,branchId:branch,branchSlug:branch,branchName:'Native service branch'}:{kind:'holding',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company};
    const context:WorkContext={userId,assignmentId,role,companyId:company,branchId:scopeKind==='holding'?null:branch,departmentId:scopeKind==='department'?department:null,holdingId:'h001',holdingSlug:'test',holdingName:'Test',name,email:`${name}@example.test`,assignmentScope:scopeKind,inherited:false,modules:['manufacturing','planning'],assignments:[],workspaces:[],scope};
    const {loadIamAuthority}=await import('../../../src/server/modules/identity_access/public/iam-store');
    context.iam=await loadIamAuthority(db,company,userId,role);
    return context;
  }
  const owner=await actor('owner','holding_owner'),reviewer=await actor('reviewer','finance_preparer'),manager=await actor('manager','finance_approver'),salesManager=await actor('sales-manager','sales_manager'),treasury=await actor('treasury','treasury');
  const {installStaffLevels,savePosition,positionSchema}=await import('../../../src/server/modules/organization/public/organization');
  await installStaffLevels(db,owner,key());
  const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Fixture accountable owner',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Native organizational authority fixture'}),key())).id);
  for(const [member,rank] of [[owner,1],[reviewer,5],[manager,4],[salesManager,3],[treasury,4]] as const){
    const position=rank===1?ceo:String((await savePosition(db,owner,null,positionSchema.parse({title:`Fixture ${member.role}`,levelId:`org-level-${company}-${rank}`,branchId:null,departmentId:null,reportsToPositionId:ceo,expectedVersion:0,reason:'Native selected worker authority fixture'}),key())).id);
    await db.prepare('UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=?').bind(`org-level-${company}-${rank}`,position,rank===1?null:`person-${owner.userId}`,`eng-${member.userId}`).run();
  }
  await db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,'ceo',?,1,?,?,1) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=1,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=1").bind(company,company,ceo,`person-${owner.userId}`).run();
  const counts=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','commerce_orders','finance_journals','finance_invoices'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')));
  expect(await counts()).toEqual([0,0,0,0,0,0,0,0]);
  const {packageGrantStore}=await import('../../../src/server/modules/commercial/public/package-grants');
  const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-zero-ledger-Manufacturing-provider'}),obligations:async()=>{throw Error('ZERO_MANUFACTURING_FIXTURE_CANNOT_WITHDRAW');}}),grant={companyId:company,packageId:'manufacturing',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Reviewed isolated Manufacturing-only foundation',evidence:'test:zero-ledger-Manufacturing'};
  async function purchase(){await store.apply({expectedVersion:0,idempotencyKey:key(),grant});}
  vi.stubEnv('ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
  return {company,branch,department,owner,reviewer,manager,salesManager,treasury,counts,purchase,store,grant,actor};
}


it('retains the original Manufacturing missing-mapping guard after real Finance references without orphan effects',async()=>{
 const z=await zeroLedgerManufacturingCompany();await z.purchase();
 const finance=await import('../../../src/server/modules/finance/public/finance'),money=await import('../../../src/server/modules/finance/public/company-money'),production=await import('../../../src/server/modules/manufacturing/public/production'),accounting=await import('../../../src/server/modules/manufacturing/public/production-accounting');
 expect(await z.counts()).toEqual([0,0,0,0,0,0,0,0]);
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(z.company).all()).results).toEqual([{package_id:'manufacturing'}]);
 const chart=await finance.createChart(z.reviewer,{name:'Actual first Manufacturing chart',baseCurrency:'THB',accountingBasis:'IFRS'},key()),accounts:Record<string,string>={};
 const roles=[['receivableAccountId','asset','receivable'],['payableAccountId','liability','payable'],['revenueAccountId','revenue','none'],['inventoryAccountId','asset','none'],['clearingAccountId','liability','none'],['cogsAccountId','expense','none'],['inputTaxAccountId','asset','none'],['outputTaxAccountId','liability','none'],['cashAccountId','asset','bank'],['wipAccountId','asset','none'],['laborAccountId','liability','none'],['overheadAccountId','liability','none'],['scrapAccountId','expense','none']] as const;
 for(const [index,[field,category,controlKind]] of roles.entries()){
  const account=await finance.createAccount(z.reviewer,{chartId:chart.id,code:`MFG-${index+1}`,name:`Real Manufacturing ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind},key());accounts[field]=String(account.id);
 }
 await finance.createPeriod(z.reviewer,{name:'Actual Manufacturing posting year',startDate:'2026-01-01',endDate:'2026-12-31'},key());
 await money.confirmManualRate(z.manager,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Original rightful Finance approver confirms a dated native Manufacturing quote'},key());
 expect(await db.prepare('SELECT count(*)::int n FROM commerce_settings WHERE company_id=?').bind(z.company).first('n')).toBe(0);
 expect(await db.prepare('SELECT count(*)::int n FROM finance_accounts WHERE company_id=?').bind(z.company).first('n')).toBe(13);
 const workspace=await production.productionWorkspace(z.reviewer,'manufacturing');expect(workspace.canConfigureAccounting).toBe(true);expect(workspace.accounts).toEqual([]);
 const command=key(),profileInput={chartId:chart.id,inventoryAccountId:accounts.inventoryAccountId,wipAccountId:accounts.wipAccountId,laborAccountId:accounts.laborAccountId,overheadAccountId:accounts.overheadAccountId,scrapAccountId:accounts.scrapAccountId};
 const image=async()=>({profiles:(await db.prepare('SELECT to_jsonb(p)::text image FROM manufacturing_accounting_profiles p WHERE company_id=? ORDER BY id').bind(z.company).all()).results,settings:(await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).all()).results,commands:await Promise.all(['command_receipts','audit_events','outbox_events'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE command_id=? ORDER BY image`).bind(command).all()).results))});
 const before=await image();
 // The separately recorded RED reached this REAL original mapping guard.
 // Keep the guard and its no-effects refusal as a negative regression; the
 // added named-owner case below must establish actual mapping before profile
 // creation and different-person review can legitimately succeed.
 await expect(accounting.createManufacturingAccountingProfile(z.reviewer,profileInput,command)).rejects.toMatchObject({status:409,code:'CONFLICT'});
 expect(await image()).toEqual(before);
});


it('sets up Manufacturing-only Finance references and immutable posting mapping before original independent accounting profile review',async()=>{
 const z=await zeroLedgerManufacturingCompany();await z.purchase();
 const refs=await import('../../../src/server/modules/finance/public/manufacturing-posting-prerequisites'),setup=await import('../../../src/server/modules/manufacturing/public/posting-setup'),profiles=await import('../../../src/server/modules/manufacturing/public/posting-profiles');
 const first=await refs.manufacturingPostingPrerequisites(z.reviewer);expect(first).toMatchObject({available:true,configured:false,functionalCurrency:'THB',canCreateChart:true,charts:[],accounts:[],periods:[],rates:[]});expect(first.contextKey).toMatch(/^[a-f0-9]{64}$/);
 const chartKey=key(),chartBody={expectedVersion:0,contextKey:first.contextKey,input:{name:'Named Manufacturing-only actual chart',baseCurrency:'THB',accountingBasis:'IFRS'}},chart=await refs.createManufacturingPostingChart(z.reviewer,chartBody,chartKey),accounts:Record<string,string>={};
 const roles=[['receivableAccountId','asset','receivable'],['payableAccountId','liability','payable'],['revenueAccountId','revenue','none'],['inventoryAccountId','asset','none'],['clearingAccountId','liability','none'],['cogsAccountId','expense','none'],['inputTaxAccountId','asset','none'],['outputTaxAccountId','liability','none'],['cashAccountId','asset','bank']] as const;
 for(const [index,[field,category,controlKind]] of roles.entries()){
  const view=await refs.manufacturingPostingPrerequisites(z.reviewer);expect(view.canCreateAccount).toBe(true);
  const account=await refs.createManufacturingPostingAccount(z.reviewer,{expectedVersion:0,contextKey:view.contextKey,input:{chartId:chart.id,code:`NAMED-${index+1}`,name:`Actual original ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key());accounts[field]=String(account.id);
 }
 const periodView=await refs.manufacturingPostingPrerequisites(z.reviewer);expect(periodView.canCreatePeriod).toBe(true);
 await refs.createManufacturingPostingPeriod(z.reviewer,{expectedVersion:0,contextKey:periodView.contextKey,input:{name:'Named actual Manufacturing year',startDate:'2026-01-01',endDate:'2026-12-31'}},key());
 const rateView=await refs.manufacturingPostingPrerequisites(z.manager);expect(rateView.canConfirmRate).toBe(true);
 await refs.confirmManufacturingPostingRate(z.manager,{expectedVersion:0,contextKey:rateView.contextKey,input:{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Original independent rightful Finance rate confirmation'}},key());
 const mappingView=await setup.manufacturingPostingSetup(z.reviewer);expect(mappingView).toMatchObject({available:true,configured:false,canConfigure:true,expectedSalesDefaults:null,unsupportedExistingMapping:false});
 const mappingKey=key(),mappingBody={expectedVersion:0,contextKey:mappingView.contextKey,reason:'Actual Manufacturing-only initial owner mapping',expectedSalesDefaults:mappingView.expectedSalesDefaults,chartId:chart.id,currency:'THB',...accounts};
 const sourceImage=async()=>({chart:await db.prepare('SELECT to_jsonb(c)::text image FROM finance_charts c WHERE id=?').bind(chart.id).first(),settings:(await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).all()).results,commands:await Promise.all(['command_receipts','audit_events','outbox_events'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE command_id=? ORDER BY image`).bind(mappingKey).all()).results))}),beforeMapping=await sourceImage(),fault=`mfg_setup_late_source_${key().replaceAll('-','')}`;
 // Native deferred fault injection only: it must roll back with the owner
 // command. This never supplies positive chart/settings/stock fixture facts.
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_charts SET name='Changed after original mapping receipt' WHERE id='${chart.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${mappingKey}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(setup.initializeManufacturingPostingSetup(z.reviewer,mappingBody,mappingKey)).rejects.toMatchObject({status:409,code:'MANUFACTURING_POSTING_SOURCE_CHANGED'});expect(await sourceImage()).toEqual(beforeMapping);}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}
 const mapped=await setup.initializeManufacturingPostingSetup(z.reviewer,mappingBody,mappingKey);expect(mapped).toMatchObject({id:z.company,status:'configured',version:1});expect(await setup.initializeManufacturingPostingSetup(z.reviewer,mappingBody,mappingKey)).toEqual(mapped);
 // The immutable nine-account mapping must not strand the remaining real
 // production-only profile accounts. These are ordinary Finance owner creates
 // on that exact mapped chart, without a Supply/Sales/full-Finance purchase.
 for(const [index,[field,category]] of ([['wipAccountId','asset'],['laborAccountId','liability'],['overheadAccountId','liability'],['scrapAccountId','expense']] as const).entries()){
  const view=await refs.manufacturingPostingPrerequisites(z.reviewer);expect(view).toMatchObject({configured:true,canCreateChart:false,canCreateAccount:true});
  const account=await refs.createManufacturingPostingAccount(z.reviewer,{expectedVersion:0,contextKey:view.contextKey,input:{chartId:chart.id,code:`NAMED-PROFILE-${index+1}`,name:`Actual original ${field}`,category,currency:'THB',isControl:false,controlKind:'none'}},key());accounts[field]=String(account.id);
 }
 const view=await profiles.manufacturingPostingProfiles(z.owner);expect(view).toMatchObject({available:true,canPrepare:true,currency:'THB',chartId:chart.id,unsupportedExistingMapping:false,profiles:[]});expect(view.accounts.map(account=>account.id)).toEqual(expect.arrayContaining([accounts.inventoryAccountId,accounts.wipAccountId,accounts.laborAccountId,accounts.overheadAccountId,accounts.scrapAccountId]));
 const prepareKey=key(),prepareBody={contextKey:view.contextKey,input:{chartId:chart.id,inventoryAccountId:accounts.inventoryAccountId,wipAccountId:accounts.wipAccountId,laborAccountId:accounts.laborAccountId,overheadAccountId:accounts.overheadAccountId,scrapAccountId:accounts.scrapAccountId}},draft=await profiles.prepareManufacturingPostingProfile(z.owner,prepareBody,prepareKey);expect(draft).toMatchObject({id:`mfg-accounting-${prepareKey}`,status:'draft',version:1});
 const ownView=await profiles.manufacturingPostingProfiles(z.owner),ownKey=key(),ownBody={contextKey:ownView.contextKey,input:{expectedVersion:1,action:'approve',reason:'Same creator cannot independently approve'}};
 await expect(profiles.decideManufacturingPostingProfile(z.owner,String(draft.id),ownBody,ownKey)).rejects.toMatchObject({status:409,code:'CONFLICT'});expect(await db.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(ownKey).first('n')).toBe(0);
 const reviewView=await profiles.manufacturingPostingProfiles(z.manager);expect(reviewView.profiles).toMatchObject([{id:draft.id,status:'draft',version:1,canApprove:true}]);
 const reviewKey=key(),reviewBody={contextKey:reviewView.contextKey,input:{expectedVersion:1,action:'approve',reason:'Independent original Finance approval of exact Manufacturing chart and accounts'}},approved=await profiles.decideManufacturingPostingProfile(z.manager,String(draft.id),reviewBody,reviewKey);expect(approved).toMatchObject({id:draft.id,status:'approved',version:2});expect(await profiles.decideManufacturingPostingProfile(z.manager,String(draft.id),reviewBody,reviewKey)).toEqual(approved);
 expect(await profiles.prepareManufacturingPostingProfile(z.owner,prepareBody,prepareKey)).toEqual(draft);
 expect(await db.prepare('SELECT status,version,created_by,reviewed_by FROM manufacturing_accounting_profiles WHERE id=?').bind(draft.id).first()).toEqual({status:'approved',version:2,created_by:z.owner.userId,reviewed_by:z.manager.userId});
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(z.company).all()).results).toEqual([{package_id:'manufacturing'}]);
 expect(await z.counts()).toEqual([1,13,1,1,1,0,0,0]);
 expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=?').bind(z.company).first('n')).toBe(0);
 expect(await db.prepare('SELECT count(*)::int n FROM manufacturing_order_postings WHERE company_id=?').bind(z.company).first('n')).toBe(0);
});

// Retained original-boundary negative after the separately sealed RED. The unchanged beforeEach
// has a separate legacy Company; this Company's only fixture SQL is the
// existing native organization/worker/assignment helper. Its Finance, mapping
// and master records below are all normal owner commands. No stock is seeded.
it('retains the original generic Count purchase guard for Manufacturing-only first physical stock after actual normal posting and master setup',async()=>{
 const z=await zeroLedgerManufacturingCompany();await z.purchase();
 const refs=await import('../../../src/server/modules/finance/public/manufacturing-posting-prerequisites'),setup=await import('../../../src/server/modules/manufacturing/public/posting-setup'),inventory=await import('../../../src/server/modules/inventory/public/inventory-counts'),{hasPermission}=await import('../../../src/lib/identity_access/permissions');
 const first=await refs.manufacturingPostingPrerequisites(z.reviewer);
 expect(first).toMatchObject({available:true,configured:false,charts:[],accounts:[],periods:[],rates:[],canCreateChart:true});
 if(typeof first.contextKey!=='string')throw Error('Actual Manufacturing chart context required');
 const chart=await refs.createManufacturingPostingChart(z.reviewer,{expectedVersion:0,contextKey:first.contextKey,input:{name:'Actual Manufacturing first-stock chart',baseCurrency:'THB',accountingBasis:'IFRS'}},key()),accounts:Record<string,string>={};
 const roles=[['receivableAccountId','asset','receivable'],['payableAccountId','liability','payable'],['revenueAccountId','revenue','none'],['inventoryAccountId','asset','none'],['clearingAccountId','liability','none'],['cogsAccountId','expense','none'],['inputTaxAccountId','asset','none'],['outputTaxAccountId','liability','none'],['cashAccountId','asset','bank']] as const;
 for(const [index,[field,category,controlKind]] of roles.entries()){
  const view=await refs.manufacturingPostingPrerequisites(z.reviewer);expect(view.canCreateAccount).toBe(true);
  if(typeof view.contextKey!=='string')throw Error('Actual Manufacturing account context required');
  const account=await refs.createManufacturingPostingAccount(z.reviewer,{expectedVersion:0,contextKey:view.contextKey,input:{chartId:chart.id,code:`PHYSICAL-${index+1}`,name:`Actual original ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key());accounts[field]=String(account.id);
 }
 const varianceView=await refs.manufacturingPostingPrerequisites(z.reviewer);
 if(typeof varianceView.contextKey!=='string')throw Error('Actual Manufacturing variance context required');
 const variance=await refs.createManufacturingPostingAccount(z.reviewer,{expectedVersion:0,contextKey:varianceView.contextKey,input:{chartId:chart.id,code:'5400',name:'Actual current-period physical stock variance',category:'expense',currency:'THB',isControl:false,controlKind:'none'}},key());
 const periodView=await refs.manufacturingPostingPrerequisites(z.reviewer);
 if(typeof periodView.contextKey!=='string')throw Error('Actual Manufacturing period context required');
 const period=await refs.createManufacturingPostingPeriod(z.reviewer,{expectedVersion:0,contextKey:periodView.contextKey,input:{name:'Actual Manufacturing first-stock year',startDate:'2026-01-01',endDate:'2026-12-31'}},key());
 const rateView=await refs.manufacturingPostingPrerequisites(z.manager);
 if(typeof rateView.contextKey!=='string')throw Error('Actual Manufacturing rate context required');
 await refs.confirmManufacturingPostingRate(z.manager,{expectedVersion:0,contextKey:rateView.contextKey,input:{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Original rightful Company approver confirms the first physical-count quote'}},key());
 const mappingView=await setup.manufacturingPostingSetup(z.reviewer);expect(mappingView).toMatchObject({configured:false,canConfigure:true,expectedSalesDefaults:null});
 if(typeof mappingView.contextKey!=='string')throw Error('Actual Manufacturing mapping context required');
 const mappingKey=key();expect(await setup.initializeManufacturingPostingSetup(z.reviewer,{expectedVersion:0,contextKey:mappingView.contextKey,reason:'Actual Manufacturing-only mapping before any physical stock',expectedSalesDefaults:mappingView.expectedSalesDefaults,chartId:chart.id,currency:'THB',...accounts},mappingKey)).toEqual({id:z.company,status:'configured',version:1});
 expect(await db.prepare('SELECT operation,record_id FROM command_receipts WHERE command_id=?').bind(mappingKey).first()).toEqual({operation:'commerce.manufacturing-posting-setup.initialize',record_id:z.company});
 const warehouse=await saveMaster(z.owner,'warehouses',null,{branchId:z.branch,code:key(),name:'Actual Manufacturing first physical warehouse'},key()),product=await saveMaster(z.owner,'products',null,{sku:key(),name:'Actually counted original Manufacturing material',itemType:'stock',price:'6',taxBps:700},key());
 // Both are unchanged real actors from the helper: selected native worker,
 // actual published IAM, original positions and independent Company reviewer.
 const preparer=z.owner,stockReviewer=z.manager;
 expect(hasPermission(preparer,'inventory.read')).toBe(true);expect(inventory.canPrepareCounts(preparer)).toBe(true);expect(inventory.canApproveCounts(stockReviewer)).toBe(true);expect(preparer.userId).not.toBe(stockReviewer.userId);
 for(const actor of [preparer,stockReviewer])expect(await db.prepare("SELECT e.status,e.position_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE p.user_id=? AND e.company_id=?").bind(actor.userId,z.company).first()).toMatchObject({status:'active',position_id:expect.any(String)});
 expect(await db.prepare('SELECT category,currency,status,control_kind FROM finance_accounts WHERE id=? AND company_id=?').bind(variance.id,z.company).first()).toEqual({category:'expense',currency:'THB',status:'active',control_kind:'none'});
 expect(await db.prepare('SELECT state FROM finance_periods WHERE id=? AND company_id=?').bind(period.id,z.company).first('state')).toBe('open');
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(z.company).all()).results).toEqual([{package_id:'manufacturing'}]);
 const absent=()=>Promise.all(['commerce_stock','commerce_stock_moves','inventory_adjustment_moves','inventory_order_reservations','inventory_quality_holds'].map(table=>db.prepare(`SELECT count(*)::int n FROM ${table} WHERE warehouse_id=? AND product_id=?`).bind(warehouse.id,product.id).first('n')));
 expect(await absent()).toEqual([0,0,0,0,0]);
 expect(await db.prepare('SELECT count(*)::int n FROM inventory_lot_balances balance JOIN inventory_lots lot ON lot.id=balance.lot_id WHERE balance.warehouse_id=? AND lot.product_id=?').bind(warehouse.id,product.id).first('n')).toBe(0);
 const command=key(),input=inventory.countSchema.parse({warehouseId:String(warehouse.id),countDate:'2026-09-06',reason:'Verify two physical units at six THB in this current period, without acquisition or opening-equity claim',lines:[{productId:String(product.id),countedQuantity:'2',unitCost:'6'}]});
 const image=async()=>({company:await Promise.all(['inventory_counts','finance_journals','finance_financial_events','money_transaction_snapshots'].map(async table=>(await db.prepare(`SELECT to_jsonb(native_row)::text image FROM ${table} native_row WHERE company_id=? ORDER BY image`).bind(z.company).all()).results)),ledger:(await db.prepare('SELECT to_jsonb(movement)::text image FROM inventory_ledger movement JOIN commerce_warehouses warehouse ON warehouse.id=movement.warehouse_id WHERE warehouse.company_id=? ORDER BY image').bind(z.company).all()).results,countLines:(await db.prepare('SELECT to_jsonb(line)::text image FROM inventory_count_lines line JOIN inventory_counts count ON count.id=line.count_id WHERE count.company_id=? ORDER BY image').bind(z.company).all()).results,journalLines:(await db.prepare('SELECT to_jsonb(line)::text image FROM finance_journal_lines line JOIN finance_journals journal ON journal.id=line.journal_id WHERE journal.company_id=? ORDER BY image').bind(z.company).all()).results,absent:await absent(),command:await Promise.all(['command_receipts','audit_events','outbox_events'].map(async table=>(await db.prepare(`SELECT to_jsonb(native_row)::text image FROM ${table} native_row WHERE command_id=? ORDER BY image`).bind(command).all()).results))});
 const before=await image();
 // The separately sealed RED reached the REAL original Supply boundary.
 // Keep that original owner refusal and whole-unit no-effects assertions; the
 // named Manufacturing constructor is a separate source, never a broad gate.
 await expect(inventory.saveCount(preparer,null,input,command)).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
 expect(await image()).toEqual(before);
});
// Ignored helper tail for the existing manufacturing/production native file.
// Invoke only after People's actual normal named Count/Flow/independent post.
// The original positive supplies its actual normally bound post-scan journal
// File. This helper writes only an original disabled legacy Count draft/cancel
// and a diagnostic trigger/sequence; no SQL business/source facts are inserted.
async function assertManufacturingInitialPhysicalStockFileWriterOverlap(context:{
 database:import('../../../src/server/modules/runtime/public/runtime/types').SqlDatabase;
 files:FileStore;companyId:string;countId:string;journalId:string;warehouseId:string;productId:string;
 preparer:WorkContext;fileActor:WorkContext;linkId:string;expectedBytes:string;
}){
 const database=context.database,{attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),inventory=await import('../../../src/server/modules/inventory/public/inventory-counts'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics');
 const count=await database.prepare('SELECT company_id,warehouse_id,journal_id,status FROM inventory_counts WHERE id=?').bind(context.countId).first();
 expect(count).toEqual({company_id:context.companyId,warehouse_id:context.warehouseId,journal_id:context.journalId,status:'posted'});
 expect(await database.prepare('SELECT 1 FROM inventory_mfg_initial_physical_sources WHERE count_id=? AND company_id=?').bind(context.countId,context.companyId).first()).not.toBeNull();
 // The existing positive has already bound/read this actual post-scan journal
 // File through the original owner API; reuse it without a second File mutation.
 const bytes=context.expectedBytes;
 const original=async()=>({count:await database.prepare('SELECT to_jsonb(c)::text image FROM inventory_counts c WHERE id=?').bind(context.countId).first('image'),lines:(await database.prepare('SELECT to_jsonb(l)::text image FROM inventory_count_lines l WHERE count_id=? ORDER BY id').bind(context.countId).all()).results,source:await database.prepare('SELECT to_jsonb(s)::text image FROM inventory_mfg_initial_physical_sources s WHERE count_id=?').bind(context.countId).first('image'),journal:await database.prepare('SELECT to_jsonb(j)::text image FROM finance_journals j WHERE id=?').bind(context.journalId).first('image'),journalLines:(await database.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l WHERE journal_id=? ORDER BY id').bind(context.journalId).all()).results,stock:await database.prepare('SELECT to_jsonb(s)::text image FROM commerce_stock s WHERE warehouse_id=? AND product_id=?').bind(context.warehouseId,context.productId).first('image')});
 const baseline=await original(),writerKey=key(),fault=`mfg_file_count_overlap_${key().replaceAll('-','')}`,mode=process.env.ERPX_COMMERCE_PACKAGE_ENFORCEMENT;
 const body=inventory.countSchema.parse({warehouseId:context.warehouseId,countDate:'2026-09-07',reason:'Original legacy physical re-count draft overlaps retained Manufacturing journal evidence read',lines:[{productId:context.productId,countedQuantity:'2',unitCost:'6'}]});
 // This is the original disabled legacy writer, not a new Manufacturing Count
 // entitlement. The named File source remains strict despite this switch.
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');
 try{
  await database.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${writerKey}') EXECUTE FUNCTION ${fault}()`);
  const observed=await measureReads(async()=>{
   let finished=false;
   const writer=inventory.saveCount(context.preparer,null,body,writerKey).finally(()=>{finished=true;}),outcome=Promise.allSettled([writer]);
   let held=false;for(let attempt=0;attempt<150;attempt++){held=!!await database.prepare(`SELECT is_called FROM ${fault}`).first('is_called');if(held||finished)break;await new Promise(resolve=>setTimeout(resolve,10));}
   if(!held){const [result]=await outcome;if(result.status==='rejected')throw result.reason;throw Error('The actual legacy writer must reach its receipt before the File read starts');}
   expect(finished).toBe(false);
   // Actual public File entry dispatches finance_journal through the original
   // withFinanceAttachmentContentRead; this calls no owner lease helper directly.
   const [created,read]=await Promise.all([outcome,attachmentContent(database,context.files,context.fileActor,context.linkId,'original')]);
   return {created:created[0],text:await read.object.text()};
  });
  expect(observed.result.created.status).toBe('fulfilled');if(observed.result.created.status!=='fulfilled')throw observed.result.created.reason;
  expect(observed.result.created.value).toMatchObject({id:`count-${writerKey}`,status:'draft',version:1});
  expect(observed.result.text).toBe(bytes);
  expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
  expect(await original()).toEqual(baseline);
  expect(await database.prepare("SELECT jsonb_exists(result_json::jsonb,'_manufacturingInitialPhysicalStockCount') marked FROM command_receipts WHERE command_id=?").bind(writerKey).first('marked')).toBe(false);
  expect(await database.prepare('SELECT 1 FROM inventory_mfg_initial_physical_sources WHERE count_id=?').bind(`count-${writerKey}`).first()).toBeNull();
  expect(await database.prepare("SELECT 1 FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(context.linkId).first()).not.toBeNull();
  // Close the legitimate legacy draft normally so it does not become an
  // unrelated pending obligation before the original Manufacturing sequence.
  expect(await inventory.transitionCount(context.preparer,`count-${writerKey}`,'cancel',{expectedVersion:1,attachmentSetIds:[],reason:'Close the legacy overlap draft without changing actual Manufacturing stock'},key())).toMatchObject({id:`count-${writerKey}`,status:'cancelled',version:2});
  expect(await original()).toEqual(baseline);
 }finally{
  try{await database.exec(`DROP TRIGGER IF EXISTS ${fault} ON command_receipts;DROP FUNCTION IF EXISTS ${fault}();DROP SEQUENCE IF EXISTS ${fault}`);}finally{vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT',mode);}
 }
 // Restore enforced generic Count denial; disabled compatibility is not a
 // source entitlement. Do not re-use the committed legacy receipt key here.
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 try{await expect(inventory.saveCount(context.preparer,null,body,key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});}finally{vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT',mode);}
 expect(await original()).toEqual(baseline);
}


// Ignored named-positive continuation. Preserve all32 currently active cases.
// No positive Finance/settings/Stock SQL: only existing native authority fixture
// facts, normal owner commands and explicitly post-scan File fixture evidence.
async function manufacturingInitialPhysicalFixture(){
 const z=await zeroLedgerManufacturingCompany();await z.purchase();
 const refs=await import('../../../src/server/modules/finance/public/manufacturing-posting-prerequisites'),setup=await import('../../../src/server/modules/manufacturing/public/posting-setup'),inventory=await import('../../../src/server/modules/inventory/public/inventory-counts'),masters=await import('../../../src/server/modules/commerce/public/manufacturing-initial-physical-stock-masters'),{hasPermission}=await import('../../../src/lib/identity_access/permissions');
 const first=await refs.manufacturingPostingPrerequisites(z.reviewer);
 expect(first).toMatchObject({available:true,configured:false,charts:[],accounts:[],periods:[],rates:[],canCreateChart:true});
 if(typeof first.contextKey!=='string')throw Error('Actual Manufacturing chart context required');
 const chart=await refs.createManufacturingPostingChart(z.reviewer,{expectedVersion:0,contextKey:first.contextKey,input:{name:'Actual Manufacturing first-stock chart',baseCurrency:'THB',accountingBasis:'IFRS'}},key()),accounts:Record<string,string>={};
 const roles=[['receivableAccountId','asset','receivable'],['payableAccountId','liability','payable'],['revenueAccountId','revenue','none'],['inventoryAccountId','asset','none'],['clearingAccountId','liability','none'],['cogsAccountId','expense','none'],['inputTaxAccountId','asset','none'],['outputTaxAccountId','liability','none'],['cashAccountId','asset','bank']] as const;
 for(const [index,[field,category,controlKind]] of roles.entries()){
  const view=await refs.manufacturingPostingPrerequisites(z.reviewer);expect(view.canCreateAccount).toBe(true);
  if(typeof view.contextKey!=='string')throw Error('Actual Manufacturing account context required');
  const account=await refs.createManufacturingPostingAccount(z.reviewer,{expectedVersion:0,contextKey:view.contextKey,input:{chartId:chart.id,code:`PHYSICAL-${index+1}`,name:`Actual original ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key());accounts[field]=String(account.id);
 }
 const varianceView=await refs.manufacturingPostingPrerequisites(z.reviewer);
 if(typeof varianceView.contextKey!=='string')throw Error('Actual Manufacturing variance context required');
 const variance=await refs.createManufacturingPostingAccount(z.reviewer,{expectedVersion:0,contextKey:varianceView.contextKey,input:{chartId:chart.id,code:'5400',name:'Actual current-period physical stock variance',category:'expense',currency:'THB',isControl:false,controlKind:'none'}},key());
 const periodView=await refs.manufacturingPostingPrerequisites(z.reviewer);
 if(typeof periodView.contextKey!=='string')throw Error('Actual Manufacturing period context required');
 const period=await refs.createManufacturingPostingPeriod(z.reviewer,{expectedVersion:0,contextKey:periodView.contextKey,input:{name:'Actual Manufacturing first-stock year',startDate:'2026-01-01',endDate:'2026-12-31'}},key());
 const rateView=await refs.manufacturingPostingPrerequisites(z.manager);
 if(typeof rateView.contextKey!=='string')throw Error('Actual Manufacturing rate context required');
 await refs.confirmManufacturingPostingRate(z.manager,{expectedVersion:0,contextKey:rateView.contextKey,input:{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Original rightful Company approver confirms the first physical-count quote'}},key());
 const mappingView=await setup.manufacturingPostingSetup(z.reviewer);expect(mappingView).toMatchObject({configured:false,canConfigure:true,expectedSalesDefaults:null});
 if(typeof mappingView.contextKey!=='string')throw Error('Actual Manufacturing mapping context required');
 const mappingKey=key();expect(await setup.initializeManufacturingPostingSetup(z.reviewer,{expectedVersion:0,contextKey:mappingView.contextKey,reason:'Actual Manufacturing-only mapping before any physical stock',expectedSalesDefaults:mappingView.expectedSalesDefaults,chartId:chart.id,currency:'THB',...accounts},mappingKey)).toEqual({id:z.company,status:'configured',version:1});
 expect(await db.prepare('SELECT operation,record_id FROM command_receipts WHERE command_id=?').bind(mappingKey).first()).toEqual({operation:'commerce.manufacturing-posting-setup.initialize',record_id:z.company});
 const profiles=await import('../../../src/server/modules/manufacturing/public/posting-profiles');
 // Prepare the original production profile before economic stock history.
 // Every extra WIP/conversion account is a normal retained-chart owner create.
 for(const [index,[field,category]] of ([['wipAccountId','asset'],['laborAccountId','liability'],['overheadAccountId','liability'],['scrapAccountId','expense']] as const).entries()){
  const view=await refs.manufacturingPostingPrerequisites(z.reviewer);if(typeof view.contextKey!=='string')throw Error('Actual retained Manufacturing profile account context required');
  accounts[field]=String((await refs.createManufacturingPostingAccount(z.reviewer,{expectedVersion:0,contextKey:view.contextKey,input:{chartId:chart.id,code:`INITIAL-PROFILE-${index+1}`,name:`Actual first-stock ${field}`,category,currency:'THB',isControl:false,controlKind:'none'}},key())).id);
 }
 const profileView=await profiles.manufacturingPostingProfiles(z.owner);if(typeof profileView.contextKey!=='string')throw Error('Actual Manufacturing profile context required');
 const profile=await profiles.prepareManufacturingPostingProfile(z.owner,{contextKey:profileView.contextKey,input:{chartId:chart.id,inventoryAccountId:accounts.inventoryAccountId,wipAccountId:accounts.wipAccountId,laborAccountId:accounts.laborAccountId,overheadAccountId:accounts.overheadAccountId,scrapAccountId:accounts.scrapAccountId}},key()),reviewView=await profiles.manufacturingPostingProfiles(z.manager);if(typeof reviewView.contextKey!=='string')throw Error('Actual independent Manufacturing profile review context required');
 expect(await profiles.decideManufacturingPostingProfile(z.manager,String(profile.id),{contextKey:reviewView.contextKey,input:{expectedVersion:1,action:'approve',reason:'Independent original approval before real initial material stock'}},key())).toMatchObject({id:profile.id,status:'approved',version:2});
 const warehouse=await masters.createManufacturingInitialPhysicalStockWarehouse(z.owner,{branchId:z.branch,code:key(),name:'Actual Manufacturing first physical warehouse'},key()),product=await masters.createManufacturingInitialPhysicalStockProduct(z.owner,{sku:key(),name:'Actually counted original Manufacturing material',itemType:'stock',price:'6',taxBps:700},key());
 // Both are unchanged real actors from the helper: selected native worker,
 // actual published IAM, original positions and independent Company reviewer.
 const preparer=z.owner,stockReviewer=z.manager;
 expect(hasPermission(preparer,'inventory.read')).toBe(true);expect(inventory.canPrepareCounts(preparer)).toBe(true);expect(inventory.canApproveCounts(stockReviewer)).toBe(true);expect(preparer.userId).not.toBe(stockReviewer.userId);
 for(const actor of [preparer,stockReviewer])expect(await db.prepare("SELECT e.status,e.position_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE p.user_id=? AND e.company_id=?").bind(actor.userId,z.company).first()).toMatchObject({status:'active',position_id:expect.any(String)});
 expect(await db.prepare('SELECT category,currency,status,control_kind FROM finance_accounts WHERE id=? AND company_id=?').bind(variance.id,z.company).first()).toEqual({category:'expense',currency:'THB',status:'active',control_kind:'none'});
 expect(await db.prepare('SELECT state FROM finance_periods WHERE id=? AND company_id=?').bind(period.id,z.company).first('state')).toBe('open');
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(z.company).all()).results).toEqual([{package_id:'manufacturing'}]);
 const absent=()=>Promise.all(['commerce_stock','commerce_stock_moves','inventory_adjustment_moves','inventory_order_reservations','inventory_quality_holds'].map(table=>db.prepare(`SELECT count(*)::int n FROM ${table} WHERE warehouse_id=? AND product_id=?`).bind(warehouse.id,product.id).first('n')));
 expect(await absent()).toEqual([0,0,0,0,0]);
 expect(await db.prepare('SELECT count(*)::int n FROM inventory_lot_balances balance JOIN inventory_lots lot ON lot.id=balance.lot_id WHERE balance.warehouse_id=? AND lot.product_id=?').bind(warehouse.id,product.id).first('n')).toBe(0);
 return {z,refs,chart,accounts,variance,period,warehouse,product,masters,inventory,preparer,stockReviewer,absent};
}

it('Manufacturing-only initial physical Count binds evidence and posts independent variance before original material issue and released output',async()=>{
 const s=await manufacturingInitialPhysicalFixture(),{z,chart,accounts,variance,period,warehouse,product,inventory,preparer,stockReviewer}=s,initial=await import('../../../src/server/modules/inventory/public/manufacturing-initial-physical-stock-count');
 const input=inventory.countSchema.parse({warehouseId:String(warehouse.id),countDate:'2026-09-06',reason:'Actual independently verified two-unit physical stock at six THB; current-period variance only',lines:[{productId:String(product.id),countedQuantity:'2',unitCost:'6'}]}),genericKey=key();
 const commandImage=async(command:string)=>Promise.all(['command_receipts','audit_events','outbox_events'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE command_id=? ORDER BY image`).bind(command).all()).results));
 const genericBefore={command:await commandImage(genericKey),absent:await s.absent(),counts:(await db.prepare('SELECT to_jsonb(c)::text image FROM inventory_counts c WHERE company_id=? ORDER BY image').bind(z.company).all()).results};
 await expect(inventory.saveCount(preparer,null,input,genericKey)).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
 expect({command:await commandImage(genericKey),absent:await s.absent(),counts:(await db.prepare('SELECT to_jsonb(c)::text image FROM inventory_counts c WHERE company_id=? ORDER BY image').bind(z.company).all()).results}).toEqual(genericBefore);
 const workspace=await initial.manufacturingInitialPhysicalStockCountWorkspace(preparer,{warehouseId:String(warehouse.id),productId:String(product.id),countDate:input.countDate});
 expect(workspace).toMatchObject({available:true,canPrepare:true,unavailableReason:null,readiness:{schemaAvailable:true,postingConfigured:true,varianceAccountAvailable:true,postingPeriodAvailable:true}});
 if(typeof workspace.contextKey!=='string')throw Error('Actual selected Manufacturing initial Count context required');
 const createKey=key(),envelope={expectedVersion:0 as const,contextKey:workspace.contextKey,input},created=await initial.prepareManufacturingInitialPhysicalStockCount(preparer,envelope,createKey),id=String(created.id);
 expect(created).toMatchObject({id:`count-${createKey}`,status:'draft',version:1});expect(await initial.prepareManufacturingInitialPhysicalStockCount(preparer,envelope,createKey)).toEqual(created);
 const raw=await db.prepare('SELECT result_json FROM command_receipts WHERE command_id=?').bind(createKey).first<string>('result_json');if(!raw)throw Error('Actual original Count receipt required');const privateReceipt=JSON.parse(raw);
 expect(privateReceipt._manufacturingInitialPhysicalStockCount).toMatchObject({kind:'manufacturing-initial-physical-stock-count',countId:id,companyId:z.company,warehouseId:warehouse.id,productId:product.id,chartId:chart.id,inventoryAccountId:accounts.inventoryAccountId,varianceAccountId:variance.id});expect(privateReceipt).not.toHaveProperty('_initialPhysicalStockCount');expect(created).not.toHaveProperty('_manufacturingInitialPhysicalStockCount');
 // Explicit post-scan fixture only. The original Count command performs the
 // REAL File policy/source binding; this is not scanner/upload acceptance.
 const sheet=await seedAttachmentSet(db,preparer,{targetType:'stock_count',targetId:id,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1}),submitKey=key(),submitInput={expectedVersion:1,attachmentSetIds:[sheet.setId]},submitted=await initial.transitionManufacturingInitialPhysicalStockCount(preparer,id,'submit',submitInput,submitKey);
 expect(submitted).toMatchObject({id,status:'submitted',version:2});expect(await initial.transitionManufacturingInitialPhysicalStockCount(preparer,id,'submit',submitInput,submitKey)).toEqual(submitted);
 expect(await db.prepare('SELECT state FROM attachment_links WHERE id=?').bind(sheet.linkId).first('state')).toBe('bound');
 async function assignReview(stepKey:'submitted'|'approved',recordVersion:number){
  const flow=await initial.manufacturingInitialPhysicalStockCountFlow(z.owner,id);expect(flow.recordVersion).toBe(recordVersion);expect(flow.canManage).toBe(true);
  const body={stepKey,userId:stockReviewer.userId,expectedVersion:flow.version,recordVersion,reason:'Original Company authority selects the actual independent Stock reviewer',saveDefault:false as const},assignKey=key(),assigned=await initial.assignManufacturingInitialPhysicalStockCountFlow(z.owner,id,body,assignKey);
  expect(assigned).toMatchObject({id,version:(flow.version||1)+1});expect(await initial.assignManufacturingInitialPhysicalStockCountFlow(z.owner,id,body,assignKey)).toEqual(assigned);expect((await initial.manufacturingInitialPhysicalStockCountFlow(stockReviewer,id)).current?.primary?.userId).toBe(stockReviewer.userId);
 }
 await assignReview('submitted',2);
 await expect(initial.transitionManufacturingInitialPhysicalStockCount(preparer,id,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'The original preparer cannot independently approve this Count'},key())).rejects.toMatchObject({status:403});
 await initial.transitionManufacturingInitialPhysicalStockCount(stockReviewer,id,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Independent Stock authority verifies actual quantity and unit cost'},key());await assignReview('approved',3);
 const postKey=key(),postInput={expectedVersion:3,attachmentSetIds:[],reason:'Post actual current-period variance into captured inventory and real expense5400'},posted=await initial.transitionManufacturingInitialPhysicalStockCount(stockReviewer,id,'post',postInput,postKey);expect(posted).toMatchObject({id,status:'posted',version:4});
 expect(await initial.transitionManufacturingInitialPhysicalStockCount(stockReviewer,id,'post',postInput,postKey)).toEqual(posted);expect(await initial.prepareManufacturingInitialPhysicalStockCount(preparer,envelope,createKey)).toEqual(created);
 const detail=await inventory.countDetail(stockReviewer,id),journalId=String(detail.count.journal_id);
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first()).toEqual({quantity_milli:2000,value_minor:1200});
 expect(await db.prepare('SELECT company_id,chart_id,period_id,status,source_type,source_id,is_opening FROM finance_journals WHERE id=?').bind(journalId).first()).toEqual({company_id:z.company,chart_id:chart.id,period_id:period.id,status:'posted',source_type:'stock_count',source_id:id,is_opening:0});
 expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(journalId).all()).results).toEqual([{account_id:accounts.inventoryAccountId,debit_minor:1200,credit_minor:0},{account_id:variance.id,debit_minor:0,credit_minor:1200}].sort((a,b)=>a.account_id.localeCompare(b.account_id)));
 const {attachmentContent,bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service'),sheetKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(sheet.sourceId).first<string>('object_key');if(!sheetKey)throw Error('Actual original sheet storage key required');
 await env.H001_FILES.put(sheetKey,'Actual first Manufacturing Count sheet',{httpMetadata:{contentType:'application/pdf'}});expect(await(await attachmentContent(db,env.H001_FILES,stockReviewer,sheet.linkId,'original')).object.text()).toBe('Actual first Manufacturing Count sheet');
 const journalFile=await seedAttachmentSet(db,z.reviewer,{targetType:'finance_journal',targetId:journalId,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:1});await bindRecordAttachmentSet(db,z.reviewer,journalFile.setId,1,key());
 const journalKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(journalFile.sourceId).first<string>('object_key');if(!journalKey)throw Error('Actual retained Count journal storage key required');await env.H001_FILES.put(journalKey,'Actual retained Manufacturing Count journal',{httpMetadata:{contentType:'application/pdf'}});expect(await(await attachmentContent(db,env.H001_FILES,z.reviewer,journalFile.linkId,'original')).object.text()).toBe('Actual retained Manufacturing Count journal');
 const {journalDetail}=await import('../../../src/server/modules/finance/public/ledger');expect((await journalDetail(z.reviewer,journalId)).journal).toMatchObject({id:journalId,source_type:'stock_count',source_id:id,status:'posted'});
 await assertManufacturingInitialPhysicalStockFileWriterOverlap({database:db,files:env.H001_FILES,companyId:z.company,countId:id,journalId,warehouseId:String(warehouse.id),productId:String(product.id),preparer,fileActor:z.reviewer,linkId:journalFile.linkId,expectedBytes:'Actual retained Manufacturing Count journal'});
 // The very same counted units now feed ORIGINAL Manufacturing commands.
 // Finance approver can author records; Company owner performs independent
 // specification/plan review and QC after the different person's report.
 const output=await s.masters.createManufacturingInitialPhysicalStockProduct(z.owner,{sku:key(),name:'Actual output from first physically counted Manufacturing material',itemType:'stock',price:'12',taxBps:700},key()),spec=await createSpecification(z.manager,{name:'Actual first-stock Manufacturing BOM',mode:'bom',outputProductId:String(output.id),batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:String(product.id),quantity:'2'}]},key());
 await reviewSpecification(z.owner,String(spec.id),{action:'approve',expectedVersion:1,reason:'Different original Company reviewer approves real first-stock material specification'},key());
 const plan=await createPlanningScenario(z.manager,{warehouseId:String(warehouse.id),specificationId:String(spec.id),specificationVersion:2,demandQuantity:'1',dueDate:'2026-09-06',capacityMinutes:100},key()),released=await reviewPlanningRecommendation(z.owner,String(plan.recommendationId),{action:'approve',expectedVersion:1,reason:'Independent actual Manufacturing release uses the counted material'},key()),orderId=String(released.orderId);
 await changeManufacturingOrder(z.manager,orderId,{action:'reserve',expectedVersion:1,expiresAt:Date.now()+3600000,reason:'Reserve precisely two actually counted material units'},key());
 await changeManufacturingOrder(z.manager,orderId,{action:'issue',expectedVersion:2,entryDate:'2026-09-06',reason:'Issue original counted material into the actual approved WIP profile'},key());
 expect(await db.prepare("SELECT COALESCE(sum(line.debit_minor-line.credit_minor),0)::text balance FROM finance_journal_lines line JOIN finance_journals journal ON journal.id=line.journal_id WHERE journal.company_id=? AND journal.status='posted' AND line.account_id=?").bind(z.company,accounts.wipAccountId).first('balance')).toBe('1200');
 await changeManufacturingOrder(z.manager,orderId,{action:'report',expectedVersion:3,outputQuantity:'1',scrapQuantity:'0',laborCost:'0',overheadCost:'0',entryDate:'2026-09-06',reason:'Different native worker reports the actual material-only output'},key());
 await changeManufacturingOrder(z.owner,orderId,{action:'pass-qc',expectedVersion:4,entryDate:'2026-09-06',reason:'Independent original Company QC releases one actual finished unit'},key());
 expect((await productionDetail(z.owner,orderId)).order.status).toBe('completed');expect(await db.prepare("SELECT COALESCE(sum(line.debit_minor-line.credit_minor),0)::text balance FROM finance_journal_lines line JOIN finance_journals journal ON journal.id=line.journal_id WHERE journal.company_id=? AND journal.status='posted' AND line.account_id=?").bind(z.company,accounts.wipAccountId).first('balance')).toBe('0');
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first()).toEqual({quantity_milli:0,value_minor:0});expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,output.id).first()).toEqual({quantity_milli:1000,value_minor:1200});
 const retainedReceipts={create:await commandImage(createKey),post:await commandImage(postKey)};
 expect(await initial.prepareManufacturingInitialPhysicalStockCount(preparer,envelope,createKey)).toEqual(created);expect(await initial.transitionManufacturingInitialPhysicalStockCount(stockReviewer,id,'post',postInput,postKey)).toEqual(posted);expect({create:await commandImage(createKey),post:await commandImage(postKey)}).toEqual(retainedReceipts);
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(z.company).all()).results).toEqual([{package_id:'manufacturing'}]);
});

// Ignored additive security probe; keep the complete accepted33-case prefix.
// Source UPDATE below is an explicit deferred fault after the real owner
// receipt, never a positive master/Stock fixture or normal lifecycle claim.
it('Manufacturing initial Count keeps strict source and Stock authority then rolls back late product drift before an unchanged-key retry',async()=>{
 const s=await manufacturingInitialPhysicalFixture(),initial=await import('../../../src/server/modules/inventory/public/manufacturing-initial-physical-stock-count'),selection={warehouseId:String(s.warehouse.id),productId:String(s.product.id),countDate:'2026-09-06'},workspace=await initial.manufacturingInitialPhysicalStockCountWorkspace(s.preparer,selection);
 expect(workspace.canPrepare).toBe(true);if(typeof workspace.contextKey!=='string')throw Error('Actual selected initial Count context required');
 const envelope={expectedVersion:0 as const,contextKey:workspace.contextKey,input:s.inventory.countSchema.parse({warehouseId:selection.warehouseId,countDate:selection.countDate,reason:'Actual source-fenced current-period physical verification before first material stock',lines:[{productId:selection.productId,countedQuantity:'2',unitCost:'6'}]})};
 const tables=['commerce_products','commerce_warehouses','commerce_settings','commerce_stock','commerce_stock_moves','inventory_ledger','inventory_counts','inventory_count_lines','inventory_count_lot_lines','inventory_count_decisions','inventory_adjustment_moves','inventory_mfg_initial_physical_sources','inventory_lots','inventory_lot_balances','inventory_lot_moves','inventory_order_reservations','inventory_quality_holds','manufacturing_material_reservations','manufacturing_inventory_moves','finance_charts','finance_accounts','finance_periods','finance_journals','finance_journal_lines','finance_book_postings','finance_financial_events','money_transaction_snapshots','command_receipts','audit_events','outbox_events','command_assertions'];
 const image=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(native_row)::text image FROM ${table} native_row ORDER BY image`).all()).results));
 // This preceding unrelated original legacy Company has no purchases. Only
 // authority/fixture provenance is reused: not a zero-ledger or withdrawal
 // claim. Disabled legacy mode cannot bypass the new named strict capability.
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(f.company).all()).results).toEqual([]);expect(s.inventory.canPrepareCounts(f.owner)).toBe(true);
 const priorMode=process.env.ERPX_COMMERCE_PACKAGE_ENFORCEMENT;vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');
 try{const before=await image();await expect(initial.prepareManufacturingInitialPhysicalStockCount(f.owner,envelope,key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await image()).toEqual(before);}finally{vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT',priorMode);}
 // Actual same-Company published Finance-preparer IAM can read Stock but
 // cannot create a Count. No context, module or permission object is forged.
 expect(s.inventory.canReadCounts(s.z.reviewer)).toBe(true);expect(s.inventory.canPrepareCounts(s.z.reviewer)).toBe(false);
 {const before=await image();await expect(initial.prepareManufacturingInitialPhysicalStockCount(s.z.reviewer,envelope,key())).rejects.toMatchObject({status:403});expect(await image()).toEqual(before);}
 // These are REAL warehouse/product identities from the unchanged legacy
 // beforeEach Company, including its original existing Stock counterexample.
 // Our genuine purchased Manufacturing actor cannot select another Company.
 {const before=await image();await expect(initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,{...envelope,input:{...envelope.input,warehouseId:warehouse,lines:[{productId:material,countedQuantity:'2',unitCost:'6'}]}},key())).rejects.toMatchObject({status:404,code:'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SELECTION_UNAVAILABLE'});expect(await image()).toEqual(before);}
 const command=key(),id=`count-${command}`,fault=`mfg_count_late_source_${key().replaceAll('-','')}`,before=await image();
 expect(s.product.id).toMatch(/^products-[a-f0-9-]+$/);
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE commerce_products SET name='Deferred changed actual initial Count product' WHERE id='${s.product.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{
  await expect(initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,envelope,command)).rejects.toMatchObject({status:409,code:'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});
  expect(await image()).toEqual(before);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}
 const created=await initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,envelope,command);expect(created).toMatchObject({id,status:'draft',version:1});
 const committed=await image();expect(await initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,envelope,command)).toEqual(created);expect(await image()).toEqual(committed);
 expect(await db.prepare('SELECT count(*)::int n FROM inventory_mfg_initial_physical_sources WHERE count_id=? AND company_id=?').bind(id,s.z.company).first('n')).toBe(1);expect(await s.absent()).toEqual([0,0,0,0,0]);
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(s.z.company).all()).results).toEqual([{package_id:'manufacturing'}]);
});

// Additive1059 boundary candidate. This does not replace the separately proven
// normal named Count→WIP→output case and never changes a migration receipt.
function manufacturing1059NativeFailure(error:unknown):{code?:string;message?:string}{
 let value=error;for(let index=0;index<8&&value&&typeof value==='object';index++){if('code' in value&&value.code==='23514')return {code:'23514',message:'message' in value&&typeof value.message==='string'?value.message:undefined};if(!('cause' in value))break;value=value.cause;}return {};
}
it('1059 rejects an actual pre1059 orphan private marker and rolls back every new native guard object without adopting it',async()=>{
 const {mkdtemp,mkdir,readdir,cp,rm}=await import('node:fs/promises'),{resolve,join}=await import('node:path'),{Pool}=await import('pg'),{PostgresDatabase}=await import('../../../src/server/modules/runtime/adapters/runtime/postgres'),{migrateDatabase}=await import('../../../src/server/modules/runtime/public/runtime/migrations');
 const roots=await mkdtemp(resolve('.runtime/tests/manufacturing1059-')),oldRoot=join(roots,'old'),targetRoot=join(roots,'target'),adminUrl=new URL(process.env.ERPX_TEST_ADMIN_URL??'postgresql://erpx_test_admin@127.0.0.1:5432/postgres');
 if(!['postgresql:','postgres:'].includes(adminUrl.protocol)||adminUrl.search||adminUrl.hash||adminUrl.hostname!=='127.0.0.1'||adminUrl.port!=='5432'||adminUrl.username!=='erpx_test_admin'||adminUrl.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');
 const databaseName=`erpx_test_mfg1059_${key().replaceAll('-','')}`;if(!/^erpx_test_mfg1059_[a-f0-9]{32}$/.test(databaseName))throw Error('ISOLATED_DATABASE_REQUIRED');const admin=new Pool({connectionString:adminUrl.href,max:1});let isolated:InstanceType<typeof PostgresDatabase>|undefined,created=false;
 try{
  const role=(await admin.query('SELECT current_user name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user')).rows[0];if(role?.name!=='erpx_test_admin'||role.rolsuper||!role.rolcreatedb||role.rolcreaterole)throw Error('UNSAFE_TEST_DATABASE_ROLE');
  await admin.query(`CREATE DATABASE "${databaseName}" TEMPLATE template0`);created=true;const targetUrl=new URL(adminUrl);targetUrl.pathname=databaseName;isolated=new PostgresDatabase({connectionString:targetUrl.href,max:2});const native=isolated;
  for(const root of [oldRoot,targetRoot])await mkdir(join(root,'holding-write'),{recursive:true});
  const sql=(await readdir(resolve('migrations/postgres/holding-write'))).filter(name=>name.endsWith('.sql')&&Number(name.slice(0,4))<=1059);expect(sql.filter(name=>name.startsWith('1059_'))).toEqual(['1059_manufacturing_initial_physical_stock_count_source.sql']);
  for(const name of sql){await cp(resolve('migrations/postgres/holding-write',name),join(targetRoot,'holding-write',name));if(Number(name.slice(0,4))<=1058)await cp(resolve('migrations/postgres/holding-write',name),join(oldRoot,'holding-write',name));}
  await migrateDatabase(native,'holding-write',oldRoot);
  const objects=()=>native.prepare("SELECT to_regclass('inventory_mfg_initial_physical_sources') source,to_regclass('inventory_mfg_initial_physical_receipt_record') receipt_index,to_regprocedure('inventory_assert_mfg_initial_physical_source(text)') assertion").first();expect(await objects()).toEqual({source:null,receipt_index:null,assertion:null});expect(await native.prepare("SELECT count(*)::int n FROM erpx_migrations WHERE name LIKE '1059_%'").first('n')).toBe(0);
  // Deliberately corrupt, private disposable data only. No Company, chart,
  // account, Count, Stock, ledger or accepted source is fabricated here.
  const command=key(),record=`count-${command}`;await native.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,'inventory.count.create','adversarial-migration-fixture',?,?,?,1,?)").bind(command,'a'.repeat(64),record,JSON.stringify({id:record,status:'draft',version:1,_manufacturingInitialPhysicalStockCount:{kind:'manufacturing-initial-physical-stock-count',countId:record}}),JSON.stringify({companyId:'deliberately-unowned-company'})).run();
  const rows=async()=>{const names=(await native.prepare("SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename").all<{tablename:string}>()).results.map(row=>row.tablename);return Promise.all(names.map(async table=>{if(!/^[a-z][a-z0-9_]*$/.test(table))throw Error('DECLARED_SCHEMA_IDENTIFIER_REQUIRED');return {table,...await native.prepare(`SELECT count(*)::int rows,md5(coalesce(jsonb_agg(to_jsonb(row) ORDER BY to_jsonb(row)::text),'[]'::jsonb)::text) digest FROM "${table}" row`).first<{rows:number;digest:string}>()};}));};
  const definitions=()=>Promise.all([
   native.prepare("SELECT relname,relkind::text,relacl::text FROM pg_class WHERE relnamespace='public'::regnamespace ORDER BY relname").all().then(row=>row.results),
   native.prepare("SELECT proname,pg_get_function_identity_arguments(oid) arguments,pg_get_functiondef(oid) definition,proacl::text FROM pg_proc WHERE pronamespace='public'::regnamespace AND prokind IN('f','p') ORDER BY proname,pg_get_function_identity_arguments(oid)").all().then(row=>row.results),
   native.prepare("SELECT relation.relname,t.tgname,pg_get_triggerdef(t.oid) definition FROM pg_trigger t JOIN pg_class relation ON relation.oid=t.tgrelid WHERE relation.relnamespace='public'::regnamespace AND NOT t.tgisinternal ORDER BY relation.relname,t.tgname").all().then(row=>row.results),
   native.prepare("SELECT relation.relname,constraint_row.conname,pg_get_constraintdef(constraint_row.oid) definition FROM pg_constraint constraint_row JOIN pg_class relation ON relation.oid=constraint_row.conrelid WHERE constraint_row.connamespace='public'::regnamespace ORDER BY relation.relname,constraint_row.conname").all().then(row=>row.results),
  ]);
  const retained=await rows(),schema=await definitions(),receipts=(await native.prepare('SELECT name,checksum,applied_at::text applied_at FROM erpx_migrations ORDER BY name').all()).results;let failure:unknown;try{await migrateDatabase(native,'holding-write',targetRoot);}catch(error){failure=error;}
  expect(manufacturing1059NativeFailure(failure)).toMatchObject({code:'23514',message:'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});expect(await objects()).toEqual({source:null,receipt_index:null,assertion:null});expect(await rows()).toEqual(retained);expect(await definitions()).toEqual(schema);expect((await native.prepare('SELECT name,checksum,applied_at::text applied_at FROM erpx_migrations ORDER BY name').all()).results).toEqual(receipts);expect(await native.prepare('SELECT record_id FROM command_receipts WHERE command_id=?').bind(command).first('record_id')).toBe(record);
 }finally{
  await isolated?.close();if(created){const databaseOwner=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[databaseName])).rows[0]?.owner;if(databaseOwner!=='erpx_test_admin')throw Error('ISOLATED_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${databaseName}"`);}await admin.end();await rm(roots,{recursive:true,force:true});
 }
},180000);

it('1059 preserves actual named Count reciprocity against precise source and receipt mutations and retains exact original replay',async()=>{
 const s=await manufacturingInitialPhysicalFixture(),initial=await import('../../../src/server/modules/inventory/public/manufacturing-initial-physical-stock-count'),selection={warehouseId:String(s.warehouse.id),productId:String(s.product.id),countDate:'2026-09-06'},view=await initial.manufacturingInitialPhysicalStockCountWorkspace(s.preparer,selection);expect(view.canPrepare).toBe(true);if(typeof view.contextKey!=='string')throw Error('Actual owner selected-source context required');
 const command=key(),body={expectedVersion:0 as const,contextKey:view.contextKey,input:{warehouseId:selection.warehouseId,countDate:selection.countDate,reason:'Actual one-line physical count for reciprocal native-source boundary proof',lines:[{productId:selection.productId,countedQuantity:'2',unitCost:'6'}]}},created=await initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,body,command),id=String(created.id),source=await db.prepare('SELECT count_command_id,manufacturing_setup_command_id FROM inventory_mfg_initial_physical_sources WHERE count_id=?').bind(id).first<{count_command_id:string;manufacturing_setup_command_id:string}>();if(!source)throw Error('Actual immutable source relation required');expect(source.count_command_id).toBe(command);const setupCommand=source.manufacturing_setup_command_id,orphanKey=key(),dualKey=key(),commands=[command,setupCommand,orphanKey,dualKey];
 const image=()=>Promise.all([
  db.prepare('SELECT to_jsonb(row)::text image FROM inventory_counts row WHERE id=?').bind(id).all().then(rows=>rows.results),
  db.prepare('SELECT to_jsonb(row)::text image FROM inventory_count_lines row WHERE count_id=? ORDER BY image').bind(id).all().then(rows=>rows.results),
  db.prepare('SELECT to_jsonb(row)::text image FROM inventory_count_lot_lines row JOIN inventory_count_lines line ON line.id=row.count_line_id WHERE line.count_id=? ORDER BY image').bind(id).all().then(rows=>rows.results),
  db.prepare('SELECT to_jsonb(row)::text image FROM inventory_mfg_initial_physical_sources row WHERE count_id=?').bind(id).all().then(rows=>rows.results),
  ...['command_receipts','audit_events','outbox_events'].map(table=>db.prepare(`SELECT to_jsonb(row)::text image FROM ${table} row WHERE command_id IN(?,?,?,?) ORDER BY image`).bind(...commands).all().then(rows=>rows.results)),
  db.prepare('SELECT to_jsonb(row)::text image FROM commerce_stock row WHERE warehouse_id=? AND product_id=?').bind(selection.warehouseId,selection.productId).all().then(rows=>rows.results),
  db.prepare('SELECT to_jsonb(row)::text image FROM inventory_ledger row WHERE source_id=? ORDER BY image').bind(id).all().then(rows=>rows.results),
  db.prepare('SELECT to_jsonb(row)::text image FROM finance_journals row WHERE company_id=? ORDER BY image').bind(s.z.company).all().then(rows=>rows.results),
  db.prepare('SELECT to_jsonb(row)::text image FROM money_transaction_snapshots row WHERE company_id=? ORDER BY image').bind(s.z.company).all().then(rows=>rows.results),
  db.prepare('SELECT table_name,revision::text revision FROM work_cache_revision_baselines ORDER BY table_name').all().then(rows=>rows.results),
 ]);
 const retained=await image(),mutations:[string,(tx:import('../../../src/server/modules/runtime/public/runtime/types').SqlDatabase)=>Promise<unknown>][]=[
  ['source update immutability',tx=>tx.prepare("UPDATE inventory_mfg_initial_physical_sources SET proof_json=jsonb_set(proof_json,'{currency}','\"USD\"'::jsonb) WHERE count_id=?").bind(id).run()],
  ['source delete immutability',tx=>tx.prepare('DELETE FROM inventory_mfg_initial_physical_sources WHERE count_id=?').bind(id).run()],
  ['marker removal birth guard',tx=>tx.prepare("UPDATE command_receipts SET result_json=(result_json::jsonb-'_manufacturingInitialPhysicalStockCount')::text WHERE command_id=?").bind(command).run()],
  ['original zero snapshot reciprocity',tx=>tx.prepare('UPDATE inventory_count_lines SET snapshot_quantity_milli=1 WHERE count_id=?').bind(id).run()],
  ['original Count creator reciprocity',tx=>tx.prepare("UPDATE inventory_counts SET created_by='different-adversarial-actor' WHERE id=?").bind(id).run()],
  ['captured setup receipt hash reciprocity',tx=>tx.prepare('UPDATE command_receipts SET request_hash=? WHERE command_id=?').bind('b'.repeat(64),setupCommand).run()],
  ['captured setup receipt status reciprocity',tx=>tx.prepare("UPDATE command_receipts SET result_json=jsonb_set(result_json::jsonb,'{status}','\"draft\"'::jsonb)::text WHERE command_id=?").bind(setupCommand).run()],
  ['orphan marked receipt reciprocal guard',tx=>tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) SELECT ?,operation,actor_id,request_hash,?,result_json,created_at,authority_json FROM command_receipts WHERE command_id=?").bind(orphanKey,`count-${orphanKey}`,command).run()],
  ['exclusive Sales or Manufacturing marker birth guard',tx=>tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) SELECT ?,operation,actor_id,request_hash,?,jsonb_set(result_json::jsonb,'{_initialPhysicalStockCount}','{}'::jsonb)::text,created_at,authority_json FROM command_receipts WHERE command_id=?").bind(dualKey,`count-${dualKey}`,command).run()],
 ];
 for(const [purpose,mutate] of mutations){let failure:unknown;try{await db.transaction(async tx=>{await mutate(tx);});}catch(error){failure=error;}expect(manufacturing1059NativeFailure(failure),purpose).toMatchObject({code:'23514',message:'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});expect(await image(),purpose).toEqual(retained);}
 expect(await initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,body,command)).toEqual(created);expect(await image()).toEqual(retained);expect((await initial.manufacturingInitialPhysicalStockCountWorkspace(s.stockReviewer,{countId:id})).record).toMatchObject({id,status:'draft',version:1});expect(await s.absent()).toEqual([0,0,0,0,0]);
});

// CURRENT-schema recovery only. Source is the real disposable native H001 DB,
// never an installed database and never described as a pre1059 upgrade.
it('restores populated current1059 Manufacturing Count provenance into a fresh owned native database without altering its source',async()=>{
 const {execFile}=await import('node:child_process'),{promisify}=await import('node:util'),{mkdtemp,chmod,rm}=await import('node:fs/promises'),{resolve,join}=await import('node:path'),{Pool}=await import('pg'),{PostgresDatabase}=await import('../../../src/server/modules/runtime/adapters/runtime/postgres'),run=promisify(execFile),pgDump='/opt/homebrew/opt/postgresql@18/bin/pg_dump',pgRestore='/opt/homebrew/opt/postgresql@18/bin/pg_restore';
 const adminUrl=new URL(process.env.ERPX_TEST_ADMIN_URL??'postgresql://erpx_test_admin@127.0.0.1:5432/postgres');if(!['postgresql:','postgres:'].includes(adminUrl.protocol)||adminUrl.search||adminUrl.hash||adminUrl.hostname!=='127.0.0.1'||adminUrl.port!=='5432'||adminUrl.username!=='erpx_test_admin'||adminUrl.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');
 const sourceIdentity=await db.prepare("SELECT current_database() name,current_user actor,pg_get_userbyid(datdba) owner,host(inet_server_addr()) host,inet_server_port() port FROM pg_database WHERE datname=current_database()").first<{name:string;actor:string;owner:string;host:string;port:number}>();if(!sourceIdentity||!/^erpx_test_[a-f0-9]{10}_h001_db$/.test(sourceIdentity.name)||sourceIdentity.actor!=='erpx_test_admin'||sourceIdentity.owner!=='erpx_test_admin'||sourceIdentity.host!=='127.0.0.1'||sourceIdentity.port!==5432)throw Error('EXACT_NATIVE_SOURCE_DATABASE_REQUIRED');
 const databaseName=`erpx_test_mfg1059_restore_${key().replaceAll('-','')}`;if(!/^erpx_test_mfg1059_restore_[a-f0-9]{32}$/.test(databaseName))throw Error('ISOLATED_DATABASE_REQUIRED');const directory=await mkdtemp(resolve('.runtime/tests/manufacturing1059-current-')),archive=join(directory,'current1059.dump'),admin=new Pool({connectionString:adminUrl.href,max:1});let restored:InstanceType<typeof PostgresDatabase>|undefined,created=false;
 type Database=import('../../../src/server/modules/runtime/public/runtime/types').SqlDatabase;
 async function image(database:Database){
  const names=(await database.prepare("SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename").all<{tablename:string}>()).results.map(row=>row.tablename),rows=await Promise.all(names.map(async table=>{if(!/^[a-z][a-z0-9_]*$/.test(table))throw Error('DECLARED_SCHEMA_IDENTIFIER_REQUIRED');return {table,...await database.prepare(`SELECT count(*)::int rows,md5(coalesce(jsonb_agg(to_jsonb(row) ORDER BY to_jsonb(row)::text),'[]'::jsonb)::text) digest FROM "${table}" row`).first<{rows:number;digest:string}>()};}));
  const [receipts,relations,functions,triggers,constraints,columns,sequences]=await Promise.all([
   database.prepare('SELECT name,checksum,applied_at::text applied_at FROM erpx_migrations ORDER BY name').all(),
   // Canonical EFFECTIVE ACLs preserve grantor/grantee/options while allowing
   // only PostgreSQL's equivalent nullable/default ACL-array representation.
   database.prepare("SELECT c.relname name,c.relkind::text kind,pg_get_userbyid(c.relowner) owner,COALESCE((SELECT jsonb_agg(jsonb_build_object('grantor',pg_get_userbyid(a.grantor),'grantee',CASE WHEN a.grantee=0 THEN 'PUBLIC' ELSE pg_get_userbyid(a.grantee) END,'privilege',a.privilege_type,'grantable',a.is_grantable) ORDER BY pg_get_userbyid(a.grantor),CASE WHEN a.grantee=0 THEN 'PUBLIC' ELSE pg_get_userbyid(a.grantee) END,a.privilege_type,a.is_grantable)::text FROM aclexplode(NULLIF(COALESCE(c.relacl,CASE WHEN c.relkind IN('r','p','v','m','f','S') THEN acldefault(CASE WHEN c.relkind='S' THEN 's'::\"char\" ELSE 'r'::\"char\" END,c.relowner) ELSE '{}'::aclitem[] END),'{}'::aclitem[])) a),'[]') acl FROM pg_class c WHERE c.relnamespace='public'::regnamespace ORDER BY c.relname").all<{name:string;kind:string;owner:string;acl:string}>(),
   database.prepare("SELECT p.proname name,pg_get_function_identity_arguments(p.oid) arguments,pg_get_functiondef(p.oid) definition,pg_get_userbyid(p.proowner) owner,COALESCE((SELECT jsonb_agg(jsonb_build_object('grantor',pg_get_userbyid(a.grantor),'grantee',CASE WHEN a.grantee=0 THEN 'PUBLIC' ELSE pg_get_userbyid(a.grantee) END,'privilege',a.privilege_type,'grantable',a.is_grantable) ORDER BY pg_get_userbyid(a.grantor),CASE WHEN a.grantee=0 THEN 'PUBLIC' ELSE pg_get_userbyid(a.grantee) END,a.privilege_type,a.is_grantable)::text FROM aclexplode(NULLIF(COALESCE(p.proacl,acldefault('f'::\"char\",p.proowner)),'{}'::aclitem[])) a),'[]') acl FROM pg_proc p WHERE p.pronamespace='public'::regnamespace AND p.prokind IN('f','p') ORDER BY p.proname,pg_get_function_identity_arguments(p.oid)").all<{name:string;arguments:string;definition:string;owner:string;acl:string}>(),
   database.prepare("SELECT c.relname relation,t.tgname name,pg_get_triggerdef(t.oid) definition FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid WHERE c.relnamespace='public'::regnamespace AND NOT t.tgisinternal ORDER BY c.relname,t.tgname").all(),
   database.prepare("SELECT c.relname relation,k.conname name,pg_get_constraintdef(k.oid) definition FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid WHERE k.connamespace='public'::regnamespace ORDER BY c.relname,k.conname").all<{relation:string;name:string;definition:string}>(),
   database.prepare("SELECT table_name relation,column_name name,ordinal_position position,udt_name type,is_nullable nullable,column_default default_value,is_identity identity,is_generated generated FROM information_schema.columns WHERE table_schema='public' ORDER BY table_name,ordinal_position").all(),
   database.prepare("SELECT sequencename name,last_value::text value FROM pg_sequences WHERE schemaname='public' ORDER BY sequencename").all(),
  ]);
  return {rows,receipts:receipts.results,schema:{relations:relations.results,functions:functions.results,triggers:triggers.results,constraints:constraints.results},columns:columns.results,sequences:sequences.results};
 }
 try{
  const role=(await admin.query('SELECT current_user name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user')).rows[0];if(role?.name!=='erpx_test_admin'||role.rolsuper||!role.rolcreatedb||role.rolcreaterole)throw Error('UNSAFE_TEST_DATABASE_ROLE');
  const s=await manufacturingInitialPhysicalFixture(),initial=await import('../../../src/server/modules/inventory/public/manufacturing-initial-physical-stock-count'),selection={warehouseId:String(s.warehouse.id),productId:String(s.product.id),countDate:'2026-09-06'},view=await initial.manufacturingInitialPhysicalStockCountWorkspace(s.preparer,selection);if(typeof view.contextKey!=='string'||!view.readiness.schemaAvailable||!view.canPrepare)throw Error('Actual current1059 selected Count source required');
  const command=key(),body={expectedVersion:0 as const,contextKey:view.contextKey,input:{warehouseId:selection.warehouseId,countDate:selection.countDate,reason:'Actual current-schema reciprocal source retained by native dump and independent fresh restore',lines:[{productId:selection.productId,countedQuantity:'2',unitCost:'6'}]}},count=await initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,body,command),id=String(count.id);expect(count).toMatchObject({id:`count-${command}`,status:'draft',version:1});expect(await db.prepare('SELECT count(*)::int n FROM inventory_mfg_initial_physical_sources WHERE count_id=?').bind(id).first('n')).toBe(1);expect(await db.prepare("SELECT count(*)::int n FROM erpx_migrations WHERE name='1059_manufacturing_initial_physical_stock_count_source.sql'").first('n')).toBe(1);
  const retained=await image(db),cliEnv={...process.env,PGHOST:adminUrl.hostname,PGPORT:adminUrl.port,PGUSER:adminUrl.username,PGPASSWORD:decodeURIComponent(adminUrl.password),PGDATABASE:sourceIdentity.name};expect((await run(pgDump,['--version'],{env:cliEnv})).stdout).toContain('18.6');expect((await run(pgRestore,['--version'],{env:cliEnv})).stdout).toContain('18.6');await run(pgDump,['--format=custom','--no-owner','--file',archive],{env:cliEnv,timeout:120000});await chmod(archive,0o600);
  await admin.query(`CREATE DATABASE "${databaseName}" TEMPLATE template0`);created=true;const restoreUrl=new URL(adminUrl);restoreUrl.pathname=databaseName;restored=new PostgresDatabase({connectionString:restoreUrl.href,max:2});
  // A fresh empty DB, with ACLs retained under the SAME restricted native owner.
  // No --clean, no restored migrations, and no source/global-binding mutation.
  await run(pgRestore,['--exit-on-error','--transaction-size=64','--no-owner','--dbname',databaseName,archive],{env:{...cliEnv,PGDATABASE:databaseName},timeout:120000});
  const actual=await image(restored);expect(actual.rows).toEqual(retained.rows);expect(actual.receipts).toEqual(retained.receipts);expect({...actual,schema:{...actual.schema,constraints:[]}}).toEqual({...retained,schema:{...retained.schema,constraints:[]}});expect(actual.schema.constraints.map(row=>({...row,definition:null}))).toEqual(retained.schema.constraints.map(row=>({...row,definition:null})));
  // Reparse ONLY differing native CHECK text, exactly like the existing1058
  // migration test. No generic whitespace/regex normalization or dropped guard.
  const sourceConstraints=retained.schema.constraints,restoredConstraints=actual.schema.constraints,sourceCanonical=sourceConstraints.map(row=>({...row})),restoredCanonical=restoredConstraints.map(row=>({...row})),mismatches=sourceConstraints.flatMap((row,index)=>row.definition===restoredConstraints[index].definition?[]:[index]),rollback=Error('ROLLBACK_TEMPORARY_RESTORE_CHECK_OBSERVER'),temporaryNames:string[]=[],quote=(name:string)=>'"'+name.replaceAll('"','""')+'"',checkType="SELECT k.contype='c' is_check FROM pg_constraint k JOIN pg_class relation ON relation.oid=k.conrelid WHERE k.connamespace='public'::regnamespace AND relation.relname=? AND k.conname=?";
  try{await restored.transaction(async tx=>{
   for(const index of mismatches){const row=sourceConstraints[index];expect(await db.prepare(checkType).bind(row.relation,row.name).first('is_check')).toBe(true);expect(await tx.prepare(checkType).bind(row.relation,row.name).first('is_check')).toBe(true);
    for(const [side,definition] of [['source',row.definition],['restored',restoredConstraints[index].definition]] as const){const name=`mfg_restore_check_${key().replaceAll('-','')}`;temporaryNames.push(name);await tx.exec(`CREATE TEMP TABLE ${quote(name)} (LIKE public.${quote(row.relation)}) ON COMMIT DROP;ALTER TABLE pg_temp.${quote(name)} ADD CONSTRAINT native_check ${definition}`);const canonical=await tx.prepare("SELECT pg_get_constraintdef(oid) definition FROM pg_constraint WHERE conrelid=to_regclass(?) AND conname='native_check'").bind(`pg_temp.${quote(name)}`).first<string>('definition');if(!canonical)throw Error('ACTUAL_NATIVE_CHECK_DEFINITION_REQUIRED');(side==='source'?sourceCanonical:restoredCanonical)[index].definition=canonical;}
   }throw rollback;
  });}catch(error){if(error!==rollback)throw error;}
  expect({...actual,schema:{...actual.schema,constraints:restoredCanonical}}).toEqual({...retained,schema:{...retained.schema,constraints:sourceCanonical}});expect(await image(restored)).toEqual(actual);for(const name of temporaryNames)expect(await restored.prepare('SELECT count(*)::int n FROM pg_class WHERE relname=?').bind(name).first('n')).toBe(0);
  await restored.prepare('SELECT inventory_assert_mfg_initial_physical_source(?)').bind(id).all();expect(await restored.prepare('SELECT count(*)::int n FROM inventory_mfg_initial_physical_sources WHERE count_id=?').bind(id).first('n')).toBe(1);
  // The restored original reciprocal immutability guard must execute, not just
  // have the right text. This private negative is rolled back in the restore DB.
  let rejected:unknown;try{await restored.prepare("UPDATE inventory_mfg_initial_physical_sources SET currency='USD' WHERE count_id=?").bind(id).run();}catch(error){rejected=error;}let nativeFailure:unknown=rejected;for(let depth=0;depth<8&&nativeFailure&&typeof nativeFailure==='object'&&!('code' in nativeFailure)&&'cause' in nativeFailure;depth++)nativeFailure=nativeFailure.cause;expect(nativeFailure).toMatchObject({code:'23514',message:'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});expect(await image(restored)).toEqual(actual);
  expect(await image(db)).toEqual(retained);expect(await initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,body,command)).toEqual(count);expect(await image(db)).toEqual(retained);expect(await s.absent()).toEqual([0,0,0,0,0]);
 }finally{
  await restored?.close();if(created){const owner=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[databaseName])).rows[0]?.owner;if(owner!=='erpx_test_admin')throw Error('ISOLATED_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${databaseName}"`);}await admin.end();await rm(directory,{recursive:true,force:true});
 }
},180000);

// Ignored one actual owner-lifecycle regression. No synthetic obligation
// reader, status repair, receipt fixture, Stock liquidation or installed Apply.
it('the Manufacturing Inventory owner retains its named Count draft while draining and settles the original cancellation without borrowing Supply',async()=>{
 const s=await manufacturingInitialPhysicalFixture(),initial=await import('../../../src/server/modules/inventory/public/manufacturing-initial-physical-stock-count');
 const {withPackageSettlementSources}=await import('../../../src/server/modules/commercial/public/package-settlement-sources'),{lockManufacturingPackageObligationSources}=await import('../../../src/server/modules/manufacturing/public/package-obligations'),{lockManufacturingInventoryPackageObligationSources,manufacturingInventoryPackageObligations}=await import('../../../src/server/modules/inventory/public/manufacturing-package-obligations'),{lockSupplyInventoryPackageObligationSources,supplyInventoryPackageObligations}=await import('../../../src/server/modules/inventory/public/supply-package-obligations');
 const observe=async()=>{
  const manufacturing=await withPackageSettlementSources(db,s.z.company,[lock=>lockManufacturingPackageObligationSources(db,lock),lock=>lockManufacturingInventoryPackageObligationSources(db,lock)],transaction=>manufacturingInventoryPackageObligations(db,transaction,{companyId:s.z.company,packageId:'manufacturing',packageVersion:1,moduleId:'inventory',now:Date.now()}));
  const supply=await withPackageSettlementSources(db,s.z.company,[lock=>lockSupplyInventoryPackageObligationSources(db,lock)],transaction=>supplyInventoryPackageObligations(db,transaction,{companyId:s.z.company,packageId:'supply',packageVersion:1,moduleId:'inventory',now:Date.now()}));
  // Collect both REAL leased owner observations before asserting ownership.
  return {manufacturing,supply};
 };
 const before=await observe();expect({manufacturing:before.manufacturing.outstanding,supply:before.supply.outstanding}).toEqual({manufacturing:0,supply:0});expect(await s.absent()).toEqual([0,0,0,0,0]);
 const selection={warehouseId:String(s.warehouse.id),productId:String(s.product.id),countDate:'2026-09-06'},workspace=await initial.manufacturingInitialPhysicalStockCountWorkspace(s.preparer,selection);if(typeof workspace.contextKey!=='string')throw Error('Actual Manufacturing initial Count context required');
 const createKey=key(),body={expectedVersion:0 as const,contextKey:workspace.contextKey,input:s.inventory.countSchema.parse({warehouseId:selection.warehouseId,countDate:selection.countDate,reason:'Actual current-period Count lifecycle verifies two physical units at six THB; not acquisition or opening equity',lines:[{productId:selection.productId,countedQuantity:'2',unitCost:'6'}]})},created=await initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,body,createKey),id=String(created.id);
 expect(created).toMatchObject({id:`count-${createKey}`,status:'draft',version:1});
 const draft=await observe();expect({manufacturing:draft.manufacturing.outstanding,supply:draft.supply.outstanding},'Actual Manufacturing draft is retained by Manufacturing Inventory, never Supply').toEqual({manufacturing:1,supply:0});expect(draft.manufacturing.revision).not.toBe(before.manufacturing.revision);expect(await s.absent()).toEqual([0,0,0,0,0]);
 // Reuse the existing normal fixture grant owner operation. This is an isolated
 // active-to-draining grant transition, not installed Apply or a withdrawal.
 await purchasedManufacturing('draining',null,'manufacturing',s.z.company);
 expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='manufacturing'").bind(s.z.company).first()).toEqual({state:'draining',version:2});
 const commandImage=async(command:string)=>Promise.all(['command_receipts','audit_events','outbox_events','command_assertions'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE command_id=? ORDER BY image`).bind(command).all()).results));
 const image=async(command:string)=>({counts:(await db.prepare('SELECT to_jsonb(c)::text image FROM inventory_counts c WHERE company_id=? ORDER BY image').bind(s.z.company).all()).results,sources:(await db.prepare('SELECT to_jsonb(source)::text image FROM inventory_mfg_initial_physical_sources source WHERE company_id=? ORDER BY image').bind(s.z.company).all()).results,journals:(await db.prepare('SELECT to_jsonb(journal)::text image FROM finance_journals journal WHERE company_id=? ORDER BY image').bind(s.z.company).all()).results,stock:await s.absent(),command:await commandImage(command)});
 const deniedKey=key(),deniedBefore=await image(deniedKey);await expect(initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,body,deniedKey)).rejects.toMatchObject({status:403,code:'PACKAGE_DRAINING'});expect(await image(deniedKey)).toEqual(deniedBefore);
 const createBefore=await image(createKey);expect(await initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,body,createKey)).toEqual(created);expect(await image(createKey)).toEqual(createBefore);
 expect(await initial.manufacturingInitialPhysicalStockCountWorkspace(s.preparer,{countId:id})).toMatchObject({available:true,contextKey:null,record:{sourceKind:'manufacturing-initial-physical-stock-count',id,status:'draft',version:1}});
 const draining=await observe();expect({manufacturing:draining.manufacturing.outstanding,supply:draining.supply.outstanding}).toEqual({manufacturing:1,supply:0});expect(draining.manufacturing.revision).toBe(draft.manufacturing.revision);
 const cancelKey=key(),cancelBody={expectedVersion:1,attachmentSetIds:[],reason:'Original preparer cancels the retained unposted physical Count during draining'},cancelled=await initial.transitionManufacturingInitialPhysicalStockCount(s.preparer,id,'cancel',cancelBody,cancelKey);expect(cancelled).toMatchObject({id,status:'cancelled',version:2});
 expect(await db.prepare("SELECT operation,record_id,result_json::jsonb->>'status' status,result_json::jsonb->>'version' version FROM command_receipts WHERE command_id=?").bind(cancelKey).first()).toEqual({operation:'inventory.count.cancel',record_id:id,status:'cancelled',version:'2'});
 expect(await db.prepare('SELECT action,actor_id,reviewed_version,command_id FROM inventory_count_decisions WHERE count_id=?').bind(id).first()).toEqual({action:'cancel',actor_id:s.preparer.userId,reviewed_version:1,command_id:cancelKey});
 const settled=await observe();expect({manufacturing:settled.manufacturing.outstanding,supply:settled.supply.outstanding}).toEqual({manufacturing:0,supply:0});expect(settled.manufacturing.revision).not.toBe(draft.manufacturing.revision);expect(await s.absent()).toEqual([0,0,0,0,0]);
 const stable={create:await image(createKey),cancel:await image(cancelKey)};expect(await initial.prepareManufacturingInitialPhysicalStockCount(s.preparer,body,createKey)).toEqual(created);expect(await initial.transitionManufacturingInitialPhysicalStockCount(s.preparer,id,'cancel',cancelBody,cancelKey)).toEqual(cancelled);expect({create:await image(createKey),cancel:await image(cancelKey)}).toEqual(stable);
 const replay=await observe();expect({manufacturing:replay.manufacturing.outstanding,supply:replay.supply.outstanding,manufacturingRevision:replay.manufacturing.revision,supplyRevision:replay.supply.revision}).toEqual({manufacturing:0,supply:0,manufacturingRevision:settled.manufacturing.revision,supplyRevision:settled.supply.revision});
 expect(await initial.manufacturingInitialPhysicalStockCountWorkspace(s.preparer,{countId:id})).toMatchObject({available:true,contextKey:null,record:{id,status:'cancelled',version:2}});expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(s.z.company).all()).results).toEqual([{package_id:'manufacturing'}]);
});
// Organization/workers are diagnostic fixture provenance. Every ledger,
// master, grant transition and mapping below is a real original owner command.
async function legacyConfiguredManufacturingCompany(){
 const z=await zeroLedgerManufacturingCompany(),finance=await import('../../../src/server/modules/finance/public/finance'),money=await import('../../../src/server/modules/finance/public/company-money'),supply=await import('../../../src/server/modules/commerce/public/posting-setup'),commerce=await import('../../../src/server/modules/commerce/public/commerce');
 const {packageGrantStore,readCompanyPackageGrants,packageGrantRevision}=await import('../../../src/server/modules/commercial/public/package-grants'),{withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations');
 const grant={companyId:z.company,packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Actual original Supply mapping before this Company adopted Manufacturing',evidence:'test:legacy-existing-Manufacturing-map'};
 const provider=packageGrantStore(db,{authorize:async()=>({userId:'isolated-legacy-existing-mapping-provider'}),obligations:(transaction,companyId,packageId)=>ownerPackageObligations(db,transaction,companyId,packageId)});
 await provider.apply({expectedVersion:0,idempotencyKey:key(),grant});
 const chart=await finance.createChart(z.reviewer,{name:'Original retained Company ledger',baseCurrency:'THB',accountingBasis:'IFRS'},key()),accounts:Record<string,string>={};
 const roles=[['receivableAccountId','asset','receivable'],['payableAccountId','liability','payable'],['revenueAccountId','revenue','none'],['inventoryAccountId','asset','none'],['clearingAccountId','liability','none'],['cogsAccountId','expense','none'],['inputTaxAccountId','asset','none'],['outputTaxAccountId','liability','none'],['cashAccountId','asset','bank'],['wipAccountId','asset','none'],['laborAccountId','liability','none'],['overheadAccountId','liability','none'],['scrapAccountId','expense','none']] as const;
 for(const [index,[field,category,controlKind]] of roles.entries())accounts[field]=String((await finance.createAccount(z.reviewer,{chartId:chart.id,code:`LEGACY-MFG-${index+1}`,name:`Original retained ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind},key())).id);
 const variance=await finance.createAccount(z.reviewer,{chartId:chart.id,code:'5400',name:'Original physical-count current-period variance',category:'expense',currency:'THB',isControl:false,controlKind:'none'},key()),period=await finance.createPeriod(z.reviewer,{name:'Original retained Company year',startDate:'2026-01-01',endDate:'2026-12-31'},key());
 await money.confirmManualRate(z.manager,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Original rightful Finance approver confirms the actual ledger quote'},key());
 const view=await supply.commercePostingSetup(z.reviewer);if(typeof view.contextKey!=='string')throw Error('Actual original Supply mapping context required');
 const nine=Object.fromEntries(Object.entries(accounts).filter(([field])=>!['wipAccountId','laborAccountId','overheadAccountId','scrapAccountId'].includes(field))),mappingKey=key(),mappingBody={expectedVersion:0,contextKey:view.contextKey,chartId:chart.id,currency:'THB',reason:'Original normal shared Company mapping remains immutable',...nine};
 expect(await supply.initializeCommercePostingSetup(z.reviewer,mappingBody,mappingKey)).toEqual({id:z.company,status:'configured',version:1});
 const warehouse=await commerce.saveMaster(z.owner,'warehouses',null,{branchId:z.branch,code:key(),name:'Original retained warehouse'},key()),product=await commerce.saveMaster(z.owner,'products',null,{sku:key(),name:'Original actually counted material',itemType:'stock',price:'6',taxBps:700},key());
 // Actual zero-obligation owner-registry withdrawal, not grant SQL or a fake
 // obligation reader. History remains; only Manufacturing will be active.
 const settlement={companyId:z.company,packageId:'supply',packageVersion:1};
 for(const [state,expectedVersion] of [['draining',1],['inactive',2]] as const){
  const grants=await readCompanyPackageGrants(db,z.company);expect(grants.find(row=>row.packageId==='supply')).toMatchObject({state:expectedVersion===1?'active':'draining',version:expectedVersion});
  const request={companyId:z.company,expectedRevision:packageGrantRevision(z.company,grants),idempotencyKey:key(),changes:[{expectedVersion,grant:{...grant,state,version:expectedVersion+1,reason:state==='draining'?'Drain original Supply before reviewing final owner settlement':'Original Supply owners have no pending documents or Stock',evidence:'test:real-zero-Supply-obligation-review'}}]};
  const preview=await withOwnerPackageObligations(db,settlement,async transaction=>{expect((await readOwnerPackageObligations(db,transaction,{...settlement,now:Date.now()})).total).toBe(0);return provider.batch.previewIn(transaction,request);});
  const applied=await withOwnerPackageObligations(db,settlement,async transaction=>{expect((await readOwnerPackageObligations(db,transaction,{...settlement,now:Date.now()})).total).toBe(0);return provider.batch.applyIn(transaction,request,async currentPreview=>{expect(currentPreview).toEqual(preview);});});expect(applied).toEqual(preview);
  expect((await readCompanyPackageGrants(db,z.company)).find(row=>row.packageId==='supply')).toMatchObject({state,version:expectedVersion+1});
  const receipt=await db.prepare('SELECT company_id,request_hash,result_json FROM company_package_change_sets WHERE id=?').bind(request.idempotencyKey).first<{company_id:string;request_hash:string;result_json:unknown}>();expect(receipt?.company_id).toBe(z.company);expect(receipt?.request_hash).toMatch(/^[a-f0-9]{64}$/);expect(typeof receipt?.result_json==='string'?JSON.parse(receipt.result_json):receipt?.result_json).toEqual(applied);
 }
 await z.purchase();
 expect((await readCompanyPackageGrants(db,z.company)).map(row=>({packageId:row.packageId,state:row.state})).sort((a,b)=>a.packageId.localeCompare(b.packageId))).toEqual([{packageId:'manufacturing',state:'active'},{packageId:'supply',state:'inactive'}]);
 const profiles=await import('../../../src/server/modules/manufacturing/public/posting-profiles'),profileView=await profiles.manufacturingPostingProfiles(z.owner);if(typeof profileView.contextKey!=='string')throw Error('Actual mapped original profile context required');
 const profile=await profiles.prepareManufacturingPostingProfile(z.owner,{contextKey:profileView.contextKey,input:{chartId:chart.id,inventoryAccountId:accounts.inventoryAccountId,wipAccountId:accounts.wipAccountId,laborAccountId:accounts.laborAccountId,overheadAccountId:accounts.overheadAccountId,scrapAccountId:accounts.scrapAccountId}},key()),review=await profiles.manufacturingPostingProfiles(z.manager);if(typeof review.contextKey!=='string')throw Error('Actual original independent profile review required');
 expect(await profiles.decideManufacturingPostingProfile(z.manager,String(profile.id),{contextKey:review.contextKey,input:{expectedVersion:1,action:'approve',reason:'Different current Finance actor approves the retained real profile'}},key())).toMatchObject({id:profile.id,status:'approved',version:2});
 expect(await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE record_id=? AND operation='commerce.manufacturing-posting-setup.initialize'").bind(z.company).first('n')).toBe(0);
 const selected={warehouseId:String(warehouse.id),productId:String(product.id),countDate:'2026-09-06'},originalMapping=await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).first<string>('image');
 const image=async()=>({mapping:await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).first<string>('image'),profiles:(await db.prepare('SELECT to_jsonb(p)::text image FROM manufacturing_accounting_profiles p WHERE company_id=? ORDER BY id').bind(z.company).all()).results,chart:await db.prepare('SELECT to_jsonb(c)::text image FROM finance_charts c WHERE id=?').bind(chart.id).first(),stock:await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first(),counts:(await db.prepare('SELECT to_jsonb(c)::text image FROM inventory_counts c WHERE company_id=? ORDER BY id').bind(z.company).all()).results,journals:(await db.prepare('SELECT to_jsonb(j)::text image FROM finance_journals j WHERE company_id=? ORDER BY id').bind(z.company).all()).results});
 return {z,chart,accounts,variance,period,warehouse,product,selected,originalMapping,mappingKey,mappingBody,supply,image};
}

it('retains the explicit Manufacturing confirmation requirement for an actual non-Manufacturing existing mapping',async()=>{
 const s=await legacyConfiguredManufacturingCompany(),setup=await import('../../../src/server/modules/manufacturing/public/posting-setup'),initial=await import('../../../src/server/modules/inventory/public/manufacturing-initial-physical-stock-count');
 const view=await setup.manufacturingPostingSetup(s.z.reviewer);expect(view).toMatchObject({available:true,configured:true,canConfigure:false,unsupportedExistingMapping:false});
 const command=key(),before=await s.image();await expect(setup.initializeManufacturingPostingSetup(s.z.reviewer,{...s.mappingBody,contextKey:view.contextKey,expectedSalesDefaults:view.expectedSalesDefaults},command)).rejects.toMatchObject({status:409,code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});expect(await s.image()).toEqual(before);expect(await db.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(0);
 // A legitimate existing mapping still needs its own explicit Manufacturing confirmation.
 // Keep both the original configured guard and the unconfirmed source refusal.
 expect(await initial.manufacturingInitialPhysicalStockCountWorkspace(s.z.owner,s.selected)).toMatchObject({available:true,canPrepare:false,unavailableReason:'manufacturing-posting-required',readiness:{schemaAvailable:true,postingConfigured:false}});
});

it('confirms a real legacy mapping unchanged before normal Manufacturing-only initial Count and independent variance posting',async()=>{
 const s=await legacyConfiguredManufacturingCompany(),setup=await import('../../../src/server/modules/manufacturing/public/posting-setup'),existing=await import('../../../src/server/modules/commerce/public/manufacturing-existing-posting-setup'),initial=await import('../../../src/server/modules/inventory/public/manufacturing-initial-physical-stock-count'),inventory=await import('../../../src/server/modules/inventory/public/inventory-counts');
 const oldView=await setup.manufacturingPostingSetup(s.z.reviewer);expect(oldView).toMatchObject({configured:true,canConfigure:false,unsupportedExistingMapping:false});
 const originalGuardKey=key(),beforeGuard=await s.image();await expect(setup.initializeManufacturingPostingSetup(s.z.reviewer,{...s.mappingBody,contextKey:oldView.contextKey,expectedSalesDefaults:oldView.expectedSalesDefaults},originalGuardKey)).rejects.toMatchObject({status:409,code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});expect(await s.image()).toEqual(beforeGuard);expect(await db.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(originalGuardKey).first('n')).toBe(0);
 expect(await initial.manufacturingInitialPhysicalStockCountWorkspace(s.z.owner,s.selected)).toMatchObject({available:true,canPrepare:false,readiness:{postingConfigured:false}});
 const preview=await existing.manufacturingExistingPostingSetup(s.z.reviewer);expect(preview).toMatchObject({available:true,schemaAvailable:true,configured:true,confirmed:false,version:0,canConfirm:true,expectedSalesDefaults:null,unavailableReason:null});if(typeof preview.contextKey!=='string')throw Error('Actual current existing-mapping preview required');
 const confirmKey=key(),body={expectedVersion:0 as const,contextKey:preview.contextKey,reason:'Current Company Finance confirms the original unchanged nine-role mapping for Manufacturing'},commandImage=async()=>Promise.all(['command_receipts','audit_events','outbox_events','command_assertions'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE command_id=? ORDER BY image`).bind(confirmKey).all()).results)),confirmationImage=async()=>({source:await s.image(),command:await commandImage(),confirmation:(await db.prepare('SELECT to_jsonb(c)::text image FROM commerce_mfg_existing_posting_confirmations c WHERE company_id=?').bind(s.z.company).all()).results});
 // Real deferred own-unit fault. It never supplies a positive financial fact.
 const fault=`existing_mfg_map_${key().replaceAll('-','')}`,beforeFault=await confirmationImage();await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_charts SET name='Changed after the original confirmation receipt' WHERE id='${s.chart.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${confirmKey}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey)).rejects.toMatchObject({status:409,code:'MANUFACTURING_POSTING_SOURCE_CHANGED'});expect(await confirmationImage()).toEqual(beforeFault);}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}
 const confirmed=await existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey);expect(confirmed).toEqual({id:s.z.company,status:'confirmed-existing',version:1});expect(confirmed).not.toHaveProperty('_manufacturingExistingPostingSetup');expect(await s.image()).toEqual(beforeGuard);
 const stableConfirmation=await confirmationImage();expect(await existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey)).toEqual(confirmed);expect(await confirmationImage()).toEqual(stableConfirmation);
 await expect(existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,{...body,reason:'A distinct body cannot reuse original confirmation'},confirmKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});await expect(existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,key())).rejects.toMatchObject({code:'MANUFACTURING_EXISTING_POSTING_ALREADY_CONFIRMED'});
 expect(await existing.manufacturingExistingPostingSetup(s.z.reviewer)).toMatchObject({confirmed:true,version:1,canConfirm:false});
 expect(await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE record_id=? AND operation='commerce.manufacturing-posting-setup.initialize'").bind(s.z.company).first('n')).toBe(0);
 const generic=inventory.countSchema.parse({warehouseId:s.selected.warehouseId,countDate:s.selected.countDate,reason:'Verified actual two physical units at six THB; current-period variance, never acquisition or opening equity',lines:[{productId:s.selected.productId,countedQuantity:'2',unitCost:'6'}]});await expect(inventory.saveCount(s.z.owner,null,generic,key())).rejects.toMatchObject({status:403});
 const workspace=await initial.manufacturingInitialPhysicalStockCountWorkspace(s.z.owner,s.selected);expect(workspace).toMatchObject({available:true,canPrepare:true,readiness:{schemaAvailable:true,postingConfigured:true,varianceAccountAvailable:true,postingPeriodAvailable:true}});if(typeof workspace.contextKey!=='string')throw Error('Actual confirmed legacy mapping Count context required');
 const createKey=key(),createBody={expectedVersion:0 as const,contextKey:workspace.contextKey,input:generic},created=await initial.prepareManufacturingInitialPhysicalStockCount(s.z.owner,createBody,createKey),id=String(created.id);expect(created).toMatchObject({id:`count-${createKey}`,status:'draft',version:1});
 const privateSource=await db.prepare('SELECT proof_json FROM inventory_mfg_initial_physical_sources WHERE count_id=?').bind(id).first<{proof_json:string}>();if(!privateSource?.proof_json)throw Error('Actual immutable Manufacturing Count source proof required');expect(JSON.parse(privateSource.proof_json) as Record<string,unknown>).toMatchObject({manufacturingSetupCommandId:confirmKey,manufacturingSetupRequestHash:expect.any(String),chartId:s.chart.id,inventoryAccountId:s.accounts.inventoryAccountId});
 const sheet=await seedAttachmentSet(db,s.z.owner,{targetType:'stock_count',targetId:id,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1});await initial.transitionManufacturingInitialPhysicalStockCount(s.z.owner,id,'submit',{expectedVersion:1,attachmentSetIds:[sheet.setId]},key());
 for(const [step,recordVersion] of [['submitted',2],['approved',3]] as const){const flow=await initial.manufacturingInitialPhysicalStockCountFlow(s.z.owner,id);await initial.assignManufacturingInitialPhysicalStockCountFlow(s.z.owner,id,{stepKey:step,userId:s.z.manager.userId,expectedVersion:flow.version,recordVersion,reason:'Original Company authority selects a different actual Stock reviewer',saveDefault:false},key());if(step==='submitted')await initial.transitionManufacturingInitialPhysicalStockCount(s.z.manager,id,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Independent actual Stock review of retained Company mapping and physical evidence'},key());}
 const postKey=key(),postBody={expectedVersion:3,attachmentSetIds:[],reason:'Post original current-period variance to captured inventory and5400 without rebinding the legacy ledger'},posted=await initial.transitionManufacturingInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey);expect(posted).toMatchObject({id,status:'posted',version:4});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(s.warehouse.id,s.product.id).first()).toEqual({quantity_milli:2000,value_minor:1200});const detail=await inventory.countDetail(s.z.manager,id),journalId=String(detail.count.journal_id);expect(await db.prepare('SELECT company_id,chart_id,period_id,status,source_type,source_id,is_opening FROM finance_journals WHERE id=?').bind(journalId).first()).toEqual({company_id:s.z.company,chart_id:s.chart.id,period_id:s.period.id,status:'posted',source_type:'stock_count',source_id:id,is_opening:0});
 expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(journalId).all()).results).toEqual([{account_id:s.accounts.inventoryAccountId,debit_minor:1200,credit_minor:0},{account_id:s.variance.id,debit_minor:0,credit_minor:1200}].sort((a,b)=>a.account_id.localeCompare(b.account_id)));
 expect(await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(s.z.company).first('image')).toBe(s.originalMapping);expect((await s.image()).profiles).toEqual(beforeGuard.profiles);
 const afterPost=await confirmationImage();expect(await existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey)).toEqual(confirmed);expect(await initial.prepareManufacturingInitialPhysicalStockCount(s.z.owner,createBody,createKey)).toEqual(created);expect(await initial.transitionManufacturingInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey)).toEqual(posted);expect(await confirmationImage()).toEqual(afterPost);
 expect(await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE record_id=? AND operation='commerce.manufacturing-posting-setup.initialize'").bind(s.z.company).first('n')).toBe(0);
 // Adverse authority fixture only, never a positive assignment expansion.
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),s.z.reviewer.assignmentId).run();await expect(existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey)).rejects.toMatchObject({status:403,code:'QUEUED_AUTHORITY_CHANGED'});expect(await confirmationImage()).toEqual(afterPost);
});

// Organization/workers are diagnostic fixture provenance. Every ledger,
// master, grant transition and mapping below is a real original owner command.
async function populatedLegacyConfiguredManufacturingCompany(){
 const z=await zeroLedgerManufacturingCompany(),finance=await import('../../../src/server/modules/finance/public/finance'),money=await import('../../../src/server/modules/finance/public/company-money'),supply=await import('../../../src/server/modules/commerce/public/posting-setup'),commerce=await import('../../../src/server/modules/commerce/public/commerce');
 const {packageGrantStore,readCompanyPackageGrants,packageGrantRevision}=await import('../../../src/server/modules/commercial/public/package-grants'),{withOwnerPackageObligations,ownerPackageObligations,readOwnerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations');
 const grant={companyId:z.company,packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Actual original Supply mapping before this Company adopted Manufacturing',evidence:'test:legacy-existing-Manufacturing-map'};
 const provider=packageGrantStore(db,{authorize:async()=>({userId:'isolated-legacy-existing-mapping-provider'}),obligations:(transaction,companyId,packageId)=>ownerPackageObligations(db,transaction,companyId,packageId)});
 await provider.apply({expectedVersion:0,idempotencyKey:key(),grant});
 const chart=await finance.createChart(z.reviewer,{name:'Original retained Company ledger',baseCurrency:'THB',accountingBasis:'IFRS'},key()),accounts:Record<string,string>={};
 const roles=[['receivableAccountId','asset','receivable'],['payableAccountId','liability','payable'],['revenueAccountId','revenue','none'],['inventoryAccountId','asset','none'],['clearingAccountId','liability','none'],['cogsAccountId','expense','none'],['inputTaxAccountId','asset','none'],['outputTaxAccountId','liability','none'],['cashAccountId','asset','bank'],['wipAccountId','asset','none'],['laborAccountId','liability','none'],['overheadAccountId','liability','none'],['scrapAccountId','expense','none']] as const;
 for(const [index,[field,category,controlKind]] of roles.entries())accounts[field]=String((await finance.createAccount(z.reviewer,{chartId:chart.id,code:`LEGACY-MFG-${index+1}`,name:`Original retained ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind},key())).id);
 const variance=await finance.createAccount(z.reviewer,{chartId:chart.id,code:'5400',name:'Original physical-count current-period variance',category:'expense',currency:'THB',isControl:false,controlKind:'none'},key()),period=await finance.createPeriod(z.reviewer,{name:'Original retained Company year',startDate:'2026-01-01',endDate:'2026-12-31'},key());
 await money.confirmManualRate(z.manager,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Original rightful Finance approver confirms the actual ledger quote'},key());
 const view=await supply.commercePostingSetup(z.reviewer);if(typeof view.contextKey!=='string')throw Error('Actual original Supply mapping context required');
 const nine=Object.fromEntries(Object.entries(accounts).filter(([field])=>!['wipAccountId','laborAccountId','overheadAccountId','scrapAccountId'].includes(field))),mappingKey=key(),mappingBody={expectedVersion:0,contextKey:view.contextKey,chartId:chart.id,currency:'THB',reason:'Original normal shared Company mapping remains immutable',...nine};
 expect(await supply.initializeCommercePostingSetup(z.reviewer,mappingBody,mappingKey)).toEqual({id:z.company,status:'configured',version:1});
 const warehouse=await commerce.saveMaster(z.owner,'warehouses',null,{branchId:z.branch,code:key(),name:'Original retained warehouse'},key()),product=await commerce.saveMaster(z.owner,'products',null,{sku:key(),name:'Original actually counted material',itemType:'stock',price:'6',taxBps:700},key());
 await z.purchase();
 const profiles=await import('../../../src/server/modules/manufacturing/public/posting-profiles'),profileView=await profiles.manufacturingPostingProfiles(z.owner);if(typeof profileView.contextKey!=='string')throw Error('Actual mapped original profile context required');
 const profile=await profiles.prepareManufacturingPostingProfile(z.owner,{contextKey:profileView.contextKey,input:{chartId:chart.id,inventoryAccountId:accounts.inventoryAccountId,wipAccountId:accounts.wipAccountId,laborAccountId:accounts.laborAccountId,overheadAccountId:accounts.overheadAccountId,scrapAccountId:accounts.scrapAccountId}},key()),review=await profiles.manufacturingPostingProfiles(z.manager);if(typeof review.contextKey!=='string')throw Error('Actual original independent profile review required');
 expect(await profiles.decideManufacturingPostingProfile(z.manager,String(profile.id),{contextKey:review.contextKey,input:{expectedVersion:1,action:'approve',reason:'Different current Finance actor approves the retained real profile'}},key())).toMatchObject({id:profile.id,status:'approved',version:2});
 expect(await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE record_id=? AND operation='commerce.manufacturing-posting-setup.initialize'").bind(z.company).first('n')).toBe(0);
 // Original Supply Count owns this genuine positive physical variance. It is
 // neither a new Manufacturing private source nor acquisition/opening equity.
 const inventory=await import('../../../src/server/modules/inventory/public/inventory-counts'),{resolveFlow,assignFlowPrimary}=await import('../../../src/server/modules/workflow/public/flow-routing');
 const supplyCountKey=key(),countInput=inventory.countSchema.parse({warehouseId:String(warehouse.id),countDate:'2026-09-06',reason:'Original independently verified two physical units at six THB as current-period variance',lines:[{productId:String(product.id),countedQuantity:'2',unitCost:'6'}]}),supplyCount=await inventory.saveCount(z.owner,null,countInput,supplyCountKey),supplyCountId=String(supplyCount.id);
 const sheet=await seedAttachmentSet(db,z.owner,{targetType:'stock_count',targetId:supplyCountId,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1});await inventory.transitionCount(z.owner,supplyCountId,'submit',{expectedVersion:1,attachmentSetIds:[sheet.setId]},key());
 for(const [step,recordVersion] of [['submitted',2],['approved',3]] as const){const flow=await resolveFlow(db,z.owner,'stock_count',supplyCountId);await assignFlowPrimary(db,z.owner,'stock_count',supplyCountId,{stepKey:step,userId:z.manager.userId,expectedVersion:flow.version,recordVersion,reason:'Original Company authority selects the different actual Stock approver',saveDefault:false},key());if(step==='submitted')await inventory.transitionCount(z.manager,supplyCountId,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Independent original review of the real physical quantity and unit cost'},key());}
 await inventory.transitionCount(z.manager,supplyCountId,'post',{expectedVersion:3,attachmentSetIds:[],reason:'Original Supply posting records current-period inventory variance through real expense5400'},key());
 expect(await db.prepare('SELECT status FROM inventory_counts WHERE id=?').bind(supplyCountId).first('status')).toBe('posted');
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first()).toEqual({quantity_milli:2000,value_minor:1200});
 expect(await db.prepare('SELECT count(*)::int n FROM inventory_mfg_initial_physical_sources WHERE count_id=?').bind(supplyCountId).first('n')).toBe(0);
 const originalCountReceipt=await db.prepare('SELECT result_json FROM command_receipts WHERE command_id=?').bind(supplyCountKey).first<string>('result_json');expect(JSON.parse(originalCountReceipt??'{}')).not.toHaveProperty('_manufacturingInitialPhysicalStockCount');
 const outputProduct=await commerce.saveMaster(z.owner,'products',null,{sku:key(),name:'Real released original Company output before Manufacturing mapping confirmation',itemType:'stock',price:'12',taxBps:700},key());
 const spec=await createSpecification(z.manager,{name:'Original pre-confirmation actual production BOM',mode:'bom',outputProductId:String(outputProduct.id),batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:String(product.id),quantity:'2'}]},key());
 await reviewSpecification(z.owner,String(spec.id),{action:'approve',expectedVersion:1,reason:'Different Company actor reviews the real original BOM'},key());
 const plan=await createPlanningScenario(z.manager,{warehouseId:String(warehouse.id),specificationId:String(spec.id),specificationVersion:2,demandQuantity:'1',dueDate:'2026-09-06',capacityMinutes:100},key()),released=await reviewPlanningRecommendation(z.owner,String(plan.recommendationId),{action:'approve',expectedVersion:1,reason:'Independent original release before Manufacturing source confirmation'},key()),productionId=String(released.orderId);
 await changeManufacturingOrder(z.manager,productionId,{action:'reserve',expectedVersion:1,expiresAt:Date.now()+3600000,reason:'Original reservation protects the physically counted source material'},key());
 await changeManufacturingOrder(z.manager,productionId,{action:'issue',expectedVersion:2,entryDate:'2026-09-06',reason:'Original actual material issue transfers the retained value into WIP'},key());
 expect(await db.prepare("SELECT COALESCE(sum(l.debit_minor-l.credit_minor),0)::text value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted' AND l.account_id=?").bind(z.company,accounts.wipAccountId).first('value')).toBe('1200');
 await changeManufacturingOrder(z.manager,productionId,{action:'report',expectedVersion:3,outputQuantity:'1',scrapQuantity:'0',laborCost:'0',overheadCost:'0',entryDate:'2026-09-06',reason:'Original native worker reports the real material-only output'},key());
 await changeManufacturingOrder(z.owner,productionId,{action:'pass-qc',expectedVersion:4,entryDate:'2026-09-06',reason:'Different original Company reviewer releases the genuine finished stock'},key());
 expect((await productionDetail(z.owner,productionId)).order).toMatchObject({id:productionId,status:'completed'});
 expect(await db.prepare("SELECT COALESCE(sum(l.debit_minor-l.credit_minor),0)::text value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted' AND l.account_id=?").bind(z.company,accounts.wipAccountId).first('value')).toBe('0');
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first()).toEqual({quantity_milli:0,value_minor:0});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,outputProduct.id).first()).toEqual({quantity_milli:1000,value_minor:1200});
 expect(await db.prepare('SELECT count(*)::int n FROM manufacturing_order_postings WHERE company_id=? AND order_id=?').bind(z.company,productionId).first<number>('n')).toBeGreaterThanOrEqual(2);
 const originalJournalIds=(await db.prepare('SELECT id FROM finance_journals WHERE company_id=? ORDER BY id').bind(z.company).all<{id:string}>()).results.map(row=>row.id);
 expect(originalJournalIds.length).toBeGreaterThanOrEqual(3);
 const journalIds=JSON.stringify(originalJournalIds),priorMoneyIds=(await db.prepare("SELECT source_type,source_id,amount_kind FROM money_transaction_snapshots WHERE company_id=? ORDER BY source_type,source_id,amount_kind").bind(z.company).all<{source_type:string;source_id:string;amount_kind:string}>()).results;
 expect(priorMoneyIds.length).toBeGreaterThan(0);
 const retained=async()=>({
  mapping:await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).first('image'),
  oldCount:(await db.prepare('SELECT to_jsonb(c)::text image FROM inventory_counts c WHERE id=?').bind(supplyCountId).all()).results,
  oldCountLines:(await db.prepare('SELECT to_jsonb(l)::text image FROM inventory_count_lines l WHERE count_id=? ORDER BY l.id').bind(supplyCountId).all()).results,
  oldCountDecisions:(await db.prepare('SELECT to_jsonb(d)::text image FROM inventory_count_decisions d WHERE count_id=? ORDER BY d.id').bind(supplyCountId).all()).results,
  production:await Promise.all(['manufacturing_orders','manufacturing_order_postings','manufacturing_cost_movements','manufacturing_accounting_profiles'].map(async table=>(await db.prepare(`SELECT to_jsonb(row)::text image FROM ${table} row WHERE company_id=? ORDER BY image`).bind(z.company).all()).results)),
  productionHistory:await Promise.all(['manufacturing_cost_snapshots','manufacturing_genealogy','manufacturing_material_reservations','manufacturing_inventory_moves'].map(async table=>(await db.prepare(`SELECT to_jsonb(row)::text image FROM ${table} row WHERE order_id=? ORDER BY image`).bind(productionId).all()).results)),
  productionEvents:(await db.prepare('SELECT to_jsonb(e)::text image FROM manufacturing_order_events e WHERE order_id=? ORDER BY e.id').bind(productionId).all()).results,
  journals:(await db.prepare('SELECT to_jsonb(j)::text image FROM finance_journals j WHERE id IN(SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY j.id').bind(journalIds).all()).results,
  lines:(await db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l WHERE journal_id IN(SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY l.id').bind(journalIds).all()).results,
  captures:(await db.prepare('SELECT to_jsonb(c)::text image FROM finance_financial_events c WHERE journal_id IN(SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY c.id').bind(journalIds).all()).results,
  books:(await db.prepare('SELECT to_jsonb(b)::text image FROM finance_book_postings b WHERE primary_journal_id IN(SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY b.primary_journal_id,b.secondary_journal_id').bind(journalIds).all()).results,
  money:await Promise.all(priorMoneyIds.map(async identity=>(await db.prepare('SELECT to_jsonb(m)::text image FROM money_transaction_snapshots m WHERE company_id=? AND source_type=? AND source_id=? AND amount_kind=?').bind(z.company,identity.source_type,identity.source_id,identity.amount_kind).all()).results)),
  originalStock:(await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_stock s WHERE warehouse_id=? AND product_id IN(?,?) ORDER BY s.product_id').bind(warehouse.id,product.id,outputProduct.id).all()).results,
 });
 const originalHistory=await retained();
 // Actual zero-obligation owner-registry withdrawal, not grant SQL or a fake
 // obligation reader. History remains; only Manufacturing will be active.
 const settlement={companyId:z.company,packageId:'supply',packageVersion:1};
 for(const [state,expectedVersion] of [['draining',1],['inactive',2]] as const){
  const grants=await readCompanyPackageGrants(db,z.company);expect(grants.find(row=>row.packageId==='supply')).toMatchObject({state:expectedVersion===1?'active':'draining',version:expectedVersion});
  const request={companyId:z.company,expectedRevision:packageGrantRevision(z.company,grants),idempotencyKey:key(),changes:[{expectedVersion,grant:{...grant,state,version:expectedVersion+1,reason:state==='draining'?'Drain original Supply before reviewing final owner settlement':'Original Supply work is terminal; retain genuine positive Stock as a revision fact',evidence:'test:real-zero-Supply-obligation-review'}}]};
  const preview=await withOwnerPackageObligations(db,settlement,async transaction=>{expect((await readOwnerPackageObligations(db,transaction,{...settlement,now:Date.now()})).total).toBe(0);return provider.batch.previewIn(transaction,request);});
  const applied=await withOwnerPackageObligations(db,settlement,async transaction=>{expect((await readOwnerPackageObligations(db,transaction,{...settlement,now:Date.now()})).total).toBe(0);return provider.batch.applyIn(transaction,request,async currentPreview=>{expect(currentPreview).toEqual(preview);});});expect(applied).toEqual(preview);
  expect((await readCompanyPackageGrants(db,z.company)).find(row=>row.packageId==='supply')).toMatchObject({state,version:expectedVersion+1});
  const receipt=await db.prepare('SELECT company_id,request_hash,result_json FROM company_package_change_sets WHERE id=?').bind(request.idempotencyKey).first<{company_id:string;request_hash:string;result_json:unknown}>();expect(receipt?.company_id).toBe(z.company);expect(receipt?.request_hash).toMatch(/^[a-f0-9]{64}$/);expect(typeof receipt?.result_json==='string'?JSON.parse(receipt.result_json):receipt?.result_json).toEqual(applied);
 }
 expect((await readCompanyPackageGrants(db,z.company)).map(row=>({packageId:row.packageId,state:row.state})).sort((a,b)=>a.packageId.localeCompare(b.packageId))).toEqual([{packageId:'manufacturing',state:'active'},{packageId:'supply',state:'inactive'}]);
 const selected={warehouseId:String(warehouse.id),productId:String(product.id),countDate:'2026-09-06'},originalMapping=await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).first<string>('image');
 const image=async()=>({mapping:await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).first<string>('image'),profiles:(await db.prepare('SELECT to_jsonb(p)::text image FROM manufacturing_accounting_profiles p WHERE company_id=? ORDER BY id').bind(z.company).all()).results,chart:await db.prepare('SELECT to_jsonb(c)::text image FROM finance_charts c WHERE id=?').bind(chart.id).first(),stock:await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first(),counts:(await db.prepare('SELECT to_jsonb(c)::text image FROM inventory_counts c WHERE company_id=? ORDER BY id').bind(z.company).all()).results,journals:(await db.prepare('SELECT to_jsonb(j)::text image FROM finance_journals j WHERE company_id=? ORDER BY id').bind(z.company).all()).results});
 expect(await retained()).toEqual(originalHistory);
 return {z,chart,accounts,variance,period,warehouse,product,selected,originalMapping,mappingKey,mappingBody,supply,image,retained,originalHistory,supplyCountId,productionId,outputProduct};
}

it('confirms a populated legacy Manufacturing ledger without changing prior Count, production, released Stock or financial captures',async()=>{
 const s=await populatedLegacyConfiguredManufacturingCompany(),setup=await import('../../../src/server/modules/manufacturing/public/posting-setup'),existing=await import('../../../src/server/modules/commerce/public/manufacturing-existing-posting-setup'),initial=await import('../../../src/server/modules/inventory/public/manufacturing-initial-physical-stock-count'),inventory=await import('../../../src/server/modules/inventory/public/inventory-counts');
 const oldView=await setup.manufacturingPostingSetup(s.z.reviewer);expect(oldView).toMatchObject({configured:true,canConfigure:false,unsupportedExistingMapping:false});
 const originalGuardKey=key(),beforeGuard=await s.image();await expect(setup.initializeManufacturingPostingSetup(s.z.reviewer,{...s.mappingBody,contextKey:oldView.contextKey,expectedSalesDefaults:oldView.expectedSalesDefaults},originalGuardKey)).rejects.toMatchObject({status:409,code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});expect(await s.image()).toEqual(beforeGuard);expect(await db.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(originalGuardKey).first('n')).toBe(0);
 expect(await initial.manufacturingInitialPhysicalStockCountWorkspace(s.z.owner,s.selected)).toMatchObject({available:true,canPrepare:false,readiness:{postingConfigured:false}});
 expect(await s.retained()).toEqual(s.originalHistory);
 const preview=await existing.manufacturingExistingPostingSetup(s.z.reviewer);expect(preview).toMatchObject({available:true,schemaAvailable:true,configured:true,confirmed:false,version:0,canConfirm:true,expectedSalesDefaults:null,unavailableReason:null});if(typeof preview.contextKey!=='string')throw Error('Actual current existing-mapping preview required');
 const confirmKey=key(),body={expectedVersion:0 as const,contextKey:preview.contextKey,reason:'Current Company Finance confirms the original unchanged nine-role mapping for Manufacturing'},commandImage=async()=>Promise.all(['command_receipts','audit_events','outbox_events','command_assertions'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE command_id=? ORDER BY image`).bind(confirmKey).all()).results)),confirmationImage=async()=>({source:await s.image(),command:await commandImage(),confirmation:(await db.prepare('SELECT to_jsonb(c)::text image FROM commerce_mfg_existing_posting_confirmations c WHERE company_id=?').bind(s.z.company).all()).results});
 // Real deferred own-unit fault. It never supplies a positive financial fact.
 const fault=`existing_mfg_map_${key().replaceAll('-','')}`,beforeFault=await confirmationImage();await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_charts SET name='Changed after the original confirmation receipt' WHERE id='${s.chart.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${confirmKey}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey)).rejects.toMatchObject({status:409,code:'MANUFACTURING_POSTING_SOURCE_CHANGED'});expect(await confirmationImage()).toEqual(beforeFault);}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}
 const confirmed=await existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey);expect(confirmed).toEqual({id:s.z.company,status:'confirmed-existing',version:1});expect(confirmed).not.toHaveProperty('_manufacturingExistingPostingSetup');expect(await s.image()).toEqual(beforeGuard);
 const stableConfirmation=await confirmationImage();expect(await existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey)).toEqual(confirmed);expect(await confirmationImage()).toEqual(stableConfirmation);
 await expect(existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,{...body,reason:'A distinct body cannot reuse original confirmation'},confirmKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});await expect(existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,key())).rejects.toMatchObject({code:'MANUFACTURING_EXISTING_POSTING_ALREADY_CONFIRMED'});
 expect(await existing.manufacturingExistingPostingSetup(s.z.reviewer)).toMatchObject({confirmed:true,version:1,canConfirm:false});
 expect(await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE record_id=? AND operation='commerce.manufacturing-posting-setup.initialize'").bind(s.z.company).first('n')).toBe(0);
 expect(await s.retained()).toEqual(s.originalHistory);
 expect(await initial.manufacturingInitialPhysicalStockCountWorkspace(s.z.owner,s.selected)).toMatchObject({canPrepare:false,readiness:{postingConfigured:true}});
 const masters=await import('../../../src/server/modules/commerce/public/manufacturing-initial-physical-stock-masters');
 s.product=await masters.createManufacturingInitialPhysicalStockProduct(s.z.owner,{sku:key(),name:'New independently counted material after unchanged legacy mapping confirmation',itemType:'stock',price:'6',taxBps:700},key());s.selected.productId=String(s.product.id);
 const generic=inventory.countSchema.parse({warehouseId:s.selected.warehouseId,countDate:s.selected.countDate,reason:'Verified actual two physical units at six THB; current-period variance, never acquisition or opening equity',lines:[{productId:s.selected.productId,countedQuantity:'2',unitCost:'6'}]});await expect(inventory.saveCount(s.z.owner,null,generic,key())).rejects.toMatchObject({status:403});
 const workspace=await initial.manufacturingInitialPhysicalStockCountWorkspace(s.z.owner,s.selected);expect(workspace).toMatchObject({available:true,canPrepare:true,readiness:{schemaAvailable:true,postingConfigured:true,varianceAccountAvailable:true,postingPeriodAvailable:true}});if(typeof workspace.contextKey!=='string')throw Error('Actual confirmed legacy mapping Count context required');
 const createKey=key(),createBody={expectedVersion:0 as const,contextKey:workspace.contextKey,input:generic},created=await initial.prepareManufacturingInitialPhysicalStockCount(s.z.owner,createBody,createKey),id=String(created.id);expect(created).toMatchObject({id:`count-${createKey}`,status:'draft',version:1});
 const privateSource=await db.prepare('SELECT proof_json FROM inventory_mfg_initial_physical_sources WHERE count_id=?').bind(id).first<{proof_json:string}>();if(!privateSource?.proof_json)throw Error('Actual immutable Manufacturing Count source proof required');expect(JSON.parse(privateSource.proof_json) as Record<string,unknown>).toMatchObject({manufacturingSetupCommandId:confirmKey,manufacturingSetupRequestHash:expect.any(String),chartId:s.chart.id,inventoryAccountId:s.accounts.inventoryAccountId});
 const sheet=await seedAttachmentSet(db,s.z.owner,{targetType:'stock_count',targetId:id,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1});await initial.transitionManufacturingInitialPhysicalStockCount(s.z.owner,id,'submit',{expectedVersion:1,attachmentSetIds:[sheet.setId]},key());
 for(const [step,recordVersion] of [['submitted',2],['approved',3]] as const){const flow=await initial.manufacturingInitialPhysicalStockCountFlow(s.z.owner,id);await initial.assignManufacturingInitialPhysicalStockCountFlow(s.z.owner,id,{stepKey:step,userId:s.z.manager.userId,expectedVersion:flow.version,recordVersion,reason:'Original Company authority selects a different actual Stock reviewer',saveDefault:false},key());if(step==='submitted')await initial.transitionManufacturingInitialPhysicalStockCount(s.z.manager,id,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Independent actual Stock review of retained Company mapping and physical evidence'},key());}
 const postKey=key(),postBody={expectedVersion:3,attachmentSetIds:[],reason:'Post original current-period variance to captured inventory and5400 without rebinding the legacy ledger'},posted=await initial.transitionManufacturingInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey);expect(posted).toMatchObject({id,status:'posted',version:4});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(s.warehouse.id,s.product.id).first()).toEqual({quantity_milli:2000,value_minor:1200});const detail=await inventory.countDetail(s.z.manager,id),journalId=String(detail.count.journal_id);expect(await db.prepare('SELECT company_id,chart_id,period_id,status,source_type,source_id,is_opening FROM finance_journals WHERE id=?').bind(journalId).first()).toEqual({company_id:s.z.company,chart_id:s.chart.id,period_id:s.period.id,status:'posted',source_type:'stock_count',source_id:id,is_opening:0});
 expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(journalId).all()).results).toEqual([{account_id:s.accounts.inventoryAccountId,debit_minor:1200,credit_minor:0},{account_id:s.variance.id,debit_minor:0,credit_minor:1200}].sort((a,b)=>a.account_id.localeCompare(b.account_id)));
 expect(await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(s.z.company).first('image')).toBe(s.originalMapping);expect((await s.image()).profiles).toEqual(beforeGuard.profiles);
 expect(await s.retained()).toEqual(s.originalHistory);
 const afterPost=await confirmationImage();expect(await existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey)).toEqual(confirmed);expect(await initial.prepareManufacturingInitialPhysicalStockCount(s.z.owner,createBody,createKey)).toEqual(created);expect(await initial.transitionManufacturingInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey)).toEqual(posted);expect(await confirmationImage()).toEqual(afterPost);
 expect(await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE record_id=? AND operation='commerce.manufacturing-posting-setup.initialize'").bind(s.z.company).first('n')).toBe(0);
 expect(await s.retained()).toEqual(s.originalHistory);
 // Adverse authority fixture only, never a positive assignment expansion.
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),s.z.reviewer.assignmentId).run();await expect(existing.confirmExistingManufacturingPostingSetup(s.z.reviewer,body,confirmKey)).rejects.toMatchObject({status:403,code:'QUEUED_AUTHORITY_CHANGED'});expect(await confirmationImage()).toEqual(afterPost);
});
