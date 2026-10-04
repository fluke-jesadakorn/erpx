import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import type { WorkItem } from "@/lib/work-item";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { hasPermission } from "@/lib/permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { decimalToMinor } from "@/lib/finance-money";
import { scaledAmount } from "@/lib/commerce-math";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { prepareAttachmentBinding } from "./attachments/service";
import { prepareManufacturingPosting } from "./production-accounting";
import { calculateProductionCapacity, planningContextGuards, type PlanningContext } from "./production-resources";
import { prepareManufacturingMaterialLots, prepareManufacturingOutputLots } from "./production-lots";
const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive(),
  date = z.string().date(),
  qty = z.string().regex(/^\d+(\.\d{1,3})?$/);
const q = (x: string) => decimalToMinor(x, 3);
const specSchema = z.object({
  name: z.string().trim().min(2).max(160),
  mode: z.enum(["bom", "formula"]),
  outputProductId: z.string().min(1),
  batchQuantity: qty,
  expectedYieldBps: z.number().int().min(1).max(20000),
  lines: z
    .array(
      z.object({
        productId: z.string().min(1),
        quantity: qty,
        substitutionGroup: z.string().max(80).nullable().optional(),
      }),
    )
    .min(1)
    .max(100),
  operations: z.array(z.object({
    resourceId: z.string().min(1), name: z.string().trim().min(2).max(160),
    setupMinutes: z.number().int().nonnegative(), runMinutesPerBatch: z.number().int().positive(),
  })).max(50).default([]),
});
const planSchema = z.object({
  warehouseId: z.string().min(1),
  specificationId: z.string().min(1),
  specificationVersion: version,
  demandQuantity: qty.optional(),
  salesOrderId: z.string().min(1).optional(),
  salesOrderVersion: version.optional(),
  dueDate: date,
  startDate: date.optional(),
  capacityMinutes: z.number().int().nonnegative().default(0),
  manualMinutesPerUnit: z.number().int().positive().default(10),
  comparisonScenarioId: z.string().min(1).optional(),
}).refine(i => i.salesOrderId ? Boolean(i.salesOrderVersion) : Boolean(i.demandQuantity), "Choose a source order/version or enter manual demand.")
  .refine(i => !i.startDate || i.startDate <= i.dueDate, "The planning start date must be on or before the due date.");
const reviewSchema = z.object({
  action: z.enum(["approve", "reject", "retire"]),
  expectedVersion: version,
  reason,
});
const recommendationSchema = z.object({
  action: z.enum(["approve", "reject"]),
  expectedVersion: version,
  reason,
});
const orderSchema = z.discriminatedUnion("action", [
  z.object({
    action: z.literal("reserve"),
    expectedVersion: version,
    expiresAt: z.number().int().positive(),
    reason,
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
    entryDate: date.optional(),
  }),
  z.object({ action: z.literal("issue"), expectedVersion: version, reason, entryDate: date.optional(),
    materialLotAllocations: z.array(z.object({ productId: z.string().min(1), lotId: z.string().min(1), quantity: qty, balanceVersion: version })).max(500).default([]),
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]) }),
  z.object({
    action: z.literal("report"),
    expectedVersion: version,
    outputQuantity: qty,
    scrapQuantity: qty,
    laborCost: z.string().regex(/^\d+(\.\d{1,2})?$/),
    overheadCost: z.string().regex(/^\d+(\.\d{1,2})?$/),
    reason,
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
    entryDate: date.optional(),
  }),
  z.object({
    action: z.enum(["pass-qc", "fail-qc", "rework", "cancel", "scrap"]),
    expectedVersion: version,
    reason,
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
    entryDate: date.optional(),
    outputLotAllocations: z.array(z.object({ quantity: qty, lotCode: z.string().trim().min(1).max(120).optional(),
      serialNumber: z.string().trim().min(1).max(120).optional(), expiryDate: date.optional() })).max(500).default([]),
  }),
  z.object({
    action: z.literal("complete-operation"), expectedVersion: version, operationId: z.string().min(1),
    laborMinutes: z.number().int().nonnegative(), machineMinutes: z.number().int().nonnegative(), reason,
    entryDate: date.optional(), attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
  }),
]);
function access(
  a: WorkContext,
  module: "planning" | "manufacturing",
  write = false,
) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, module))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      `Your current role cannot access ${module}.`,
    );
}
async function specRecord(
  write: SqlDatabase,
  a: WorkContext,
  id: string,
  v?: number,
) {
  return write
    .prepare(
      `SELECT * FROM manufacturing_specifications WHERE id=? AND company_id=? ${v ? "AND version=?" : ""}`,
    )
    .bind(...(v ? [id, a.companyId, v] : [id, a.companyId]))
    .first<Row>();
}
export async function productionWorkspace(
  a: WorkContext,
  module: "planning" | "manufacturing",
) {
  access(a, module);
  const { write } = await holdingResources(a.holdingId),
    [specs, plans, recommendations, orders, products, warehouses, resources, calendar, accountingProfiles, demandSources, accounts] =
      await Promise.all([
        write
          .prepare(
            "SELECT * FROM manufacturing_specifications WHERE company_id=? ORDER BY updated_at DESC",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            "SELECT p.*,s.name specification_name FROM planning_scenarios p JOIN manufacturing_specifications s ON s.id=p.specification_id WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY p.updated_at DESC",
          )
          .bind(a.companyId, a.branchId, a.branchId)
          .all<Row>(),
        write
          .prepare(
            "SELECT r.*,s.company_id,s.branch_id FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY r.updated_at DESC",
          )
          .bind(a.companyId, a.branchId, a.branchId)
          .all<Row>(),
        write
          .prepare(
            "SELECT o.*,s.name specification_name,p.name output_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id JOIN commerce_products p ON p.id=s.output_product_id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC",
          )
          .bind(a.companyId, a.branchId, a.branchId)
          .all<Row>(),
        write
          .prepare(
            "SELECT id,sku,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name",
          )
          .bind(a.companyId)
          .all<Row>(),
        write
          .prepare(
            "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name",
          )
          .bind(a.companyId, a.branchId, a.branchId)
          .all<Row>(),
        write.prepare("SELECT * FROM production_resources WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name")
          .bind(a.companyId, a.branchId, a.branchId).all<Row>(),
        write.prepare(`SELECT c.* FROM production_resource_calendar c JOIN production_resources r ON r.id=c.resource_id
          WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY c.work_date DESC LIMIT 500`)
          .bind(a.companyId, a.branchId, a.branchId).all<Row>(),
        write.prepare("SELECT id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id FROM manufacturing_accounting_profiles WHERE company_id=? ORDER BY updated_at DESC")
          .bind(a.companyId).all<Row>(),
        write.prepare(`SELECT o.id,o.number,o.version,o.due_date,l.product_id,l.quantity_milli-l.fulfilled_milli remaining_milli
          FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id WHERE o.company_id=? AND o.kind='sales'
          AND o.state IN ('confirmed','partially_fulfilled') AND l.quantity_milli>l.fulfilled_milli AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.due_date,o.number`)
          .bind(a.companyId, a.branchId, a.branchId).all<Row>(),
        hasPermission(a, "finance.read") ? write.prepare("SELECT a.id,a.code,a.name,a.chart_id,a.category FROM finance_accounts a JOIN commerce_settings s ON s.chart_id=a.chart_id WHERE a.company_id=? AND s.company_id=? AND a.status='active' AND a.control_kind='none' ORDER BY a.code")
          .bind(a.companyId, a.companyId).all<Row>() : Promise.resolve({ results: [] as Row[] }),
      ]);
  return {
    specifications: specs.results,
    plans: plans.results,
    recommendations: recommendations.results,
    orders: orders.results,
    products: products.results,
    warehouses: warehouses.results,
    resources: resources.results,
    calendar: calendar.results,
    accountingProfiles: accountingProfiles.results,
    demandSources: demandSources.results,
    accounts: accounts.results,
    canConfigureAccounting: !a.branchId && !a.departmentId && hasPermission(a, "finance.write"),
    canReviewAccounting: !a.branchId && !a.departmentId && hasPermission(a, "finance.approve"),
    canManageResources: !a.departmentId && hasPermission(a, "records.write") && hasPermission(a, "records.review"),
  };
}
export type PlanningExecutiveEvidenceMetric =
  | "planning-shortage-scenarios"
  | "proposed-planning-recommendations"
  | "capacity-feasible-recommendations";
