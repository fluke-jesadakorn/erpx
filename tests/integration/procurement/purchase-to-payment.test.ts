import { beforeAll, beforeEach, expect, it, vi } from "vitest";

import { applyMigrations, env, type TestMigration } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import {
  actOnEmployeeRequest,
  employeeRequestSchema,
  getEmployeeRequest,
  requestActionSchema,
  saveEmployeeRequest,
} from "../../../src/server/modules/records/public/employee-requests";
import { saveServiceTeam, serviceTeamSchema } from "../../../src/server/modules/support/public/service-teams";
import { actRfq, createRfq, rfqDetail } from "../../../src/server/modules/procurement/public/procurement-rfqs";
import {
  fulfillOrder,
  getOrder,
  invoiceFulfillment,
  orderSchema,
  saveMaster,
  saveOrder,
  transitionOrder,
} from "../../../src/server/modules/commerce/public/commerce";
import { assignFlowPrimary, resolveFlow } from "../../../src/server/modules/workflow/public/flow-routing";
import { importBankStatement, postInvoice, postJournal } from "../../../src/server/modules/finance/public/finance";
import { captureSupplierBill, decideSupplierBill } from "../../../src/server/modules/procurement/public/supplier-bill-matching";
import {
  confirmSupplierInstruction,
  createSupplierBatch,
  decideSupplierBatch,
  exportSupplierBatch,
  supplierBatchWorkspace,
} from "../../../src/server/modules/procurement/public/supplier-payments";
import { purchaseRequestProgress } from "../../../src/server/modules/procurement/public/purchase-request-progress";

vi.mock("server-only", () => ({}));
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));

let f: Awaited<ReturnType<typeof expenseFixture>>;
let buyer: WorkContext;
let purchasingManager: WorkContext;
let warehouse: WorkContext;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});

beforeEach(async () => {
  f = await expenseFixture();
  buyer = { ...f.it, role: "purchasing_operator", assignmentId: await f.grant(f.it, "purchasing_operator") };
  purchasingManager = { ...f.tie, role: "purchasing_manager", assignmentId: await f.grant(f.tie, "purchasing_manager") };
  warehouse = { ...f.head, role: "warehouse_manager", assignmentId: await f.grant(f.head, "warehouse_manager") };
  await db.batch([
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'asset','THB',1,1)").bind(`${f.company}-inventory`, f.chart, f.company, "INVENTORY", "Inventory"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'liability','THB',1,1)").bind(`${f.company}-clearing`, f.chart, f.company, "CLEARING", "Goods received clearing"),
    db.prepare("UPDATE commerce_settings SET inventory_account_id=?,clearing_account_id=? WHERE company_id=?").bind(`${f.company}-inventory`, `${f.company}-clearing`, f.company),
  ]);
  await saveServiceTeam(db, f.owner, null, serviceTeamSchema.parse({
    name: "Purchasing acceptance team",
    members: [{ userId: buyer.userId, responsibility: "lead" }],
    coverage: [{ category: "purchasing", scopeKind: "holding", scopeNodeId: f.company }],
  }), key());
});

async function act(actor: WorkContext, requestId: string, action: string, extra: Record<string, unknown> = {}) {
  const request = await getEmployeeRequest(db, actor, requestId);
  return actOnEmployeeRequest(db, actor, requestId, requestActionSchema.parse({
    action,
    expectedVersion: request.version,
    note: "Purchase-to-payment acceptance evidence",
    ...extra,
  }), key());
}

async function assign(kind: "order" | "journal" | "invoice", id: string, stepKey: string, actor: WorkContext) {
  const flow = await resolveFlow(db, f.owner, kind, id);
  await assignFlowPrimary(db, f.owner, kind, id, {
    stepKey,
    userId: actor.userId,
    reason: "Cross-department acceptance owner",
    saveDefault: false,
    expectedVersion: flow.version,
    recordVersion: flow.recordVersion,
  }, key());
}

