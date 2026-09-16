import { samePerson } from "@/lib/iam";
import { hasPermission } from "@/lib/permissions";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { requirePermission } from "./iam-permissions";
import { z } from "zod";
import { sha256 } from "./crypto";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { decimalToMinor } from "@/lib/finance-money";
import { quantityMilli, lineAmounts, scaledAmount } from "@/lib/commerce-math";

export type Row = Record<string, string | number | null>;
const amount = z
  .string()
  .regex(
    /^\d+(\.\d{1,2})?$/,
    "Use a positive amount with at most two decimal places.",
  );
const quantity = z
  .string()
  .regex(
    /^\d+(\.\d{1,3})?$/,
    "Use a quantity with at most three decimal places.",
  );
export const orderSchema = z.object({
  kind: z.enum(["quote", "sales", "purchase"]),
  requestId: z.string().min(1).max(180).optional(),
  requestVersion: z.number().int().positive().optional(),
  partyId: z.string().min(1),
  warehouseId: z.string().min(1),
  orderDate: z.iso.date(),
  dueDate: z.iso.date(),
  note: z.string().max(2000).default(""),
  lines: z
    .array(
      z.object({
        productId: z.string().min(1),
        quantity,
        price: amount,
        taxBps: z.number().int().min(0).max(10000),
      }),
    )
    .min(1)
    .max(20),
  expectedVersion: z.number().int().positive().optional(),
});
export const transitionSchema = z.object({
  action: z.enum(["submit", "approve", "return", "cancel", "convert"]),
  expectedVersion: z.number().int().positive(),
});
export const fulfillmentSchema = z.object({
  expectedVersion: z.number().int().positive(),
  date: z.iso.date(),
  lines: z
    .array(z.object({ lineId: z.string().min(1), quantity }))
    .min(1)
    .max(20),
});
export const invoiceFromDeliverySchema = z.object({
  expectedVersion: z.number().int().positive(),
  date: z.iso.date(),
  dueDate: z.iso.date(),
});
export const settlementSchema = z.object({
  paymentSlipId: z.string().min(1).optional(),
  expectedVersion: z.number().int().positive(),
  date: z.iso.date(),
  amount,
  reference: z.string().trim().min(2).max(100),
});
export const productSchema = z.object({
  sku: z.string().trim().min(2).max(40),
  name: z.string().trim().min(2).max(160),
  unit: z.string().trim().min(1).max(30).default("each"),
  price: amount,
  taxBps: z.number().int().min(0).max(10000).default(0),
  status: z.enum(["active", "archived"]).default("active"),
  expectedVersion: z.number().int().positive().optional(),
});
export const partyMasterSchema = z.object({
  kind: z.enum(["customer", "supplier"]),
  code: z.string().trim().min(2).max(40),
  name: z.string().trim().min(2).max(160),
  paymentTermsDays: z.number().int().min(0).max(365).default(30),
  status: z.enum(["active", "blocked"]).default("active"),
  expectedVersion: z.number().int().positive().optional(),
});
export const warehouseSchema = z.object({
  code: z.string().trim().min(2).max(40),
  name: z.string().trim().min(2).max(160),
  branchId: z.string().min(1),
  status: z.enum(["active", "archived"]).default("active"),
  expectedVersion: z.number().int().positive().optional(),
});
const roles = {sales:"commerce.sales",purchase:"commerce.purchase",stock:"inventory.prepare",finance:"finance.write",read:"commerce.read"} as const;
export function commercePermissions(actor: WorkContext) {
  return {
    sales: hasPermission(actor, roles.sales),
    purchase: hasPermission(actor, roles.purchase),
    stock: hasPermission(actor, roles.stock),
    finance: hasPermission(actor, roles.finance),
    read: hasPermission(actor, roles.read),
    approve: hasPermission(actor,"commerce.approve"),
  };
}
function allowed(actor: WorkContext, kind: keyof typeof roles) {
  return requirePermission(actor, roles[kind]);
}

