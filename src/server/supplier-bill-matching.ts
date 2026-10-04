import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import type { WorkItem } from "@/lib/work-item";
import type { Row, invoiceFromDeliverySchema } from "./commerce";
import type { JournalLineInput } from "./finance";
import { getOrder } from "./commerce";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import { quantityMilli, lineAmounts } from "@/lib/commerce-math";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { authorityGuard, companyAuthority } from "./staff-policy";

const reason = z.string().trim().min(3).max(2000);
const version = z.number().int().positive();
const receiptInputSchema = z.object({ fulfillmentId: z.string().min(1), expectedVersion: version });
const billLineSchema = z.object({
  fulfillmentLineId: z.string().min(1),
  quantity: z.string().regex(/^\d{1,12}(\.\d{1,3})?$/),
  price: z.string().regex(/^\d{1,12}(\.\d{1,2})?$/),
  taxBps: z.number().int().min(0).max(10000),
});

/** Legacy single-receipt input remains accepted while the UI uses receipts[]. */
export const supplierBillSchema = z.object({
  fulfillmentId: z.string().min(1).optional(),
  expectedVersion: version.optional(),
  receipts: z.array(receiptInputSchema).min(1).max(50).optional(),
  reference: z.string().trim().min(2).max(160),
  invoiceDate: z.iso.date(),
  dueDate: z.iso.date(),
  evidence: reason,
  lines: z.array(billLineSchema).min(1).max(200),
}).superRefine((input, context) => {
  const legacy = Boolean(input.fulfillmentId || input.expectedVersion);
  if (legacy && (!input.fulfillmentId || !input.expectedVersion)) {
    context.addIssue({ code: "custom", path: ["fulfillmentId"], message: "Receipt and version must be supplied together." });
  }
  if (Boolean(input.receipts) === legacy) {
    context.addIssue({ code: "custom", path: ["receipts"], message: "Use either receipts or one legacy receipt, not both." });
  }
});
export const supplierBillDecisionSchema = z.object({
  action: z.enum(["approve", "return"]),
  expectedVersion: version,
  reason,
  varianceAccountId: z.string().min(1).optional(),
});
export const supplierInvoicePolicySchema = z.object({
  matchingMode: z.enum(["optional", "required"]),
  expectedVersion: z.number().int().nonnegative(),
  reason,
});

const scope = "m.company_id=? AND (?::text IS NULL OR m.branch_id=?) AND (?::text IS NULL OR m.department_id=?)";
const args = (actor: WorkContext) => [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];

export async function getSupplierBill(actor: WorkContext, id: string) {
  requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const row = await write.prepare(`
    SELECT m.*,p.name supplier_name,o.number order_number,f.version fulfillment_version,
      i.status invoice_status,i.created_by invoice_created_by,i.version invoice_version,
      (SELECT count(*) FROM procurement_bill_match_receipts br WHERE br.match_id=m.id) receipt_count
    FROM procurement_bill_matches m
    JOIN finance_parties p ON p.id=m.supplier_id
    JOIN commerce_orders o ON o.id=m.order_id
    JOIN commerce_fulfillments f ON f.id=m.fulfillment_id
    LEFT JOIN finance_invoices i ON i.id=m.invoice_id
    WHERE m.id=? AND ${scope}
  `).bind(id, ...args(actor)).first<Row>();
  if (!row) throw new ApiError(404, "SUPPLIER_BILL_NOT_FOUND", "Supplier invoice review not found in this scope.");
  return row;
}

function event(db: SqlDatabase, actor: WorkContext, id: string, key: string, nextVersion: number, action: string, note: string) {
  return db.prepare("INSERT INTO procurement_bill_match_events VALUES(?,?,?,?,?,?,?)")
    .bind(`bill-event-${key}`, id, nextVersion, action, actor.userId, note, Date.now());
}

function notice(db: SqlDatabase, id: string, key: string) {
  return db.prepare(`
    INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT DISTINCT ?||'-'||a.user_id,m.company_id,a.user_id,'supplier_bill',m.id,m.version,
      CASE m.status
        WHEN 'returned' THEN 'Supplier invoice returned: obtain corrected evidence and resubmit from the receipt'
        WHEN 'approved' THEN 'Supplier invoice matched: assigned PO owner can prepare AP'
        WHEN 'invoiced' THEN 'Matched AP draft needs independent Finance posting'
        WHEN 'posted' THEN 'Matched supplier invoice posted to GL'
        ELSE 'Supplier invoice needs independent quantity and price review'
      END,m.updated_at
    FROM procurement_bill_matches m
    JOIN assignments a ON a.company_id=m.company_id
      AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(a.role,a.company_id,'finance.write')
      AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=m.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=m.department_id))
    WHERE m.id=? ON CONFLICT DO NOTHING
  `).bind(key, id);
}

