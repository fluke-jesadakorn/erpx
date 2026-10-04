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
import { availableServiceSpares, consumptionLotSchema, prepareInventoryConsumption } from "./inventory-consumption";
import { snapshotMoney } from "./company-money";
const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive(),
  date = z.string().date();
const createAgreementSchema = z.object({
  branchId: z.string().min(1),
  departmentId: z.string().nullable().optional(),
  customerId: z.string().min(1),
  assetId: z.string().nullable().optional(),
  title: z.string().trim().min(3).max(160),
  startDate: date,
  endDate: date,
  currency: z
    .string()
    .length(3)
    .transform((x) => x.toUpperCase()),
  visitRateMinor: z.number().int().nonnegative(),
  warrantyDiscountBps: z.number().int().min(0).max(10000).default(0),
});
const agreementActionSchema = z.object({
  action: z.enum(["activate", "reject", "retire"]),
  expectedVersion: version,
  reason,
});
const createOrderSchema = z.object({
  ticketId: z.string().min(1),
  ticketVersion: version,
  agreementId: z.string().min(1),
  agreementVersion: version,
  assetId: z.string().nullable().optional(),
  issue: reason,
  site: z.string().trim().min(3).max(300),
  technicianId: z.string().min(1),
  scheduledAt: z.number().int().positive().nullable().optional(),
});
const orderActionSchema = z.discriminatedUnion("action", [
  z.object({
    action: z.literal("schedule"),
    expectedVersion: version,
    technicianId: z.string().min(1),
    scheduledAt: z.number().int().positive(),
    reason,
  }),
  z.object({
    action: z.literal("reserve-part"),
    expectedVersion: version,
    warehouseId: z.string().min(1),
    productId: z.string().min(1),
    quantityMilli: z.number().int().positive(),
    expiresAt: z.number().int().positive(),
    reason,
  }),
  z.object({
    action: z.literal("issue-part"),
    expectedVersion: version,
    reservationId: z.string().min(1),
    quantityMilli: z.number().int().positive(),
    entryDate: date.optional(),
    allocations: z.array(consumptionLotSchema).max(200).default([]),
    reason,
  }),
  z.object({
    action: z.enum(["dispatch", "start", "resume", "cancel"]),
    expectedVersion: version,
    reason,
  }),
  z.object({
    action: z.literal("exception"),
    expectedVersion: version,
    reason,
  }),
  z.object({
    action: z.literal("complete"),
    expectedVersion: version,
    laborMinutes: z.number().int().nonnegative(),
    evidence: z.string().trim().min(3).max(4000),
    customerConfirmation: z.string().trim().min(3).max(4000),
    reason,
  }),
  z.object({
    action: z.literal("release-billing"),
    expectedVersion: version,
    reason,
  }),
]);
const aScope =
    "g.company_id=? AND (?::text IS NULL OR g.branch_id=?) AND (?::text IS NULL OR g.department_id=?)",
  oScope =
    "o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)",
  args = (a: WorkContext) => [
    a.companyId,
    a.branchId,
    a.branchId,
    a.departmentId,
    a.departmentId,
  ];