function scope(actor: WorkContext, alias = "o") {
  return {
    sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`,
    args: [
      actor.companyId,
      actor.branchId,
      actor.branchId,
      actor.departmentId,
      actor.departmentId,
    ],
  };
}
async function settings(db: SqlDatabase, actor: WorkContext) {
  const row = await db
    .prepare("SELECT * FROM commerce_settings WHERE company_id=?")
    .bind(actor.companyId)
    .first<Row>();
  if (!row)
    throw new ApiError(
      409,
      "COMMERCE_SETUP_REQUIRED",
      "Configure the company posting accounts before processing orders.",
    );
  return row;
}
async function period(db: SqlDatabase, actor: WorkContext, date: string) {
  const rows = await db
    .prepare(
      "SELECT id FROM finance_periods WHERE company_id=? AND start_date<=? AND end_date>=? AND state='open'",
    )
    .bind(actor.companyId, date, date)
    .all<Row>();
  if (rows.results.length !== 1)
    throw new ApiError(
      409,
      "PERIOD_UNAVAILABLE",
      "Exactly one open accounting period is required for this date.",
    );
  return String(rows.results[0].id);
}
function versionGuard(
  db: SqlDatabase,
  key: string,
  table: string,
  id: string,
  version: number,
  states: string[],
) {
  return assertion(
    db,
    key,
    `version-${table}`,
    `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND ${table === "commerce_orders" ? "state" : "status"} IN (${states.map(() => "?").join(",")}))`,
    [id, version, ...states],
  );
}
async function orderRow(db: SqlDatabase, actor: WorkContext, id: string) {
  const s = scope(actor);
  const row = await db
    .prepare(
      `SELECT o.*,p.name party_name,w.name warehouse_name FROM commerce_orders o JOIN finance_parties p ON p.id=o.party_id JOIN commerce_warehouses w ON w.id=o.warehouse_id WHERE o.id=? AND ${s.sql}`,
    )
    .bind(id, ...s.args)
    .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "ORDER_NOT_FOUND",
      "Order not found in this workspace.",
    );
  return row;
}
export async function commerceOverview(actor: WorkContext) {
  actor = allowed(actor, "read");
  const { write } = await holdingResources(actor.holdingId);
  const s = scope(actor);
  const [orders, products, parties, warehouses, stock, moves, nodes, config] =
    await Promise.all([
      write
        .prepare(
          `SELECT o.*,p.name party_name FROM commerce_orders o JOIN finance_parties p ON p.id=o.party_id WHERE ${s.sql} ORDER BY o.created_at DESC LIMIT 250`,
        )
        .bind(...s.args)
        .all<Row>(),
      write
        .prepare(
          "SELECT * FROM commerce_products WHERE company_id=? ORDER BY sku",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT * FROM finance_parties WHERE company_id=? AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT * FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name",
        )
        .bind(actor.companyId, actor.branchId, actor.branchId)
        .all<Row>(),
      write
        .prepare(
          "SELECT s.*,p.sku,p.name,w.name warehouse_name FROM commerce_stock s JOIN commerce_products p ON p.id=s.product_id JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?) ORDER BY p.sku,w.name",
        )
        .bind(actor.companyId, actor.branchId, actor.branchId)
        .all<Row>(),
      write
        .prepare(
          "SELECT m.*,p.sku,w.name warehouse_name,m.source_number order_number,m.source_id order_id,m.movement_date fulfillment_date FROM inventory_ledger m JOIN commerce_products p ON p.id=m.product_id JOIN commerce_warehouses w ON w.id=m.warehouse_id WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?) ORDER BY m.created_at DESC LIMIT 100",
        )
        .bind(actor.companyId, actor.branchId, actor.branchId)
        .all<Row>(),
      write
        .prepare(
          "SELECT id,display_name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?)",
        )
        .bind(actor.companyId, actor.branchId, actor.branchId)
        .all<Row>(),
      settings(write, actor),
    ]);
  return {
    orders: orders.results,
    products: products.results,
    parties: parties.results,
    warehouses: warehouses.results,
    stock: stock.results,
    moves: moves.results,
    nodes: nodes.results,
    currency: String(config.currency),
  };
}
export async function getOrder(actor: WorkContext, id: string) {
  actor = allowed(actor, "read");
  const { write } = await holdingResources(actor.holdingId);
  const order = await orderRow(write, actor, id);
  const [lines, fulfillments, history, children] = await Promise.all([
    write
      .prepare(
        "SELECT l.*,p.sku FROM commerce_order_lines l JOIN commerce_products p ON p.id=l.product_id WHERE l.order_id=? ORDER BY l.ordinal",
      )
      .bind(id)
      .all<Row>(),
    write
      .prepare(
        "SELECT f.*,i.invoice_number,i.total_minor,i.outstanding_minor,i.status invoice_status FROM commerce_fulfillments f LEFT JOIN finance_invoices i ON i.id=f.invoice_id WHERE f.order_id=? ORDER BY f.created_at DESC",
      )
      .bind(id)
      .all<Row>(),
    write
      .prepare(
        "SELECT a.action,a.actor_id,COALESCE((SELECT legal_name FROM hr_people p WHERE p.user_id=a.actor_id),a.actor_id) actor_name,a.created_at,a.record_version FROM audit_events a WHERE a.record_id=? ORDER BY a.created_at DESC LIMIT 50",
      )
      .bind(id)
      .all<Row>(),
    write
      .prepare("SELECT id,number FROM commerce_orders WHERE source_id=?")
      .bind(id)
      .all<Row>(),
  ]);
  return {
    order,
    lines: lines.results,
    fulfillments: fulfillments.results,
    history: history.results,
    children: children.results,
  };
}
export async function saveOrder(
  actor: WorkContext,
  id: string | null,
  input: z.infer<typeof orderSchema>,
  key: string,
) {
  actor = allowed(actor, input.kind === "purchase" ? "purchase" : "sales");
  const { write } = await holdingResources(actor.holdingId);
  const orderId = id ?? `order-${key}`;
  const linked = id ? await write.prepare("SELECT id FROM employee_requests WHERE company_id=? AND purchase_order_id=?").bind(actor.companyId,id).first<{id:string}>() : null;
  const requestId = linked?.id ?? input.requestId;
  if (requestId) {
    const request = await (await import("./employee-requests")).getEmployeeRequest(write,actor,requestId);
    if(input.kind!=="purchase") throw new ApiError(422,"PURCHASE_REQUIRED","Resource requests hand off to purchase orders.");
    actor={...actor,branchId:request.branch_id,departmentId:request.department_id};
  }
  const config = await settings(write, actor);
  const previous = await replayCommand(
    write,
    actor,
    key,
    id ? "commerce.order.update" : "commerce.order.create",
    id ?? `order-${key}`,
    input,
  );
  if (previous) return previous;
  const handoff = requestId ? await (await import("./employee-requests")).prepareRequestOrderHandoff(write, actor, requestId, orderId, key, input.requestVersion, !!id) : null;
  const existing = id ? await orderRow(write, actor, id) : null;
  if (existing && existing.kind !== input.kind)
    throw new ApiError(
      422,
      "ORDER_KIND_LOCKED",
      "The document type cannot change.",
    );
  if (input.dueDate < input.orderDate)
    throw new ApiError(
      422,
      "INVALID_DATES",
      "Due date must be on or after order date.",
    );
  if (new Set(input.lines.map((l) => l.productId)).size !== input.lines.length)
    throw new ApiError(422, "DUPLICATE_PRODUCT", "Use one line per product.");
  const warehouse = await write
    .prepare(
      "SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)",
    )
    .bind(input.warehouseId, actor.companyId, actor.branchId, actor.branchId)
    .first<Row>();
  if (!warehouse)
    throw new ApiError(
      422,
      "WAREHOUSE_INVALID",
      "Choose an active warehouse in this workspace.",
    );
  const record = id ?? `order-${key}`;
  const version = existing ? (input.expectedVersion ?? 0) + 1 : 1;
  const now = Date.now();
  let subtotal = 0,
    tax = 0;
  const lineRows = [];
  for (const [index, line] of input.lines.entries()) {
    const product = await write
      .prepare(
        "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'",
      )
      .bind(line.productId, actor.companyId)
      .first<Row>();
    if (!product)
      throw new ApiError(422, "PRODUCT_INVALID", "Choose an active product.");
    const qty = quantityMilli(line.quantity);
    const price = decimalToMinor(line.price);
    const values = lineAmounts(qty, price, line.taxBps);
    subtotal += values.subtotal;
    tax += values.tax;
    lineRows.push({
      id: `${record}-line-${index}`,
      product,
      line,
      qty,
      price,
      ...values,
      index,
    });
  }
  if (!Number.isSafeInteger(subtotal + tax) || subtotal <= 0)
    throw new ApiError(
      422,
      "ORDER_AMOUNT_INVALID",
      "Order total must be positive and within supported limits.",
    );
  const steps = [
    assertion(
      write,
      key,
      "party",
      "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=? AND status='active' AND currency=? AND person_id IS NULL)",
      [
        input.partyId,
        actor.companyId,
        input.kind === "purchase" ? "supplier" : "customer",
        config.currency,
      ],
    ),
  ];
  if (existing) {
    steps.push(
      versionGuard(
        write,
        key,
        "commerce_orders",
        record,
        input.expectedVersion ?? 0,
        ["draft"],
      ),
    );
    steps.push(
      write
        .prepare(
          "UPDATE commerce_orders SET party_id=?,warehouse_id=?,branch_id=?,order_date=?,due_date=?,note=?,subtotal_minor=?,tax_minor=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          input.partyId,
          input.warehouseId,
          warehouse.branch_id,
          input.orderDate,
          input.dueDate,
          input.note,
          subtotal,
          tax,
          now,
          record,
        ),
    );
    steps.push(
      write
        .prepare("DELETE FROM commerce_order_lines WHERE order_id=?")
        .bind(record),
    );
  } else
    steps.push(
      write
        .prepare(
          "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          record,
          actor.companyId,
          warehouse.branch_id,
          actor.departmentId,
          input.kind,
          `${input.kind === "quote" ? "QT" : input.kind === "sales" ? "SO" : "PO"}-${(await sha256(key)).slice(0, 24).toUpperCase()}`,
          input.partyId,
          input.warehouseId,
          config.currency,
          input.orderDate,
          input.dueDate,
          input.note,
          subtotal,
          tax,
          actor.userId,
          now,
          now,
        ),
    );
  for (const l of lineRows) {
    steps.push(
      assertion(
        write,
        key,
        `product-${l.index}`,
        "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND version=? AND status='active')",
        [l.product.id, l.product.version],
      ),
    );
    steps.push(
      write
        .prepare(
          "INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          l.id,
          record,
          l.line.productId,
          l.product.name,
          l.qty,
          l.price,
          l.line.taxBps,
          l.subtotal,
          l.tax,
          l.index,
        ),
    );
  }
  if (handoff) steps.push(...handoff.steps);
  return executeCommand(
    write,
    actor,
    key,
    existing ? "commerce.order.update" : "commerce.order.create",
    input,
    record,
    version,
    { id: record, version },
    steps,
  );
}
export async function transitionOrder(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof transitionSchema>,
  key: string,
) {
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(
    write,
    actor,
    key,
    `commerce.order.${input.action}`,
    id,
    input,
  );
  if (previous) return previous;
  const row = await orderRow(write, actor, id);
  actor = allowed(actor, row.kind === "purchase" ? "purchase" : "sales");
  if (
    input.action === "approve" &&
    (!commercePermissions(actor).approve || samePerson(actor,row.created_by))
  )
    throw new ApiError(
      403,
      "INDEPENDENT_APPROVER_REQUIRED",
      "A different manager must approve this document.",
    );
  if (input.action === "convert" && row.kind !== "quote")
    throw new ApiError(
      422,
      "QUOTE_REQUIRED",
      "Only a quotation can be converted.",
    );
  const from =
    input.action === "submit" || input.action === "cancel"
      ? ["draft"]
      : input.action === "convert"
        ? ["confirmed"]
        : ["submitted"];
  const state =
    input.action === "submit"
      ? "submitted"
      : input.action === "approve"
        ? "confirmed"
        : input.action === "return"
          ? "draft"
          : input.action === "convert"
            ? "converted"
            : "cancelled";
  const steps = [
    versionGuard(
      write,
      key,
      "commerce_orders",
      id,
      input.expectedVersion,
      from,
    ),
    write
      .prepare(
        "UPDATE commerce_orders SET state=?,approved_by=?,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(
        state,
        input.action === "approve" ? actor.userId : row.approved_by,
        Date.now(),
        id,
      ),
  ];
  let salesId: string | null = null;
  if (input.action === "convert") {
    salesId = `order-${key}`;
    steps.push(
      write
        .prepare(
          "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,subtotal_minor,tax_minor,source_id,created_by,created_at,updated_at) SELECT ?,company_id,branch_id,department_id,'sales',?,party_id,warehouse_id,currency,order_date,due_date,note,subtotal_minor,tax_minor,id,?,?,? FROM commerce_orders WHERE id=?",
        )
        .bind(
          salesId,
          `SO-${(await sha256(key)).slice(0, 24).toUpperCase()}`,
          actor.userId,
          Date.now(),
          Date.now(),
          id,
        ),
    );
    steps.push(
      write
        .prepare(
          "INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) SELECT ?||'-'||ordinal,?,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal FROM commerce_order_lines WHERE order_id=?",
        )
        .bind(salesId, salesId, id),
    );
  }
  return executeCommand(
    write,
    actor,
    key,
    `commerce.order.${input.action}`,
    input,
    id,
    input.expectedVersion + 1,
    { id, state, version: input.expectedVersion + 1, salesId },
    steps,
  );
}

type Posting = { account: string; debit: number; credit: number };
function journalSteps(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  journalId: string,
  config: Row,
  periodId: string,
  date: string,
  source: string,
  sourceId: string,
  lines: Posting[],
  status: "draft" | "posted" = "posted",
) {
  const debit = lines.reduce((s, l) => s + l.debit, 0),
    credit = lines.reduce((s, l) => s + l.credit, 0);
  if (debit !== credit || !Number.isSafeInteger(debit))
    throw new ApiError(
      422,
      "UNBALANCED_JOURNAL",
      "The journal is not balanced.",
    );
  const now = Date.now();
  const steps = [
    assertion(
      db,
      key,
      `period-${journalId}`,
      "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",
      [periodId, actor.companyId, date, date],
    ),
    db
      .prepare(
        "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,1,?,?,?,?,?)",
      )
      .bind(
        journalId,
        actor.companyId,
        config.chart_id,
        periodId,
        `GL-${journalId.slice(-36)}`,
        date,
        config.currency,
        source,
        sourceId,
        `${source} ${sourceId}`,
        status,
        actor.userId,
        status === "posted" ? actor.userId : null,
        status === "posted" ? now : null,
        now,
        now,
      ),
  ];
  for (const [i, line] of lines.filter((l) => l.debit || l.credit).entries()) {
    steps.push(
      assertion(
        db,
        key,
        `account-${journalId}-${i}`,
        "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active')",
        [line.account, actor.companyId, config.chart_id],
      ),
    );
    steps.push(
      db
        .prepare(
          "INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          `${journalId}-${i}`,
          journalId,
          line.account,
          line.debit,
          line.credit,
          line.debit,
          line.credit,
          i,
        ),
    );
  }
  return steps;
}
export async function fulfillOrder(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof fulfillmentSchema>,
  key: string,
) {
  actor = allowed(actor, "stock");
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(
    write,
    actor,
    key,
    "commerce.order.fulfill",
    id,
    input,
  );
  if (previous) return previous;
  const order = await orderRow(write, actor, id);
  if (order.kind === "quote")
    throw new ApiError(
      422,
      "ORDER_REQUIRED",
      "Convert the quotation to a sales order first.",
    );
  const config = await settings(write, actor);
  const periodId = await period(write, actor, input.date);
  const fulfillment = `delivery-${key}`;
  const journal = `journal-${key}`;
  const incoming = order.kind === "purchase";
  if (new Set(input.lines.map((l) => l.lineId)).size !== input.lines.length)
    throw new ApiError(
      422,
      "DUPLICATE_LINE",
      "Each order line may appear once.",
    );
  const steps = [
    versionGuard(write, key, "commerce_orders", id, input.expectedVersion, [
      "confirmed",
      "partially_fulfilled",
    ]),
    write
      .prepare(
        "INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at) VALUES(?,?,?,?,?,?)",
      )
      .bind(
        fulfillment,
        id,
        `${incoming ? "GR" : "DN"}-${(await sha256(key)).slice(0, 24).toUpperCase()}`,
        input.date,
        actor.userId,
        Date.now(),
      ),
  ];
  let value = 0;
  for (const [i, entry] of input.lines.entries()) {
    const line = await write
      .prepare("SELECT * FROM commerce_order_lines WHERE id=? AND order_id=?")
      .bind(entry.lineId, id)
      .first<Row>();
    if (!line) throw new ApiError(422, "LINE_INVALID", "Order line not found.");
    const qty = quantityMilli(entry.quantity);
    const remaining =
      Number(line.quantity_milli) - Number(line.fulfilled_milli);
    if (qty > remaining)
      throw new ApiError(
        422,
        "OVER_FULFILLMENT",
        "Quantity exceeds the remaining order quantity.",
      );
    const stock = await write
      .prepare(
        "SELECT * FROM commerce_stock WHERE warehouse_id=? AND product_id=?",
      )
      .bind(order.warehouse_id, line.product_id)
      .first<Row>();
    if (!incoming && Number(stock?.quantity_milli ?? 0) < qty)
      throw new ApiError(
        409,
        "INSUFFICIENT_STOCK",
        "There is not enough stock in this warehouse. Receive the purchase first.",
      );
    const previous = await write
      .prepare(
        "SELECT COALESCE(SUM(subtotal_minor),0) subtotal,COALESCE(SUM(tax_minor),0) tax FROM commerce_fulfillment_lines WHERE order_line_id=?",
      )
      .bind(line.id)
      .first<{ subtotal: number; tax: number }>();
    const amounts =
      qty === remaining
        ? {
            subtotal:
              Number(line.subtotal_minor) - Number(previous?.subtotal ?? 0),
            tax: Number(line.tax_minor) - Number(previous?.tax ?? 0),
          }
        : lineAmounts(qty, Number(line.price_minor), Number(line.tax_bps));
    const cost = incoming
      ? amounts.subtotal
      : qty === Number(stock?.quantity_milli)
        ? Number(stock?.value_minor)
        : scaledAmount(
            Number(stock?.value_minor ?? 0),
            qty,
            Number(stock?.quantity_milli ?? 1),
          );
    value += cost;
    steps.push(
      assertion(
        write,
        key,
        `movement-date-${i}`,
        "NOT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)",
        [order.warehouse_id, line.product_id, input.date],
      ),
    );
    steps.push(
      assertion(
        write,
        key,
        `line-${i}`,
        "EXISTS(SELECT 1 FROM commerce_order_lines WHERE id=? AND fulfilled_milli=? AND quantity_milli-fulfilled_milli>=?)",
        [line.id, line.fulfilled_milli, qty],
      ),
    );
    if (stock)
      steps.push(
        assertion(
          write,
          key,
          `stock-${i}`,
          "EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND product_id=? AND version=?)",
          [order.warehouse_id, line.product_id, stock.version],
        ),
      );
    else
      steps.push(
        write
          .prepare(
            "INSERT INTO commerce_stock(warehouse_id,product_id) VALUES(?,?)",
          )
          .bind(order.warehouse_id, line.product_id),
      );
    steps.push(
      write
        .prepare(
          "UPDATE commerce_stock SET quantity_milli=quantity_milli+?,value_minor=value_minor+?,version=version+1 WHERE warehouse_id=? AND product_id=?",
        )
        .bind(
          incoming ? qty : -qty,
          incoming ? cost : -cost,
          order.warehouse_id,
          line.product_id,
        ),
    );
    steps.push(
      write
        .prepare(
          "UPDATE commerce_order_lines SET fulfilled_milli=fulfilled_milli+? WHERE id=?",
        )
        .bind(qty, line.id),
    );
    steps.push(
      write
        .prepare(
          "INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES(?,?,?,?,?,?,?)",
        )
        .bind(
          `${fulfillment}-${i}`,
          fulfillment,
          line.id,
          qty,
          cost,
          amounts.subtotal,
          amounts.tax,
        ),
    );
    steps.push(
      write
        .prepare("INSERT INTO commerce_stock_moves VALUES(?,?,?,?,?,?,?)")
        .bind(
          `${fulfillment}-${i}`,
          order.warehouse_id,
          line.product_id,
          fulfillment,
          incoming ? qty : -qty,
          incoming ? cost : -cost,
          Date.now(),
        ),
    );
  }
  steps.push(
    ...journalSteps(
      write,
      actor,
      key,
      journal,
      config,
      periodId,
      input.date,
      incoming ? "goods_receipt" : "delivery",
      fulfillment,
      [
        {
          account: String(
            incoming ? config.inventory_account_id : config.cogs_account_id,
          ),
          debit: value,
          credit: 0,
        },
        {
          account: String(
            incoming ? config.clearing_account_id : config.inventory_account_id,
          ),
          debit: 0,
          credit: value,
        },
      ],
      "draft",
    ),
  );
  steps.push(
    write
      .prepare("UPDATE commerce_fulfillments SET journal_id=? WHERE id=?")
      .bind(journal, fulfillment),
  );
  steps.push(
    write
      .prepare(
        "UPDATE commerce_orders SET state=CASE WHEN EXISTS(SELECT 1 FROM commerce_order_lines WHERE order_id=? AND fulfilled_milli<quantity_milli) THEN 'partially_fulfilled' ELSE 'fulfilled' END,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(id, Date.now(), id),
  );
  return executeCommand(
    write,
    actor,
    key,
    "commerce.order.fulfill",
    input,
    id,
    input.expectedVersion + 1,
    { id, fulfillmentId: fulfillment, version: input.expectedVersion + 1 },
    steps,
  );
}
export async function invoiceFulfillment(
  actor: WorkContext,
  fulfillmentId: string,
  input: z.infer<typeof invoiceFromDeliverySchema>,
  key: string,
) {
  actor = allowed(actor, "finance");
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(
    write,
    actor,
    key,
    "commerce.invoice.create",
    fulfillmentId,
    input,
  );
  if (previous) return previous;
  const f = await write
    .prepare("SELECT * FROM commerce_fulfillments WHERE id=?")
    .bind(fulfillmentId)
    .first<Row>();
  if (!f)
    throw new ApiError(
      404,
      "DELIVERY_NOT_FOUND",
      "Receipt or delivery not found.",
    );
  const order = await orderRow(write, actor, String(f.order_id));
  if (!await write.prepare("SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND status='posted'").bind(f.journal_id,actor.companyId).first())
    throw new ApiError(409,"STOCK_GL_REQUIRED","An accountant must review and post the goods movement GL before invoicing.");
  if (input.dueDate < input.date || input.date < String(f.fulfillment_date))
    throw new ApiError(
      422,
      "INVALID_DATES",
      "Invoice and due dates must follow the delivery date.",
    );
  const totals = await write
    .prepare(
      "SELECT SUM(subtotal_minor) subtotal,SUM(tax_minor) tax FROM commerce_fulfillment_lines WHERE fulfillment_id=?",
    )
    .bind(fulfillmentId)
    .first<{ subtotal: number; tax: number }>();
  if (!totals || !totals.subtotal)
    throw new ApiError(422, "EMPTY_DELIVERY", "There are no billable lines.");
  const config = await settings(write, actor);
  const invoice = `invoice-${key}`;
  const payable = order.kind === "purchase";
  const total = totals.subtotal + totals.tax;
  const steps = [
    assertion(
      write,
      key,
      "delivery",
      "EXISTS(SELECT 1 FROM commerce_fulfillments WHERE id=? AND version=? AND invoice_id IS NULL)",
      [fulfillmentId, input.expectedVersion],
    ),
    write
      .prepare(
        "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'commerce_fulfillment',?,'draft',?,?,?)",
      )
      .bind(
        invoice,
        actor.companyId,
        order.party_id,
        payable ? "payable" : "receivable",
        `${payable ? "BILL" : "INV"}-${(await sha256(key)).slice(0, 24).toUpperCase()}`,
        input.date,
        input.dueDate,
        config.currency,
        totals.subtotal,
        totals.tax,
        total,
        total,
        payable ? config.clearing_account_id : config.receivable_account_id,
        payable ? config.payable_account_id : config.revenue_account_id,
        payable ? config.input_tax_account_id : config.output_tax_account_id,
        fulfillmentId,
        actor.userId,
        Date.now(),
        Date.now(),
      ),
    write
      .prepare(
        "UPDATE commerce_fulfillments SET invoice_id=?,version=version+1 WHERE id=?",
      )
      .bind(invoice, fulfillmentId),
  ];
  return executeCommand(
    write,
    actor,
    key,
    "commerce.invoice.create",
    input,
    fulfillmentId,
    input.expectedVersion + 1,
    { id: String(order.id), invoiceId: invoice },
    steps,
  );
}
export async function settleInvoice(
  actor: WorkContext,
  invoiceId: string,
  input: z.infer<typeof settlementSchema>,
  key: string,
) {
  actor = allowed(actor, "finance");
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(
    write,
    actor,
    key,
    "commerce.invoice.settle",
    invoiceId,
    input,
  );
  if (previous) return previous;
  const invoice = await write
    .prepare(
      "SELECT * FROM finance_invoices WHERE id=? AND company_id=? AND source_type IN ('commerce_fulfillment','standalone','expense_claim')",
    )
    .bind(invoiceId, actor.companyId)
    .first<Row>();
  if (!invoice)
    throw new ApiError(
      404,
      "INVOICE_NOT_FOUND",
      "Connected workflow invoice not found.",
    );
  const fulfillment =
    invoice.source_type === "commerce_fulfillment"
      ? await write
          .prepare("SELECT order_id FROM commerce_fulfillments WHERE id=?")
          .bind(invoice.source_id)
          .first<{ order_id: string }>()
      : null;
  if (fulfillment) await orderRow(write, actor, fulfillment.order_id);
  else if (actor.branchId || actor.departmentId)
    throw new ApiError(
      403,
      "COMPANY_SCOPE_REQUIRED",
      "Standalone invoices require a holding-level finance assignment.",
    );
  if (
    invoice.kind === "payable" &&
    (!hasPermission(actor,"finance.pay") ||
      samePerson(actor,invoice.created_by))
  )
    throw new ApiError(
      403,
      "PAYMENT_APPROVER_REQUIRED",
      "A different finance approver or treasury operator must record the supplier payment.",
    );
  const expense =
    invoice.source_type === "expense_claim"
      ? await write
          .prepare(
            "SELECT x.*,p.user_id claimant_user_id FROM finance_expenses x JOIN hr_people p ON p.id=x.employee_id WHERE x.id=? AND x.invoice_id=? AND x.company_id=?",
          )
          .bind(invoice.source_id, invoiceId, actor.companyId)
          .first<Row>()
      : null;
  if (
    invoice.source_type === "expense_claim" &&
    (!expense ||
      [
        expense.created_by,
        expense.prepared_by,
        expense.claimant_user_id,
      ].includes(actor.userId))
  )
    throw new ApiError(
      403,
      "SELF_REIMBURSEMENT_FORBIDDEN",
      "A different finance approver or treasury operator must record reimbursement.",
    );
  if (expense?.workflow_version === 2)
    throw new ApiError(
      409,
      "EXPENSE_PAYMENT_REQUIRED",
      "Record payment from the expense claim and attach its bank slip.",
    );
  if(invoice.kind==="payable"&&!input.paymentSlipId)throw new ApiError(422,"PAYMENT_SLIP_REQUIRED","Upload the bank payment slip before recording payment.");
  const prepared = await prepareInvoiceSettlement(
    write,
    actor,
    invoice,
    expense,
    input,
    key,
  );
  if(invoice.kind==="payable")prepared.steps.unshift(
    assertion(write,key,"payment-slip","EXISTS(SELECT 1 FROM attachments WHERE id=? AND owner_type='invoice_payment' AND owner_id=? AND revoked_at IS NULL AND media_type IN ('image/jpeg','image/png','image/webp','application/pdf'))",[input.paymentSlipId ?? null,invoiceId]),
    assertion(write,key,"unique-payment-slip","NOT EXISTS(SELECT 1 FROM finance_payments p JOIN attachments used ON used.id=p.payment_slip_id JOIN attachments proposed ON proposed.id=? WHERE p.company_id=? AND used.checksum_sha256=proposed.checksum_sha256)",[input.paymentSlipId ?? null,actor.companyId]),
  );

  return executeCommand(
    write,
    actor,
    key,
    "commerce.invoice.settle",
    input,
    invoiceId,
    input.expectedVersion + 1,
    { ...prepared.result, id: fulfillment?.order_id ?? invoiceId },
    prepared.steps,
  );
}
export async function prepareInvoiceSettlement(
  write: SqlDatabase,
  actor: WorkContext,
  invoice: Row,
  expense: Row | null,
  input: z.infer<typeof settlementSchema>,
  key: string,
) {
  const invoiceId = String(invoice.id);
  if (input.date < String(invoice.invoice_date))
    throw new ApiError(
      422,
      "INVALID_PAYMENT_DATE",
      "Payment date cannot precede the invoice.",
    );
  const amountMinor = decimalToMinor(input.amount);
  if (amountMinor <= 0 || amountMinor > Number(invoice.outstanding_minor))
    throw new ApiError(
      422,
      "PAYMENT_EXCEEDS_BALANCE",
      "Amount must be positive and no greater than the outstanding balance.",
    );
  const config = await settings(write, actor),
    periodId = await period(write, actor, input.date);
  const payment = `payment-${key}`,
    journal = `journal-${key}`;
  const payable = invoice.kind === "payable";
  if (invoice.currency !== config.currency)
    throw new ApiError(
      409,
      "CURRENCY_MAPPING_REQUIRED",
      "Configure settlement accounts for this invoice currency.",
    );
  const lines = [
    {
      account: String(
        payable ? invoice.credit_account_id : config.cash_account_id,
      ),
      debit: amountMinor,
      credit: 0,
    },
    {
      account: String(
        payable ? config.cash_account_id : invoice.debit_account_id,
      ),
      debit: 0,
      credit: amountMinor,
    },
  ];
  const steps = [
    versionGuard(
      write,
      key,
      "finance_invoices",
      invoiceId,
      input.expectedVersion,
      ["posted", "partially_paid"],
    ),
    assertion(
      write,
      key,
      "balance",
      "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND outstanding_minor>=?)",
      [invoiceId, amountMinor],
    ),
    ...journalSteps(
      write,
      actor,
      key,
      journal,
      config,
      periodId,
      input.date,
      "commerce_payment",
      payment,
      lines,
    ),
    write
      .prepare(
        "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at,payment_slip_id) VALUES(?,?,?,?,?,?,?,?,0,?,'settled',?,?,?,?,?,?)",
      )
      .bind(
        payment,
        actor.companyId,
        invoice.party_id,
        payable ? "payment" : "receipt",
        input.reference,
        input.date,
        config.currency,
        amountMinor,
        config.cash_account_id,
        journal,
        actor.userId,
        payable ? actor.userId : null,
        Date.now(),
        Date.now(),
        input.paymentSlipId ?? null,
      ),
    write
      .prepare(
        "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)",
      )
      .bind(
        `allocation-${key}`,
        actor.companyId,
        payment,
        invoiceId,
        amountMinor,
        actor.userId,
        Date.now(),
      ),
    write
      .prepare(
        "UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(amountMinor, amountMinor, Date.now(), invoiceId),
  ];
  if (expense)
    steps.push(
      assertion(
        write,
        key,
        "expense-claimant",
        "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND user_id IS DISTINCT FROM ?)",
        [String(expense.employee_id), actor.userId],
      ),
      versionGuard(
        write,
        key,
        "finance_expenses",
        String(expense.id),
        Number(expense.version),
        ["posted"],
      ),
      assertion(
        write,
        key,
        "no-export",
        "NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE source_type='expense' AND source_id=? AND status!='cancelled')",
        [String(expense.id)],
      ),
      write
        .prepare(
          "UPDATE finance_expenses SET status=CASE WHEN ?=0 THEN 'reimbursed' ELSE 'posted' END,workflow_stage=CASE WHEN ?=0 THEN 'completed' ELSE 'payment' END,reimbursed_at=CASE WHEN ?=0 THEN ?::bigint ELSE NULL END,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          Number(invoice.outstanding_minor) - amountMinor,
          Number(invoice.outstanding_minor) - amountMinor,
          Number(invoice.outstanding_minor) - amountMinor,
          Date.now(),
          Date.now(),
          expense.id,
        ),
    );
  return {
    result: {
      id: invoiceId,
      paymentId: payment,
      outstandingMinor: Number(invoice.outstanding_minor) - amountMinor,
    },
    steps,
  };
}
export async function saveMaster(
  actor: WorkContext,
  kind: "products" | "parties" | "warehouses",
  id: string | null,
  input: unknown,
  key: string,
) {
  const { write } = await holdingResources(actor.holdingId);
  const config = await settings(write, actor);
  const record = id ?? `${kind}-${key}`;
  const previous = await replayCommand(
    write,
    actor,
    key,
    `commerce.${kind}.${id ? "update" : "create"}`,
    record,
    input,
  );
  if (previous) return previous;
  const steps: SqlStatement[] = [];
  let version = 1;
  if (kind === "products") {
    actor = allowed(actor, "stock");
    const v = productSchema.parse(input);
    version = id ? (v.expectedVersion ?? 0) + 1 : 1;
    if (id)
      steps.push(
        assertion(
          write,
          key,
          "master",
          "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND version=?)",
          [id, actor.companyId, v.expectedVersion ?? 0],
        ),
      );
    steps.push(
      id
        ? write
            .prepare(
              "UPDATE commerce_products SET sku=?,name=?,unit=?,price_minor=?,tax_bps=?,status=?,version=version+1 WHERE id=?",
            )
            .bind(
              v.sku,
              v.name,
              v.unit,
              decimalToMinor(v.price),
              v.taxBps,
              v.status,
              id,
            )
        : write
            .prepare(
              "INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status) VALUES(?,?,?,?,?,?,?,?)",
            )
            .bind(
              record,
              actor.companyId,
              v.sku,
              v.name,
              v.unit,
              decimalToMinor(v.price),
              v.taxBps,
              v.status,
            ),
    );
  } else if (kind === "parties") {
    const v = partyMasterSchema.parse(input);
    actor = allowed(actor, v.kind === "supplier" ? "purchase" : "sales");
    version = id ? (v.expectedVersion ?? 0) + 1 : 1;
    if (id)
      steps.push(
        assertion(
          write,
          key,
          "master",
          "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=? AND version=? AND person_id IS NULL)",
          [id, actor.companyId, v.kind, v.expectedVersion ?? 0],
        ),
      );
    steps.push(
      id
        ? write
            .prepare(
              "UPDATE finance_parties SET code=?,name=?,payment_terms_days=?,status=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(v.code, v.name, v.paymentTermsDays, v.status, Date.now(), id)
        : write
            .prepare(
              "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              record,
              actor.companyId,
              v.kind,
              v.code,
              v.name,
              config.currency,
              v.paymentTermsDays,
              v.status,
              Date.now(),
              Date.now(),
            ),
    );
  } else {
    actor = allowed(actor, "stock");
    const v = warehouseSchema.parse(input);
    version = id ? (v.expectedVersion ?? 0) + 1 : 1;
    if (actor.branchId && v.branchId !== actor.branchId)
      throw new ApiError(
        403,
        "SCOPE_FORBIDDEN",
        "Choose your assigned branch.",
      );
    steps.push(
      assertion(
        write,
        key,
        "branch",
        "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",
        [v.branchId, actor.companyId],
      ),
    );
    if (id) {
      steps.push(
        assertion(
          write,
          key,
          "master",
          "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND version=?)",
          [id, actor.companyId, v.branchId, v.expectedVersion ?? 0],
        ),
      );
      if (v.status === "archived")
        steps.push(
          assertion(
            write,
            key,
            "stock-empty",
            "NOT EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND quantity_milli>0)",
            [id],
          ),
        );
    }
    steps.push(
      id
        ? write
            .prepare(
              "UPDATE commerce_warehouses SET code=?,name=?,status=?,version=version+1 WHERE id=?",
            )
            .bind(v.code, v.name, v.status, id)
        : write
            .prepare(
              "INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,?)",
            )
            .bind(
              record,
              actor.companyId,
              v.branchId,
              v.code,
              v.name,
              v.status,
            ),
    );
  }
  return executeCommand(
    write,
    actor,
    key,
    `commerce.${kind}.${id ? "update" : "create"}`,
    input,
    record,
    version,
    { id: record, version },
    steps,
  );
}