export async function supplierBillWorkspace(actor: WorkContext) {
  requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  return (await write.prepare(`
    SELECT m.*,p.name supplier_name,o.number order_number,
      COALESCE((SELECT json_agg(br.fulfillment_id ORDER BY f.fulfillment_date,f.id)::text
        FROM procurement_bill_match_receipts br JOIN commerce_fulfillments f ON f.id=br.fulfillment_id
        WHERE br.match_id=m.id),'[]') receipt_ids_json,
      (SELECT count(*) FROM procurement_bill_match_receipts br WHERE br.match_id=m.id) receipt_count
    FROM procurement_bill_matches m
    JOIN finance_parties p ON p.id=m.supplier_id
    JOIN commerce_orders o ON o.id=m.order_id
    WHERE ${scope} ORDER BY m.updated_at DESC
  `).bind(...args(actor)).all<Row>()).results;
}

/**
 * Posted, unbilled purchase receipts that Finance can combine into one
 * supplier invoice.  This is intentionally a separate read model from the
 * review queue: a consolidated invoice may span several purchase orders,
 * while each receipt still retains its own version and goods GL evidence.
 */
export async function supplierBillCaptureOptions(actor: WorkContext) {
  requirePermission(actor, "finance.write");
  requirePermission(actor, "commerce.read");
  const { write } = await holdingResources(actor.holdingId);
  const receipts = (await write.prepare(`
    SELECT f.*,o.number order_number,o.party_id supplier_id,o.currency order_currency,
      o.branch_id,o.department_id,p.name supplier_name,j.status stock_journal_status,
      j.currency journal_currency,j.chart_id journal_chart_id
    FROM commerce_fulfillments f
    JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=? AND o.kind='purchase'
    JOIN finance_parties p ON p.id=o.party_id
    JOIN finance_journals j ON j.id=f.journal_id AND j.status='posted'
    WHERE f.invoice_id IS NULL
      AND (?::text IS NULL OR o.branch_id=?)
      AND (?::text IS NULL OR o.department_id=?)
      AND NOT EXISTS (
        SELECT 1 FROM procurement_bill_match_receipts br
        WHERE br.fulfillment_id=f.id AND br.active=TRUE
      )
    ORDER BY p.name,o.number,f.fulfillment_date,f.id
  `).bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<Row>()).results;
  if (!receipts.length) return { fulfillments: [], lines: [], currency: null };
  const ids = receipts.map(receipt => String(receipt.id));
  const placeholders = ids.map(() => "?").join(",");
  const lines = (await write.prepare(`
    SELECT fl.*,ol.product_id,ol.description,p.sku,p.unit,p.tracking_mode
    FROM commerce_fulfillment_lines fl
    JOIN commerce_order_lines ol ON ol.id=fl.order_line_id
    JOIN commerce_products p ON p.id=ol.product_id
    WHERE fl.fulfillment_id IN (${placeholders})
    ORDER BY fl.fulfillment_id,ol.ordinal
  `).bind(...ids).all<Row>()).results;
  return { fulfillments: receipts, lines, currency: String(receipts[0].order_currency) };
}

export async function supplierInvoicePolicy(actor: WorkContext) {
  requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const row = await write.prepare(`
    SELECT p.*,COALESCE(person.legal_name,p.updated_by) updated_by_name
    FROM procurement_invoice_policies p
    LEFT JOIN hr_people person ON person.company_id=p.company_id AND person.user_id=p.updated_by AND person.merged_into_id IS NULL
    WHERE p.company_id=?
  `).bind(actor.companyId).first<Row>();
  const canManage = hasPermission(actor, "finance.approve") && await companyAuthority(write, actor, "finance.approve");
  const events = canManage ? (await write.prepare(`
    SELECT e.*,COALESCE(person.legal_name,e.actor_id) actor_name
    FROM procurement_invoice_policy_events e
    LEFT JOIN hr_people person ON person.company_id=e.company_id AND person.user_id=e.actor_id AND person.merged_into_id IS NULL
    WHERE e.company_id=? ORDER BY e.version DESC
  `).bind(actor.companyId).all<Row>()).results : [];
  return {
    matchingMode: String(row?.matching_mode ?? "optional") as "optional" | "required",
    version: Number(row?.version ?? 0),
    updatedByName: row?.updated_by_name ? String(row.updated_by_name) : null,
    updatedAt: row?.updated_at ? Number(row.updated_at) : null,
    canManage,
    events,
  };
}

