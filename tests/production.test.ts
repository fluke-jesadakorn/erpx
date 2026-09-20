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
it("runs BOM planning through reservation, actual issue, output, independent QC, cost and genealogy", async () => {
  const { id } = await order();
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
    },
    key(),
  );
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
  await changeManufacturingOrder(
    reviewer,
    id,
    {
      action: "pass-qc",
      expectedVersion: 4,
      reason: "Independent inspection passed",
    },
    key(),
  );
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