export async function planningExecutiveEvidenceRow(
  db: SqlDatabase,
  a: WorkContext,
  metric: PlanningExecutiveEvidenceMetric,
  recordId: string,
  from: string,
  to: string,
) {
  access(a, "planning");
  const branchScope = "(?::text IS NULL OR s.branch_id=?)";
  const sourceJoins = `
    LEFT JOIN manufacturing_specifications spec ON spec.id=s.specification_id AND spec.company_id=s.company_id
    LEFT JOIN commerce_products output ON output.id=spec.output_product_id AND output.company_id=s.company_id
    LEFT JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=s.company_id
    LEFT JOIN organization_nodes branch ON branch.id=s.branch_id AND branch.company_id=s.company_id
    LEFT JOIN planning_recommendations r ON r.scenario_id=s.id`;
  const shortageProjection = `
    LEFT JOIN LATERAL (
      SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'product_name',COALESCE(p.name,'Unknown product'),
        'required_milli',(entry.value->>'requiredMilli')::numeric::text,
        'available_milli',(entry.value->>'availableMilli')::numeric::text,
        'shortage_milli',(entry.value->>'shortageMilli')::numeric::text
      ) ORDER BY COALESCE(p.name,entry.value->>'productId')) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0),'[]'::jsonb)::text shortage_items_json,
      count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count
      FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)
      LEFT JOIN commerce_products p ON p.id=entry.value->>'productId' AND p.company_id=s.company_id
    ) shortage ON TRUE`;
  const projection = `SELECT s.id scenario_id,s.company_id,s.branch_id,branch.display_name branch_name,
      s.warehouse_id,w.name warehouse_name,s.specification_id,spec.name specification_name,
      spec.output_product_id,output.name output_product_name,s.specification_version,
      s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,
      s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,
      r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,
      r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,
      r.scenario_version,r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,
      COALESCE(shortage.shortage_count,'0') shortage_count,COALESCE(shortage.shortage_items_json,'[]') shortage_items_json
    FROM planning_scenarios s ${sourceJoins} ${shortageProjection}`;
  if (metric === "planning-shortage-scenarios") {
    return db.prepare(`${projection}
      WHERE s.id=? AND s.company_id=? AND ${branchScope} AND s.due_date BETWEEN ? AND ?
        AND jsonb_path_exists(COALESCE(s.shortages_json::jsonb,'[]'::jsonb),'$[*] ? (@.shortageMilli > 0)') LIMIT 1`)
      .bind(recordId,a.companyId,a.branchId,a.branchId,from,to).first<Row>();
  }
  const eligibility = metric === "proposed-planning-recommendations"
    ? "r.status='proposed'"
    : "r.status IN ('proposed','approved') AND r.required_minutes<=s.capacity_minutes";
  return db.prepare(`${projection}
    WHERE r.id=? AND s.company_id=? AND ${branchScope} AND r.feasible_date BETWEEN ? AND ? AND ${eligibility} LIMIT 1`)
    .bind(recordId,a.companyId,a.branchId,a.branchId,from,to).first<Row>();
}
export async function createSpecification(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, "manufacturing", true);
  const i = specSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `mfg-spec-${key}`,
    op = "manufacturing.specification.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  if (
    new Set(i.lines.map((x) => x.productId)).size !== i.lines.length ||
    i.lines.some((x) => x.productId === i.outputProductId)
  )
    throw new ApiError(
      422,
      "INVALID_SPECIFICATION_LINES",
      "Materials must be unique and different from output.",
    );
  const now = Date.now(),
    result = { id, status: "draft", version: 1 },
    steps: SqlStatement[] = [
      assertion(
        write,
        key,
        "output",
        "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active')",
        [i.outputProductId, a.companyId],
      ),
      write
        .prepare(
          "INSERT INTO manufacturing_specifications VALUES(?,?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
        )
        .bind(
          id,
          a.companyId,
          i.name,
          i.mode,
          i.outputProductId,
          q(i.batchQuantity),
          i.expectedYieldBps,
          a.userId,
          now,
          now,
        ),
    ];
  i.lines.forEach((x, n) =>
    steps.push(
      assertion(
        write,
        key,
        `material-${n}`,
        "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status='active')",
        [x.productId, a.companyId],
      ),
      write
        .prepare(
          "INSERT INTO manufacturing_specification_lines VALUES(?,?,?,?,?,?)",
        )
        .bind(
          `mfg-spec-line-${key}-${n}`,
          id,
          x.productId,
          q(x.quantity),
          x.substitutionGroup ?? null,
          n,
        ),
    ),
  );
  i.operations.forEach((operation, n) => steps.push(
    assertion(write, key, `routing-resource-${n}`, "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?))", [operation.resourceId, a.companyId, a.branchId, a.branchId]),
    write.prepare("INSERT INTO manufacturing_specification_operations(id,specification_id,resource_id,name,setup_minutes,run_minutes_per_batch,ordinal) VALUES(?,?,?,?,?,?,?)")
      .bind(`mfg-operation-${key}-${n}`, id, operation.resourceId, operation.name, operation.setupMinutes, operation.runMinutesPerBatch, n),
  ));
  return executeCommand(write, a, key, op, i, id, 1, result, steps);
}
export async function reviewSpecification(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, "manufacturing", true);
  requirePermission(a, "records.review");
  const i = reviewSchema.parse(raw), { write } = await holdingResources(a.holdingId), op = `manufacturing.specification.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const row = await specRecord(write, a, id);
  if (!row)
    throw new ApiError(
      404,
      "SPECIFICATION_NOT_FOUND",
      "Specification not found.",
    );
  const expected = i.action === "retire" ? "approved" : "draft",
    status =
      i.action === "approve"
        ? "approved"
        : i.action === "reject"
          ? "rejected"
          : "retired";
  if (
    row.status !== expected ||
    Number(row.version) !== i.expectedVersion ||
    (["approve", "reject"].includes(i.action) && row.created_by === a.userId)
  )
    throw new ApiError(
      409,
      "CONFLICT",
      "Independent current review is required.",
    );
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    i.expectedVersion + 1,
    { id, status, version: i.expectedVersion + 1 },
    [
      assertion(
        write,
        key,
        "spec",
        "EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status=? AND (?='retire' OR created_by<>?))",
        [id, a.companyId, i.expectedVersion, expected, i.action, a.userId],
      ),
      write
        .prepare(
          "UPDATE manufacturing_specifications SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, a.userId, Date.now(), id),
    ],
  );
}
export type PlanningScenarioResult = {
  id: string; recommendationId: string; status: string; version: number;
  shortages: Array<{ productId: string; requiredMilli: number; availableMilli: number; incomingMilli: number; shortageMilli: number }>;
  feasibleDate: string; requiredMinutes: number; capacityMinutes: number; context: PlanningContext;
};
export async function createPlanningScenario(a: WorkContext, raw: unknown, key: string): Promise<PlanningScenarioResult> {
  access(a, "planning", true);
  const i = planSchema.parse(raw), { write } = await holdingResources(a.holdingId), id = `plan-${key}`,
    recommendationId = `plan-recommendation-${key}`, op = "planning.scenario.create",
    prior = await replayCommand<PlanningScenarioResult>(write, a, key, op, id, i);
  if (prior) return prior;
  const spec = await specRecord(write, a, i.specificationId, i.specificationVersion);
  if (!spec || spec.status !== "approved")
    throw new ApiError(409, "PLANNING_SOURCE_CHANGED", "Choose a current approved specification.");
  const warehouse = await write.prepare("SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)")
    .bind(i.warehouseId, a.companyId, a.branchId, a.branchId).first<Row>();
  if (!warehouse) throw new ApiError(404, "WAREHOUSE_NOT_FOUND", "Warehouse not found in scope.");
  const context: PlanningContext = { incomingSupply: [], stock: [], startDate: i.startDate ?? i.dueDate, comparisonScenarioId: i.comparisonScenarioId ?? null };
  let demand = i.demandQuantity ? q(i.demandQuantity) : 0;
  if (i.salesOrderId) {
    const source = await write.prepare(`SELECT o.id,o.version,l.quantity_milli-l.fulfilled_milli remaining_milli
      FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id WHERE o.id=? AND o.company_id=? AND o.warehouse_id=?
      AND o.kind='sales' AND o.version=? AND o.state IN ('confirmed','partially_fulfilled') AND l.product_id=? AND l.quantity_milli>l.fulfilled_milli`)
      .bind(i.salesOrderId, a.companyId, i.warehouseId, i.salesOrderVersion!, spec.output_product_id).first<Row>();
    if (!source) throw new ApiError(409, "PLANNING_DEMAND_SOURCE_CHANGED", "Choose the current confirmed Sales order for this output and warehouse.");
    demand = Number(source.remaining_milli);
    context.demandSource = { id: String(source.id), version: Number(source.version), quantityMilli: demand };
  }
  if (!Number.isSafeInteger(demand) || demand <= 0) throw new ApiError(422, "PLANNING_DEMAND_REQUIRED", "Demand must be a positive exact quantity.");
  const factor = demand / Number(spec.batch_quantity_milli),
    lines = (await write.prepare("SELECT * FROM manufacturing_specification_lines WHERE specification_id=? ORDER BY ordinal")
      .bind(i.specificationId).all<Row>()).results,
    shortages: Array<{ productId: string; requiredMilli: number; availableMilli: number; incomingMilli: number; shortageMilli: number }> = [];
  for (const line of lines) {
    const required = Math.ceil(Number(line.quantity_milli) * factor),
      stock = await write.prepare(`SELECT s.version, GREATEST(0,quantity_milli-inventory_quarantined_quantity(s.warehouse_id,s.product_id)
        -COALESCE((SELECT sum(quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)) available
        FROM commerce_stock s WHERE warehouse_id=? AND product_id=?`).bind(i.warehouseId, line.product_id).first<{ available: number; version: number }>(),
      incoming = (await write.prepare(`SELECT o.id,o.version,o.due_date,l.quantity_milli-l.fulfilled_milli incoming_milli
        FROM commerce_orders o JOIN commerce_order_lines l ON l.order_id=o.id WHERE o.company_id=? AND o.warehouse_id=? AND o.kind='purchase'
        AND o.state IN ('confirmed','partially_fulfilled') AND o.due_date<=? AND l.product_id=? AND l.quantity_milli>l.fulfilled_milli ORDER BY o.due_date,o.id`)
        .bind(a.companyId, i.warehouseId, i.dueDate, line.product_id).all<Row>()).results,
      incomingMilli = incoming.reduce((sum, source) => sum + Number(source.incoming_milli), 0), available = Number(stock?.available ?? 0);
    context.stock!.push({ productId: String(line.product_id), version: Number(stock?.version ?? 0) });
    context.incomingSupply!.push(...incoming.map(source => ({ id: String(source.id), version: Number(source.version), productId: String(line.product_id), quantityMilli: Number(source.incoming_milli), dueDate: String(source.due_date) })));
    shortages.push({ productId: String(line.product_id), requiredMilli: required, availableMilli: available, incomingMilli, shortageMilli: Math.max(0, required - available - incomingMilli) });
  }
  const capacity = await calculateProductionCapacity(write, a, { specificationId: i.specificationId, branchId: String(warehouse.branch_id), factor,
    startDate: context.startDate!, dueDate: i.dueDate }),
    requiredMinutes = capacity?.requiredMinutes ?? Math.ceil(demand / 1000) * i.manualMinutesPerUnit,
    days = Math.max(0, Math.ceil((requiredMinutes - i.capacityMinutes) / 480)),
    feasible = capacity?.feasibleDate ?? new Date(Date.parse(i.dueDate + "T00:00:00Z") + days * 86400000).toISOString().slice(0, 10),
    capacityMinutes = capacity?.capacityMinutes ?? i.capacityMinutes;
  context.capacityBasis = capacity ? "approved_routing_resource_calendar" : "manual_capacity_assumption";
  context.allocations = capacity?.allocations ?? [];
  context.operations = capacity?.operations ?? [];
  const now = Date.now(), result = { id, recommendationId, status: "calculated", version: 1, shortages, feasibleDate: feasible,
    requiredMinutes, capacityMinutes, context };
  const guards = planningContextGuards(write, a, key, context);
  if (i.comparisonScenarioId) {
    const comparison = await write.prepare(`SELECT s.id,s.version,s.shortages_json,r.required_minutes,r.feasible_date FROM planning_scenarios s
      JOIN planning_recommendations r ON r.scenario_id=s.id WHERE s.id=? AND s.company_id=? AND s.branch_id=? AND s.specification_id=?`)
      .bind(i.comparisonScenarioId, a.companyId, warehouse.branch_id, i.specificationId).first<Row>();
    if (!comparison) throw new ApiError(404, "PLANNING_COMPARISON_NOT_FOUND", "Compare a scenario for the same output, branch and specification.");
    const previousShortages = JSON.parse(String(comparison.shortages_json)) as Array<{ shortageMilli: number }>;
    context.comparison = { scenarioId: String(comparison.id), sourceVersion: Number(comparison.version),
      shortageDeltaMilli: shortages.reduce((n, shortage) => n + shortage.shortageMilli, 0) - previousShortages.reduce((n, shortage) => n + shortage.shortageMilli, 0),
      requiredMinutesDelta: requiredMinutes - Number(comparison.required_minutes), feasibleDate: String(comparison.feasible_date), currentFeasibleDate: feasible };
    guards.push(assertion(write, key, "comparison", "EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND branch_id=? AND specification_id=? AND version=?)", [i.comparisonScenarioId, a.companyId, String(warehouse.branch_id), i.specificationId, Number(comparison.version)]));
  }
  return executeCommand(write, a, key, op, i, id, 1, result, [
    assertion(write, key, "specification-source", "EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status='approved')", [i.specificationId, a.companyId, i.specificationVersion]),
    ...guards,
    write.prepare(`INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,status,created_by,version,created_at,updated_at,context_json)
      VALUES(?,?,?,?,?,?,?,?,?,?,'calculated',?,1,?,?,?)`).bind(id, a.companyId, warehouse.branch_id, i.warehouseId, i.specificationId,
        i.specificationVersion, demand, i.dueDate, capacityMinutes, JSON.stringify(shortages), a.userId, now, now, JSON.stringify(context)),
    write.prepare("INSERT INTO planning_recommendations VALUES(?,?,?,?,?,?,'proposed',?,NULL,1,?,?)")
      .bind(recommendationId, id, 1, demand, requiredMinutes, feasible, a.userId, now, now),
    write.prepare("INSERT INTO planning_scenario_events VALUES(?,?,1,'calculate',?,?,?)")
      .bind(`planning-event-${key}`, id, a.userId, context.demandSource ? "Calculated from confirmed Sales demand and captured supply/capacity" : "Calculated from declared manual demand and captured supply/capacity", now),
  ]);
}
export async function cancelPlanningScenario(a: WorkContext, id: string, raw: unknown, key: string) {
  access(a, "planning", true);
  const i = z.object({ action: z.literal("cancel"), expectedVersion: version, reason }).parse(raw), { write } = await holdingResources(a.holdingId),
    op = "planning.scenario.cancel", prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const row = await write.prepare("SELECT * FROM planning_scenarios WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)")
    .bind(id, a.companyId, a.branchId, a.branchId).first<Row>();
  if (!row) throw new ApiError(404, "PLANNING_SCENARIO_NOT_FOUND", "Scenario not found in this planning scope.");
  if (row.created_by !== a.userId && !hasPermission(a, "records.review")) throw new ApiError(403, "PLANNING_OWNER_REQUIRED", "The planner or an authorized reviewer must cancel this scenario.");
  if (row.status !== "calculated" || Number(row.version) !== i.expectedVersion) throw new ApiError(409, "PLANNING_SCENARIO_RELEASED", "Only the current unreleased scenario can be cancelled. Cancel a released order through Manufacturing.");
  const now = Date.now();
  return executeCommand(write, a, key, op, i, id, i.expectedVersion + 1, { id, status: "cancelled", version: i.expectedVersion + 1 }, [
    write.prepare("SELECT id FROM planning_recommendations WHERE scenario_id=? FOR UPDATE").bind(id),
    assertion(write, key, "scenario", "EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND version=? AND status='calculated')", [id, a.companyId, i.expectedVersion]),
    write.prepare("UPDATE planning_scenarios SET status='cancelled',version=version+1,updated_at=? WHERE id=?").bind(now, id),
    write.prepare("UPDATE planning_recommendations SET status='rejected',reviewed_by=?,version=version+1,updated_at=? WHERE scenario_id=? AND status='proposed'").bind(a.userId, now, id),
    write.prepare("INSERT INTO planning_scenario_events VALUES(?,?,?,'cancel',?,?,?)").bind(`planning-event-${key}`, id, i.expectedVersion + 1, a.userId, i.reason, now),
  ]);
}
export async function reviewPlanningRecommendation(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, "planning", true);
  requirePermission(a, "records.review");
  const i = recommendationSchema.parse(raw), { write } = await holdingResources(a.holdingId), op = `planning.recommendation.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const row = await write
      .prepare(
        "SELECT r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)",
      )
      .bind(id, a.companyId, a.branchId, a.branchId)
      .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "PLANNING_RECOMMENDATION_NOT_FOUND",
      "Recommendation not found.",
    );
  if (
    row.status !== "proposed" ||
    Number(row.version) !== i.expectedVersion ||
    row.created_by === a.userId
  )
    throw new ApiError(
      409,
      "CONFLICT",
      "Independent current review is required.",
    );
  const status = i.action === "approve" ? "approved" : "rejected";
  const now = Date.now(),
    steps: SqlStatement[] = [
      write.prepare("SELECT id FROM planning_recommendations WHERE id=? FOR UPDATE").bind(id),
      assertion(
        write,
        key,
        "recommendation",
        "EXISTS(SELECT 1 FROM planning_recommendations WHERE id=? AND version=? AND status='proposed' AND created_by<>?)",
        [id, i.expectedVersion, a.userId],
      ),
      write
        .prepare(
          "UPDATE planning_recommendations SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, a.userId, now, id),
    ];
  let orderId: null | string = null;
  if (i.action === "approve") {
    const context = JSON.parse(String(row.context_json)) as PlanningContext;
    steps.push(...planningContextGuards(write, a, key, context),
      assertion(write, key, "specification-source", "EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status='approved')", [String(row.specification_id), a.companyId, Number(row.specification_version)]));
    for (const [n, allocation] of (context.allocations ?? []).entries()) steps.push(
      assertion(write, key, `capacity-reservation-${n}`, "COALESCE((SELECT sum(minutes) FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)+?<=?", [allocation.resourceId, allocation.workDate, allocation.minutes, allocation.availableMinutes]),
      write.prepare("INSERT INTO planning_resource_allocations(id,scenario_id,resource_id,work_date,minutes,created_by,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(`planning-capacity-${key}-${n}`, row.scenario_id, allocation.resourceId, allocation.workDate, allocation.minutes, a.userId, now),
    );
    orderId = `mfg-order-${key}`;
    steps.push(
      write
        .prepare(
          "INSERT INTO manufacturing_orders(id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,specification_version,planned_quantity_milli,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'planned',?,?,?)",
        )
        .bind(
          orderId,
          a.companyId,
          row.branch_id,
          row.warehouse_id,
          id,
          row.specification_id,
          row.specification_version,
          row.recommended_quantity_milli,
          a.userId,
          now,
          now,
        ),
      write
        .prepare(
          "UPDATE planning_scenarios SET status='released',version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, row.scenario_id),
      write.prepare("INSERT INTO planning_scenario_events(id,scenario_id,version,action,actor_id,reason,created_at) SELECT ?,id,version,'release',?,?,? FROM planning_scenarios WHERE id=?")
        .bind(`planning-event-${key}`, a.userId, i.reason, now, row.scenario_id),
      write
        .prepare(
          "INSERT INTO manufacturing_order_events VALUES(?,?,1,'release',? ,?,'planned','{}',?)",
        )
        .bind(`mfg-event-${key}`, orderId, a.userId, i.reason, now),
    );
  }
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    i.expectedVersion + 1,
    { id, status, version: i.expectedVersion + 1, orderId },
    steps,
  );
}
export async function changeManufacturingOrder(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, "manufacturing", true);
  const i = orderSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    row = await write
      .prepare(
        "SELECT o.*,s.output_product_id,s.batch_quantity_milli,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id JOIN commerce_products p ON p.id=s.output_product_id WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)",
      )
      .bind(id, a.companyId, a.branchId, a.branchId)
      .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "MANUFACTURING_ORDER_NOT_FOUND",
      "Order not found.",
    );
  const op = `manufacturing.order.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  if (Number(row.version) !== i.expectedVersion)
    throw new ApiError(409, "CONFLICT", "Order changed. Reload and retry.");
  const now = Date.now(),
    next = i.expectedVersion + 1,
    lines = (
      await write
        .prepare(
          "SELECT l.*,p.tracking_mode FROM manufacturing_specification_lines l JOIN commerce_products p ON p.id=l.product_id WHERE l.specification_id=? ORDER BY l.ordinal",
        )
        .bind(row.specification_id)
        .all<Row>()
    ).results,
    factor =
      Number(row.planned_quantity_milli) / Number(row.batch_quantity_milli),
    steps: SqlStatement[] = [
      write.prepare("SELECT id FROM manufacturing_orders WHERE id=? FOR UPDATE").bind(id),
      assertion(write, key, "order", "EXISTS(SELECT 1 FROM manufacturing_orders WHERE id=? AND version=? AND status=?)", [id, i.expectedVersion, row.status]),
    ], entryDate = i.entryDate ?? new Date(now).toISOString().slice(0, 10);
  let status = "";
  let journalId: string | null = null;
  if (i.action === "reserve") {
    if (row.status !== "planned" || i.expiresAt <= now)
      throw new ApiError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Reserve a planned order with a future expiry.",
      );
    status = "reserved";
    for (const [n, line] of lines.entries()) {
      const required = Math.ceil(Number(line.quantity_milli) * factor);
      steps.push(
        write.prepare("SELECT warehouse_id FROM commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE").bind(row.warehouse_id, line.product_id),
        assertion(
          write,
          key,
          `available-${n}`,
          "COALESCE((SELECT quantity_milli FROM commerce_stock WHERE warehouse_id=? AND product_id=?),0)-COALESCE((SELECT sum(quantity_milli) FROM inventory_active_reservations WHERE warehouse_id=? AND product_id=?),0)>=?",
          [
            row.warehouse_id,
            line.product_id,
            row.warehouse_id,
            line.product_id,
            required,
          ],
        ),
        write
          .prepare(
            "INSERT INTO manufacturing_material_reservations VALUES(?,?,?,?,?,0,0,?,?,?)",
          )
          .bind(
            `mfg-reservation-${key}-${n}`,
            id,
            row.warehouse_id,
            line.product_id,
            required,
            i.expiresAt,
            a.userId,
            now,
          ),
      );
    }
  } else if (i.action === "issue") {
    if (row.status !== "reserved")
      throw new ApiError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Issue a reserved order.",
      );
    status = "in_progress";
    let material = 0;
    for (const [n, line] of lines.entries()) {
      const reservation = await write
          .prepare(
            "SELECT * FROM manufacturing_material_reservations WHERE order_id=? AND product_id=?",
          )
          .bind(id, line.product_id)
          .first<Row>(),
        stock = await write
          .prepare(
            "SELECT * FROM commerce_stock WHERE warehouse_id=? AND product_id=?",
          )
          .bind(row.warehouse_id, line.product_id)
          .first<Row>();
      if (!reservation || !stock)
        throw new ApiError(
          409,
          "MATERIAL_RESERVATION_REQUIRED",
          "Material reservation is missing.",
        );
      const amount = Number(reservation.quantity_milli), lotPosting = await prepareManufacturingMaterialLots(write, a, {
        key, orderId: id, orderVersion: next, warehouseId: String(row.warehouse_id), productId: String(line.product_id), trackingMode: String(line.tracking_mode),
        requiredMilli: amount, entryDate, allocations: i.materialLotAllocations,
      }), value = lotPosting?.valueMinor ?? scaledAmount(Number(stock.value_minor), amount, Number(stock.quantity_milli));
      material += value;
      steps.push(
        write.prepare("SELECT warehouse_id FROM commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE").bind(row.warehouse_id, line.product_id),
        assertion(write, key, `stock-source-${n}`, "EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND product_id=? AND version=? AND quantity_milli>=? AND value_minor>=?)", [row.warehouse_id, line.product_id, Number(stock.version), amount, value]),
        assertion(
          write,
          key,
          `reservation-${n}`,
          "EXISTS(SELECT 1 FROM manufacturing_material_reservations WHERE id=? AND consumed_milli=0 AND released_milli=0 AND expires_at>?)",
          [reservation.id, now],
        ),
        write
          .prepare(
            "UPDATE manufacturing_material_reservations SET consumed_milli=quantity_milli WHERE id=?",
          )
          .bind(reservation.id),
        write
          .prepare(
            "UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?",
          )
          .bind(amount, value, row.warehouse_id, line.product_id),
        ...(lotPosting?.steps ?? []),
        assertion(write, key, `material-movement-date-${n}`, "NOT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)", [row.warehouse_id, line.product_id, entryDate]),
        write.prepare("INSERT INTO manufacturing_inventory_moves(id,order_id,warehouse_id,product_id,phase,source_order_version,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,?,'issue',?,?,?,?,?,?)")
          .bind(`mfg-stock-${key}-${n}`, id, row.warehouse_id, line.product_id, next, -amount, -value, entryDate, a.userId, now),
      );
    }
    steps.push(
      write
        .prepare(
          "UPDATE manufacturing_orders SET material_cost_minor=? WHERE id=?",
        )
        .bind(material, id),
    );
    const posting = await prepareManufacturingPosting(write, a, { key, orderId: id, orderVersion: next, phase: "issue", entryDate, material });
    steps.push(...posting.steps); journalId = posting.journalId;
  } else if (i.action === "report") {
    if (row.status !== "in_progress")
      throw new ApiError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Report an in-progress order.",
      );
    status = "awaiting_qc";
    if (q(i.outputQuantity) <= 0) throw new ApiError(422, "MANUFACTURING_OUTPUT_REQUIRED", "Report a positive output quantity; independently scrap failed output when no usable output remains.");
    if (row.output_tracking_mode === "serial" && q(i.outputQuantity) % 1000 !== 0) throw new ApiError(422, "SERIAL_QUANTITY_INVALID", "Serialized finished output must contain whole units.");
    steps.push(assertion(write, key, "completed-routing", `NOT EXISTS(SELECT 1 FROM manufacturing_specification_operations op WHERE op.specification_id=?
      AND NOT EXISTS(SELECT 1 FROM manufacturing_operation_events e WHERE e.order_id=? AND e.operation_id=op.id))`, [String(row.specification_id), id]));
    const labor = decimalToMinor(i.laborCost), overhead = decimalToMinor(i.overheadCost);
    steps.push(
      write
        .prepare(
          "UPDATE manufacturing_orders SET reported_output_milli=?,reported_scrap_milli=?,labor_cost_minor=?,overhead_cost_minor=? WHERE id=?",
        )
        .bind(
          q(i.outputQuantity),
          q(i.scrapQuantity),
          Number(row.labor_cost_minor) + labor,
          Number(row.overhead_cost_minor) + overhead,
          id,
        ),
    );
    const posting = await prepareManufacturingPosting(write, a, { key, orderId: id, orderVersion: next, phase: "report", entryDate, labor, overhead });
    steps.push(...posting.steps); journalId = posting.journalId;
  } else if (i.action === "pass-qc") {
    requirePermission(a, "records.review");
    if (row.status !== "awaiting_qc")
      throw new ApiError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "QC reviews reported output.",
      );
    const reporter = await write
      .prepare(
        "SELECT actor_id FROM manufacturing_order_events WHERE order_id=? AND action='report' ORDER BY version DESC LIMIT 1",
      )
      .bind(id)
      .first<string>("actor_id");
    if (String(reporter) === a.userId)
      throw new ApiError(
        409,
        "CONFLICT",
        "QC must be independent from output reporting.",
      );
    steps.push(assertion(write, key, "independent-qc", "EXISTS(SELECT 1 FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=? AND actor_id<>?)", [id, i.expectedVersion, a.userId]));
    status = "completed";
    const output = Number(row.reported_output_milli),
      total =
        Number(row.material_cost_minor) +
        Number(row.labor_cost_minor) +
        Number(row.overhead_cost_minor);
    steps.push(
      write
        .prepare(
          "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,?,?,1) ON CONFLICT(warehouse_id,product_id) DO UPDATE SET quantity_milli=commerce_stock.quantity_milli+excluded.quantity_milli,value_minor=commerce_stock.value_minor+excluded.value_minor,version=commerce_stock.version+1",
        )
        .bind(row.warehouse_id, row.output_product_id, output, total),
      ...await prepareManufacturingOutputLots(write, a, { key, orderId: id, orderVersion: next, warehouseId: String(row.warehouse_id), productId: String(row.output_product_id),
        trackingMode: String(row.output_tracking_mode), outputMilli: output, totalMinor: total, entryDate, allocations: i.outputLotAllocations }),
      assertion(write, key, "output-movement-date", "NOT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)", [row.warehouse_id, row.output_product_id, entryDate]),
      write.prepare("INSERT INTO manufacturing_inventory_moves(id,order_id,warehouse_id,product_id,phase,source_order_version,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,?,'output',?,?,?,?,?,?)")
        .bind(`mfg-stock-${key}`, id, row.warehouse_id, row.output_product_id, next, output, total, entryDate, a.userId, now),
      write
        .prepare(
          "INSERT INTO manufacturing_cost_snapshots VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          `mfg-cost-${key}`,
          id,
          next,
          row.material_cost_minor,
          row.labor_cost_minor,
          row.overhead_cost_minor,
          total,
          output,
          now,
        ),
    );
    for (const [n, line] of lines.entries())
      steps.push(
        write
          .prepare(
            "INSERT INTO manufacturing_genealogy VALUES(?,?,?,?,?,?,?,?)",
          )
          .bind(
            `mfg-genealogy-${key}-${n}`,
            id,
            line.product_id,
            row.output_product_id,
            Math.ceil(Number(line.quantity_milli) * factor),
            output,
            next,
            now,
          ),
      );
    const posting = await prepareManufacturingPosting(write, a, { key, orderId: id, orderVersion: next, phase: "output", entryDate, output: total, quantityMilli: output });
    steps.push(...posting.steps); journalId = posting.journalId;
  } else if (i.action === "fail-qc") {
    requirePermission(a, "records.review");
    if (row.status !== "awaiting_qc")
      throw new ApiError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "QC reviews reported output.",
      );
    status = "qc_failed";
    const reporter = await write.prepare("SELECT actor_id FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=?")
      .bind(id, i.expectedVersion).first<string>("actor_id");
    if (!reporter || reporter === a.userId) throw new ApiError(409, "CONFLICT", "QC must be independent from output reporting.");
    steps.push(assertion(write, key, "independent-qc", "EXISTS(SELECT 1 FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=? AND actor_id<>?)", [id, i.expectedVersion, a.userId]));
  } else if (i.action === "scrap") {
    requirePermission(a, "records.review");
    if (row.status !== "qc_failed") throw new ApiError(409, "INVALID_MANUFACTURING_TRANSITION", "Only independently failed output can be written off.");
    const reporter = await write.prepare("SELECT actor_id FROM manufacturing_order_events WHERE order_id=? AND action='report' ORDER BY version DESC LIMIT 1").bind(id).first<string>("actor_id");
    if (!reporter || reporter === a.userId) throw new ApiError(409, "CONFLICT", "The reporter cannot approve failed output disposal.");
    status = "cancelled";
    const total = Number(row.material_cost_minor) + Number(row.labor_cost_minor) + Number(row.overhead_cost_minor),
      posting = await prepareManufacturingPosting(write, a, { key, orderId: id, orderVersion: next, phase: "scrap", entryDate, output: total });
    steps.push(...posting.steps); journalId = posting.journalId;
  } else if (i.action === "complete-operation") {
    if (row.status !== "in_progress") throw new ApiError(409, "INVALID_MANUFACTURING_TRANSITION", "Routing operations belong to an in-progress run.");
    const operation = await write.prepare("SELECT * FROM manufacturing_specification_operations WHERE id=? AND specification_id=?")
      .bind(i.operationId, row.specification_id).first<Row>();
    if (!operation) throw new ApiError(404, "MANUFACTURING_OPERATION_NOT_FOUND", "Routing operation not found in this run.");
    steps.push(assertion(write, key, "routing-sequence", `NOT EXISTS(SELECT 1 FROM manufacturing_specification_operations op WHERE op.specification_id=? AND op.ordinal<?
      AND NOT EXISTS(SELECT 1 FROM manufacturing_operation_events e WHERE e.order_id=? AND e.operation_id=op.id))`, [String(row.specification_id), Number(operation.ordinal), id]),
      write.prepare("INSERT INTO manufacturing_operation_events(id,order_id,operation_id,action,source_order_version,labor_minutes,machine_minutes,evidence,created_by,created_at) VALUES(?,?,?,'complete',?,?,?,?,?,?)")
        .bind(`mfg-operation-event-${key}`, id, i.operationId, next, i.laborMinutes, i.machineMinutes, i.reason, a.userId, now));
    status = "in_progress";
  } else if (i.action === "rework") {
    if (row.status !== "qc_failed")
      throw new ApiError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Only failed output can re-enter work.",
      );
    status = "in_progress";
  } else {
    if (!["planned", "reserved"].includes(String(row.status)))
      throw new ApiError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Only unissued orders can be cancelled.",
      );
    status = "cancelled";
    steps.push(
      write
        .prepare(
          "UPDATE manufacturing_material_reservations SET released_milli=quantity_milli-consumed_milli,release_reason='Manufacturing order cancelled' WHERE order_id=?",
        )
        .bind(id),
    );
  }
  if (["completed", "cancelled"].includes(status)) steps.push(write.prepare(`UPDATE planning_resource_allocations SET released_at=? WHERE scenario_id=
    (SELECT r.scenario_id FROM planning_recommendations r JOIN manufacturing_orders o ON o.recommendation_id=r.id WHERE o.id=?) AND released_at IS NULL`).bind(now, id));
  const attachmentPurpose = i.action === "report"
    ? "run_report_evidence"
    : ["pass-qc", "fail-qc"].includes(i.action)
      ? "qc_evidence"
      : i.action === "rework"
        ? "rework_action_evidence"
        : null;
  if (i.attachmentSetIds.length && !attachmentPurpose)
    throw new ApiError(
      422,
      "MANUFACTURING_ATTACHMENT_ACTION",
      "Files can be bound only to output reporting, QC review, or rework.",
    );
  let attachmentBinding: null | Awaited<ReturnType<typeof prepareAttachmentBinding>> = null;
  if (attachmentPurpose && i.attachmentSetIds.length) {
    const componentRef = i.action === "report" ? "output-report" : i.action === "rework" ? "rework" : "qc-review";
    const placeholders = i.attachmentSetIds.map(() => "?").join(",");
    const sets = (await write.prepare(`SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${placeholders})`).bind(a.companyId, ...i.attachmentSetIds).all<{id:string;purpose_key:string;component_kind:string;component_ref:string|null}>()).results;
    if (
      sets.length !== new Set(i.attachmentSetIds).size ||
      sets.some((set) => set.purpose_key !== attachmentPurpose || set.component_kind !== "action" || set.component_ref !== componentRef)
    )
      throw new ApiError(
        422,
        "MANUFACTURING_ATTACHMENT_PURPOSE",
        "Choose only evidence prepared for this exact production action.",
      );
    attachmentBinding = await prepareAttachmentBinding(write, a, {
        attachmentSetIds: i.attachmentSetIds,
        bindingContract: {
          targetType: "production_run",
          bindingType: `manufacturing_${i.action}`,
          contexts: [{ purposeKey: attachmentPurpose, componentKind: "action", componentRef }],
        },
        targetId: id,
        expectedTargetVersion: i.expectedVersion,
        commandId: key,
        bindingId: `mfg-event-${key}`,
      });
  }
  if (attachmentBinding) steps.unshift(...attachmentBinding.guards);
  steps.push(
    write
      .prepare(
        "UPDATE manufacturing_orders SET status=?,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(status, now, id),
    write
      .prepare(
        "INSERT INTO manufacturing_order_events VALUES(?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        `mfg-event-${key}`,
        id,
        next,
        i.action,
        a.userId,
        i.reason,
        status,
        JSON.stringify(i),
        now,
      ),
  );
  if (attachmentBinding) steps.push(...attachmentBinding.steps);
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    next,
    { id, status, version: next, journalId },
    steps,
  );
}
export async function productionDetail(a: WorkContext, id: string) {
  access(a, "manufacturing");
  const { write } = await holdingResources(a.holdingId),
    order = await write
      .prepare(
        "SELECT o.*,s.name specification_name,s.mode,p.name output_name,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id JOIN commerce_products p ON p.id=s.output_product_id WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)",
      )
      .bind(id, a.companyId, a.branchId, a.branchId)
      .first<Row>();
  if (!order)
    throw new ApiError(
      404,
      "MANUFACTURING_ORDER_NOT_FOUND",
      "Order not found.",
    );
  return {
    order,
    materials: (
      await write
        .prepare(
          "SELECT r.*,p.name product_name,p.tracking_mode FROM manufacturing_material_reservations r JOIN commerce_products p ON p.id=r.product_id WHERE r.order_id=? ORDER BY p.name",
        )
        .bind(id)
        .all<Row>()
    ).results,
    history: (
      await write
        .prepare(
          "SELECT * FROM manufacturing_order_events WHERE order_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()
    ).results,
    cost: await write
      .prepare("SELECT * FROM manufacturing_cost_snapshots WHERE order_id=?")
      .bind(id)
      .first<Row>(),
    genealogy: (
      await write
        .prepare("SELECT * FROM manufacturing_genealogy WHERE order_id=?")
        .bind(id)
        .all<Row>()
    ).results,
    postings: (await write.prepare("SELECT p.*,j.status journal_status FROM manufacturing_order_postings p JOIN finance_journals j ON j.id=p.journal_id WHERE p.order_id=? ORDER BY p.source_order_version")
      .bind(id).all<Row>()).results,
    operations: (await write.prepare(`SELECT op.*,r.name resource_name,e.labor_minutes,e.machine_minutes,e.evidence,e.created_at completed_at
      FROM manufacturing_specification_operations op JOIN production_resources r ON r.id=op.resource_id
      LEFT JOIN manufacturing_operation_events e ON e.operation_id=op.id AND e.order_id=? WHERE op.specification_id=? ORDER BY op.ordinal`)
      .bind(id, order.specification_id).all<Row>()).results,
    materialLots: (await write.prepare(`SELECT l.*,b.quantity_milli,b.version balance_version FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id
      JOIN manufacturing_specification_lines ml ON ml.product_id=l.product_id WHERE ml.specification_id=? AND l.company_id=? AND b.warehouse_id=? AND b.quantity_milli>0
      AND NOT EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.warehouse_id=b.warehouse_id AND (h.lot_id=l.id OR (h.lot_id IS NULL AND h.product_id=l.product_id)) AND h.status NOT IN ('released','cancelled','disposed')) ORDER BY l.expiry_date NULLS LAST,l.id`)
      .bind(order.specification_id, a.companyId, order.warehouse_id).all<Row>()).results,
    lotGenealogy: (await write.prepare(`SELECT g.*,i.lot_id input_lot_id,il.lot_code input_lot_code,il.serial_number input_serial_number,
      ol.lot_id output_lot_id,l.lot_code output_lot_code,l.serial_number output_serial_number FROM manufacturing_lot_genealogy g
      JOIN manufacturing_material_lot_issues i ON i.id=g.input_issue_id JOIN inventory_lots il ON il.id=i.lot_id
      JOIN manufacturing_output_lots ol ON ol.id=g.output_lot_id JOIN inventory_lots l ON l.id=ol.lot_id WHERE g.order_id=? ORDER BY g.id`)
      .bind(id).all<Row>()).results,
  };
}
export async function manufacturingWork(a: WorkContext): Promise<WorkItem[]> {
  if (!hasModuleAccess(a.modules, "manufacturing")) return [];
  const { write } = await holdingResources(a.holdingId),
    rows = (
      await write
        .prepare(
          "SELECT o.*,s.name specification_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id WHERE o.company_id=? AND o.status NOT IN ('completed','cancelled') AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC",
        )
        .bind(a.companyId, a.branchId, a.branchId)
        .all<Row>()
    ).results;
  return rows.map((x) => ({
    id: String(x.id),
    type: "manufacturing_order",
    title: `Production · ${x.specification_name}`,
    detail: `${Number(x.planned_quantity_milli) / 1000} planned`,
    href: `/manufacturing/${x.id}`,
    status: String(x.status),
    updatedAt: Number(x.updated_at),
    queue: "todo",
    canAct: true,
    nextAction:
      String(x.status) === "planned"
        ? "Reserve materials"
        : String(x.status) === "reserved"
          ? "Issue materials"
          : String(x.status) === "awaiting_qc"
            ? "Record QC disposition"
            : "Report output",
    relationships: ["team"],
  }));
}