export async function saveSupplierInvoicePolicy(actor: WorkContext, raw: unknown, key: string) {
  const input = supplierInvoicePolicySchema.parse(raw);
  requirePermission(actor, "finance.approve");
  const { write } = await holdingResources(actor.holdingId);
  const operation = "procurement.invoice-policy.update", recordId = actor.companyId;
  const prior = await replayCommand(write, actor, key, operation, recordId, input);
  if (prior) return prior;
  if (!await companyAuthority(write, actor, "finance.approve")) throw new ApiError(403, "COMPANY_FINANCE_AUTHORITY_REQUIRED", "A company-wide Finance approver or owner must change supplier invoice matching policy.");
  const current = await write.prepare("SELECT matching_mode,version FROM procurement_invoice_policies WHERE company_id=?").bind(actor.companyId).first<Row>();
  const version = Number(current?.version ?? 0);
  if (version !== input.expectedVersion) throw new ApiError(409, "STALE_POLICY", "The supplier invoice policy changed. Reload and review its current version.");
  const previous = String(current?.matching_mode ?? "optional") as "optional" | "required";
  if (previous === input.matchingMode) throw new ApiError(409, "POLICY_UNCHANGED", `Supplier invoice matching is already ${previous}.`);
  const nextVersion = version + 1, now = Date.now();
  return executeCommand(write, actor, key, operation, input, recordId, nextVersion, { id: recordId, version: nextVersion, matchingMode: input.matchingMode }, [
    authorityGuard(write, key, actor, "finance.approve"),
    assertion(write, key, "policy-version", input.expectedVersion === 0 ? "NOT EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=?)" : "EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND version=?)", input.expectedVersion === 0 ? [actor.companyId] : [actor.companyId, input.expectedVersion]),
    write.prepare(`
      INSERT INTO procurement_invoice_policies(company_id,matching_mode,version,updated_by,updated_at) VALUES(?,?,?,?,?)
      ON CONFLICT(company_id) DO UPDATE SET matching_mode=excluded.matching_mode,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at
    `).bind(actor.companyId, input.matchingMode, nextVersion, actor.userId, now),
    write.prepare("INSERT INTO procurement_invoice_policy_events(id,company_id,version,previous_mode,matching_mode,actor_id,reason,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`supplier-policy-${key}`, actor.companyId, nextVersion, previous, input.matchingMode, actor.userId, input.reason, now),
  ], { companyId: actor.companyId, branchId: null, departmentId: null });
}

export async function supplierBillDetail(actor: WorkContext, id: string) {
  const bill = await getSupplierBill(actor, id);
  const { write } = await holdingResources(actor.holdingId);
  const [lines, receipts, events, accounts] = await Promise.all([
    write.prepare(`
      SELECT l.*,ol.description,fl.fulfillment_id,f.number receipt_number,f.fulfillment_date,o.number order_number
      FROM procurement_bill_match_lines l
      JOIN commerce_fulfillment_lines fl ON fl.id=l.fulfillment_line_id
      JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id
      JOIN commerce_orders o ON o.id=f.order_id
      JOIN commerce_order_lines ol ON ol.id=fl.order_line_id
      WHERE l.match_id=? ORDER BY f.fulfillment_date,f.id,ol.ordinal
    `).bind(id).all<Row>(),
    write.prepare(`
      SELECT br.*,f.number receipt_number,f.fulfillment_date,f.version current_version,f.invoice_id,o.id order_id,o.number order_number
      FROM procurement_bill_match_receipts br JOIN commerce_fulfillments f ON f.id=br.fulfillment_id
      JOIN commerce_orders o ON o.id=f.order_id
      WHERE br.match_id=? ORDER BY f.fulfillment_date,f.id
    `).bind(id).all<Row>(),
    write.prepare("SELECT * FROM procurement_bill_match_events WHERE match_id=? ORDER BY version DESC").bind(id).all<Row>(),
    write.prepare("SELECT a.id,a.code,a.name FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active' ORDER BY a.code")
      .bind(actor.companyId, bill.chart_id).all<Row>(),
  ]);
  const flow = hasPermission(actor, "commerce.read")
    ? await (await import("./flow-routing")).resolveFlow(write, actor, "order", String(bill.order_id))
    : null;
  return {
    bill,
    lines: lines.results,
    receipts: receipts.results,
    events: events.results,
    accounts: accounts.results,
    canReview: hasPermission(actor, "finance.approve") && hasPermission(actor, "finance.write") &&
      !samePerson(actor, bill.created_by) && (bill.status !== "invoiced" || bill.invoice_status === "draft" && !samePerson(actor, bill.invoice_created_by)),
    canPrepare: hasPermission(actor, "finance.write") && flow?.steps.find(step => step.key === "invoicing")?.primary?.userId === actor.userId,
  };
}

export async function captureSupplierBill(actor: WorkContext, raw: unknown, key: string) {
  const input = supplierBillSchema.parse(raw);
  requirePermission(actor, "finance.write");
  requirePermission(actor, "commerce.read");
  const { write } = await holdingResources(actor.holdingId);
  const id = `bill-match-${key}`;
  const operation = "procurement.bill.capture";
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;

  const requested = input.receipts ?? [{ fulfillmentId: input.fulfillmentId!, expectedVersion: input.expectedVersion! }];
  if (new Set(requested.map(receipt => receipt.fulfillmentId)).size !== requested.length) throw new ApiError(422, "DUPLICATE_RECEIPT", "Select each goods receipt once.");
  if (new Set(input.lines.map(line => line.fulfillmentLineId)).size !== input.lines.length) throw new ApiError(422, "DUPLICATE_BILL_LINE", "Match each receipt line once.");

  const receiptRows: Row[] = [];
  for (const selected of requested) {
    const receipt = await write.prepare(`
      SELECT f.*,j.status journal_status,j.currency journal_currency,j.chart_id journal_chart_id
      FROM commerce_fulfillments f LEFT JOIN finance_journals j ON j.id=f.journal_id WHERE f.id=?
    `).bind(selected.fulfillmentId).first<Row>();
    if (!receipt) throw new ApiError(404, "RECEIPT_NOT_FOUND", "Goods receipt not found.");
    receiptRows.push(receipt);
  }
  receiptRows.sort((left, right) => String(left.fulfillment_date).localeCompare(String(right.fulfillment_date)) || String(left.id).localeCompare(String(right.id)));
  const primary = receiptRows[0];
  // Load every source order through the normal authorization path.  A bill
  // may span multiple POs, but only when they are the same supplier, branch,
  // department and currency so the resulting AP document remains scoped and
  // posts to one controlled chart.
  const sourceOrders = await Promise.all(
    [...new Set(receiptRows.map(receipt => String(receipt.order_id)))].map(orderId => getOrder(actor, orderId)),
  );
  const ordersById = new Map(sourceOrders.map(source => [String(source.order.id), source.order]));
  const order = ordersById.get(String(primary.order_id));
  if (!order || order.kind !== "purchase" || receiptRows.some(receipt => receipt.invoice_id)) {
    throw new ApiError(409, "UNBILLED_PURCHASE_REQUIRED", "Select posted, unbilled purchase receipts.");
  }
  if (receiptRows.some(receipt => {
    const candidate = ordersById.get(String(receipt.order_id));
    return !candidate || candidate.kind !== "purchase" || candidate.party_id !== order.party_id ||
      candidate.currency !== order.currency || candidate.branch_id !== order.branch_id ||
      candidate.department_id !== order.department_id;
  })) {
    throw new ApiError(409, "CONSOLIDATED_BILL_SCOPE_MISMATCH", "A consolidated supplier invoice must use the same supplier, currency, branch and department across its purchase orders.");
  }
  if (input.invoiceDate < String(receiptRows.at(-1)!.fulfillment_date) || input.dueDate < input.invoiceDate) throw new ApiError(422, "BILL_DATES_INVALID", "Invoice and due dates must follow every selected receipt.");

  const config = await write.prepare("SELECT * FROM commerce_settings WHERE company_id=?").bind(actor.companyId).first<Row>();
  if (!config) throw new ApiError(409, "BILL_POSTING_PROFILE_REQUIRED", "Review the Company posting profile before matching supplier invoices.");
  let clearing: string | null = null;
  for (const receipt of receiptRows) {
    if (receipt.journal_status !== "posted" || receipt.journal_currency !== order.currency || receipt.journal_chart_id !== config.chart_id) throw new ApiError(409, "BILL_POSTING_PROFILE_REQUIRED", "Post every selected receipt GL with a compatible Company posting profile first.");
    const postings = (await write.prepare("SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal").bind(receipt.journal_id).all<Row>()).results;
    const receiptClearing = postings.find(line => Number(line.credit_minor) > 0)?.account_id;
    if (postings.length !== 2 || !receiptClearing || clearing && clearing !== receiptClearing) throw new ApiError(409, "BILL_SOURCE_POSTING_INVALID", "Selected receipts must use the same reviewed goods-receipt clearing account.");
    clearing = String(receiptClearing);
  }

  const receiptIds = receiptRows.map(receipt => String(receipt.id));
  const placeholders = receiptIds.map(() => "?").join(",");
  const actual = (await write.prepare(`
    SELECT fl.*,ol.product_id,ol.description,p.sku,p.unit,p.tracking_mode
    FROM commerce_fulfillment_lines fl
    JOIN commerce_order_lines ol ON ol.id=fl.order_line_id
    JOIN commerce_products p ON p.id=ol.product_id
    WHERE fl.fulfillment_id IN (${placeholders})
    ORDER BY fl.fulfillment_id,ol.ordinal
  `).bind(...receiptIds).all<Row>()).results;
  if (input.lines.length !== actual.length || input.lines.some(line => !actual.some(sourceLine => sourceLine.id === line.fulfillmentLineId))) throw new ApiError(422, "BILL_COMPLETE_RECEIPT_REQUIRED", "Match every line from every selected receipt exactly once.");
  const prepared = input.lines.map((line, index) => {
    const sourceLine = actual.find(candidate => candidate.id === line.fulfillmentLineId);
    const quantity = quantityMilli(line.quantity);
    const price = decimalToMinor(line.price);
    if (!sourceLine || quantity <= 0) throw new ApiError(422, "BILL_LINE_INVALID", "Use selected receipt lines and positive invoiced quantities.");
    return { index, line: sourceLine, quantity, price, taxBps: line.taxBps, ...lineAmounts(quantity, price, line.taxBps) };
  });
  const subtotal = prepared.reduce((total, line) => total + line.subtotal, 0);
  const tax = prepared.reduce((total, line) => total + line.tax, 0);
  const expected = actual.reduce((total, line) => total + Number(line.subtotal_minor), 0);
  const quantityMatches = prepared.every(line => line.quantity === Number(line.line.quantity_milli));
  if (!Number.isSafeInteger(subtotal + tax) || subtotal + tax <= 0 || prepared.some(line => line.subtotal <= 0)) throw new ApiError(422, "BILL_AMOUNT_INVALID", "The supplier invoice must have a positive supported total.");

  const now = Date.now();
  const steps: SqlStatement[] = [write.prepare(`
    INSERT INTO procurement_bill_matches(id,company_id,branch_id,department_id,order_id,fulfillment_id,supplier_id,currency,chart_id,clearing_account_id,payable_account_id,tax_account_id,reference,invoice_date,due_date,evidence,expected_subtotal_minor,subtotal_minor,tax_minor,quantity_matches,status,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'review',?,?,?)
  `).bind(id, actor.companyId, order.branch_id, order.department_id, order.id, primary.id, order.party_id, order.currency, config.chart_id, clearing, config.payable_account_id, config.input_tax_account_id, input.reference, input.invoiceDate, input.dueDate, input.evidence, expected, subtotal, tax, quantityMatches, actor.userId, now, now)];
  for (const [index, receipt] of receiptRows.entries()) {
    const selected = requested.find(candidate => candidate.fulfillmentId === receipt.id)!;
    steps.push(
      assertion(write, key, `receipt-${index}`, "EXISTS(SELECT 1 FROM commerce_fulfillments f JOIN finance_journals j ON j.id=f.journal_id WHERE f.id=? AND f.order_id=? AND f.version=? AND f.invoice_id IS NULL AND j.status='posted')", [receipt.id, receipt.order_id, selected.expectedVersion]),
      write.prepare("INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version) VALUES(?,?,?,?)").bind(`${id}-receipt-${index}`, id, receipt.id, selected.expectedVersion + 1),
      write.prepare("UPDATE commerce_fulfillments SET version=version+1 WHERE id=?").bind(receipt.id),
    );
  }
  for (const line of prepared) steps.push(write.prepare("INSERT INTO procurement_bill_match_lines VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`${id}-${line.index}`, id, line.line.id, line.quantity, line.line.quantity_milli, line.price, line.taxBps, line.line.subtotal_minor, line.subtotal, line.tax));
  steps.push(event(write, actor, id, key, 1, "capture", input.evidence), notice(write, id, key));
  return executeCommand(write, actor, key, operation, input, id, 1, { id, version: 1, receiptCount: receiptRows.length }, steps, { companyId: actor.companyId, branchId: String(order.branch_id), departmentId: order.department_id ? String(order.department_id) : null });
}

export async function decideSupplierBill(actor: WorkContext, id: string, raw: unknown, key: string) {
  const input = supplierBillDecisionSchema.parse(raw);
  requirePermission(actor, "finance.write");
  requirePermission(actor, "finance.approve");
  const { write } = await holdingResources(actor.holdingId);
  const operation = `procurement.bill.${input.action}`;
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;
  const bill = await getSupplierBill(actor, id);
  if (samePerson(actor, bill.created_by) || bill.status === "invoiced" && samePerson(actor, bill.invoice_created_by)) throw new ApiError(403, "INDEPENDENT_BILL_REVIEW_REQUIRED", "A different Finance reviewer must accept or return the supplier invoice and its AP draft.");
  const returnDraft = input.action === "return" && bill.status === "invoiced" && bill.invoice_status === "draft";
  if (bill.status !== "review" && !(input.action === "return" && bill.status === "approved") && !returnDraft) throw new ApiError(409, "BILL_REVIEW_REQUIRED", "Only unposted supplier invoices can be returned; posted invoices require the source credit workflow.");
  const variance = await write.prepare("SELECT id FROM procurement_bill_match_lines WHERE match_id=? AND subtotal_minor<>expected_subtotal_minor LIMIT 1").bind(id).first();
  const steps: SqlStatement[] = [assertion(write, key, "bill-review", `EXISTS(SELECT 1 FROM procurement_bill_matches m WHERE m.id=? AND m.version=? AND m.status=? AND ${scope})`, [id, input.expectedVersion, bill.status, ...args(actor)])];
  if (input.action === "approve") {
    if (!Number(bill.quantity_matches)) throw new ApiError(409, "BILL_QUANTITY_VARIANCE", "Invoiced quantities differ from the selected receipts. Return the invoice for correction; do not approve unreceived quantities.");
    if (variance) {
      if (!input.varianceAccountId) throw new ApiError(422, "PRICE_VARIANCE_ACCOUNT_REQUIRED", "Choose the Company-approved expense account for purchase price variance.");
      const account = await write.prepare("SELECT a.id FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active'").bind(input.varianceAccountId, actor.companyId, bill.chart_id).first();
      if (!account) throw new ApiError(422, "PRICE_VARIANCE_ACCOUNT_INVALID", "Use an active expense account in this Company chart's functional currency.");
      steps.push(assertion(write, key, "variance-account", "EXISTS(SELECT 1 FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active')", [input.varianceAccountId, actor.companyId, bill.chart_id]));
    }
  }
  if (returnDraft) {
    steps.push(
      assertion(write, key, "unposted-ap", `EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=? AND i.company_id=? AND i.status='draft' AND i.version=? AND i.journal_id IS NULL AND i.source_type='commerce_fulfillment' AND NOT EXISTS(SELECT 1 FROM finance_allocations WHERE invoice_id=i.id) AND NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts br JOIN commerce_fulfillments f ON f.id=br.fulfillment_id WHERE br.match_id=? AND f.invoice_id IS DISTINCT FROM i.id))`, [bill.invoice_id, actor.companyId, bill.invoice_version, id]),
      write.prepare("UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?").bind(Date.now(), bill.invoice_id),
      write.prepare("UPDATE commerce_fulfillments SET invoice_id=NULL,version=version+1 WHERE id IN(SELECT fulfillment_id FROM procurement_bill_match_receipts WHERE match_id=?)").bind(id),
    );
  }
  const status = input.action === "approve" ? "approved" : "returned";
  if (status === "returned") steps.push(write.prepare("UPDATE procurement_bill_match_receipts SET active=FALSE WHERE match_id=? AND active=TRUE").bind(id));
  steps.push(
    write.prepare("UPDATE procurement_bill_matches SET status=?,reviewed_by=?,review_reason=?,variance_account_id=?,version=version+1,updated_at=? WHERE id=?").bind(status, actor.userId, input.reason, input.action === "approve" && variance ? input.varianceAccountId ?? null : bill.variance_account_id ?? null, Date.now(), id),
    event(write, actor, id, key, input.expectedVersion + 1, returnDraft ? "return-ap-draft" : input.action, input.reason),
    notice(write, id, key),
  );
  return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1, { id, version: input.expectedVersion + 1, status, ...(returnDraft ? { voidedInvoiceId: bill.invoice_id } : {}) }, steps, { companyId: actor.companyId, branchId: String(bill.branch_id), departmentId: bill.department_id ? String(bill.department_id) : null });
}

/** PO invoice owner appends all selected receipt links to the AP creation transaction. */
export async function supplierBillInvoiceHandoff(db: SqlDatabase, actor: WorkContext, fulfillment: Row, input: z.infer<typeof invoiceFromDeliverySchema>, invoiceId: string, key: string) {
  const history = await db.prepare("SELECT br.match_id id FROM procurement_bill_match_receipts br WHERE br.fulfillment_id=? LIMIT 1").bind(fulfillment.id).first<Row>();
  const matchingRequired = await db.prepare("SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND matching_mode='required'").bind(actor.companyId).first();
  if (!history && !input.matchId && !matchingRequired) return null;
  if (!input.matchId) throw new ApiError(409, "SUPPLIER_BILL_MATCH_REQUIRED", matchingRequired ? "Company policy requires an independently approved supplier invoice match before AP can be prepared." : "Use the approved supplier invoice review for this receipt.");
  const bill = await getSupplierBill(actor, input.matchId);
  const receipts = (await db.prepare("SELECT * FROM procurement_bill_match_receipts WHERE match_id=? ORDER BY id").bind(bill.id).all<Row>()).results;
  if (bill.fulfillment_id !== fulfillment.id || bill.status !== "approved" || !Number(bill.quantity_matches) || input.date !== bill.invoice_date || input.dueDate !== bill.due_date || !receipts.length) throw new ApiError(409, "SUPPLIER_BILL_MATCH_REQUIRED", "Use the approved review, its primary receipt and its invoice dates.");
  const nextVersion = Number(bill.version) + 1;
  const steps: SqlStatement[] = [assertion(db, key, "supplier-bill-handoff", "EXISTS(SELECT 1 FROM procurement_bill_matches WHERE id=? AND company_id=? AND fulfillment_id=? AND status='approved' AND version=? AND invoice_id IS NULL)", [bill.id, actor.companyId, fulfillment.id, input.matchVersion ?? 0])];
  for (const [index, receipt] of receipts.entries()) steps.push(assertion(db, key, `supplier-bill-receipt-${index}`, "EXISTS(SELECT 1 FROM procurement_bill_match_receipts br JOIN commerce_fulfillments f ON f.id=br.fulfillment_id WHERE br.id=? AND br.match_id=? AND br.active=TRUE AND f.version=br.captured_version AND f.invoice_id IS NULL)", [receipt.id, bill.id]));
  steps.push(
    db.prepare("UPDATE commerce_fulfillments SET invoice_id=?,version=version+1 WHERE id IN(SELECT fulfillment_id FROM procurement_bill_match_receipts WHERE match_id=? AND active=TRUE)").bind(invoiceId, bill.id),
    db.prepare("UPDATE procurement_bill_matches SET status='invoiced',invoice_id=?,version=version+1,updated_at=? WHERE id=?").bind(invoiceId, Date.now(), bill.id),
    event(db, actor, String(bill.id), key, nextVersion, "invoice-created", `Draft invoice ${invoiceId} prepared from ${receipts.length} receipt(s) for independent GL posting`),
    notice(db, String(bill.id), key),
  );
  return { bill, steps };
}

export function matchedBillLines(bill: Row): JournalLineInput[] {
  const expected = Number(bill.expected_subtotal_minor), subtotal = Number(bill.subtotal_minor), tax = Number(bill.tax_minor), variance = subtotal - expected;
  const lines: JournalLineInput[] = [];
  if (expected) lines.push({ accountId: String(bill.clearing_account_id), debit: minorToDecimal(expected), credit: "0" });
  if (variance) lines.push({ accountId: String(bill.variance_account_id), debit: variance > 0 ? minorToDecimal(variance) : "0", credit: variance < 0 ? minorToDecimal(-variance) : "0" });
  if (tax) lines.push({ accountId: String(bill.tax_account_id), debit: minorToDecimal(tax), credit: "0" });
  lines.push({ accountId: String(bill.payable_account_id), debit: "0", credit: minorToDecimal(subtotal + tax) });
  return lines;
}

export async function matchedInvoicePosting(db: SqlDatabase, actor: WorkContext, invoice: Row, key: string) {
  const bill = await db.prepare("SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=?").bind(invoice.id, actor.companyId).first<Row>();
  if (!bill) return null;
  if (bill.status !== "invoiced" || invoice.currency !== bill.currency || invoice.kind !== "payable" || invoice.party_id !== bill.supplier_id || invoice.invoice_date !== bill.invoice_date || invoice.due_date !== bill.due_date || invoice.debit_account_id !== bill.clearing_account_id || invoice.credit_account_id !== bill.payable_account_id || invoice.tax_account_id !== bill.tax_account_id || Number(invoice.total_minor) !== Number(bill.subtotal_minor) + Number(bill.tax_minor) || Number(invoice.subtotal_minor) !== Number(bill.subtotal_minor) || Number(invoice.tax_minor) !== Number(bill.tax_minor)) throw new ApiError(409, "SUPPLIER_BILL_SOURCE_CHANGED", "Review the matched invoice source before posting.");
  return { lines: matchedBillLines(bill), steps: [assertion(db, key, "supplier-bill-post", "EXISTS(SELECT 1 FROM procurement_bill_matches WHERE id=? AND version=? AND status='invoiced' AND invoice_id=?)", [bill.id, bill.version, invoice.id]), db.prepare("UPDATE procurement_bill_matches SET status='posted',version=version+1,updated_at=? WHERE id=?").bind(Date.now(), bill.id), event(db, actor, String(bill.id), key, Number(bill.version) + 1, "posted", "Matched invoice and price variance posted to GL"), notice(db, String(bill.id), key)] };
}

export async function supplierBillWork(actor: WorkContext): Promise<WorkItem[]> {
  if (!hasPermission(actor, "finance.read")) return [];
  const { write } = await holdingResources(actor.holdingId);
  const records = (await supplierBillWorkspace(actor)).filter(row => ["review", "approved", "returned"].includes(String(row.status)));
  const items: WorkItem[] = [];
  for (const row of records) {
    const correction = row.status === "returned";
    if (correction) {
      const latest = await write.prepare("SELECT id,status FROM procurement_bill_matches WHERE fulfillment_id=? ORDER BY (status<>'returned') DESC,created_at DESC,id DESC LIMIT 1").bind(row.fulfillment_id).first<Row>();
      if (latest?.id !== row.id || latest.status !== "returned") continue;
    }
    const review = row.status === "review";
    let canAct = hasPermission(actor, "finance.write") && hasPermission(actor, "finance.approve") && !samePerson(actor, row.created_by);
    let blocker: string | null = null, responsible = "Independent Finance reviewer";
    if (review) {
      const eligible = await write.prepare("SELECT count(DISTINCT c.user_id) n FROM assignments c WHERE c.company_id=? AND iam_assignment_active(c.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(c.role,c.company_id,'finance.write') AND iam_role_allows(c.role,c.company_id,'finance.approve') AND (c.scope_kind='holding' OR (c.scope_kind='branch' AND c.scope_node_id=?) OR (c.scope_kind='department' AND c.scope_node_id=?)) AND c.user_id<>? AND NOT EXISTS(SELECT 1 FROM iam_members cm JOIN iam_members creator ON creator.company_id=cm.company_id AND creator.person_id=cm.person_id WHERE cm.company_id=c.company_id AND cm.user_id=c.user_id AND creator.user_id=?)").bind(actor.companyId, row.branch_id, row.department_id, row.created_by, row.created_by).first("n");
      if (!Number(eligible)) blocker = "No independent Finance reviewer; Company administrator must repair role coverage";
    } else {
      const flow = await (await import("./flow-routing")).resolveFlow(write, actor, "order", String(row.order_id)), step = flow.steps.find(candidate => candidate.key === "invoicing");
      blocker = correction ? step?.primary ? null : "The purchase order invoicing owner is missing or no longer eligible. Reassign with a reason." : step?.blocker ?? null;
      canAct = hasPermission(actor, "finance.write") && (!correction || hasPermission(actor, "commerce.read")) && !blocker && step?.primary?.userId === actor.userId;
      responsible = step?.primary?.name ?? "Assigned purchase order invoicing owner";
    }
    items.push({ id: String(row.id), type: "supplier_bill", actionKind: review && canAct ? "approval" : "work", title: `Supplier invoice · ${row.reference}`, detail: `${row.order_number} · ${row.receipt_count} receipt(s) · ${row.currency} ${minorToDecimal(Number(row.subtotal_minor) + Number(row.tax_minor))}`, href: correction ? `/commerce/orders/${row.order_id}` : `/supplier-bills/${row.id}`, status: String(row.status), updatedAt: Number(row.updated_at), queue: canAct ? "todo" : "waiting", canAct, blocker, responsibleQueue: responsible, nextAction: review ? Number(row.quantity_matches) ? "Review invoice and purchase price variance" : "Return invoice with quantity discrepancy" : correction ? "Obtain corrected supplier invoice and resubmit from the receipt" : "Prepare matched AP invoice", dueAt: Date.parse(String(row.due_date) + "T00:00:00Z"), relationships: [canAct ? "assigned" : "team"] });
  }
  return items;
}