const technician = `EXISTS(SELECT 1 FROM assignments x JOIN hr_people p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=o.company_id AND x.user_id=o.technician_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=o.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=o.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`;
const reviewer = (alias: "g" | "o") =>
  `EXISTS(SELECT 1 FROM assignments x JOIN hr_people p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=${alias}.company_id AND x.user_id=? AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.review') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=${alias}.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=${alias}.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`;
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "field_service"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Field Service.",
    );
}
type Agreement = Row & {
  id: string;
  version: number;
  status: string;
  created_by: string;
  updated_at: number;
  can_review: number;
  warranty_discount_bps: number;
};
type Order = Row & {
  id: string;
  version: number;
  status: string;
  technician_id: string;
  created_by: string;
  updated_at: number;
  scheduled_at: number | null;
  technician_eligible: number;
  can_review: number;
  issue: string;
  site: string;
  exception_reason: string | null;
};
export async function fieldServiceWorkspace(a: WorkContext) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    agreements = (
      await write
        .prepare(
          `SELECT g.*,p.name customer_name,a.code asset_code,CASE WHEN ${reviewer("g")} THEN 1 ELSE 0 END can_review FROM field_service_agreements g JOIN finance_parties p ON p.id=g.customer_id LEFT JOIN operational_assets a ON a.id=g.asset_id WHERE ${aScope} ORDER BY g.updated_at DESC,g.id`,
        )
        .bind(a.userId, ...args(a))
        .all<Agreement>()
    ).results,
    orders = (
      await write
        .prepare(
          `SELECT o.*,t.title ticket_title,p.name customer_name,g.title agreement_title,a.code asset_code,CASE WHEN ${technician} THEN 1 ELSE 0 END technician_eligible,CASE WHEN ${reviewer("o")} THEN 1 ELSE 0 END can_review,(SELECT legal_name FROM hr_people WHERE company_id=o.company_id AND user_id=o.technician_id AND merged_into_id IS NULL LIMIT 1) technician_name FROM field_service_orders o JOIN support_tickets t ON t.id=o.ticket_id JOIN finance_parties p ON p.id=t.customer_id JOIN field_service_agreements g ON g.id=o.agreement_id LEFT JOIN operational_assets a ON a.id=o.asset_id WHERE ${oScope} ORDER BY o.updated_at DESC,o.id`,
        )
        .bind(a.userId, ...args(a))
        .all<Order>()
    ).results,
    customers = (
      await write
        .prepare(
          "SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    assets = (
      await write
        .prepare(
          "SELECT id,code,name,version FROM operational_assets WHERE company_id=? AND lifecycle_status<>'retired' ORDER BY code",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    tickets = (
      await write
        .prepare(
          "SELECT id,title,version,customer_id,branch_id,department_id FROM support_tickets WHERE company_id=? AND source_kind='sales_order' AND status IN ('open','in_progress','waiting_customer') ORDER BY updated_at DESC",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    people = (
      await write
        .prepare(
          "SELECT DISTINCT p.user_id id,p.legal_name name FROM hr_people p JOIN assignments x ON x.user_id=p.user_id AND x.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') ORDER BY p.legal_name",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    nodes = (
      await write
        .prepare(
          "SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN ('branch','headquarters','department') ORDER BY kind,display_name",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results;
  return {
    agreements,
    orders,
    customers,
    assets,
    tickets,
    people,
    nodes,
    canWrite: hasPermission(a, "records.write"),
  };
}
export async function fieldServiceDetail(a: WorkContext, id: string) {
  const data = await fieldServiceWorkspace(a),
    order = data.orders.find((x) => x.id === id);
  if (!order)
    throw new ApiError(
      404,
      "FIELD_SERVICE_ORDER_NOT_FOUND",
      "Service order not found in this scope.",
    );
  const { write } = await holdingResources(a.holdingId);
  return {
    ...data,
    order,
    spares:await availableServiceSpares(write,a,String(order.branch_id)),
    parts: (
      await write
        .prepare(
          "SELECT r.*,p.sku,p.name FROM field_service_part_reservations r JOIN commerce_products p ON p.id=r.product_id WHERE r.order_id=? ORDER BY r.created_at,r.id",
        )
        .bind(id)
        .all<Row>()
    ).results,
    consumptions:(await write.prepare("SELECT p.*,j.status journal_status,product.name product_name FROM operational_inventory_consumptions p JOIN finance_journals j ON j.id=p.journal_id JOIN commerce_products product ON product.id=p.product_id WHERE p.source_kind='field_service' AND p.source_id=? ORDER BY p.created_at,p.id").bind(id).all<Row>()).results,
    history: (
      await write
        .prepare(
          "SELECT * FROM field_service_order_events WHERE order_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()
    ).results,
    billing: await write
      .prepare("SELECT * FROM field_service_billing_releases WHERE order_id=?")
      .bind(id)
      .first<Row>(),
  };
}
export async function createAgreement(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const input = createAgreementSchema.parse(raw),
    branch = a.branchId ?? input.branchId,
    department = a.departmentId ?? input.departmentId ?? null;
  if (input.endDate < input.startDate)
    throw new ApiError(
      422,
      "FIELD_SERVICE_DATE_RANGE",
      "Agreement end date must be on or after its start date.",
    );
  const { write } = await holdingResources(a.holdingId),
    id = `service-agreement-${key}`,
    op = "field.agreement.create",
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now=Date.now(),money=await snapshotMoney(write,a,{sourceType:'field_service_agreement',sourceId:id,amountKind:'visit_rate',transactionDate:input.startDate,currency:input.currency,currencyScale:2,amountUnits:String(input.visitRateMinor)});
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    1,
    { id, version: 1, status: "draft" },
    [
      money.statement,
      assertion(
        write,
        key,
        "branch",
        "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind IN ('branch','headquarters') AND status='active')",
        [branch, a.companyId],
      ),
      ...(department
        ? [
            assertion(
              write,
              key,
              "department",
              "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND branch_id=? AND status='active')",
              [department, a.companyId, branch],
            ),
          ]
        : []),
      assertion(
        write,
        key,
        "customer",
        "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND currency=?)",
        [input.customerId, a.companyId, input.currency],
      ),
      ...(input.assetId
        ? [
            assertion(
              write,
              key,
              "asset",
              "EXISTS(SELECT 1 FROM operational_assets WHERE id=? AND company_id=? AND branch_id=? AND lifecycle_status<>'retired')",
              [input.assetId, a.companyId, branch],
            ),
          ]
        : []),
      write
        .prepare(
          "INSERT INTO field_service_agreements(id,company_id,branch_id,department_id,customer_id,asset_id,title,start_date,end_date,currency,visit_rate_minor,warranty_discount_bps,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          a.companyId,
          branch,
          department,
          input.customerId,
          input.assetId ?? null,
          input.title,
          input.startDate,
          input.endDate,
          input.currency,
          input.visitRateMinor,
          input.warrantyDiscountBps,
          a.userId,
          now,
          now,
        ),
      write
        .prepare(
          "INSERT INTO field_service_agreement_events VALUES(?,?,1,'create',?,'Agreement drafted for independent review','draft',?)",
        )
        .bind(`service-agreement-event-${key}`, id, a.userId, now),
    ],
  );
}
export async function changeAgreement(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const input = agreementActionSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    op = `field.agreement.${input.action}`,
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const row = await write
    .prepare(
      `SELECT g.* FROM field_service_agreements g WHERE g.id=? AND ${aScope}`,
    )
    .bind(id, ...args(a))
    .first<Agreement>();
  if (!row)
    throw new ApiError(
      404,
      "FIELD_SERVICE_AGREEMENT_NOT_FOUND",
      "Agreement not found.",
    );
  requirePermission(a, "records.review");
  const expected = input.action === "retire" ? "active" : "draft",
    status =
      input.action === "activate"
        ? "active"
        : input.action === "reject"
          ? "rejected"
          : "retired",
    now = Date.now();
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1, status },
    [
      assertion(
        write,
        key,
        "review",
        `EXISTS(SELECT 1 FROM field_service_agreements g WHERE g.id=? AND g.version=? AND g.status=? AND g.created_by<>? AND ${reviewer("g")})`,
        [id, input.expectedVersion, expected, a.userId, a.userId],
      ),
      ...(input.action === "retire"
        ? [
            assertion(
              write,
              key,
              "no-open-orders",
              "NOT EXISTS(SELECT 1 FROM field_service_orders WHERE agreement_id=? AND status NOT IN ('completed','cancelled','billed'))",
              [id],
            ),
          ]
        : []),
      write
        .prepare(
          "UPDATE field_service_agreements SET status=?,reviewed_by=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, a.userId, now, now, id),
      write
        .prepare(
          "INSERT INTO field_service_agreement_events VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          `service-agreement-event-${key}`,
          id,
          input.expectedVersion + 1,
          input.action,
          a.userId,
          input.reason,
          status,
          now,
        ),
    ],
  );
}
export async function createServiceOrder(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const input = createOrderSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `service-order-${key}`,
    op = "field.order.create",
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now = Date.now(),
    status = input.scheduledAt ? "scheduled" : "open";
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    1,
    { id, version: 1, status },
    [
      assertion(
        write,
        key,
        "source",
        `EXISTS(SELECT 1 FROM support_tickets t JOIN field_service_agreements g ON g.id=? AND g.customer_id=t.customer_id AND g.company_id=t.company_id AND g.branch_id=t.branch_id AND g.department_id IS NOT DISTINCT FROM t.department_id WHERE t.id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND g.version=? AND g.status='active' AND g.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND g.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?))`,
        [
          input.agreementId,
          input.ticketId,
          input.ticketVersion,
          input.agreementVersion,
          ...args(a),
        ],
      ),
      ...(input.assetId
        ? [
            assertion(
              write,
              key,
              "asset",
              "EXISTS(SELECT 1 FROM operational_assets a JOIN field_service_agreements g ON g.id=? WHERE a.id=? AND a.company_id=g.company_id AND a.branch_id=g.branch_id AND (g.asset_id IS NULL OR g.asset_id=a.id) AND a.lifecycle_status NOT IN ('retired','retirement_pending'))",
              [input.agreementId, input.assetId],
            ),
          ]
        : []),
      write
        .prepare(
          "INSERT INTO field_service_orders(id,company_id,branch_id,department_id,ticket_id,ticket_version,agreement_id,agreement_version,asset_id,issue,site,technician_id,scheduled_at,status,created_by,created_at,updated_at) SELECT ?,company_id,branch_id,department_id,id,?,?,?,?,?,?,?,?,?,?,?,? FROM support_tickets WHERE id=?",
        )
        .bind(
          id,
          input.ticketVersion,
          input.agreementId,
          input.agreementVersion,
          input.assetId ?? null,
          input.issue,
          input.site,
          input.technicianId,
          input.scheduledAt ?? null,
          status,
          a.userId,
          now,
          now,
          input.ticketId,
        ),
      assertion(
        write,
        key,
        "technician",
        `EXISTS(SELECT 1 FROM field_service_orders o WHERE o.id=? AND ${technician})`,
        [id],
      ),
      write
        .prepare(
          "INSERT INTO field_service_order_events VALUES(?,?,1,'create',?,?,?,?,?,?::jsonb,?)",
        )
        .bind(
          `service-order-event-${key}`,
          id,
          a.userId,
          input.issue,
          status,
          input.technicianId,
          input.scheduledAt ?? null,
          JSON.stringify({ site: input.site }),
          now,
        ),
      write
        .prepare(
          "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,technician_id,'field_service_order',id,version,'New field service order: review appointment and required context',updated_at FROM field_service_orders WHERE id=?",
        )
        .bind(`service-order-notice-${key}`, id),
    ],
  );
}
export async function changeServiceOrder(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const input = orderActionSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    op = `field.order.${input.action}`,
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const row = await write
    .prepare(
      `SELECT o.*,g.visit_rate_minor,g.warranty_discount_bps,g.currency,g.status agreement_status,a.warranty_end,CASE WHEN ${technician} THEN 1 ELSE 0 END technician_eligible,CASE WHEN ${reviewer("o")} THEN 1 ELSE 0 END can_review FROM field_service_orders o JOIN field_service_agreements g ON g.id=o.agreement_id LEFT JOIN operational_assets a ON a.id=o.asset_id WHERE o.id=? AND ${oScope}`,
    )
    .bind(a.userId, id, ...args(a))
    .first<
      Order & {
        agreement_id: string;
        visit_rate_minor: number;
        warranty_discount_bps: number;
        warranty_end: string | null;
        currency: string;
        agreement_status: string;
        ticket_id: string;
        customer_id: string;
      }
    >();
  if (!row)
    throw new ApiError(
      404,
      "FIELD_SERVICE_ORDER_NOT_FOUND",
      "Service order not found.",
    );
  const manage =
      hasPermission(a, "records.review") && Number(row.can_review) > 0,
    own = row.technician_id === a.userId && Number(row.technician_eligible) > 0;
  if (!own && !manage)
    throw new ApiError(
      403,
      "FIELD_SERVICE_OWNER_REQUIRED",
      "The technician or an eligible reviewer must perform this action.",
    );
  const allowed: Record<string, string[]> = {
      schedule: ["open", "scheduled"],
      reserve_part: ["open", "scheduled"],
      dispatch: ["scheduled"],
      start: ["dispatched"],
      exception: ["dispatched", "in_visit"],
      resume: ["exception"],
      issue_part: ["in_visit"],
      complete: ["in_visit"],
      release_billing: ["completed"],
      cancel: ["open", "scheduled", "dispatched", "exception"],
    },
    actionKey = input.action.replaceAll("-", "_");
  if (!allowed[actionKey]?.includes(row.status))
    throw new ApiError(
      409,
      "FIELD_SERVICE_STATE_CHANGED",
      "Refresh the service order before continuing.",
    );
  const now = Date.now(),
    steps = [
      assertion(
        write,
        key,
        "version",
        `EXISTS(SELECT 1 FROM field_service_orders o WHERE o.id=? AND o.version=? AND ${oScope})`,
        [id, input.expectedVersion, ...args(a)],
      ),
    ];
  let status = row.status,
    tech = row.technician_id,
    scheduled = row.scheduled_at,
    exceptionReason = row.exception_reason,
    details: Record<string, unknown> = {};
  if (input.action === "schedule") {
    status = "scheduled";
    tech = input.technicianId;
    scheduled = input.scheduledAt;
    steps.push(
      assertion(
        write,
        key,
        "technician",
        `EXISTS(SELECT 1 FROM field_service_orders o WHERE o.id=? AND ${technician.replaceAll("o.technician_id", "?::text")})`,
        [id, input.technicianId],
      ),
    );
  } else if (input.action === "reserve-part") {
    if (input.expiresAt <= now)
      throw new ApiError(
        422,
        "FIELD_SERVICE_PART_EXPIRY",
        "Part reservation must expire in the future.",
      );
    details = {
      warehouseId: input.warehouseId,
      productId: input.productId,
      quantityMilli: input.quantityMilli,
    };
    steps.push(
      write
        .prepare(
          "UPDATE commerce_stock SET version=version+1 WHERE warehouse_id=? AND product_id=?",
        )
        .bind(input.warehouseId, input.productId),
      assertion(
        write,
        key,
        "available",
        "EXISTS(SELECT 1 FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE s.warehouse_id=? AND s.product_id=? AND w.company_id=? AND w.branch_id=? AND s.quantity_milli-COALESCE((SELECT SUM(quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)>=? AND NOT EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status NOT IN ('released','cancelled','disposed')))",
        [
          input.warehouseId,
          input.productId,
          a.companyId,
          row.branch_id,
          input.quantityMilli,
        ],
      ),
      write
        .prepare(
          "INSERT INTO field_service_part_reservations(id,order_id,warehouse_id,product_id,quantity_milli,expires_at,created_by,reason,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          `field-part-${key}`,
          id,
          input.warehouseId,
          input.productId,
          input.quantityMilli,
          input.expiresAt,
          a.userId,
          input.reason,
          now,
        ),
    );
  } else if (input.action === "dispatch") {
    status = "dispatched";
    steps.push(
      assertion(
        write,
        key,
        "parts-current",
        "NOT EXISTS(SELECT 1 FROM field_service_part_reservations WHERE order_id=? AND quantity_milli>consumed_milli+released_milli AND expires_at<=?)",
        [id, now],
      ),
    );
  } else if (input.action === "start") {
    status = "in_visit";
    steps.push(
      write
        .prepare(
          "UPDATE field_service_orders SET visit_started_at=? WHERE id=?",
        )
        .bind(now, id),
    );
  } else if (input.action === "exception") {
    status = "exception";
    exceptionReason = input.reason;
  } else if (input.action === "resume") {
    status = "in_visit";
    exceptionReason = null;
  } else if(input.action==='issue-part'){
    const reservation=await write.prepare('SELECT * FROM field_service_part_reservations WHERE id=? AND order_id=?').bind(input.reservationId,id).first<Row>();
    if(!reservation)throw new ApiError(404,'FIELD_SERVICE_PART_NOT_FOUND','Select a part reserved for this service order.');
    const consumption=await prepareInventoryConsumption(write,a,{key,sourceKind:'field_service',sourceId:id,sourceVersion:input.expectedVersion+1,branchId:String(row.branch_id),departmentId:row.department_id?String(row.department_id):null,
      warehouseId:String(reservation.warehouse_id),productId:String(reservation.product_id),quantityMilli:input.quantityMilli,entryDate:input.entryDate??new Date(now).toISOString().slice(0,10),reason:input.reason,
      ownReservationMilli:Number(reservation.quantity_milli)-Number(reservation.consumed_milli)-Number(reservation.released_milli),allocations:input.allocations,
      beforeStock:[assertion(write,key,'part',"EXISTS(SELECT 1 FROM field_service_part_reservations WHERE id=? AND order_id=? AND quantity_milli-consumed_milli-released_milli>=? AND expires_at>?)",[input.reservationId,id,input.quantityMilli,now]),write.prepare('UPDATE field_service_part_reservations SET consumed_milli=consumed_milli+? WHERE id=?').bind(input.quantityMilli,input.reservationId)]});
    details={reservationId:input.reservationId,quantityMilli:input.quantityMilli,consumptionId:consumption.id,costMinor:consumption.valueMinor,journalId:consumption.journalId};
    steps.push(...consumption.steps);
  } else if (input.action === "complete") {
    steps.push(assertion(write,key,'parts-gl-posted',"NOT EXISTS(SELECT 1 FROM operational_inventory_consumptions p JOIN finance_journals j ON j.id=p.journal_id WHERE p.source_kind='field_service' AND p.source_id=? AND j.status<>'posted')",[id]));
    status = "completed";
    details = {
      laborMinutes: input.laborMinutes,
      evidence: input.evidence,
      customerConfirmation: input.customerConfirmation,
    };
    steps.push(
      write
        .prepare(
          "UPDATE field_service_orders SET visit_completed_at=?,labor_minutes=?,completion_evidence=?,customer_confirmation=?,exception_reason=NULL WHERE id=?",
        )
        .bind(
          now,
          input.laborMinutes,
          input.evidence,
          input.customerConfirmation,
          id,
        ),
      write
        .prepare(
          "UPDATE field_service_part_reservations SET released_milli=quantity_milli-consumed_milli,release_reason='Visit completed' WHERE order_id=? AND quantity_milli>consumed_milli+released_milli",
        )
        .bind(id),
    );
  } else if (input.action === "release-billing") {
    if (!manage || row.technician_id === a.userId)
      throw new ApiError(
        409,
        "INDEPENDENT_BILLING_RELEASE_REQUIRED",
        "A different eligible reviewer must release billing.",
      );
    status = "billed";
    const invoiceDate = new Date(now).toISOString().slice(0, 10),
      warrantyDiscountBps = Math.max(0, Math.min(10000, Number(row.warranty_discount_bps ?? 0))),
      warrantyApplied = Boolean(row.warranty_end && String(row.warranty_end) >= invoiceDate),
      amountMinor = warrantyApplied
        ? Math.floor((Number(row.visit_rate_minor) * (10000 - warrantyDiscountBps)) / 10000)
        : Number(row.visit_rate_minor);
    const postingAccounts = await write
      .prepare(
        `SELECT ar.id receivable_account_id,rev.id revenue_account_id
         FROM finance_accounts ar
         JOIN finance_charts c ON c.id=ar.chart_id AND c.company_id=ar.company_id
         JOIN finance_accounts rev ON rev.chart_id=ar.chart_id AND rev.company_id=ar.company_id
         WHERE ar.company_id=? AND ar.status='active' AND ar.control_kind='receivable'
           AND rev.status='active' AND rev.category='revenue' AND ar.currency=? AND rev.currency=? AND c.base_currency=?
         ORDER BY ar.code,rev.code LIMIT 1`,
      )
      .bind(a.companyId, row.currency, row.currency, row.currency)
      .first<{ receivable_account_id: string; revenue_account_id: string }>();
    if (!postingAccounts)
      throw new ApiError(
        422,
        "FIELD_SERVICE_FINANCE_SETUP_REQUIRED",
        "Configure an active receivable control account and revenue account in Finance before releasing field service billing.",
      );
    steps.push(
      (await snapshotMoney(write,a,{sourceType:'field_service_billing_release',sourceId:`field-billing-${key}`,amountKind:'invoice_total',transactionDate:invoiceDate,currency:String(row.currency),currencyScale:2,amountUnits:String(amountMinor)})).statement,
      assertion(
        write,
        key,
        "completion",
        "EXISTS(SELECT 1 FROM field_service_order_events WHERE order_id=? AND action='complete' AND actor_id<>?)",
        [id, a.userId],
      ),
      assertion(
        write,
        key,
        "agreement",
        "EXISTS(SELECT 1 FROM field_service_agreements WHERE id=? AND status='active')",
        [row.agreement_id],
      ),
      assertion(
        write,
        key,
        "finance-accounts",
        "EXISTS(SELECT 1 FROM finance_accounts ar JOIN finance_accounts rev ON rev.chart_id=ar.chart_id AND rev.company_id=ar.company_id JOIN finance_charts c ON c.id=ar.chart_id WHERE ar.id=? AND rev.id=? AND ar.company_id=? AND ar.status='active' AND ar.control_kind='receivable' AND rev.status='active' AND rev.category='revenue' AND ar.currency=? AND rev.currency=? AND c.base_currency=?)",
        [
          postingAccounts.receivable_account_id,
          postingAccounts.revenue_account_id,
          a.companyId,
          row.currency,
          row.currency,
          row.currency,
        ],
      ),
      write
        .prepare(
          "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,created_by,created_at,updated_at) SELECT ?,o.company_id,t.customer_id,'receivable',?,?,?,?,?,0,?,?,?,?,'field_service',?,'draft',?,?,? FROM field_service_orders o JOIN support_tickets t ON t.id=o.ticket_id WHERE o.id=?",
        )
        .bind(
          `field-invoice-${key}`,
          `FS-${id}`,
          invoiceDate,
          new Date(now + 30 * 86400000).toISOString().slice(0, 10),
          row.currency,
          amountMinor,
          amountMinor,
          amountMinor,
          postingAccounts.receivable_account_id,
          postingAccounts.revenue_account_id,
          id,
          a.userId,
          now,
          now,
          id,
        ),
      write
        .prepare(
          "INSERT INTO field_service_billing_releases(id,order_id,agreement_id,customer_id,currency,amount_minor,warranty_applied,warranty_discount_bps,warranty_end_snapshot,pricing_date,source_order_version,released_by,reason,created_at) SELECT ?,o.id,o.agreement_id,t.customer_id,?,?,?,?,?,?,?, ?,?,? FROM field_service_orders o JOIN support_tickets t ON t.id=o.ticket_id WHERE o.id=?",
        )
        .bind(
          `field-billing-${key}`,
          row.currency,
          amountMinor,
          warrantyApplied,
          warrantyDiscountBps,
          row.warranty_end ?? null,
          invoiceDate,
          input.expectedVersion + 1,
          a.userId,
          input.reason,
          now,
          id,
        ),
    );
  } else {
    status = "cancelled";
    steps.push(
      assertion(write,key,'parts-gl-posted',"NOT EXISTS(SELECT 1 FROM operational_inventory_consumptions p JOIN finance_journals j ON j.id=p.journal_id WHERE p.source_kind='field_service' AND p.source_id=? AND j.status<>'posted')",[id]),
      write
        .prepare(
          "UPDATE field_service_part_reservations SET released_milli=quantity_milli-consumed_milli,release_reason='Service order cancelled' WHERE order_id=? AND quantity_milli>consumed_milli+released_milli",
        )
        .bind(id),
    );
  }
  steps.push(
    write
      .prepare(
        "UPDATE field_service_orders SET status=?,technician_id=?,scheduled_at=?,exception_reason=?,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(status, tech, scheduled, exceptionReason, now, id),
    write
      .prepare(
        "INSERT INTO field_service_order_events VALUES(?,?,?,?,?,?,?,?,?,?::jsonb,?)",
      )
      .bind(
        `service-order-event-${key}`,
        id,
        input.expectedVersion + 1,
        input.action,
        a.userId,
        input.reason,
        status,
        tech,
        scheduled,
        JSON.stringify(details),
        now,
      ),
    write
      .prepare(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,technician_id,'field_service_order',id,version,'Field service order updated: review current next action',updated_at FROM field_service_orders WHERE id=?",
      )
      .bind(`service-order-notice-${key}`, id),
  );
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1, status },
    steps,
  );
}
export async function fieldServiceWork(a: WorkContext): Promise<WorkItem[]> {
  if (
    !hasPermission(a, "records.read") ||
    !hasModuleAccess(a.modules, "field_service")
  )
    return [];
  const data = await fieldServiceWorkspace(a),
    agreementWork = data.agreements
      .filter((g) => g.status === "draft")
      .map((g) => ({
        id: String(g.id),
        type: "field_service" as const,
        title: `Agreement review · ${g.title}`,
        detail: `${g.customer_name} · ${g.start_date} to ${g.end_date}`,
        href: "/field-service",
        status: String(g.status),
        updatedAt: Number(g.updated_at),
        queue:
          Number(g.can_review) && g.created_by !== a.userId
            ? ("todo" as const)
            : ("waiting" as const),
        canAct: Boolean(Number(g.can_review) && g.created_by !== a.userId),
        responsibleQueue: "Independent service manager",
        relationships: [
          g.created_by === a.userId
            ? ("requested" as const)
            : ("approval" as const),
        ],
      })),
    orders = data.orders.map((o) => {
      const terminal = ["cancelled", "billed"].includes(o.status),
        manage = hasPermission(a, "records.review") && Number(o.can_review) > 0,
        own = o.technician_id === a.userId && Number(o.technician_eligible) > 0,
        canAct = !terminal && (own || manage);
      return {
        id: String(o.id),
        type: "field_service" as const,
        title: `Field visit · ${o.ticket_title}`,
        detail: `${o.site} · ${o.customer_name}`,
        href: `/field-service/${o.id}`,
        status: String(o.status),
        updatedAt: Number(o.updated_at),
        dueAt: o.scheduled_at ? Number(o.scheduled_at) : null,
        queue: terminal
          ? ("completed" as const)
          : canAct
            ? ("todo" as const)
            : ("waiting" as const),
        canAct,
        responsibleQueue:
          o.status === "completed"
            ? "Independent billing reviewer"
            : "Assigned technician",
        nextAction:
          o.status === "completed"
            ? "Review visit evidence and release billing"
            : o.status === "exception"
              ? "Resolve site exception"
              : "Continue scheduled field visit",
        blocker: Number(o.technician_eligible)
          ? o.exception_reason
          : "Assigned technician is no longer eligible; reviewer reassignment is required",
        relationships: [
          o.technician_id === a.userId
            ? ("assigned" as const)
            : o.created_by === a.userId
              ? ("requested" as const)
              : ("team" as const),
        ],
      };
    });
  return [...agreementWork, ...orders];
}
