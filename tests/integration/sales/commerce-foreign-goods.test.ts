import { fulfillmentSchema } from "../../../src/server/modules/commerce/public/commerce";
import { confirmPaymentSchema } from "../../../src/server/modules/finance/public/finance";
import { returnRequestSchema } from "../../../src/server/modules/commerce/public/commerce";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { deliveryEvidence } from "../../support/helpers/delivery-evidence";
import { assignFlowPrimary, resolveFlow } from "../../../src/server/modules/workflow/public/flow-routing";
import { approvePayment, confirmPayment, createAccount, createAccountSchema, createJournal, createPayment, createPeriod, decideFxRevaluation, importBankStatement, postInvoice, postJournal, prepareFxRevaluation, releasePayment } from "../../../src/server/modules/finance/public/finance";
import { confirmManualRate } from "../../../src/server/modules/finance/public/company-money";
import { completeCommerceReturn, decideCommerceReturn, fulfillOrder, getOrder, invoiceFulfillment, orderSchema, requestCommerceReturn, saveMaster, saveOrder, transitionOrder } from "../../../src/server/modules/commerce/public/commerce";
import { captureSupplierBill, decideSupplierBill } from "../../../src/server/modules/procurement/public/supplier-bill-matching";
import { approveRefund, confirmRefundInstruction, exportRefundInstructions, prepareRefundInstruction, recoverRefundExport, refundConfirmationSchema, refundInstructionSchema, refundWorkspace } from "../../../src/server/modules/finance/public/refunds";
import { invoiceHistoryCte } from "../../../src/server/modules/finance/public/finance-history";
import { enableAttachmentV2ForTest } from "../../support/helpers/attachment-v2";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));
let f: Awaited<ReturnType<typeof expenseFixture>>;
let ids: { inventory: string; clearing: string; payable: string; receivable: string; bank: string; gain: string; loss: string; variance: string };
let october: string;
beforeAll(async () => { enableAttachmentV2ForTest(); await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  for (const [actor, role] of [[f.manager, "finance_approver"], [f.head, "purchasing_manager"], [f.reviewer, "finance_preparer"], [f.tie, "finance_approver"]] as const) {
    actor.role = role; actor.branchId = null; actor.departmentId = null; actor.assignmentId = await f.grant(actor, role);
  }
  f.payer.branchId = null; f.payer.departmentId = null;
  // Only configuration is fixture data. Every goods, invoice, payment, return,
  // revaluation and refund below is born through its normal owner command.
  await db.prepare("UPDATE finance_periods SET name='September',start_date='2026-09-01',end_date='2026-09-30' WHERE id=?").bind(f.period).run();
  october = String((await createPeriod(f.owner, { name: "October", startDate: "2026-10-01", endDate: "2026-10-31" }, key())).id);
  const entries = [
    ["inventory", "asset", "THB", "none"], ["clearing", "liability", "THB", "none"],
    ["payable", "liability", "USD", "payable"], ["receivable", "asset", "USD", "receivable"],
    ["bank", "asset", "USD", "bank"], ["gain", "other_income", "THB", "none"],
    ["loss", "other_expense", "THB", "none"], ["variance", "expense", "THB", "none"],
  ] as const;
  ids = {} as typeof ids;
  for (const [name, category, currency, controlKind] of entries) ids[name] = String((await createAccount(f.owner, createAccountSchema.parse({ chartId: f.chart, code: key(), name: `Foreign goods ${name}`, category, currency, controlKind }), key())).id);
  await db.prepare("UPDATE commerce_settings SET currency='USD',inventory_account_id=?,clearing_account_id=?,payable_account_id=?,receivable_account_id=?,cash_account_id=? WHERE company_id=?")
    .bind(ids.inventory, ids.clearing, ids.payable, ids.receivable, ids.bank, f.company).run();
  for (const [effectiveDate, currencyPerUsd] of [["2026-09-28", "35.123456"], ["2026-09-29", "36"], ["2026-09-30", "37"], ["2026-10-05", "38"]])
    await confirmManualRate(f.manager, { currency: "THB", effectiveDate, currencyPerUsd, note: "Declared synthetic dated FX adapter; no bank certification" }, key());
});
async function assign(kind: "order" | "invoice" | "journal", id: string, stepKey: string, userId: string) {
  const flow = await resolveFlow(db, f.owner, kind, id);
  await assignFlowPrimary(db, f.owner, kind, id, { stepKey, userId, reason: "Independent foreign goods source responsibility", saveDefault: false, expectedVersion: flow.version, recordVersion: flow.recordVersion }, key());
}
async function postGoods(journalId: string) { await assign("journal", journalId, "draft", f.manager.userId); await postJournal(f.manager, journalId, key(), { expectedVersion: 1 }); }
async function balance(account: string) {
  return Number(await db.prepare("SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status IN ('posted','reversed') AND l.account_id=?").bind(f.company, account).first("n"));
}
async function goods(kind: "purchase" | "sales", source?: { warehouseId: string; productId: string; lotId: string }, price = "50", taxBps = 0, stockValue = 351235) {
  const warehouseId = source?.warehouseId ?? String((await saveMaster(f.owner, "warehouses", null, { branchId: f.branch, code: key(), name: "Foreign goods warehouse" }, key())).id);
  const productId = source?.productId ?? String((await saveMaster(f.owner, "products", null, { sku: key(), name: "Foreign lot tracked product", price, trackingMode: "lot" }, key())).id);
  const partyId = String((await saveMaster(f.owner, "parties", null, { kind: kind === "purchase" ? "supplier" : "customer", code: key(), name: "Fictional USD trading counterparty" }, key())).id);
  f.head.role = kind === "purchase" ? "purchasing_manager" : "sales_manager"; f.head.assignmentId = await f.grant(f.head, f.head.role);
  const orderId = String((await saveOrder(f.owner, null, orderSchema.parse({ kind, partyId, warehouseId, orderDate: "2026-09-28", dueDate: "2026-09-29", lines: [{ productId, quantity: "2", price, taxBps }] }), key())).id);
  await transitionOrder(f.owner, orderId, { action: "submit", expectedVersion: 1 }, key());
  await transitionOrder(f.head, orderId, { action: "approve", expectedVersion: 2 }, key());
  await assign("order", orderId, "stock_gl", f.manager.userId);
  const details = await getOrder(f.owner, orderId), allocations = kind === "purchase" ? [{ lotCode: key(), quantity: "2" }] : [{ lotId: source!.lotId, quantity: "2" }];
  const input = { expectedVersion: 3, date: kind === "purchase" ? "2026-09-28" : "2026-09-29", lines: [{ lineId: String(details.lines[0].id), quantity: "2", allocations }], ...(kind === "sales" ? await deliveryEvidence(db, f.owner, orderId) : {}) }, command = key();
  const fulfilled = await fulfillOrder(f.owner, orderId, fulfillmentSchema.parse(input), command);
  expect(await fulfillOrder(f.owner, orderId, fulfillmentSchema.parse(input), command)).toEqual(fulfilled);
  const fulfillmentId = String(fulfilled.fulfillmentId), journalId = String(await db.prepare("SELECT journal_id FROM commerce_fulfillments WHERE id=?").bind(fulfillmentId).first("journal_id"));
  await postGoods(journalId);
  await assign("order", orderId, "invoicing", f.owner.userId);
  const lineId = String(await db.prepare("SELECT id FROM commerce_fulfillment_lines WHERE fulfillment_id=?").bind(fulfillmentId).first("id")), lotId = String(await db.prepare("SELECT lot_id FROM inventory_fulfillment_lot_allocations WHERE fulfillment_line_id=?").bind(lineId).first("lot_id"));
  expect(await db.prepare("SELECT stock_currency,value_minor FROM commerce_fulfillment_lines WHERE id=?").bind(lineId).first()).toEqual({ stock_currency: "THB", value_minor: stockValue });
  expect(await db.prepare("SELECT document_currency,stock_currency,stock_value_minor FROM commerce_goods_valuations WHERE fulfillment_id=?").bind(fulfillmentId).first()).toEqual({ document_currency: "USD", stock_currency: "THB", stock_value_minor: stockValue });
  return { warehouseId, productId, partyId, orderId, fulfillmentId, lineId, lotId, journalId };
}
async function invoice(source: Awaited<ReturnType<typeof goods>>, matched = false) {
  let matchId: string | undefined;
  if (matched) {
    const bill = await captureSupplierBill(f.owner, { fulfillmentId: source.fulfillmentId, expectedVersion: 1, reference: key(), invoiceDate: "2026-09-29", dueDate: "2026-09-30", evidence: "Synthetic USD supplier bill independently checked against actual goods", lines: [{ fulfillmentLineId: source.lineId, quantity: "2", price: "60", taxBps: 0 }] }, key());
    matchId = String(bill.id);
    await decideSupplierBill(f.manager, matchId, { action: "approve", expectedVersion: 1, reason: "Independently reviewed document price variance", varianceAccountId: ids.variance }, key());
  }
  const result = await invoiceFulfillment(f.owner, source.fulfillmentId, { expectedVersion: matched ? 2 : 1, date: "2026-09-29", dueDate: "2026-09-30", ...(matchId ? { matchId, matchVersion: 2 } : {}) }, key()), invoiceId = String(result.invoiceId);
  await assign("invoice", invoiceId, "draft", f.manager.userId);
  const input = { expectedVersion: 1, periodId: f.period, entryDate: "2026-09-29", fxGainAccountId: ids.gain, fxLossAccountId: ids.loss };
  if (!await db.prepare("SELECT 1 FROM commerce_orders WHERE id=? AND kind='sales'").bind(source.orderId).first()) {
    await expect(postInvoice(f.manager, invoiceId, { expectedVersion: 1, periodId: f.period, entryDate: "2026-09-29" }, key())).rejects.toMatchObject({ code: "FX_GAIN_ACCOUNT_REQUIRED" });
    await expect(postInvoice(f.manager, invoiceId, { ...input, fxLossAccountId: ids.gain }, key())).rejects.toMatchObject({ code: "FX_ACCOUNT_INVALID" });
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE source_type='invoice' AND source_id=?").bind(invoiceId).first("n")).toBe(0);
  }
  const command = key(), posted = await postInvoice(f.manager, invoiceId, input, command);
  expect(await postInvoice(f.manager, invoiceId, input, command)).toEqual(posted);
  return invoiceId;
}
async function payment(invoiceId: string, partyId: string, amount: string, kind: "payment" | "receipt") {
  const result = await createPayment(f.reviewer, { partyId, kind, paymentNumber: key(), paymentDate: "2026-09-29", currency: "USD", amount, bankAccount: ids.bank }, key()), id = String(result.id);
  await approvePayment(f.manager, id, { expectedVersion: 1, proof: "Independent original foreign invoice approval" }, key());
  await releasePayment(f.payer, id, { expectedVersion: 2, proof: "Treasury releases the approved foreign payment" }, key());
  const signed = kind === "payment" ? `-${amount}` : amount;
  const statement = await importBankStatement(f.reviewer, { bankAccount: ids.bank, statementNumber: key(), statementDate: "2026-09-29", opening: "0", closing: signed, sourceHash: key(), rows: [{ externalId: key(), transactionDate: "2026-09-29", amount: signed, currency: "USD" }] }, key()), bankId = String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id"));
  await confirmPayment(f.payer, id, confirmPaymentSchema.parse({ expectedVersion: 3, periodId: f.period, bankTransactionId: bankId, bankVersion: 1, bankReference: key(), proof: "Declared synthetic bank adapter matched by Treasury", allocations: [{ invoiceId, amount }], fxGainAccountId: ids.gain, fxLossAccountId: ids.loss }), key());
}
async function revalue() {
  const run = await prepareFxRevaluation(f.reviewer, { chartId: f.chart, periodId: f.period, entryDate: "2026-09-30", rates: [{ currency: "USD", rate: "37" }], gainAccountId: ids.gain, lossAccountId: ids.loss, proof: "Reviewed synthetic closing quote at actual month end" }, key());
  await expect(decideFxRevaluation(f.reviewer, String(run.id), { expectedVersion: 1, proof: "Self review rejected" }, "approve", key())).rejects.toMatchObject({ status: 403 });
  await decideFxRevaluation(f.manager, String(run.id), { expectedVersion: 1, proof: "Independent Finance review of the immutable closing exposure" }, "approve", key());
}
async function returned(source: Awaited<ReturnType<typeof goods>>, missingFx?: "FX_GAIN_ACCOUNT_REQUIRED" | "FX_LOSS_ACCOUNT_REQUIRED") {
  const expectedVersion = Number(await db.prepare("SELECT version FROM commerce_fulfillments WHERE id=?").bind(source.fulfillmentId).first("version"));
  const requested = await requestCommerceReturn(f.owner, source.fulfillmentId, returnRequestSchema.parse({ expectedVersion, date: "2026-10-01", reason: "Fictional counterparty accepted one returned lot unit", lines: [{ lineId: source.lineId, quantity: "1", allocations: [{ lotId: source.lotId, quantity: "1" }], condition: "unknown", conditionNote: "Returned unit retained for independent Quality review", qualityDueDate: "2026-10-02" }] }), key()), id = String(requested.id);
  await expect(decideCommerceReturn(f.owner, id, "approve", { expectedVersion: 1, reason: "Self review rejected" }, key())).rejects.toMatchObject({ code: "INDEPENDENT_APPROVER_REQUIRED" });
  const decision = await decideCommerceReturn(f.head, id, "approve", { expectedVersion: 1, reason: "Independent original lot and customer or supplier return approval" }, key());
  for (const journalId of [decision.stockJournalId, decision.creditJournalId, decision.goodsFxJournalId].filter(Boolean)) await postGoods(String(journalId));
  if (missingFx) {
    await expect(completeCommerceReturn(f.manager, id, { expectedVersion: 2 }, key())).rejects.toMatchObject({ code: missingFx });
    expect(await db.prepare("SELECT status FROM commerce_returns WHERE id=?").bind(id).first("status")).toBe("approved");
  }
  const input = { expectedVersion: 2, fxGainAccountId: ids.gain, fxLossAccountId: ids.loss }, command = key(), result = await completeCommerceReturn(f.manager, id, input, command);
  expect(await completeCommerceReturn(f.manager, id, input, command)).toEqual(result);
  return { ...result, id, decision };
}
async function refund(obligation: string, amount: string) {
  let owner = (await refundWorkspace(f.payer, obligation)).owner;
  if (owner.status === "pending") await approveRefund(f.tie, obligation, { expectedVersion: Number(owner.version), reason: "Independent original credit and beneficiary review", beneficiaryName: "Fictional USD counterparty", beneficiaryAccount: "SANDBOX-USD-001" }, key());
  owner = (await refundWorkspace(f.payer, obligation)).owner;
  const prepared = await prepareRefundInstruction(f.payer, obligation, refundInstructionSchema.parse({ expectedVersion: Number(owner.version), amount, plannedDate: "2026-10-05", bankAccount: ids.bank, cashAccountId: ids.bank, fxGainAccountId: ids.gain, fxLossAccountId: ids.loss }), key()), id = String(prepared.id);
  const exported = await exportRefundInstructions(f.payer, { instructions: [{ id, expectedVersion: 1 }] }, key());
  expect(await recoverRefundExport(f.payer, id)).toMatchObject({ sha256: exported.sha256, content: exported.content });
  const signed = owner.direction === "outgoing" ? `-${amount}` : amount, statement = await importBankStatement(f.reviewer, { bankAccount: ids.bank, statementNumber: key(), statementDate: "2026-10-05", opening: "0", closing: signed, sourceHash: key(), rows: [{ externalId: key(), transactionDate: "2026-10-05", amount: signed, currency: "USD" }] }, key()), bankId = String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id"));
  const input = refundConfirmationSchema.parse({ expectedVersion: 2, bankTransactionId: bankId, bankVersion: 1, periodId: october, bankReference: key(), evidence: "Treasury reviewed exact dated synthetic refund settlement" }), command = key(), result = await confirmRefundInstruction(f.payer, id, input, command);
  expect(await confirmRefundInstruction(f.payer, id, input, command)).toEqual(result);
  return result;
}

