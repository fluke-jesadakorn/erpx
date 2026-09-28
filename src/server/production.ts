import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import type { WorkItem } from "@/lib/work-item";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { decimalToMinor } from "@/lib/finance-money";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { prepareAttachmentBinding } from "./attachments/service";
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
});
const planSchema = z.object({
  warehouseId: z.string().min(1),
  specificationId: z.string().min(1),
  specificationVersion: version,
  demandQuantity: qty,
  dueDate: date,
  capacityMinutes: z.number().int().nonnegative(),
});
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
  }),
  z.object({ action: z.literal("issue"), expectedVersion: version, reason, attachmentSetIds: z.array(z.string().min(1)).max(10).default([]) }),
  z.object({
    action: z.literal("report"),
    expectedVersion: version,
    outputQuantity: qty,
    scrapQuantity: qty,
    laborCost: z.string().regex(/^\d+(\.\d{1,2})?$/),
    overheadCost: z.string().regex(/^\d+(\.\d{1,2})?$/),
    reason,
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
  }),
  z.object({
    action: z.enum(["pass-qc", "fail-qc", "rework", "cancel"]),
    expectedVersion: version,
    reason,
    attachmentSetIds: z.array(z.string().min(1)).max(10).default([]),
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
    [specs, plans, recommendations, orders, products, warehouses] =
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
      ]);
  return {
    specifications: specs.results,
    plans: plans.results,
    recommendations: recommendations.results,
    orders: orders.results,
    products: products.results,
    warehouses: warehouses.results,
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
  const i = reviewSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    row = await specRecord(write, a, id);
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
  const op = `manufacturing.specification.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
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
        "EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND version=? AND status=?)",
        [id, i.expectedVersion, expected],
      ),
      write
        .prepare(
          "UPDATE manufacturing_specifications SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, a.userId, Date.now(), id),
    ],
  );
}
export async function createPlanningScenario(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, "planning", true);
  const i = planSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    spec = await specRecord(
      write,
      a,
      i.specificationId,
      i.specificationVersion,
    );
  if (!spec || spec.status !== "approved")
    throw new ApiError(
      409,
      "PLANNING_SOURCE_CHANGED",
      "Choose a current approved specification.",
    );
  const warehouse = await write
    .prepare(
      "SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)",
    )
    .bind(i.warehouseId, a.companyId, a.branchId, a.branchId)
    .first<Row>();
  if (!warehouse)
    throw new ApiError(
      404,
      "WAREHOUSE_NOT_FOUND",
      "Warehouse not found in scope.",
    );
  const lines = (
      await write
        .prepare(
          "SELECT * FROM manufacturing_specification_lines WHERE specification_id=? ORDER BY ordinal",
        )
        .bind(i.specificationId)
        .all<Row>()
    ).results,
    demand = q(i.demandQuantity),
    factor = demand / Number(spec.batch_quantity_milli),
    shortages: Array<{
      productId: string;
      requiredMilli: number;
      availableMilli: number;
      shortageMilli: number;
    }> = [];
  for (const line of lines) {
    const required = Math.ceil(Number(line.quantity_milli) * factor),
      stock = await write
        .prepare(
          "SELECT quantity_milli-COALESCE((SELECT sum(quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) available FROM commerce_stock s WHERE warehouse_id=? AND product_id=?",
        )
        .bind(i.warehouseId, line.product_id)
        .first<{ available: number }>();
    shortages.push({
      productId: String(line.product_id),
      requiredMilli: required,
      availableMilli: Number(stock?.available ?? 0),
      shortageMilli: Math.max(0, required - Number(stock?.available ?? 0)),
    });
  }
  const requiredMinutes = Math.ceil(demand / 1000) * 10,
    days = Math.max(0, Math.ceil((requiredMinutes - i.capacityMinutes) / 480)),
    feasible = new Date(Date.parse(i.dueDate + "T00:00:00Z") + days * 86400000)
      .toISOString()
      .slice(0, 10),
    id = `plan-${key}`,
    recommendationId = `plan-recommendation-${key}`,
    op = "planning.scenario.create",
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    result = {
      id,
      recommendationId,
      status: "calculated",
      version: 1,
      shortages,
      feasibleDate: feasible,
    };
  return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO planning_scenarios VALUES(?,?,?,?,?,?,?,?,?,?, 'calculated',?,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        warehouse.branch_id,
        i.warehouseId,
        i.specificationId,
        i.specificationVersion,
        demand,
        i.dueDate,
        i.capacityMinutes,
        JSON.stringify(shortages),
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO planning_recommendations VALUES(?,?,?,?,?,?,'proposed',?,NULL,1,?,?)",
      )
      .bind(
        recommendationId,
        id,
        1,
        demand,
        requiredMinutes,
        feasible,
        a.userId,
        now,
        now,
      ),
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
  const i = recommendationSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    row = await write
      .prepare(
        "SELECT r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.created_by scenario_created_by FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)",
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
  const status = i.action === "approve" ? "approved" : "rejected",
    op = `planning.recommendation.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const now = Date.now(),
    steps: SqlStatement[] = [
      assertion(
        write,
        key,
        "recommendation",
        "EXISTS(SELECT 1 FROM planning_recommendations WHERE id=? AND version=? AND status='proposed')",
        [id, i.expectedVersion],
      ),
      write
        .prepare(
          "UPDATE planning_recommendations SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, a.userId, now, id),
    ];
  let orderId: null | string = null;
  if (i.action === "approve") {
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
        "SELECT o.*,s.output_product_id,s.batch_quantity_milli FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)",
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
          "SELECT * FROM manufacturing_specification_lines WHERE specification_id=? ORDER BY ordinal",
        )
        .bind(row.specification_id)
        .all<Row>()
    ).results,
    factor =
      Number(row.planned_quantity_milli) / Number(row.batch_quantity_milli),
    steps: SqlStatement[] = [];
  let status = "";
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
      const amount = Number(reservation.quantity_milli),
        value = Math.round(
          (Number(stock.value_minor) * amount) / Number(stock.quantity_milli),
        );
      material += value;
      steps.push(
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
      );
    }
    steps.push(
      write
        .prepare(
          "UPDATE manufacturing_orders SET material_cost_minor=? WHERE id=?",
        )
        .bind(material, id),
    );
  } else if (i.action === "report") {
    if (row.status !== "in_progress")
      throw new ApiError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "Report an in-progress order.",
      );
    status = "awaiting_qc";
    steps.push(
      write
        .prepare(
          "UPDATE manufacturing_orders SET reported_output_milli=?,reported_scrap_milli=?,labor_cost_minor=?,overhead_cost_minor=? WHERE id=?",
        )
        .bind(
          q(i.outputQuantity),
          q(i.scrapQuantity),
          decimalToMinor(i.laborCost),
          decimalToMinor(i.overheadCost),
          id,
        ),
    );
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
      .first("actor_id");
    if (String(reporter) === a.userId)
      throw new ApiError(
        409,
        "CONFLICT",
        "QC must be independent from output reporting.",
      );
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
  } else if (i.action === "fail-qc") {
    requirePermission(a, "records.review");
    if (row.status !== "awaiting_qc")
      throw new ApiError(
        409,
        "INVALID_MANUFACTURING_TRANSITION",
        "QC reviews reported output.",
      );
    status = "qc_failed";
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
    assertion(
      write,
      key,
      "order",
      "EXISTS(SELECT 1 FROM manufacturing_orders WHERE id=? AND version=? AND status=?)",
      [id, i.expectedVersion, row.status],
    ),
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
    { id, status, version: next },
    steps,
  );
}
export async function productionDetail(a: WorkContext, id: string) {
  access(a, "manufacturing");
  const { write } = await holdingResources(a.holdingId),
    order = await write
      .prepare(
        "SELECT o.*,s.name specification_name,s.mode,p.name output_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id JOIN commerce_products p ON p.id=s.output_product_id WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)",
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
          "SELECT r.*,p.name product_name FROM manufacturing_material_reservations r JOIN commerce_products p ON p.id=r.product_id WHERE r.order_id=? ORDER BY p.name",
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