it("runs an approved ten-laptop request through RFQ, 6+4 receipts, one consolidated AP invoice, bank evidence and payment", async () => {
  const request = await saveEmployeeRequest(db, f.claimant, null, employeeRequestSchema.parse({
    kind: "purchase",
    title: "Laptop 10 เครื่องสำหรับทีมบริการลูกค้า",
    description: "Replace expired devices and equip new staff.",
    location: "สำนักงานใหญ่ ชั้น 4",
    branchId: f.branch,
    departmentId: f.department,
    neededDate: "2026-10-15",
    currency: "THB",
    items: [{ description: "Business laptop with three-year warranty", quantity: "10", estimatedCost: "25000" }],
  }), key());
  await act(f.claimant, request.id, "submit");
  await act(f.head, request.id, "approve");
  await act(f.reviewer, request.id, "approve");
  await act(f.manager, request.id, "approve");
  await act(buyer, request.id, "assign", { assigneeUserId: buyer.userId });
  await act(buyer, request.id, "start");

  const warehouseMaster = await saveMaster(f.owner, "warehouses", null, { branchId: f.branch, code: `WH-${key()}`, name: "Laptop receiving" }, key());
  const product = await saveMaster(f.owner, "products", null, { sku: `LAP-${key()}`, name: "Business laptop", price: "25000" }, key());
  const supplier = await saveMaster(f.owner, "parties", null, { kind: "supplier", code: `SUP-${key()}`, name: "Demo laptop supplier" }, key());
  const approvedRequest = await getEmployeeRequest(db, buyer, request.id);
  const rfq = await createRfq(buyer, {
    requestId: request.id,
    requestVersion: approvedRequest.version,
    title: "Laptop procurement",
    warehouseId: String(warehouseMaster.id),
    neededDate: "2026-10-15",
    demandEvidence: "Ten approved laptops mapped to source request item 1",
    lines: [{ requestItemOrdinal: 0, productId: String(product.id), quantity: "10" }],
  }, key());
  let rfqView = await rfqDetail(buyer, String(rfq.id));
  await actRfq(buyer, String(rfq.id), {
    action: "offer",
    expectedVersion: 1,
    supplierId: String(supplier.id),
    reference: "DEMO-QUOTE-10",
    evidence: "Supplier quotation for ten laptops",
    arrivalDate: "2026-10-15",
    validUntil: "2026-12-31",
    lines: [{ lineId: String(rfqView.lines[0].id), price: "25000", taxBps: 0 }],
  }, key());
  rfqView = await rfqDetail(buyer, String(rfq.id));
  await actRfq(buyer, String(rfq.id), { action: "submit", expectedVersion: 2, offerId: String(rfqView.current[0].id), reason: "Best compliant offer" }, key());
  await actRfq(purchasingManager, String(rfq.id), { action: "approve", expectedVersion: 3, reason: "Independent supplier award review" }, key());

  rfqView = await rfqDetail(buyer, String(rfq.id));
  const purchaseOrder = await saveOrder(buyer, null, orderSchema.parse({
    kind: "purchase",
    requestId: request.id,
    requestVersion: rfqView.currentRequestVersion,
    rfqId: String(rfq.id),
    rfqVersion: 4,
    partyId: String(supplier.id),
    warehouseId: String(warehouseMaster.id),
    orderDate: "2026-10-01",
    dueDate: "2026-10-15",
    lines: [{ productId: String(product.id), quantity: "10", price: "25000", taxBps: 0 }],
  }), key());
  const orderId = String(purchaseOrder.id);
  await transitionOrder(buyer, orderId, { action: "submit", expectedVersion: 1 }, key());
  await assign("order", orderId, "submitted", purchasingManager);
  await transitionOrder(purchasingManager, orderId, { action: "approve", expectedVersion: 2 }, key());
  await assign("order", orderId, "confirmed", warehouse);
  await assign("order", orderId, "stock_gl", f.manager);
  await assign("order", orderId, "invoicing", f.reviewer);

  const orderLine = String((await getOrder(buyer, orderId)).lines[0].id);
  const receipts = [];
  for (const [index, quantity] of ["6", "4"].entries()) {
    const expectedVersion = Number((await getOrder(warehouse, orderId)).order.version);
    const receipt = await fulfillOrder(warehouse, orderId, { expectedVersion, date: `2026-10-${10 + index}`, lines: [{ lineId: orderLine, quantity }] }, key());
    const fulfillmentId = String(receipt.fulfillmentId);
    const journalId = String(await db.prepare("SELECT journal_id FROM commerce_fulfillments WHERE id=?").bind(fulfillmentId).first("journal_id"));
    await assign("journal", journalId, "draft", f.manager);
    await postJournal(f.manager, journalId, key(), { expectedVersion: 1 });
    receipts.push({ fulfillmentId, quantity });
  }

  const receiptLines = await Promise.all(receipts.map(async receipt => ({
    ...receipt,
    lineId: String(await db.prepare("SELECT id FROM commerce_fulfillment_lines WHERE fulfillment_id=?").bind(receipt.fulfillmentId).first("id")),
  })));
  const invoiceDate = "2026-10-13";
  const dueDate = "2026-11-12";
  const bill = await captureSupplierBill(f.reviewer, {
    receipts: receipts.map(receipt => ({ fulfillmentId: receipt.fulfillmentId, expectedVersion: 1 })),
    reference: "SUPPLIER-INVOICE-CONSOLIDATED",
    invoiceDate,
    dueDate,
    evidence: "One supplier invoice agrees to both the six-unit and four-unit goods receipts",
    lines: receiptLines.map(receipt => ({ fulfillmentLineId: receipt.lineId, quantity: receipt.quantity, price: "25000", taxBps: 0 })),
  }, key());
  expect(bill.receiptCount).toBe(2);
  await decideSupplierBill(f.manager, String(bill.id), { action: "approve", expectedVersion: 1, reason: "Both receipts, price and supplier evidence agree" }, key());
  const invoice = await invoiceFulfillment(f.reviewer, receipts[0].fulfillmentId, { expectedVersion: 2, date: invoiceDate, dueDate, matchId: String(bill.id), matchVersion: 2 }, key());
  const invoiceId = String(invoice.invoiceId);
  await assign("invoice", invoiceId, "draft", f.manager);
  await postInvoice(f.manager, invoiceId, { expectedVersion: 1, periodId: f.period, entryDate: invoiceDate }, key());

  const payable = await db.prepare("SELECT id,version,outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first<{ id: string; version: number; outstanding_minor: number }>();
  const batch = await createSupplierBatch(f.reviewer, {
    bankAccountId: f.accounts.cash,
    plannedDate: "2026-11-15",
    items: [{
      invoiceId: payable!.id,
      expectedVersion: Number(payable!.version),
      amount: String(Number(payable!.outstanding_minor) / 100),
      beneficiaryName: "Demo laptop supplier",
      beneficiaryAccount: "SANDBOX-ACCOUNT-001",
    }],
  }, key());
  const batchId = String(batch.id);
  await decideSupplierBatch(f.manager, batchId, { expectedVersion: 1, reason: "Invoices and beneficiary independently reviewed" }, key(), "approve");
  await exportSupplierBatch(f.payer, batchId, { expectedVersion: 2 }, key());

  const statementReference = "DEMO-BANK-250000";
  await importBankStatement(f.reviewer, {
    bankAccount: f.accounts.cash,
    statementNumber: "DEMO-BANK-NOV-2026",
    statementDate: "2026-11-16",
    opening: "250000",
    closing: "0",
    sourceHash: key(),
    rows: [{
      externalId: statementReference,
      transactionDate: "2026-11-16",
      description: "Consolidated laptop supplier payment",
      amount: "-250000",
      currency: "THB",
    }],
  }, key());
  const workspace = await supplierBatchWorkspace(f.payer, batchId);
  expect(workspace.items).toHaveLength(1);
  const bank = await db.prepare("SELECT id,version FROM finance_bank_transactions WHERE company_id=? AND external_id=?").bind(f.company, statementReference).first<{ id: string; version: number }>();
  await confirmSupplierInstruction(f.payer, String(workspace.items[0].id), {
    expectedVersion: 2,
    bankTransactionId: bank!.id,
    bankVersion: Number(bank!.version),
    periodId: f.period,
    bankReference: statementReference,
    evidence: "Exact outgoing bank transaction matched to approved supplier instruction",
  }, key());

  const progress = await purchaseRequestProgress(db, f.claimant, request.id);
  expect(progress.steps.map(step => [step.id, step.state])).toEqual([
    ["approval", "complete"],
    ["sourcing", "complete"],
    ["order", "complete"],
    ["receiving", "complete"],
    ["invoice", "complete"],
    ["payment", "complete"],
  ]);
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(warehouseMaster.id, product.id).first()).toEqual({ quantity_milli: 10000, value_minor: 25000000 });
  expect(await db.prepare("SELECT status,outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({ status: "paid", outstanding_minor: 0 });
  expect((await db.prepare("SELECT id,invoice_id FROM commerce_fulfillments WHERE id IN (?,?) ORDER BY id").bind(...receipts.map(receipt=>receipt.fulfillmentId)).all()).results.every(receipt=>receipt.invoice_id===invoiceId)).toBe(true);
  const journalTotals = (await db.prepare("SELECT j.id,SUM(l.debit_minor) debit,SUM(l.credit_minor) credit FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.company_id=? AND j.status='posted' GROUP BY j.id").bind(f.company).all<{ id: string; debit: number; credit: number }>()).results;
  expect(journalTotals.length).toBeGreaterThanOrEqual(4);
  expect(journalTotals.every(row => Number(row.debit) === Number(row.credit))).toBe(true);
});