describe("native foreign Commerce goods carrying", () => {
  it("rejects an unrelated account currency at independent journal posting while keeping the draft and every stock balance unchanged", async () => {
    const euro=String((await createAccount(f.owner,createAccountSchema.parse({chartId:f.chart,code:key(),name:"Unrelated EUR account",category:"expense",currency:"EUR"}),key())).id);
    const journal=await createJournal(f.reviewer,{chartId:f.chart,periodId:f.period,entryDate:"2026-09-29",currency:"USD",memo:"Declared invalid third-currency review case",sourceType:"manual",isOpening:false,lines:[{accountId:euro,debit:"1",credit:"0"},{accountId:ids.payable,debit:"0",credit:"1"}]},key());
    await assign("journal",journal.id,"draft",f.manager.userId);
    await expect(postJournal(f.manager,journal.id,key(),{expectedVersion:1})).rejects.toMatchObject({code:"CONFLICT",cause:{code:"23514",detail:expect.stringContaining("active-accounts")}});
    expect(await db.prepare("SELECT status,version FROM finance_journals WHERE id=?").bind(journal.id).first()).toEqual({status:"draft",version:1});
    expect(await balance(euro)).toBe(0);expect(await balance(ids.payable)).toBe(0);
    expect(await db.prepare("SELECT COUNT(*) n FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=?").bind(f.company).first("n")).toBe(0);
  });
  it("allocates original tax and clearing cents across partial credits and reverses every posted source account exactly", async () => {
    await confirmManualRate(f.manager, { currency: "THB", effectiveDate: "2026-09-29", currencyPerUsd: "35.5", note: "Synthetic rounding edge quote" }, key());
    const source = await goods("purchase", undefined, "0.01", 5000, 70), invoiceId = await invoice(source);
    const posted = await db.prepare("SELECT total_minor,base_total_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first();
    expect(posted).toEqual({ total_minor: 3, base_total_minor: 107 });
    expect(await balance(ids.inventory)).toBe(70); expect(await balance(ids.clearing)).toBe(0); expect(await balance(f.accounts.tax)).toBe(36);
    const first = await returned(source), last = await returned(source);
    expect(first).toMatchObject({ creditedMinor: 2, refundMinor: 0 }); expect(last).toMatchObject({ creditedMinor: 1, refundMinor: 0 });
    expect(await db.prepare("SELECT SUM(base_credit_minor) amount FROM commerce_returns WHERE fulfillment_id=? AND status='posted'").bind(source.fulfillmentId).first("amount")).toBe(107);
    for (const account of [ids.inventory, ids.clearing, ids.payable, f.accounts.tax, ids.gain, ids.loss]) expect(await balance(account)).toBe(0);
    expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(source.warehouseId, source.productId).first()).toEqual({ quantity_milli: 0, value_minor: 0 });
  });
  it.each([false, true])("retains receipt-date functional stock through revalued partial supplier returns and dated refunds (matched=%s)", async matched => {
    const source = await goods("purchase"), invoiceId = await invoice(source, matched), total = matched ? 12000 : 10000, half = total / 200;
    expect(await db.prepare("SELECT currency,fx_rate_micros,total_minor,base_total_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({ currency: "USD", fx_rate_micros: 36000000, total_minor: total, base_total_minor: total * 36 });
    expect(await db.prepare("SELECT receipt_base_minor,invoice_clearing_base_minor,adjustment_minor FROM commerce_invoice_goods_carrying WHERE invoice_id=?").bind(invoiceId).first()).toEqual({ receipt_base_minor: 351235, invoice_clearing_base_minor: 360000, adjustment_minor: 8765 });
    const evidence=await db.prepare("SELECT g.invoice_fx_rate_micros,g.invoice_rate_basis,g.invoice_used_latest_fallback,r.effective_date::text effective_date,r.currency_per_usd::text quote FROM commerce_invoice_goods_carrying g JOIN company_fx_rates r ON r.id=g.invoice_target_rate_id WHERE g.invoice_id=?").bind(invoiceId).first();
    expect(evidence).toEqual({invoice_fx_rate_micros:36000000,invoice_rate_basis:"confirmed_company_quote",invoice_used_latest_fallback:false,effective_date:"2026-09-29",quote:"36.000000000000000000"});
    expect(await balance(ids.inventory)).toBe(351235); expect(await balance(ids.clearing)).toBe(0);
    const original = await db.prepare("SELECT * FROM commerce_goods_valuations WHERE fulfillment_id=?").bind(source.fulfillmentId).first();
    await payment(invoiceId, source.partyId, String(half), "payment"); await revalue();
    expect(await db.prepare("SELECT outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({ outstanding_minor: total / 2, base_outstanding_minor: total / 2 * 37 });
    const first = await returned(source, "FX_GAIN_ACCOUNT_REQUIRED");
    expect(first).toMatchObject({ creditedMinor: total / 2, refundMinor: 0 });
    expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(source.warehouseId, source.productId).first()).toEqual({ quantity_milli: 1000, value_minor: 175617 });
    expect(await db.prepare("SELECT base_credit_minor,base_credited_minor,base_refund_minor FROM commerce_returns WHERE id=?").bind(first.id).first()).toEqual({ base_credit_minor: total / 2 * 36, base_credited_minor: total / 2 * 37, base_refund_minor: 0 });
    const last = await returned(source), refundId = String(last.refundId);
    expect(last).toMatchObject({ creditedMinor: 0, refundMinor: total / 2 });
    expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(source.warehouseId, source.productId).first()).toEqual({ quantity_milli: 0, value_minor: 0 });
    expect(await balance(ids.inventory)).toBe(0); expect(await balance(ids.clearing)).toBe(0); expect(await balance(ids.variance)).toBe(0);
    const a = await refund(refundId, "40.01"), b = await refund(refundId, ((total / 2 - 4001) / 100).toFixed(2));
    expect(Number(a.baseCarryingMinor) + Number(b.baseCarryingMinor)).toBe(total / 2 * 36);
    expect(Number(a.cashBaseMinor) + Number(b.cashBaseMinor)).toBe(total / 2 * 38);
    expect((await refundWorkspace(f.payer, refundId)).owner).toMatchObject({ status: "settled", outstanding_minor: 0, base_outstanding_minor: 0 });
    expect(await balance(ids.payable)).toBe(0);
    expect(await db.prepare(`${invoiceHistoryCte} SELECT historical_outstanding_minor,historical_base_outstanding_minor FROM historical_invoice_balances WHERE id=?`).bind(f.company, "2026-10-05", invoiceId).first()).toEqual({ historical_outstanding_minor: 0, historical_base_outstanding_minor: 0 });
    expect(await db.prepare("SELECT * FROM commerce_goods_valuations WHERE fulfillment_id=?").bind(source.fulfillmentId).first()).toEqual(original);
    await expect(db.prepare("UPDATE commerce_goods_valuations SET fx_rate_micros=38000000 WHERE fulfillment_id=?").bind(source.fulfillmentId).run()).rejects.toThrow("COMMERCE_GOODS_EVIDENCE_IMMUTABLE");
    await expect(db.prepare("UPDATE commerce_fulfillment_lines SET value_minor=1 WHERE id=?").bind(source.lineId).run()).rejects.toThrow("COMMERCE_FULFILLMENT_VALUE_IMMUTABLE");
    const snapshot = await db.prepare("SELECT SUM(s.amount_units) amount,SUM(s.usd_units) usd FROM money_transaction_snapshots s JOIN commerce_return_lines l ON s.source_id=l.id JOIN commerce_returns r ON r.id=l.return_id WHERE s.company_id=? AND s.source_type='commerce_return_line' AND s.amount_kind='stock_value' AND s.currency='THB' AND r.fulfillment_id=?").bind(f.company, source.fulfillmentId).first();
    expect(snapshot).toEqual({ amount: -351235, usd: -10000 });
  });
  it("restores original functional lot cost and clears revalued USD receivables with independently settled customer refunds", async () => {
    const purchase = await goods("purchase"), bill = await invoice(purchase);
    await payment(bill, purchase.partyId, "100", "payment");
    const source = await goods("sales", purchase, "100"), invoiceId = await invoice(source);
    expect(await balance(ids.inventory)).toBe(0);
    expect(await db.prepare("SELECT currency,fx_rate_micros,total_minor,base_total_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({ currency: "USD", fx_rate_micros: 36000000, total_minor: 20000, base_total_minor: 720000 });
    expect(await db.prepare("SELECT currency,fx_rate_micros FROM finance_journals WHERE id=?").bind(source.journalId).first()).toEqual({ currency: "THB", fx_rate_micros: 1000000 });
    await payment(invoiceId, source.partyId, "150", "receipt"); await revalue();
    const first = await returned(source, "FX_LOSS_ACCOUNT_REQUIRED"), last = await returned(source);
    expect(first).toMatchObject({ creditedMinor: 5000, refundMinor: 5000 });
    expect(last).toMatchObject({ creditedMinor: 0, refundMinor: 10000 });
    expect(await db.prepare("SELECT base_credit_minor,base_credited_minor,base_refund_minor FROM commerce_returns WHERE id=?").bind(first.id).first()).toEqual({ base_credit_minor: 360000, base_credited_minor: 185000, base_refund_minor: 180000 });
    expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(source.warehouseId, source.productId).first()).toEqual({ quantity_milli: 2000, value_minor: 351235 });
    expect(await db.prepare("SELECT quantity_milli,value_minor FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?").bind(source.lotId, source.warehouseId).first()).toEqual({ quantity_milli: 2000, value_minor: 351235 });
    expect(await balance(ids.inventory)).toBe(351235);
    expect(await db.prepare("SELECT inventory_quarantined_quantity(?,?) n").bind(source.warehouseId, source.productId).first("n")).toBe(2000);
    const { inspectQualityHold, releaseQualityHold } = await import("../../../src/server/modules/quality/public/quality");
    f.it.role = "quality_manager"; f.it.branchId = null; f.it.departmentId = null; f.it.assignmentId = await f.grant(f.it, "quality_manager");
    const holds = (await db.prepare("SELECT h.id FROM inventory_quality_holds h JOIN commerce_return_lot_allocations a ON a.id=h.source_id JOIN commerce_return_lines l ON l.id=a.return_line_id WHERE l.return_id IN (?,?)").bind(first.id, last.id).all<{ id: string }>()).results;
    expect(holds).toHaveLength(2);
    for (const { id } of holds) {
      await inspectQualityHold(f.owner, id, { expectedVersion: 1, result: "pass", evidence: "Actual returned lot condition measured against the product", measurements: [], attachmentIds: [], attachmentSetIds: [] }, key());
      await releaseQualityHold(f.it, id, { expectedVersion: 2, reason: "Independent Quality review accepts this retained original lot" }, key());
    }
    expect(await db.prepare("SELECT inventory_quarantined_quantity(?,?) n").bind(source.warehouseId, source.productId).first("n")).toBe(0);
    const a = await refund(String(first.refundId), "50"), b = await refund(String(last.refundId), "100");
    expect(a).toMatchObject({ baseCarryingMinor: 180000, cashBaseMinor: 190000, realizedFxMinor: -10000 });
    expect(b).toMatchObject({ baseCarryingMinor: 360000, cashBaseMinor: 380000, realizedFxMinor: -20000 });
    expect(await balance(ids.receivable)).toBe(0);
    expect(await db.prepare(`${invoiceHistoryCte} SELECT historical_outstanding_minor,historical_base_outstanding_minor FROM historical_invoice_balances WHERE id=?`).bind(f.company, "2026-10-05", invoiceId).first()).toEqual({ historical_outstanding_minor: 0, historical_base_outstanding_minor: 0 });
    expect(await db.prepare("SELECT SUM(r.value_minor) value,SUM(r.quantity_milli) quantity FROM commerce_return_lines r JOIN commerce_returns h ON h.id=r.return_id WHERE h.fulfillment_id=?").bind(source.fulfillmentId).first()).toEqual({ value: 351235, quantity: 2000 });
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_refund_obligations WHERE invoice_id=? AND status='settled'").bind(invoiceId).first("n")).toBe(2);
  });
});
