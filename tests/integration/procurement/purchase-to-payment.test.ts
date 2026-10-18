import {verifySupplierBillProjections,verifySupplierBankProjections,verifySupplierBillFiles} from "../../support/helpers/procurement-sources";
import {packageGrantStore,readCompanyPackageGrants} from "../../../src/server/modules/commercial/public/package-grants";
import {reviewedSecondaryBook} from "../../support/helpers/finance-book-fixture";
import {enableAttachmentV2ForTest,seedAttachmentSet,attachmentBindingState} from "../../support/helpers/attachment-v2";
import {attachmentContent,createAttachmentSet} from "../../../src/server/modules/files/public/attachments/service";
import {getPayment,createInvoice,invoiceSchema,getInvoice} from "../../../src/server/modules/finance/public/finance";
import {financePurchaseControls} from "../../../src/server/modules/finance/public/purchases";
import {financeProductDocumentEntry} from "../../../src/server/modules/finance/public/document-purchases";
import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";

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
  supplierInvoiceBatchWorkspace,
  getSupplierBatch,
  listSupplierBatches,
} from "../../../src/server/modules/procurement/public/supplier-payments";
import { purchaseRequestProgress } from "../../../src/server/modules/procurement/public/purchase-request-progress";

vi.mock("server-only", () => ({}));
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));

afterEach(()=>vi.unstubAllEnvs());
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

async function rawSupplyFinance(){const {withPackageSettlementSources}=await import("../../../src/server/modules/commercial/public/package-settlement-sources"),{lockSupplyProcurementPackageObligationSources}=await import("../../../src/server/modules/procurement/public/package-obligations"),{lockSupplyInventoryPackageObligationSources}=await import("../../../src/server/modules/inventory/public/supply-package-obligations"),{lockSupplyFinancePackageObligationSources,supplyFinancePackageObligations}=await import("../../../src/server/modules/finance/public/supply-package-obligations");return withPackageSettlementSources(db,f.company,[lock=>lockSupplyProcurementPackageObligationSources(db,lock),lock=>lockSupplyInventoryPackageObligationSources(db,lock),lock=>lockSupplyFinancePackageObligationSources(db,lock)],database=>supplyFinancePackageObligations(db,database,{companyId:f.company,packageId:"supply",packageVersion:1,moduleId:"finance",now:Date.now()}));}
async function purchaseToPayment(enforced=false,withdraw=false,reversal=false,returnAudit=false,multipleCorrections=false){
 expect((await rawSupplyFinance()).outstanding).toBe(0);
 let unrelatedInvoice:string|null=null;
 const store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-purchase-to-payment-provider"}),obligations:async()=>{throw Error("No Supply inactive witness is invented");}}),grant={companyId:f.company,packageId:"supply",packageVersion:1,state:"active" as const,startsAt:0,endsAt:null,version:1,reason:"Actual consolidated Purchase to supplier settlement",evidence:"test:supply-ap-bank"};
 if(enforced){
  enableAttachmentV2ForTest();await db.prepare("UPDATE finance_accounts SET control_kind='bank',is_control=1 WHERE id=?").bind(f.accounts.cash).run();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await reviewedSecondaryBook(db,f.reviewer,f.manager,f.chart,f.period,"2026-09-01");
  // Retained provider observation from before purchase cutover; this does not
  // grant a Supply buyer the general bank-statement import command.
  await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:"DEMO-BANK-NOV-2026",statementDate:"2026-11-16",opening:"250000",closing:"0",sourceHash:key(),rows:multipleCorrections?[{externalId:"DEMO-PARTIAL-150000",transactionDate:"2026-11-16",description:"Original first partial laptop payment",amount:"-150000",currency:"THB"},{externalId:"DEMO-PARTIAL-100000",transactionDate:"2026-11-16",description:"Original final partial laptop payment",amount:"-100000",currency:"THB"}]:[{externalId:"DEMO-BANK-250000",transactionDate:"2026-11-16",description:"Consolidated laptop supplier payment",amount:"-250000",currency:"THB"}]},key());
  const unrelatedParty=await saveMaster(f.owner,"parties",null,{kind:"supplier",code:key(),name:"Retained unrelated standalone supplier"},key()),unrelated=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:String(unrelatedParty.id),kind:"payable",invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-10",currency:"THB",subtotal:"1",tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key());unrelatedInvoice=String(unrelated.id);await assign("invoice",unrelatedInvoice,"draft",f.manager);await postInvoice(f.manager,unrelatedInvoice,{expectedVersion:1,periodId:f.period,entryDate:"2026-09-02"},key());
  vi.stubEnv("ERPX_COMMERCE_PACKAGE_ENFORCEMENT","enforced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 }

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

  if(enforced)await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});
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
  if(enforced)await verifySupplierBillProjections(f,String(bill.id));
  await decideSupplierBill(f.manager, String(bill.id), { action: "approve", expectedVersion: 1, reason: "Both receipts, price and supplier evidence agree" }, key());
  const invoice = await invoiceFulfillment(f.reviewer, receipts[0].fulfillmentId, { expectedVersion: 2, date: invoiceDate, dueDate, matchId: String(bill.id), matchVersion: 2 }, key());
  const invoiceId = String(invoice.invoiceId);
  await assign("invoice", invoiceId, "draft", f.manager);
  await postInvoice(f.manager, invoiceId, { expectedVersion: 1, periodId: f.period, entryDate: invoiceDate }, key());

  expect((await rawSupplyFinance()).outstanding).toBeGreaterThan(0);
  let payable = await db.prepare("SELECT id,version,outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first<{ id: string; version: number; outstanding_minor: number }>();
  if(enforced){
   const named=await supplierInvoiceBatchWorkspace(f.reviewer,invoiceId);expect(named.invoices.map(row=>row.id)).toEqual([invoiceId]);expect(named.sourcePurchase).toMatchObject({capability:"finance.inventory-valuation",settleExisting:true,history:true});await expect(supplierInvoiceBatchWorkspace(f.reviewer,unrelatedInvoice!)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(getInvoice(f.reviewer,unrelatedInvoice!)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
   const before=await db.prepare("SELECT count(*) n FROM finance_supplier_batches WHERE company_id=?").bind(f.company).first("n");await expect(createSupplierBatch(f.reviewer,{bankAccountId:f.accounts.cash,plannedDate:"2026-11-15",items:[{invoiceId,expectedVersion:Number(payable!.version),amount:"1",beneficiaryName:"Actual purchased supplier",beneficiaryAccount:"FICTITIOUS-SOURCE"},{invoiceId:unrelatedInvoice!,expectedVersion:2,amount:"1",beneficiaryName:"Unrelated supplier cannot join",beneficiaryAccount:"FICTITIOUS-OTHER"}]},key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await db.prepare("SELECT count(*) n FROM finance_supplier_batches WHERE company_id=?").bind(f.company).first("n")).toBe(before);expect(await db.prepare("SELECT version FROM finance_invoices WHERE id=?").bind(invoiceId).first("version")).toBe(payable!.version);
   // Populate the retained mixed batch through its original normal command,
   // then enable the new boundary. This is the disposable migration fixture,
   // never a purchased grant or a SQL-created financial reservation.
   vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","disabled");let retainedId:string;
   try{const retained=await createSupplierBatch(f.reviewer,{bankAccountId:f.accounts.cash,plannedDate:"2026-11-15",items:[{invoiceId,expectedVersion:Number(payable!.version),amount:"1",beneficiaryName:"Actual purchased supplier",beneficiaryAccount:"FICTITIOUS-SOURCE"},{invoiceId:unrelatedInvoice!,expectedVersion:2,amount:"1",beneficiaryName:"Retained standalone supplier",beneficiaryAccount:"FICTITIOUS-OTHER"}]},key());retainedId=String(retained.id);}finally{vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");}
   await expect(getSupplierBatch(f.reviewer,retainedId!)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect((await listSupplierBatches(f.reviewer)).some(row=>row.id===retainedId)).toBe(false);await expect(exportSupplierBatch(f.payer,retainedId!,{expectedVersion:1},key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
   const assignmentVersion=Number(await db.prepare("SELECT version FROM assignments WHERE id=?").bind(f.reviewer.assignmentId).first("version"));expect(await financeProductDocumentEntry(db,{companyId:f.company,branchId:null,departmentId:null,userId:f.reviewer.userId,assignmentId:f.reviewer.assignmentId,assignmentVersion,role:f.reviewer.role},{kind:"supplier-batch",id:retainedId!})).toBeNull();
   await expect(createAttachmentSet(db,f.reviewer,{targetType:"finance_supplier_batch",targetId:retainedId!,purposeKey:"payment_batch_support",componentKind:"record",expectedTargetVersion:1},key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
   vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","disabled");try{await decideSupplierBatch(f.reviewer,retainedId!,{expectedVersion:1,reason:"Cancel the retained mixed migration fixture through its original owner"},key(),"cancel");}finally{vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");}
   payable=await db.prepare("SELECT id,version,outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first<{id:string;version:number;outstanding_minor:number}>();
  }
  if(multipleCorrections){await verifyMultiplePartialCorrections(invoiceId,unrelatedInvoice!);return;}
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
  if(!enforced)await importBankStatement(f.reviewer, {
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
  const instructionId=String(workspace.items[0].id),proof=enforced?await seedAttachmentSet(db,f.payer,{targetType:"finance_supplier_batch",targetId:batchId,purposeKey:"payment_proof",componentKind:"action",componentRef:`supplier-instruction:${instructionId}`,expectedTargetVersion:Number(workspace.batch!.version)}):null;
  const confirmation={
    expectedVersion: 2,
    bankTransactionId: bank!.id,
    bankVersion: Number(bank!.version),
    periodId: f.period,
    bankReference: statementReference,
    evidence: "Exact outgoing bank transaction matched to approved supplier instruction",
    attachmentSetIds:proof?[proof.setId]:[],
  },confirmationKey=key();
  if(enforced){
   const fault=`supplier_batch_source_${key().replaceAll('-','')}`,tables=["commerce_fulfillments","finance_supplier_batches","finance_supplier_instructions","finance_invoices","finance_payments","finance_allocations","finance_journals","finance_journal_lines","finance_book_postings","finance_bank_transactions","attachment_sets","attachment_links","attachment_events","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE commerce_fulfillments SET invoice_id=NULL WHERE invoice_id='${invoiceId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${confirmationKey}') EXECUTE FUNCTION ${fault}()`);
   try{await expect(confirmSupplierInstruction(f.payer,instructionId,confirmation,confirmationKey)).rejects.toMatchObject({code:"FINANCE_DOCUMENT_SOURCE_CHANGED"});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
  }
  const confirmed=await confirmSupplierInstruction(f.payer,instructionId,confirmation,confirmationKey);expect(await confirmSupplierInstruction(f.payer,instructionId,confirmation,confirmationKey)).toEqual(confirmed);
  if(proof)expect(await attachmentBindingState(db,proof.setId)).toMatchObject({set_state:"bound",link_state:"bound"});

  expect(await db.prepare("SELECT outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({outstanding_minor:0,base_outstanding_minor:0});
  expect(await db.prepare("SELECT amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor FROM finance_allocations WHERE payment_id=?").bind(confirmed.paymentId).first()).toEqual({amount_minor:25000000,base_invoice_minor:25000000,base_payment_minor:25000000,realized_fx_minor:0});
  expect((await rawSupplyFinance()).outstanding).toBe(0);
  if(returnAudit){
   const {withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations}=await import("../../../src/server/modules/commercial/public/owner-package-obligations"),{packageGrantRevision}=await import("../../../src/server/modules/commercial/public/package-grants"),scope={companyId:f.company,packageId:"supply",packageVersion:1},observe=()=>withOwnerPackageObligations(db,scope,database=>readOwnerPackageObligations(db,database,{...scope,now:Date.now()}));
   expect((await observe()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([["procurement",0],["inventory",0],["quality",0],["finance",0]]);
   const {requestCommerceReturn,decideCommerceReturn}=await import("../../../src/server/modules/commerce/public/commerce"),fulfillmentId=String(receipts[0].fulfillmentId),line=await db.prepare("SELECT id FROM commerce_fulfillment_lines WHERE fulfillment_id=? ORDER BY id LIMIT 1").bind(fulfillmentId).first<{id:string}>(),version=Number(await db.prepare("SELECT version FROM commerce_fulfillments WHERE id=?").bind(fulfillmentId).first("version")),returnKey=key(),returnInput={expectedVersion:version,date:"2026-11-16",reason:"Supplier asks independent review of an actual received laptop after original AP settlement",lines:[{lineId:line!.id,quantity:"1"}]};
   const pending=await requestCommerceReturn(buyer,fulfillmentId,returnInput,returnKey);expect(await requestCommerceReturn(buyer,fulfillmentId,returnInput,returnKey)).toEqual(pending);expect(await db.prepare("SELECT status FROM commerce_returns WHERE id=?").bind(pending.id).first("status")).toBe("requested");
   const grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==="supply")!,request={companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:"inactive" as const,version:current.version+1,reason:"Pending SupplierReturn must be reviewed before software withdrawal",evidence:"test:supply-pending-return"}}]},normal=packageGrantStore(db,{authorize:async()=>({userId:"isolated-original-Supply-provider"}),obligations:(database,companyId,packageId)=>ownerPackageObligations(db,database,companyId,packageId)}),review=(apply=false)=>withOwnerPackageObligations(db,scope,database=>apply?normal.batch.applyIn(database,request,async()=>{}):normal.batch.previewIn(database,request));
   expect((await observe()).observations.find(row=>row.moduleId==="procurement")!.outstanding).toBeGreaterThan(0);await expect(review()).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");await expect(review(true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
   const rejectionKey=key(),rejectionInput={expectedVersion:1,reason:"Independent supplier and warehouse evidence confirms the delivered laptop will be retained"},rejected=await decideCommerceReturn(purchasingManager,String(pending.id),"reject",rejectionInput,rejectionKey);expect(await decideCommerceReturn(purchasingManager,String(pending.id),"reject",rejectionInput,rejectionKey)).toEqual(rejected);
   expect((await observe()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([["procurement",0],["inventory",0],["quality",0],["finance",0]]);const preview=await review();expect(await review(true)).toEqual(preview);return;
  }
  if(reversal){
   const {reversePayment,createPayment,approvePayment,releasePayment,confirmPayment}=await import("../../../src/server/modules/finance/public/finance");
   const {supplierInvoiceCorrectionControls}=await import("../../../src/server/modules/finance/public/supplier-payment-corrections");
   const originalPaymentId=String(confirmed.paymentId),reverseKey=key(),reverseInput={expectedVersion:1,periodId:f.period,entryDate:"2026-11-16",reason:"Independently correct the actual original supplier payment allocation"};
   const tables=["commerce_fulfillments","finance_payments","finance_payment_reversals","finance_invoices","finance_allocations","finance_journals","finance_journal_lines","finance_book_postings","finance_financial_events","finance_bank_transactions","assignments","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results));
   const revocation=`supplier_correction_iam_${key().replaceAll('-','')}`,beforeReverse=await snapshot();
   await db.exec(`CREATE FUNCTION ${revocation}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${revocation} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${reverseKey}') EXECUTE FUNCTION ${revocation}()`);
   try{await expect(reversePayment(f.manager,originalPaymentId,reverseInput,reverseKey)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(await snapshot()).toEqual(beforeReverse);}finally{await db.exec(`DROP TRIGGER ${revocation} ON audit_events;DROP FUNCTION ${revocation}()`);}
   const reversed=await reversePayment(f.manager,originalPaymentId,reverseInput,reverseKey);expect(await reversePayment(f.manager,originalPaymentId,reverseInput,reverseKey)).toEqual(reversed);
   expect((await getPayment(f.manager,originalPaymentId)).sourcePurchase).toMatchObject({capability:"finance.inventory-valuation",history:true,settleExisting:true});
   expect(await db.prepare("SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({status:"posted",outstanding_minor:25000000,base_outstanding_minor:25000000});expect((await rawSupplyFinance()).outstanding).toBeGreaterThan(0);
   expect(await supplierInvoiceCorrectionControls(f.reviewer,invoiceId)).toMatchObject({correctionPaymentId:originalPaymentId,amountMinor:25000000,bankAccountId:f.accounts.cash,settleExisting:true});
   const partyId=String(await db.prepare("SELECT party_id FROM finance_invoices WHERE id=?").bind(invoiceId).first("party_id")),bankVersion=Number(await db.prepare("SELECT version FROM finance_bank_transactions WHERE id=?").bind(bank!.id).first("version"));
   // The original SupplierBatch keeps the old bank instruction immutable. Its
   // bank-row uniqueness is not bypassed: the established controlled Payment
   // correction owner can allocate the original observation to its real source.
   const createInput={partyId,kind:"payment" as const,paymentNumber:key(),paymentDate:"2026-11-16",currency:"THB",amount:"250000",bankAccount:f.accounts.cash,settlementInvoiceId:invoiceId,correctionPaymentId:originalPaymentId};
   const originalCount=await db.prepare("SELECT count(*) n FROM finance_payments").first("n");
   for(const invalid of [{...createInput,correctionPaymentId:key()},{...createInput,settlementInvoiceId:unrelatedInvoice!}])await expect(createPayment(f.reviewer,invalid,key())).rejects.toMatchObject({code:"SUPPLIER_CORRECTION_SOURCE_REQUIRED"});
   await expect(createPayment(f.reviewer,{...createInput,amount:"1"},key())).rejects.toMatchObject({code:"SUPPLIER_CORRECTION_AMOUNT_MISMATCH"});
   const {correctionPaymentId:_original,...unanchored}=createInput;void _original;await expect(createPayment(f.reviewer,unanchored,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
   expect(await db.prepare("SELECT count(*) n FROM finance_payments").first("n")).toBe(originalCount);
   const createKey=key(),fault=`supplier_correction_source_${key().replaceAll('-','')}`,beforeCreate=await snapshot();
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE commerce_fulfillments SET invoice_id=NULL WHERE invoice_id='${invoiceId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${createKey}') EXECUTE FUNCTION ${fault}()`);
   try{await expect(createPayment(f.reviewer,createInput,createKey)).rejects.toMatchObject({code:"SUPPLIER_PAYMENT_CORRECTION_SOURCE_CHANGED"});expect(await snapshot()).toEqual(beforeCreate);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
   const replacement=await createPayment(f.reviewer,createInput,createKey),replacementId=String(replacement.id);expect(await createPayment(f.reviewer,createInput,createKey)).toEqual(replacement);
   expect((await getPayment(f.manager,replacementId)).sourcePurchase).toMatchObject({capability:"finance.inventory-valuation",history:true,settleExisting:true});expect(await supplierInvoiceCorrectionControls(f.reviewer,invoiceId)).toBeUndefined();
   await expect(createPayment(f.reviewer,{...createInput,paymentNumber:key()},key())).rejects.toMatchObject({code:"SUPPLIER_CORRECTION_ALREADY_PREPARED"});
   await approvePayment(f.manager,replacementId,{expectedVersion:1,proof:"Independent correction of the actual original supplier allocation"},key());await releasePayment(f.payer,replacementId,{expectedVersion:2,proof:"Independent Treasury verifies the original retained outgoing bank observation"},key());
   const replacementProof=await seedAttachmentSet(db,f.payer,{targetType:"finance_payment",targetId:replacementId,purposeKey:"payment_proof",componentKind:"action",componentRef:"bank-confirmation",expectedTargetVersion:3});
   const replacementKey=key(),replacementInput={expectedVersion:3,periodId:f.period,bankTransactionId:bank!.id,bankVersion,bankReference:statementReference,proof:"Exact retained original debit is allocated through the normal controlled correction",allocations:[{invoiceId,amount:"250000"}],attachmentSetIds:[replacementProof.setId]};
   await expect(confirmPayment(f.payer,replacementId,{...replacementInput,bankTransactionId:key()},key())).rejects.toMatchObject({code:"SUPPLIER_CORRECTION_BANK_SOURCE_INVALID"});
   const replacementResult=await confirmPayment(f.payer,replacementId,replacementInput,replacementKey);expect(await confirmPayment(f.payer,replacementId,replacementInput,replacementKey)).toEqual(replacementResult);
   expect(await attachmentBindingState(db,replacementProof.setId)).toMatchObject({set_state:"bound",link_state:"bound"});
   const entryActor={companyId:f.company,branchId:null,departmentId:null,userId:f.reviewer.userId,assignmentId:f.reviewer.assignmentId,assignmentVersion:Number(await db.prepare("SELECT version FROM assignments WHERE id=?").bind(f.reviewer.assignmentId).first("version")),role:f.reviewer.role};
   for(const target of [{kind:"payment" as const,id:originalPaymentId},{kind:"payment" as const,id:replacementId},{kind:"journal" as const,id:String(reversed.reversalJournalId)}])expect(await financeProductDocumentEntry(db,entryActor,target)).toEqual({capability:"finance.inventory-valuation",purchasePurpose:"history"});
   const {withholdingWorkspace}=await import("../../../src/server/modules/finance/public/finance-withholding");await expect(withholdingWorkspace(f.reviewer,replacementId)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
   expect(await db.prepare("SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({status:"paid",outstanding_minor:0,base_outstanding_minor:0});expect((await rawSupplyFinance()).outstanding).toBe(0);
   const {withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations}=await import("../../../src/server/modules/commercial/public/owner-package-obligations"),{packageGrantRevision}=await import("../../../src/server/modules/commercial/public/package-grants"),scope={companyId:f.company,packageId:"supply",packageVersion:1};
   const raw=await withOwnerPackageObligations(db,scope,database=>readOwnerPackageObligations(db,database,{...scope,now:Date.now()}));expect(raw.observations.map(row=>[row.moduleId,row.outstanding])).toEqual([["procurement",0],["inventory",0],["quality",0],["finance",0]]);
   const grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==="supply")!,request={companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:"inactive" as const,version:current.version+1,reason:"Actual independent correction settled the retained supplier bank observation",evidence:"test:supply-normal-correction-withdrawal"}}]},normal=packageGrantStore(db,{authorize:async()=>({userId:"isolated-original-Supply-provider"}),obligations:(database,companyId,packageId)=>ownerPackageObligations(db,database,companyId,packageId)}),review=(apply=false)=>withOwnerPackageObligations(db,scope,database=>apply?normal.batch.applyIn(database,request,async()=>{}):normal.batch.previewIn(database,request));
   const objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(replacementProof.sourceId).first("object_key")),bytes="%PDF-1.7 actual corrected Supplier payment evidence";await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(replacementProof.sourceId).run();
   const preview=await review();expect(await review(true)).toEqual(preview);expect(await review(true)).toEqual(preview);
   expect(await createPayment(f.reviewer,createInput,createKey)).toEqual(replacement);expect(await reversePayment(f.manager,originalPaymentId,reverseInput,reverseKey)).toEqual(reversed);expect(await confirmPayment(f.payer,replacementId,replacementInput,replacementKey)).toEqual(replacementResult);
   expect((await getPayment(f.payer,replacementId)).sourcePurchase).toMatchObject({settleExisting:false,history:true});expect(await (await attachmentContent(db,env.H001_FILES,f.payer,replacementProof.linkId,"original")).object.text()).toBe(bytes);
   await expect(reversePayment(f.manager,replacementId,{...reverseInput,expectedVersion:4},key())).rejects.toMatchObject({code:"PACKAGE_INACTIVE"});
   await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.payer.assignmentId).run();await expect(confirmPayment(f.payer,replacementId,replacementInput,replacementKey)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});await expect(attachmentContent(db,env.H001_FILES,f.payer,replacementProof.linkId,"original")).rejects.toMatchObject({status:404});expect(await (await env.H001_FILES.get(objectKey))!.text()).toBe(bytes);return;
  }
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
 if(enforced){
  expect((await readCompanyPackageGrants(db,f.company)).map(row=>[row.packageId,row.state])).toEqual([["supply","draining"]]);expect((await getPayment(f.reviewer,String(confirmed.paymentId))).payment.id).toBe(confirmed.paymentId);
  const assignmentVersion=Number(await db.prepare("SELECT version FROM assignments WHERE id=?").bind(f.reviewer.assignmentId).first("version"));expect(await financeProductDocumentEntry(db,{companyId:f.company,branchId:null,departmentId:null,userId:f.reviewer.userId,assignmentId:f.reviewer.assignmentId,assignmentVersion,role:f.reviewer.role},{kind:"supplier-batch",id:batchId})).toEqual({capability:"finance.inventory-valuation",purchasePurpose:"history"});
  expect(await financeProductDocumentEntry(db,{companyId:f.company,branchId:f.branch,departmentId:null,userId:f.reviewer.userId,assignmentId:f.reviewer.assignmentId,assignmentVersion,role:f.reviewer.role},{kind:"supplier-batch",id:batchId})).toBeNull();
  await verifySupplierBillFiles(f,String(bill.id));await verifySupplierBankProjections(f,String(bill.id),batchId);
  if(withdraw){
   const {withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations}=await import("../../../src/server/modules/commercial/public/owner-package-obligations"),{packageGrantRevision}=await import("../../../src/server/modules/commercial/public/package-grants"),scope={companyId:f.company,packageId:"supply",packageVersion:1};
   const raw=await withOwnerPackageObligations(db,scope,database=>readOwnerPackageObligations(db,database,{...scope,now:Date.now()}));expect(raw.observations.map(row=>[row.moduleId,row.outstanding])).toEqual([["procurement",0],["inventory",0],["quality",0],["finance",0]]);
   const grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==="supply")!,request={companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:"inactive" as const,version:current.version+1,reason:"Actual independent consolidated AP and supplier bank duties are settled",evidence:"test:supply-complete-ap-withdrawal"}}]},normal=packageGrantStore(db,{authorize:async()=>({userId:"isolated-original-Supply-provider"}),obligations:(database,companyId,packageId)=>ownerPackageObligations(db,database,companyId,packageId)});
   const review=(apply=false)=>withOwnerPackageObligations(db,scope,database=>apply?normal.batch.applyIn(database,request,async()=>{}):normal.batch.previewIn(database,request));
   const objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(proof!.sourceId).first("object_key")),bytes="%PDF-1.7 actual independently confirmed Supply bank evidence";await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(proof!.sourceId).run();
   // A terminal label plus an unproven version jump is not a File receipt.
   const originalVersion=Number(await db.prepare("SELECT version FROM procurement_bill_matches WHERE id=?").bind(bill.id).first("version"));await db.prepare("UPDATE procurement_bill_matches SET version=version+1 WHERE id=?").bind(bill.id).run();
   try{await expect(review()).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");}finally{await db.prepare("UPDATE procurement_bill_matches SET version=? WHERE id=?").bind(originalVersion,bill.id).run();}
   const preview=await review();expect(await review(true)).toEqual(preview);expect(await review(true)).toEqual(preview);expect((await readCompanyPackageGrants(db,f.company)).map(row=>[row.packageId,row.state])).toEqual([["supply","inactive"]]);
   expect(await confirmSupplierInstruction(f.payer,instructionId,confirmation,confirmationKey)).toEqual(confirmed);expect((await getPayment(f.reviewer,String(confirmed.paymentId))).payment.id).toBe(confirmed.paymentId);expect((await supplierBatchWorkspace(f.payer,batchId)).sourcePurchase).toMatchObject({settleExisting:false,history:true});
   expect(await (await attachmentContent(db,env.H001_FILES,f.payer,proof!.linkId,"original")).object.text()).toBe(bytes);await expect(createAttachmentSet(db,f.payer,{targetType:"finance_supplier_batch",targetId:batchId,purposeKey:"payment_batch_support",componentKind:"record",expectedTargetVersion:Number((await supplierBatchWorkspace(f.payer,batchId)).batch!.version)},key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
   await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.payer.assignmentId).run();await expect(confirmSupplierInstruction(f.payer,instructionId,confirmation,confirmationKey)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});await expect(attachmentContent(db,env.H001_FILES,f.payer,proof!.linkId,"original")).rejects.toMatchObject({status:404});expect(await (await env.H001_FILES.get(objectKey))!.text()).toBe(bytes);return;
  }
  // Actual target classification remains visible alongside an ordinary full
  // Banking purchase. The pure presentation contract separately covers a
  // retained-history-only Banking period without faking a withdrawal witness.
  const retainedFinance={...grant,packageId:"finance",reason:"Banking visibility alongside actual Supply settlement",evidence:"test:supplier-source-fallback"};await store.apply({expectedVersion:0,idempotencyKey:key(),grant:retainedFinance});
  expect((await financePurchaseControls(f.payer))?.["finance.banking"]).toMatchObject({history:true});expect((await supplierBatchWorkspace(f.payer,batchId)).sourcePurchase).toMatchObject({capability:"finance.inventory-valuation",settleExisting:true,history:true});expect((await supplierInvoiceBatchWorkspace(f.reviewer,invoiceId)).sourcePurchase).toMatchObject({settleExisting:true});expect((await supplierBatchWorkspace(f.reviewer)).sourcePurchase).toBeUndefined();
  await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.payer.assignmentId).run();await expect(confirmSupplierInstruction(f.payer,instructionId,confirmation,confirmationKey)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});
 }
}
it("runs an approved ten-laptop request through RFQ, 6+4 receipts, one consolidated AP invoice, bank evidence and payment",()=>purchaseToPayment());
it("runs Supply-only RFQ and 6+4 receipts through drained consolidated AP, independent posting, exact bank-batch source fences and retained receipt history",()=>purchaseToPayment(true));

it("withdraws Supply only after actual consolidated AP and independent bank settlement, retaining source history, exact replay and Legal-held bytes without full Finance",()=>purchaseToPayment(true,true));

it("retains actual normal supplier payment reversal and replacement history without inventing a perpetual Supply Finance obligation",()=>purchaseToPayment(true,false,true));

it("blocks Supply withdrawal for an actual pending SupplierReturn after fully paid AP until independent normal rejection",()=>purchaseToPayment(true,false,false,true));

async function verifyMultiplePartialCorrections(invoiceId:string,unrelatedInvoiceId:string){
 const {reversePayment,createPayment,approvePayment,releasePayment,confirmPayment}=await import("../../../src/server/modules/finance/public/finance"),{supplierInvoiceCorrections}=await import("../../../src/server/modules/finance/public/supplier-payment-corrections"),{supplierCorrectionInput}=await import("../../../src/lib/finance/supplier-payment-correction"),{withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations}=await import("../../../src/server/modules/commercial/public/owner-package-obligations"),{packageGrantRevision}=await import("../../../src/server/modules/commercial/public/package-grants");
 const scope={companyId:f.company,packageId:"supply",packageVersion:1},observe=()=>withOwnerPackageObligations(db,scope,database=>readOwnerPackageObligations(db,database,{...scope,now:Date.now()})),originals:{paymentId:string;amount:string;amountMinor:number;bankId:string;bankReference:string;reverseKey:string;reverseInput:{expectedVersion:number;periodId:string;entryDate:string;reason:string};reversed?:unknown}[]=[];
 for(const amount of ["150000","100000"]){
  const invoice=await db.prepare("SELECT version FROM finance_invoices WHERE id=?").bind(invoiceId).first<{version:number}>(),batch=await createSupplierBatch(f.reviewer,{bankAccountId:f.accounts.cash,plannedDate:"2026-11-15",items:[{invoiceId,expectedVersion:Number(invoice!.version),amount,beneficiaryName:"Actual original partial supplier payment",beneficiaryAccount:"SANDBOX-PARTIAL-SUPPLIER"}]},key()),batchId=String(batch.id);
  await decideSupplierBatch(f.manager,batchId,{expectedVersion:1,reason:"Independent review of this exact partial supplier instruction"},key(),"approve");await exportSupplierBatch(f.payer,batchId,{expectedVersion:2},key());
  const workspace=await supplierBatchWorkspace(f.payer,batchId),instructionId=String(workspace.items[0].id),bankReference=`DEMO-PARTIAL-${amount}`,bank=await db.prepare("SELECT id,version FROM finance_bank_transactions WHERE company_id=? AND external_id=?").bind(f.company,bankReference).first<{id:string;version:number}>(),proof=await seedAttachmentSet(db,f.payer,{targetType:"finance_supplier_batch",targetId:batchId,purposeKey:"payment_proof",componentKind:"action",componentRef:`supplier-instruction:${instructionId}`,expectedTargetVersion:Number(workspace.batch!.version)}),confirmationKey=key(),confirmation={expectedVersion:2,bankTransactionId:bank!.id,bankVersion:Number(bank!.version),periodId:f.period,bankReference,evidence:"Actual independent original partial SupplierBatch bank confirmation",attachmentSetIds:[proof.setId]},confirmed=await confirmSupplierInstruction(f.payer,instructionId,confirmation,confirmationKey);
  expect(await confirmSupplierInstruction(f.payer,instructionId,confirmation,confirmationKey)).toEqual(confirmed);expect(await db.prepare("SELECT amount_minor,base_invoice_minor,base_payment_minor FROM finance_allocations WHERE payment_id=?").bind(confirmed.paymentId).first()).toEqual({amount_minor:Number(amount)*100,base_invoice_minor:Number(amount)*100,base_payment_minor:Number(amount)*100});
  originals.push({paymentId:String(confirmed.paymentId),amount,amountMinor:Number(amount)*100,bankId:bank!.id,bankReference,reverseKey:key(),reverseInput:{expectedVersion:1,periodId:f.period,entryDate:"2026-11-16",reason:"Independent review corrects this actual original partial supplier payment"}});
 }
 expect(await db.prepare("SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({status:"paid",outstanding_minor:0,base_outstanding_minor:0});expect((await observe()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([["procurement",0],["inventory",0],["quality",0],["finance",0]]);
 for(const original of originals){original.reversed=await reversePayment(f.manager,original.paymentId,original.reverseInput,original.reverseKey);expect(await reversePayment(f.manager,original.paymentId,original.reverseInput,original.reverseKey)).toEqual(original.reversed);}
 expect(await db.prepare("SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({status:"posted",outstanding_minor:25000000,base_outstanding_minor:25000000});
 const options=await supplierInvoiceCorrections(f.reviewer,invoiceId);expect(options).toMatchObject({capability:"finance.inventory-valuation",settleExisting:true,history:true});expect(options!.sources.map(row=>row.paymentId).sort()).toEqual(originals.map(row=>row.paymentId).sort());expect(options!.sources.map(row=>row.amountMinor).sort()).toEqual([10000000,15000000]);expect(options!.sources.every(row=>row.bankAccountId===f.accounts.cash&&row.bankAccountLabel.length>0&&row.paymentNumber.length>0)).toBe(true);expect(await supplierInvoiceCorrections(f.reviewer,unrelatedInvoiceId)).toBeUndefined();expect(await supplierInvoiceCorrections({...f.reviewer,branchId:f.branch},invoiceId)).toBeUndefined();
 const partyId=String(await db.prepare("SELECT party_id FROM finance_invoices WHERE id=?").bind(invoiceId).first("party_id")),selected=options!.sources[1],other=options!.sources[0],originalCount=await db.prepare("SELECT count(*) n FROM finance_payments").first("n"),selectedInput=supplierCorrectionInput(selected,{id:invoiceId,partyId,currency:"THB"},{paymentNumber:key(),paymentDate:"2026-11-16"});
 await expect(createPayment(f.reviewer,{...selectedInput,amount:String(other.amountMinor/100)},key())).rejects.toMatchObject({code:"SUPPLIER_CORRECTION_AMOUNT_MISMATCH"});expect(await db.prepare("SELECT count(*) n FROM finance_payments").first("n")).toBe(originalCount);
 const replacements:{id:string;input:typeof selectedInput;key:string;source:typeof selected;original:typeof originals[number];confirmKey?:string;confirmInput?:{expectedVersion:number;periodId:string;bankTransactionId:string;bankVersion:number;bankReference:string;proof:string;allocations:{invoiceId:string;amount:string}[];attachmentSetIds:string[]};confirmed?:unknown;proof?:Awaited<ReturnType<typeof seedAttachmentSet>>}[]=[];
 // Explicitly choose the non-first option. Each captured body keeps that
 // original identity; the remaining choice is available independently.
 for(const source of [selected,other]){
  const input=supplierCorrectionInput(source,{id:invoiceId,partyId,currency:"THB"},{paymentNumber:key(),paymentDate:"2026-11-16"}),createKey=key(),[created,concurrentReplay]=await Promise.all([createPayment(f.reviewer,input,createKey),createPayment(f.reviewer,input,createKey)]),id=String(created.id);expect(concurrentReplay).toEqual(created);expect(await createPayment(f.reviewer,input,createKey)).toEqual(created);expect(created).toMatchObject({correctionPaymentId:source.paymentId,settlementInvoiceId:invoiceId,amount:input.amount,status:"draft"});
  await expect(createPayment(f.reviewer,{...input,paymentNumber:key()},key())).rejects.toMatchObject({code:"SUPPLIER_CORRECTION_ALREADY_PREPARED"});const remaining=await supplierInvoiceCorrections(f.reviewer,invoiceId);expect(remaining!.sources.some(row=>row.paymentId===source.paymentId)).toBe(false);expect(remaining!.prepared).toContainEqual({paymentId:id,paymentNumber:input.paymentNumber,status:"draft",originalPaymentId:source.paymentId});expect((await getPayment(f.manager,id)).sourcePurchase).toMatchObject({capability:"finance.inventory-valuation",settleExisting:true,history:true});replacements.push({id,input,key:createKey,source,original:originals.find(row=>row.paymentId===source.paymentId)!});
 }
 expect(await db.prepare("SELECT count(*) n FROM finance_payments").first("n")).toBe(Number(originalCount)+2); expect((await supplierInvoiceCorrections(f.reviewer,invoiceId))!.sources).toEqual([]);expect((await supplierInvoiceCorrections(f.reviewer,invoiceId))!.prepared!.map(row=>row.paymentId).sort()).toEqual(replacements.map(row=>row.id).sort());
 for(const replacement of replacements){
  await approvePayment(f.manager,replacement.id,{expectedVersion:1,proof:"Independent approver matches this selected original partial payment"},key());await releasePayment(f.payer,replacement.id,{expectedVersion:2,proof:"Separate Treasury payer checks the exact selected original bank observation"},key());
  const workspace=await getPayment(f.payer,replacement.id);expect(workspace.bankTransactions.map(row=>row.id)).toEqual([replacement.original.bankId]);expect(workspace.invoices.map(row=>row.id)).toEqual([invoiceId]);const bankVersion=Number(await db.prepare("SELECT version FROM finance_bank_transactions WHERE id=?").bind(replacement.original.bankId).first("version")),proof=await seedAttachmentSet(db,f.payer,{targetType:"finance_payment",targetId:replacement.id,purposeKey:"payment_proof",componentKind:"action",componentRef:"bank-confirmation",expectedTargetVersion:3}),confirmInput={expectedVersion:3,periodId:f.period,bankTransactionId:replacement.original.bankId,bankVersion,bankReference:replacement.original.bankReference,proof:"Selected original partial bank observation reconciles only this exact independently reviewed correction",allocations:[{invoiceId,amount:replacement.original.amount}],attachmentSetIds:[proof.setId]},confirmKey=key();
  await expect(confirmPayment(f.payer,replacement.id,{...confirmInput,bankTransactionId:originals.find(row=>row.paymentId!==replacement.source.paymentId)!.bankId},key())).rejects.toMatchObject({code:"SUPPLIER_CORRECTION_BANK_SOURCE_INVALID"});const [confirmed,concurrentConfirmation]=await Promise.all([confirmPayment(f.payer,replacement.id,confirmInput,confirmKey),confirmPayment(f.payer,replacement.id,confirmInput,confirmKey)]);expect(concurrentConfirmation).toEqual(confirmed);replacement.confirmed=confirmed;replacement.confirmKey=confirmKey;replacement.confirmInput=confirmInput;replacement.proof=proof;expect(await confirmPayment(f.payer,replacement.id,confirmInput,confirmKey)).toEqual(replacement.confirmed);expect(await db.prepare("SELECT amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor FROM finance_allocations WHERE payment_id=?").bind(replacement.id).first()).toEqual({amount_minor:replacement.source.amountMinor,base_invoice_minor:replacement.source.amountMinor,base_payment_minor:replacement.source.amountMinor,realized_fx_minor:0});
 }
 expect(await db.prepare("SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({status:"paid",outstanding_minor:0,base_outstanding_minor:0});expect((await observe()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([["procurement",0],["inventory",0],["quality",0],["finance",0]]);
 const grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==="supply")!,request={companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:"inactive" as const,version:current.version+1,reason:"Both actual original partial supplier payments were independently corrected and settled",evidence:"test:supply-multiple-partial-corrections"}}]},normal=packageGrantStore(db,{authorize:async()=>({userId:"isolated-original-Supply-provider"}),obligations:(database,companyId,packageId)=>ownerPackageObligations(db,database,companyId,packageId)}),review=(apply=false)=>withOwnerPackageObligations(db,scope,database=>apply?normal.batch.applyIn(database,request,async()=>{}):normal.batch.previewIn(database,request));
 const proof=replacements[0].proof!,objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(proof.sourceId).first("object_key")),bytes="%PDF-1.7 exact independently corrected partial supplier bank evidence";await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(proof.sourceId).run();const preview=await review();expect(await review(true)).toEqual(preview);expect(await review(true)).toEqual(preview);
 expect(await supplierInvoiceCorrections(f.reviewer,invoiceId)).toMatchObject({history:true,settleExisting:false,sources:[]});expect((await supplierInvoiceCorrections(f.reviewer,invoiceId))!.prepared!.map(row=>[row.paymentId,row.status]).sort()).toEqual(replacements.map(row=>[row.id,"settled"]).sort());
 for(const replacement of replacements){expect(await createPayment(f.reviewer,replacement.input,replacement.key)).toMatchObject({id:replacement.id});expect(await confirmPayment(f.payer,replacement.id,replacement.confirmInput!,replacement.confirmKey!)).toEqual(replacement.confirmed);expect((await getPayment(f.payer,replacement.id)).sourcePurchase).toMatchObject({history:true,settleExisting:false});}for(const original of originals)expect(await reversePayment(f.manager,original.paymentId,original.reverseInput,original.reverseKey)).toEqual(original.reversed);
 expect(await (await attachmentContent(db,env.H001_FILES,f.payer,proof.linkId,"original")).object.text()).toBe(bytes);await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.payer.assignmentId).run();await expect(confirmPayment(f.payer,replacements[0].id,replacements[0].confirmInput!,replacements[0].confirmKey!)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});await expect(attachmentContent(db,env.H001_FILES,f.payer,proof.linkId,"original")).rejects.toMatchObject({status:404});expect(await (await env.H001_FILES.get(objectKey))!.text()).toBe(bytes);
}
it("explicitly selects between two independently reversed partial Supplier payments and settles both exact original bank observations without full Banking",()=>purchaseToPayment(true,false,false,false,true));

async function privateBillFixture(){
 enableAttachmentV2ForTest();const warehouseMaster=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Private payable source warehouse'},key()),product=await saveMaster(f.owner,'products',null,{sku:key(),name:'Private payable source product',price:'10'},key()),party=await saveMaster(f.owner,'parties',null,{kind:'supplier',code:key(),name:'Private payable original supplier'},key()),order=await saveOrder(buyer,null,orderSchema.parse({kind:'purchase',partyId:party.id,warehouseId:warehouseMaster.id,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product.id,quantity:'1',price:'10',taxBps:0}]}),key()),orderId=String(order.id);
 await transitionOrder(buyer,orderId,{action:'submit',expectedVersion:1},key());await assign('order',orderId,'submitted',purchasingManager);await transitionOrder(purchasingManager,orderId,{action:'approve',expectedVersion:2},key());await assign('order',orderId,'confirmed',warehouse);await assign('order',orderId,'stock_gl',f.manager);const row=await getOrder(f.owner,orderId),received=await fulfillOrder(warehouse,orderId,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:String(row.lines[0].id),quantity:'1'}]},key()),fulfillmentId=String(received.fulfillmentId),journalId=String(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(fulfillmentId).first('journal_id'));await assign('journal',journalId,'draft',f.manager);await postJournal(f.manager,journalId,key(),{expectedVersion:1});await assign('order',orderId,'invoicing',f.reviewer);
 const lineId=String(await db.prepare('SELECT id FROM commerce_fulfillment_lines WHERE fulfillment_id=?').bind(fulfillmentId).first('id')),bill=await captureSupplierBill(f.reviewer,{fulfillmentId,expectedVersion:1,reference:key(),invoiceDate:'2026-09-02',dueDate:'2026-09-03',evidence:'Original actual supplier document agrees to received physical quantity',lines:[{fulfillmentLineId:lineId,quantity:'1',price:'10',taxBps:0}]},key());await decideSupplierBill(f.manager,String(bill.id),{action:'approve',expectedVersion:1,reason:'Original independent receiving and supplier price review'},key());const invoice=await invoiceFulfillment(f.reviewer,fulfillmentId,{expectedVersion:2,date:'2026-09-02',dueDate:'2026-09-03',matchId:String(bill.id),matchVersion:2},key()),id=String(invoice.invoiceId);await assign('invoice',id,'draft',f.manager);await postInvoice(f.manager,id,{expectedVersion:1,periodId:f.period,entryDate:'2026-09-02'},key());return {id,orderId,partyId:String(party.id)};
}
it('fresh supplier bill alias private bytes retain actual independently posted Purchase AP and held evidence under Supply-only draining',async()=>{
 const source=await privateBillFixture(),{bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service'),file=await seedAttachmentSet(db,f.reviewer,{targetType:'supplier_bill',targetId:source.id,purposeKey:'invoice_document',componentKind:'record',expectedTargetVersion:2});await bindRecordAttachmentSet(db,f.reviewer,file.setId,2,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes='Retained exact independently posted source payable document';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();
 vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(attachmentContent(db,env.H001_FILES,f.reviewer,file.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const grant={companyId:f.company,packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Exact original payable source history only',evidence:'test:supplier-bill-private-alias'},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-payable-files-provider'}),obligations:async()=>{throw Error('PAYABLE_FILE_FIXTURE_CANNOT_INACTIVATE');}});await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});
 let consumed=0;const files:import('../../../src/server/modules/runtime/public/runtime/types').FileStore={get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;},put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};expect(await(await attachmentContent(db,files,f.reviewer,file.linkId,'original')).object.text()).toBe(bytes);await expect(db.transaction(tx=>attachmentContent(tx,files,f.reviewer,file.linkId,'original'))).rejects.toMatchObject({code:'SUPPLIER_BILL_FILE_READ_TRANSACTION_REQUIRED'});
 for(const fault of ['invoice','receipt','party','iam'] as const){const trigger=`payable_byte_${key().replaceAll('-','')}`,effect=fault==='invoice'?`UPDATE finance_invoices SET version=version+1 WHERE id='${source.id}'`:fault==='receipt'?`UPDATE commerce_orders SET due_date='2026-12-31' WHERE id='${source.orderId}'`:fault==='party'?`UPDATE finance_parties SET name=name||' unexpected source edit' WHERE id='${source.partyId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.reviewer.assignmentId}'`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{await expect(attachmentContent(db,files,f.reviewer,file.linkId,'original')).rejects.toMatchObject({code:fault==='iam'?'ATTACHMENT_FILE_READ_CHANGED':'SUPPLIER_BILL_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(source.id).first('version')).toBe(3);expect(await db.prepare('SELECT due_date FROM commerce_orders WHERE id=?').bind(source.orderId).first('due_date')).toBe('2026-09-03');expect(await db.prepare('SELECT name FROM finance_parties WHERE id=?').bind(source.partyId).first('name')).toBe('Private payable original supplier');expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.reviewer.assignmentId).first()).toEqual({revoked_at:null,version:1});}
 expect(consumed).toBe(5);expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(file.linkId).first('n'))).toBe(1);expect(await(await attachmentContent(db,files,f.reviewer,file.linkId,'original')).object.text()).toBe(bytes);expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['supply']);await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.reviewer.assignmentId).run();await expect(attachmentContent(db,files,f.reviewer,file.linkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(consumed).toBe(6);expect(await db.prepare("SELECT reason FROM attachment_access_events WHERE link_id=? AND outcome='denied' ORDER BY created_at DESC LIMIT 1").bind(file.linkId).first('reason')).toBe('ATTACHMENT_TARGET_NOT_FOUND');
});
async function standalonePrivateBill(){
 enableAttachmentV2ForTest();const party=await saveMaster(f.owner,'parties',null,{kind:'supplier',code:key(),name:'Standalone real Finance supplier'},key()),invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:String(party.id),kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-02',dueDate:'2026-09-03',currency:'THB',subtotal:'1',tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key()),id=String(invoice.id),file=await seedAttachmentSet(db,f.reviewer,{targetType:'supplier_bill',targetId:id,purposeKey:'tax_document',componentKind:'record',expectedTargetVersion:1}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Retained ordinary Finance payable tax evidence',{httpMetadata:{contentType:'application/pdf'}});return {id,file};
}
it('fresh supplier bill alias preserves ordinary full Finance history without a Commerce source or any Supply purchase',async()=>{
 const {id,file}=await standalonePrivateBill(),{bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service');await bindRecordAttachmentSet(db,f.reviewer,file.setId,1,key());vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const grant={companyId:f.company,packageId:'finance',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Original ordinary standalone Finance payable history',evidence:'test:standalone-finance-private-payable'},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-ordinary-payable-provider'}),obligations:async()=>{throw Error('ORDINARY_PAYABLE_FIXTURE_CANNOT_INACTIVATE');}});await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});expect(await(await attachmentContent(db,env.H001_FILES,f.reviewer,file.linkId,'original')).object.text()).toBe('Retained ordinary Finance payable tax evidence');expect(await db.prepare('SELECT source_type FROM finance_invoices WHERE id=?').bind(id).first('source_type')).toBe('standalone');expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['finance']);
});
it('fresh supplier bill alias sees an actual normal disabled File bind before its first payable source snapshot without deadlocks or retries',async()=>{
 const {id,file}=await standalonePrivateBill(),{bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),trigger=`payable_snapshot_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF(SELECT version FROM finance_invoices WHERE id='${id}')<>2 THEN RAISE EXCEPTION 'SUPPLIER_BILL_FILE_STALE_FIRST_SNAPSHOT';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{const observed=await measureReads(()=>committedSourceVisibility(db,tx=>bindRecordAttachmentSet(tx,f.reviewer,file.setId,1,key()),()=>attachmentContent(db,env.H001_FILES,f.reviewer,file.linkId,'original')));expect(await observed.result.object.text()).toBe('Retained ordinary Finance payable tax evidence');expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}
});

type PostingSetupInput = import('../../../src/server/modules/commerce/public/posting-setup').CommercePostingSetupInput;
const postingFields = ['receivableAccountId','payableAccountId','revenueAccountId','inventoryAccountId','clearingAccountId','cogsAccountId','inputTaxAccountId','outputTaxAccountId','cashAccountId'] as const;

async function supplyInitialPostingFixture(currency = 'THB') {
  // The existing expense fixture imports a preconfigured legacy mapping. This
  // diagnostic removal models an unconfigured source; it is not evidence of
  // normal new-Company creation. Every replacement account/mapping below is
  // created through the original normal owner APIs.
  expect(await db.prepare('SELECT count(*) n FROM commerce_orders WHERE company_id=?').bind(f.company).first('n')).toBe(0);
  expect(await db.prepare('SELECT count(*) n FROM commerce_goods_valuations WHERE company_id=?').bind(f.company).first('n')).toBe(0);
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(f.company).first()).not.toBeNull();
  await db.prepare('DELETE FROM commerce_settings WHERE company_id=?').bind(f.company).run();
  const store = packageGrantStore(db, {authorize:async()=>({userId:'isolated-initial-Supply-provider'}), obligations:async()=>{throw Error('INITIAL_POSTING_FIXTURE_HAS_NO_WITHDRAWAL_WITNESS');}});
  const grant = {companyId:f.company,packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Initial native receiving and valuation foundation',evidence:'test:supply-initial-posting'};
  await store.apply({expectedVersion:0,idempotencyKey:key(),grant});
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
  vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
  vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
  const {loadIamAuthority} = await import('../../../src/server/modules/identity_access/public/iam-store');
  for (const actor of [f.owner,f.reviewer,f.manager,buyer,purchasingManager,warehouse]) actor.iam = await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);
  const {createAccount} = await import('../../../src/server/modules/finance/public/finance');
  const definitions = [
    ['receivableAccountId','asset','receivable',currency],
    ['payableAccountId','liability','payable',currency],
    ['revenueAccountId','revenue','none','THB'],
    ['inventoryAccountId','asset','none','THB'],
    ['clearingAccountId','liability','none','THB'],
    ['cogsAccountId','expense','none','THB'],
    ['inputTaxAccountId','asset','none','THB'],
    ['outputTaxAccountId','liability','none','THB'],
    ['cashAccountId','asset','bank',currency],
  ] as const;
  const mapping:Pick<PostingSetupInput,typeof postingFields[number]> = {receivableAccountId:'',payableAccountId:'',revenueAccountId:'',inventoryAccountId:'',clearingAccountId:'',cogsAccountId:'',inputTaxAccountId:'',outputTaxAccountId:'',cashAccountId:''};
  for (const [field,category,controlKind,accountCurrency] of definitions) {
    const account = await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:`Initial Supply ${field}`,category,currency:accountCurrency,isControl:controlKind!=='none',controlKind},key());
    mapping[field] = String(account.id);
  }
  const api = await import('../../../src/server/modules/commerce/public/posting-setup');
  const view = await api.commercePostingSetup(f.reviewer);
  expect(view).toMatchObject({available:true,configured:false,version:0,canConfigure:true,configuration:null});
  expect(view.contextKey).toMatch(/^[a-f0-9]{64}$/);
  if (!view.contextKey) throw Error('Native initial posting context required');
  const input:PostingSetupInput = {expectedVersion:0,contextKey:view.contextKey,chartId:f.chart,currency,reason:'Original Company Finance preparer reviewed all nine real account meanings',...mapping};
  return {...api,input,view,store,grant};
}

async function postingCommandImage(command:string) {
  return Promise.all(['command_receipts','audit_events','outbox_events'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE command_id=? ORDER BY image`).bind(command).all()).results));
}

it('initial Supply posting setup creates all nine normal accounts then receives, values and independently posts AP without Finance, Sales or People purchases',async()=>{
  const setup = await supplyInitialPostingFixture(),command=key(),result=await setup.initializeCommercePostingSetup(f.reviewer,setup.input,command);
  expect(result).toEqual({id:f.company,status:'configured',version:1});
  expect(await setup.initializeCommercePostingSetup(f.reviewer,setup.input,command)).toEqual(result);
  await expect(setup.initializeCommercePostingSetup(f.reviewer,{...setup.input,reason:'Different reviewed body cannot reuse the committed key'},command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  const configured = await setup.commercePostingSetup(f.reviewer);
  expect(configured).toMatchObject({available:true,configured:true,version:1,canConfigure:false,contextKey:setup.input.contextKey});
  expect(configured.configuration).toEqual(Object.fromEntries(Object.entries(setup.input).filter(([field])=>!['expectedVersion','contextKey','reason'].includes(field))));
  const accountRows = (await db.prepare('SELECT id,category,control_kind,currency FROM finance_accounts WHERE id IN (?,?,?,?,?,?,?,?,?) ORDER BY id').bind(...postingFields.map(field=>setup.input[field])).all()).results;
  expect(accountRows).toHaveLength(9);
  await expect(setup.initializeCommercePostingSetup(f.reviewer,{...setup.input,inventoryAccountId:setup.input.inputTaxAccountId},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});
  const source=await privateBillFixture();
  const valuation=await db.prepare('SELECT v.*,j.status journal_status,j.created_by,j.posted_by FROM commerce_goods_valuations v JOIN finance_journals j ON j.id=v.journal_id JOIN commerce_fulfillments ff ON ff.id=v.fulfillment_id WHERE ff.order_id=?').bind(source.orderId).first<{journal_id:string;document_currency:string;journal_currency:string;stock_currency:string;stock_value_minor:number;journal_status:string;created_by:string;posted_by:string}>();
  expect(valuation).toMatchObject({document_currency:'THB',journal_currency:'THB',stock_currency:'THB',stock_value_minor:1000,journal_status:'posted',posted_by:f.manager.userId});
  if(!valuation)throw Error('Actual receiving valuation required');
  expect(valuation.created_by).not.toBe(valuation.posted_by);
  expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal').bind(valuation.journal_id).all()).results).toEqual(expect.arrayContaining([
    expect.objectContaining({account_id:setup.input.inventoryAccountId,debit_minor:1000,credit_minor:0}),
    expect.objectContaining({account_id:setup.input.clearingAccountId,debit_minor:0,credit_minor:1000}),
  ]));
  const payable=await getInvoice(f.reviewer,source.id);
  expect(payable.invoice).toMatchObject({status:'posted',currency:'THB',outstanding_minor:1000,credit_account_id:setup.input.payableAccountId});
  const invoiceJournal=String(payable.invoice.journal_id);
  expect(await db.prepare('SELECT posted_by FROM finance_journals WHERE id=?').bind(invoiceJournal).first('posted_by')).toBe(f.manager.userId);
  const assignmentVersion=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.reviewer.assignmentId).first('version'));
  for(const target of [{kind:'invoice' as const,id:source.id},{kind:'journal' as const,id:valuation.journal_id},{kind:'journal' as const,id:invoiceJournal}]) expect(await financeProductDocumentEntry(db,{companyId:f.company,branchId:null,departmentId:null,userId:f.reviewer.userId,assignmentId:f.reviewer.assignmentId,assignmentVersion,role:f.reviewer.role},target)).toEqual({capability:'finance.inventory-valuation',purchasePurpose:'history'});
  expect((await financePurchaseControls(f.reviewer))?.['finance.accounting']).toMatchObject({newWork:false,history:false});
  expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['supply']);
  // A later normal catalogue addition cannot invalidate the committed command.
  const {createAccount}=await import('../../../src/server/modules/finance/public/finance');
  await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:'Later unused normal Supply asset',category:'asset',currency:'THB',isControl:false,controlKind:'none'},key());
  await setup.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...setup.grant,state:'draining',version:2}});
  expect(await setup.initializeCommercePostingSetup(f.reviewer,setup.input,command)).toEqual(result);
  expect(await setup.commercePostingSetup(f.reviewer)).toMatchObject({configured:true,canConfigure:false});
  expect(await db.prepare("SELECT count(*) n FROM command_receipts WHERE command_id=? AND operation='commerce.posting-setup.initialize'").bind(command).first('n')).toBe(1);
  await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.reviewer.assignmentId).run();
  await expect(setup.initializeCommercePostingSetup(f.reviewer,setup.input,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
});

it('initial Supply posting setup preserves mixed document and functional account currencies and permits legitimate shared non-control accounts',async()=>{
  const setup=await supplyInitialPostingFixture('USD'),input={...setup.input,inputTaxAccountId:setup.input.inventoryAccountId};
  expect(await setup.initializeCommercePostingSetup(f.reviewer,input,key())).toMatchObject({status:'configured'});
  const view=await setup.commercePostingSetup(f.reviewer);
  expect(view.configuration).toMatchObject({currency:'USD',chartId:f.chart,inputTaxAccountId:input.inventoryAccountId});
  expect(view.accounts.find(row=>row.id===input.payableAccountId)).toMatchObject({category:'liability',controlKind:'payable',currency:'USD'});
  expect(view.accounts.find(row=>row.id===input.inventoryAccountId)).toMatchObject({category:'asset',controlKind:'none',currency:'THB'});
  // The original order takes its document currency from the retained mapping.
  await expect(privateBillFixture()).rejects.toMatchObject({code:'FX_GAIN_ACCOUNT_REQUIRED'});
  const source=await db.prepare('SELECT i.id,ff.order_id FROM finance_invoices i JOIN commerce_fulfillments ff ON ff.invoice_id=i.id WHERE i.company_id=?').bind(f.company).first<{id:string;order_id:string}>();
  if(!source)throw Error('Original independently reviewed foreign supplier invoice required');
  const {createAccount}=await import('../../../src/server/modules/finance/public/finance'),gain=await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:'Original functional FX gain',category:'other_income',currency:'THB',isControl:false,controlKind:'none'},key()),loss=await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:'Original functional FX loss',category:'other_expense',currency:'THB',isControl:false,controlKind:'none'},key());
  await postInvoice(f.manager,source.id,{expectedVersion:1,periodId:f.period,entryDate:'2026-09-02',fxGainAccountId:gain.id,fxLossAccountId:loss.id},key());
  expect(await db.prepare('SELECT currency,status,outstanding_minor FROM finance_invoices WHERE id=?').bind(source.id).first()).toEqual({currency:'USD',status:'posted',outstanding_minor:1000});
  expect(await db.prepare('SELECT v.document_currency,v.stock_currency,v.stock_value_minor FROM commerce_goods_valuations v JOIN commerce_fulfillments ff ON ff.id=v.fulfillment_id WHERE ff.order_id=?').bind(source.order_id).first()).toEqual({document_currency:'USD',stock_currency:'THB',stock_value_minor:36000});
});

it('initial Supply posting setup rejects every wrong native account meaning plus inactive, foreign-chart, foreign-Company and unrelated-currency references',async()=>{
  const setup=await supplyInitialPostingFixture(),{createAccount,createChart,changeAccountStatus}=await import('../../../src/server/modules/finance/public/finance');
  const invalid=await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:'Normal equity cannot stand in for any posting role',category:'equity',currency:'THB',isControl:false,controlKind:'none'},key());
  let context=(await setup.commercePostingSetup(f.reviewer)).contextKey!;
  for(const field of postingFields){const command=key();await expect(setup.initializeCommercePostingSetup(f.reviewer,{...setup.input,contextKey:context,[field]:invalid.id},command)).rejects.toMatchObject({code:'COMMERCE_POSTING_ACCOUNTS_INVALID'});expect(await postingCommandImage(command)).toEqual([[],[],[]]);}
  // Correct category with an incorrect control remains invalid for all three
  // original control accounts, rather than merely passing a category check.
  for(const [field,wrong] of [['receivableAccountId',setup.input.inventoryAccountId],['payableAccountId',setup.input.clearingAccountId],['cashAccountId',setup.input.inventoryAccountId]] as const) await expect(setup.initializeCommercePostingSetup(f.reviewer,{...setup.input,contextKey:context,[field]:wrong},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_ACCOUNTS_INVALID'});
  const otherChart=await createChart(f.reviewer,{name:'Other normal Company chart',baseCurrency:'THB',accountingBasis:'IFRS'},key()),otherAccount=await createAccount(f.reviewer,{chartId:otherChart.id,code:key(),name:'Other chart native inventory',category:'asset',currency:'THB',isControl:false,controlKind:'none'},key()),thirdCurrency=await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:'Unrelated EUR asset',category:'asset',currency:'EUR',isControl:false,controlKind:'none'},key()),foreign=await expenseFixture();
  context=(await setup.commercePostingSetup(f.reviewer)).contextKey!;
  for(const input of [{...setup.input,chartId:otherChart.id},{...setup.input,inventoryAccountId:otherAccount.id},{...setup.input,inventoryAccountId:foreign.accounts.cash},{...setup.input,inventoryAccountId:thirdCurrency.id}]) await expect(setup.initializeCommercePostingSetup(f.reviewer,{...input,contextKey:context},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_ACCOUNTS_INVALID'});
  await changeAccountStatus(f.reviewer,f.chart,setup.input.inventoryAccountId,{status:'inactive',expectedVersion:1,reason:'Retire unused native account before setup'},key());
  context=(await setup.commercePostingSetup(f.reviewer)).contextKey!;
  await expect(setup.initializeCommercePostingSetup(f.reviewer,{...setup.input,contextKey:context},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_ACCOUNTS_INVALID'});
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(f.company).first()).toBeNull();
});

it('initial Supply posting setup binds fresh references, current Company Finance IAM and actual purchased authority',async()=>{
  const {commercePostingSetup,initializeCommercePostingSetup}=await import('../../../src/server/modules/commerce/public/posting-setup');
  const unavailable=await commercePostingSetup(f.reviewer);
  expect(unavailable).toMatchObject({available:false,configured:null,version:null,configuration:null,charts:[],accounts:[],contextKey:null,unavailableReason:'purchase-required'});
  await expect(initializeCommercePostingSetup(f.reviewer,{},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  const setup=await supplyInitialPostingFixture(),{createAccount}=await import('../../../src/server/modules/finance/public/finance');
  await createAccount(f.reviewer,{chartId:f.chart,code:key(),name:'Normal new source invalidates the captured reference context',category:'asset',currency:'THB',isControl:false,controlKind:'none'},key());
  await expect(setup.initializeCommercePostingSetup(f.reviewer,setup.input,key())).rejects.toMatchObject({code:'COMMERCE_POSTING_CONTEXT_CHANGED'});
  for(const actor of [{...f.reviewer,branchId:f.branch},buyer]){
    expect(await setup.commercePostingSetup(actor)).toMatchObject({available:false,configuration:null,charts:[],accounts:[],contextKey:null});
    await expect(setup.initializeCommercePostingSetup(actor,setup.input,key())).rejects.toMatchObject({status:403});
  }
  const authorityContext=(await setup.commercePostingSetup(f.reviewer)).contextKey;
  await db.prepare('UPDATE assignments SET version=version+1 WHERE id=?').bind(f.reviewer.assignmentId).run();
  await expect(setup.initializeCommercePostingSetup(f.reviewer,{...setup.input,contextKey:authorityContext},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_CONTEXT_CHANGED'});
  await setup.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...setup.grant,state:'draining',version:2}});
  expect(await setup.commercePostingSetup(f.reviewer)).toMatchObject({available:true,configured:false,canConfigure:false});
  await expect(setup.initializeCommercePostingSetup(f.reviewer,{...setup.input,contextKey:(await setup.commercePostingSetup(f.reviewer)).contextKey},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
  await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.reviewer.assignmentId).run();
  await expect(setup.initializeCommercePostingSetup(f.reviewer,setup.input,key())).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(f.company).first()).toBeNull();
});

it('initial Supply posting setup serializes duplicate and competing normal initializers without retry-masked conflicts',async()=>{
  const setup=await supplyInitialPostingFixture(),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),commands=[key(),key()];
  const observed=await measureReads(()=>Promise.allSettled(commands.map(command=>setup.initializeCommercePostingSetup(f.reviewer,setup.input,command))));
  expect(observed.result.filter(result=>result.status==='fulfilled')).toHaveLength(1);
  expect(observed.result.filter(result=>result.status==='rejected')).toHaveLength(1);
  const winner=observed.result.findIndex(result=>result.status==='fulfilled'),successful=observed.result[winner],rejected=observed.result.find(result=>result.status==='rejected');
  if(!successful||successful.status!=='fulfilled'||!rejected||rejected.status!=='rejected')throw Error('Exactly one normal initializer must commit');
  expect(rejected.reason).toMatchObject({code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});
  const command=commands[winner],duplicates=await measureReads(()=>Promise.all([setup.initializeCommercePostingSetup(f.reviewer,setup.input,command),setup.initializeCommercePostingSetup(f.reviewer,setup.input,command)]));
  expect(duplicates.result).toEqual([successful.value,successful.value]);
  for(const field of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(observed.metrics[field]??0).toBe(0);
  for(const field of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(duplicates.metrics[field]??0).toBe(0);
  expect(await db.prepare('SELECT count(*) n FROM commerce_settings WHERE company_id=?').bind(f.company).first('n')).toBe(1);
  await expect(setup.initializeCommercePostingSetup(f.reviewer,setup.input,key())).rejects.toMatchObject({code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});
  expect(await db.prepare('SELECT operation,actor_id,record_id FROM command_receipts WHERE command_id=?').bind(command).first()).toEqual({operation:'commerce.posting-setup.initialize',actor_id:f.reviewer.userId,record_id:f.company});
  expect(await postingCommandImage(commands[1-winner])).toEqual([[],[],[]]);
});

it('initial Supply posting setup exact product entry uses original module-free Finance authority and current source readiness without selecting a role',async()=>{
  const {commercePostingSetupProductEntry}=await import('../../../src/server/modules/commerce/public/posting-setup'),scope={companyId:f.company,branchId:null,departmentId:null,userId:f.reviewer.userId,assignmentId:f.reviewer.assignmentId,assignmentVersion:Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.reviewer.assignmentId).first('version')),role:f.reviewer.role,sessionId:key()};
  expect(await commercePostingSetupProductEntry(db,scope)).toBeNull();
  await supplyInitialPostingFixture();
  expect(await db.prepare('SELECT modules_json FROM assignments WHERE id=?').bind(scope.assignmentId).first('modules_json')).toBe('[]');
  const image=()=>Promise.all([
    db.prepare('SELECT to_jsonb(a)::text image FROM assignments a WHERE company_id=? ORDER BY id').bind(f.company).all().then(rows=>rows.results),
    db.prepare('SELECT to_jsonb(c)::text image FROM active_work_contexts c WHERE company_id=? ORDER BY session_id').bind(f.company).all().then(rows=>rows.results),
  ]);
  const before=await image();
  expect(await commercePostingSetupProductEntry(db,scope)).toMatchObject({revision:expect.stringMatching(/^[a-f0-9]{64}$/),purchasePurpose:'history'});
  for(const invalid of [{...scope,branchId:f.branch},{...scope,departmentId:f.department},{...scope,assignmentVersion:scope.assignmentVersion+1},{...scope,userId:buyer.userId},{...scope,companyId:`foreign-${f.company}`},{...scope,sessionId:''}])expect(await commercePostingSetupProductEntry(db,invalid)).toBeNull();
  vi.stubEnv('NODE_ENV','production');
  expect(await commercePostingSetupProductEntry(db,scope)).toBeNull();
  vi.stubEnv('NODE_ENV','test');
  expect(await image()).toEqual(before);
  await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(scope.assignmentId).run();
  expect(await commercePostingSetupProductEntry(db,scope)).toBeNull();
});

it('initial Supply posting setup waits for an actual normal account writer before its first snapshot without deadlocks or retries',async()=>{
  const setup=await supplyInitialPostingFixture(),{changeAccountStatus}=await import('../../../src/server/modules/finance/public/finance'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),command=key(),fault=`posting_writer_${key().replaceAll('-','')}`;
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try{
    const observed=await measureReads(async()=>{
      const writer=changeAccountStatus(f.reviewer,f.chart,setup.input.inventoryAccountId,{status:'inactive',expectedVersion:1,reason:'Normal writer retires unused selected account'},command);
      let staged=false;
      for(let i=0;i<100;i++){staged=!!await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called');if(staged)break;await new Promise(resolve=>setTimeout(resolve,10));}
      expect(staged).toBe(true);
      await expect(setup.initializeCommercePostingSetup(f.reviewer,setup.input,key())).rejects.toMatchObject({code:'COMMERCE_POSTING_CONTEXT_CHANGED'});
      await writer;
    });
    expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
    expect(await db.prepare('SELECT status FROM finance_accounts WHERE id=?').bind(setup.input.inventoryAccountId).first('status')).toBe('inactive');
    expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(f.company).first()).toBeNull();
  }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
});

it('initial Supply posting setup rolls back its entire receipt, mapping and evidence after real deferred account, authority or purchase loss',async()=>{
  const setup=await supplyInitialPostingFixture();
  for(const faultKind of ['account','authority','purchase'] as const){
    if(faultKind==='purchase')await setup.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...setup.grant,endsAt:Date.now()+1500,version:2,reason:'Actual dated purchase expires during deferred owner effects',evidence:'test:initial-posting-final-purchase-clock'}});
    const command=key(),fault=`posting_final_${key().replaceAll('-','')}`,before={account:await db.prepare('SELECT to_jsonb(a)::text image FROM finance_accounts a WHERE id=?').bind(setup.input.inventoryAccountId).first('image'),assignment:await db.prepare('SELECT to_jsonb(a)::text image FROM assignments a WHERE id=?').bind(f.reviewer.assignmentId).first('image'),grant:await db.prepare("SELECT to_jsonb(g)::text image FROM company_package_grants g WHERE company_id=? AND package_id='supply'").bind(f.company).first('image')};
    const effect=faultKind==='account'?`UPDATE finance_accounts SET name='Unreviewed deferred reference',version=version+1 WHERE id='${setup.input.inventoryAccountId}'`:faultKind==='authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.reviewer.assignmentId}'`:'PERFORM pg_sleep(1.7)';
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      await expect(setup.initializeCommercePostingSetup(f.reviewer,setup.input,command)).rejects.toMatchObject({code:faultKind==='authority'?'QUEUED_AUTHORITY_CHANGED':faultKind==='account'?'COMMERCE_POSTING_SOURCE_CHANGED':'PACKAGE_CAPABILITY_CHANGED'});
      expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);
      expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(f.company).first()).toBeNull();
      expect(await postingCommandImage(command)).toEqual([[],[],[]]);
      expect({account:await db.prepare('SELECT to_jsonb(a)::text image FROM finance_accounts a WHERE id=?').bind(setup.input.inventoryAccountId).first('image'),assignment:await db.prepare('SELECT to_jsonb(a)::text image FROM assignments a WHERE id=?').bind(f.reviewer.assignmentId).first('image'),grant:await db.prepare("SELECT to_jsonb(g)::text image FROM company_package_grants g WHERE company_id=? AND package_id='supply'").bind(f.company).first('image')}).toEqual(before);
    }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
  await setup.store.apply({expectedVersion:2,idempotencyKey:key(),grant:{...setup.grant,version:3,reason:'Normal provider renews the isolated fixture after the clock proof',evidence:'test:initial-posting-after-clock'}});
  expect(await setup.initializeCommercePostingSetup(f.reviewer,setup.input,key())).toMatchObject({status:'configured'});
});

it('initial Supply posting setup never repairs a deliberately missing legacy mapping by rebinding actual historical orders',async()=>{
  const setup=await supplyInitialPostingFixture();
  await setup.initializeCommercePostingSetup(f.reviewer,setup.input,key());
  const source=await privateBillFixture(),before=(await db.prepare('SELECT to_jsonb(o)::text image FROM commerce_orders o WHERE id=?').bind(source.orderId).first('image'));
  // Explicit corrupted-import diagnostic, not a normal owner mutation or a
  // positive new-Company setup. The initializer must conservatively refuse it.
  await db.prepare('DELETE FROM commerce_settings WHERE company_id=?').bind(f.company).run();
  const view=await setup.commercePostingSetup(f.reviewer),command=key();
  await expect(setup.initializeCommercePostingSetup(f.reviewer,{...setup.input,contextKey:view.contextKey},command)).rejects.toMatchObject({code:'COMMERCE_POSTING_HISTORY_REQUIRES_REVIEW'});
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(f.company).first()).toBeNull();
  expect(await db.prepare('SELECT to_jsonb(o)::text image FROM commerce_orders o WHERE id=?').bind(source.orderId).first('image')).toBe(before);
  expect(await postingCommandImage(command)).toEqual([[],[],[]]);
});

async function zeroLedgerSupplyCompany() {
  // Seed organization and actual native worker/assignment authority only.
  // This Company has NEVER had a chart/account/period/quote/settings row.
  // It is a disposable Company fixture, not normal Company onboarding proof.
  const company=`zero-supply-${key()}`,branch=`${company}-branch`,department=`${company}-department`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero ledger Supply fixture',?)").bind(company,company,company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Native receiving branch',?,?,?)").bind(branch,branch,company,company,branch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Native Supply team',?,?,?)").bind(department,department,branch,company,branch),
  ]);
  async function actor(name:string,role:string):Promise<WorkContext>{
    const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`;
    await db.batch([
      db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,'[]',?,0,9999999999999,'holding')").bind(assignmentId,userId,role,company),
      db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${userId}`,userId,company,name),
      db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Native worker','THB','fixture',1,1)").bind(`eng-${userId}`,`person-${userId}`,company,branch,department),
    ]);
    const context:WorkContext={userId,assignmentId,role,companyId:company,branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'test',holdingName:'Test',name,email:`${name}@example.test`,assignmentScope:'holding',inherited:false,modules:[],assignments:[],workspaces:[],scope:{kind:'holding',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company}};
    const {loadIamAuthority}=await import('../../../src/server/modules/identity_access/public/iam-store');
    context.iam=await loadIamAuthority(db,company,userId,role);
    return context;
  }
  const owner=await actor('owner','holding_owner'),reviewer=await actor('reviewer','finance_preparer'),manager=await actor('manager','finance_approver'),buyer=await actor('buyer','purchasing_operator'),purchasingManager=await actor('purchasing-manager','purchasing_manager'),warehouse=await actor('warehouse','warehouse_manager');
  const {installStaffLevels,savePosition,positionSchema}=await import('../../../src/server/modules/organization/public/organization');
  await installStaffLevels(db,owner,key());
  const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Fixture accountable owner',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Native organizational authority fixture'}),key())).id);
  for(const [member,rank] of [[owner,1],[reviewer,5],[manager,4],[buyer,4],[purchasingManager,3],[warehouse,4]] as const){
    const position=rank===1?ceo:String((await savePosition(db,owner,null,positionSchema.parse({title:`Fixture ${member.role}`,levelId:`org-level-${company}-${rank}`,branchId:null,departmentId:null,reportsToPositionId:ceo,expectedVersion:0,reason:'Native selected worker authority fixture'}),key())).id);
    await db.prepare('UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=?').bind(`org-level-${company}-${rank}`,position,rank===1?null:`person-${owner.userId}`,`eng-${member.userId}`).run();
  }
  await db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,'ceo',?,1,?,?,1) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=1,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=1").bind(company,company,ceo,`person-${owner.userId}`).run();
  const counts=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','commerce_orders','finance_journals','finance_invoices'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')));
  expect(await counts()).toEqual([0,0,0,0,0,0,0,0]);
  const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-zero-ledger-Supply-provider'}),obligations:async()=>{throw Error('ZERO_LEDGER_FIXTURE_CANNOT_WITHDRAW');}}),grant={companyId:company,packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Reviewed isolated zero-ledger receiving foundation',evidence:'test:zero-ledger-Supply'};
  async function purchase(){await store.apply({expectedVersion:0,idempotencyKey:key(),grant});}
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
  return {company,branch,department,owner,reviewer,manager,buyer,purchasingManager,warehouse,counts,purchase,store,grant};
}

async function zeroSupplyEnvelope(actor:WorkContext) {
  const {supplyPostingPrerequisites}=await import('../../../src/server/modules/finance/public/supply-posting-prerequisites');
  const view=await supplyPostingPrerequisites(actor);
  expect(view.available).toBe(true);expect(view.contextKey).toMatch(/^[a-f0-9]{64}$/);
  if(!view.contextKey)throw Error('Current original Supply prerequisite context required');
  return {expectedVersion:0 as const,contextKey:view.contextKey};
}

async function zeroSupplyReceipt(z:Awaited<ReturnType<typeof zeroLedgerSupplyCompany>>,periodId:string,mapping:PostingSetupInput) {
  async function assignSource(kind:'order'|'journal'|'invoice',id:string,stepKey:string,actor:WorkContext){
    const flow=await resolveFlow(db,z.owner,kind,id);
    await assignFlowPrimary(db,z.owner,kind,id,{stepKey,userId:actor.userId,reason:'Original independent zero-ledger owner',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  }
  const warehouse=await saveMaster(z.owner,'warehouses',null,{branchId:z.branch,code:key(),name:'Normal first receiving warehouse'},key()),product=await saveMaster(z.owner,'products',null,{sku:key(),name:'Normal first received product',price:'10'},key()),supplier=await saveMaster(z.owner,'parties',null,{kind:'supplier',code:key(),name:'Normal first supplier'},key()),order=await saveOrder(z.buyer,null,orderSchema.parse({kind:'purchase',partyId:supplier.id,warehouseId:warehouse.id,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product.id,quantity:'1',price:'10',taxBps:0}]}),key()),orderId=String(order.id);
  await transitionOrder(z.buyer,orderId,{action:'submit',expectedVersion:1},key());await assignSource('order',orderId,'submitted',z.purchasingManager);await transitionOrder(z.purchasingManager,orderId,{action:'approve',expectedVersion:2},key());await assignSource('order',orderId,'confirmed',z.warehouse);await assignSource('order',orderId,'stock_gl',z.manager);
  const detail=await getOrder(z.buyer,orderId),received=await fulfillOrder(z.warehouse,orderId,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:String(detail.lines[0].id),quantity:'1'}]},key()),fulfillmentId=String(received.fulfillmentId),journalId=String(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(fulfillmentId).first('journal_id'));
  await assignSource('journal',journalId,'draft',z.manager);await postJournal(z.manager,journalId,key(),{expectedVersion:1});await assignSource('order',orderId,'invoicing',z.reviewer);
  const lineId=String(await db.prepare('SELECT id FROM commerce_fulfillment_lines WHERE fulfillment_id=?').bind(fulfillmentId).first('id')),bill=await captureSupplierBill(z.reviewer,{fulfillmentId,expectedVersion:1,reference:key(),invoiceDate:'2026-09-02',dueDate:'2026-09-03',evidence:'First normal supplier document matches actual receiving',lines:[{fulfillmentLineId:lineId,quantity:'1',price:'10',taxBps:0}]},key());
  await decideSupplierBill(z.manager,String(bill.id),{action:'approve',expectedVersion:1,reason:'Original separate supplier document reviewer'},key());
  const invoice=await invoiceFulfillment(z.reviewer,fulfillmentId,{expectedVersion:2,date:'2026-09-02',dueDate:'2026-09-03',matchId:String(bill.id),matchVersion:2},key()),invoiceId=String(invoice.invoiceId);
  await assignSource('invoice',invoiceId,'draft',z.manager);await postInvoice(z.manager,invoiceId,{expectedVersion:1,periodId,entryDate:'2026-09-02'},key());
  expect(await db.prepare('SELECT v.stock_value_minor,v.stock_currency,j.posted_by,j.created_by FROM commerce_goods_valuations v JOIN finance_journals j ON j.id=v.journal_id WHERE v.fulfillment_id=?').bind(fulfillmentId).first()).toEqual({stock_value_minor:1000,stock_currency:'THB',posted_by:z.manager.userId,created_by:z.warehouse.userId});
  expect((await getInvoice(z.reviewer,invoiceId)).invoice).toMatchObject({status:'posted',currency:'THB',outstanding_minor:1000,credit_account_id:mapping.payableAccountId});
  expect(await db.prepare("SELECT count(*) n FROM money_transaction_snapshots s JOIN company_fx_rates r ON r.id=s.fx_rate_id WHERE s.company_id=? AND r.confirmed_by=?").bind(z.company,z.manager.userId).first<number>('n')).toBeGreaterThan(0);
  const assignmentVersion=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(z.reviewer.assignmentId).first('version'));
  expect(await financeProductDocumentEntry(db,{companyId:z.company,branchId:null,departmentId:null,userId:z.reviewer.userId,assignmentId:z.reviewer.assignmentId,assignmentVersion,role:z.reviewer.role},{kind:'invoice',id:invoiceId})).toEqual({capability:'finance.inventory-valuation',purchasePurpose:'history'});
  return {orderId,fulfillmentId,journalId,invoiceId};
}

it('zero-ledger Supply creates chart nine accounts period and actual approved quote through normal owners before its first receiving and AP posting',async()=>{
  const z=await zeroLedgerSupplyCompany();await z.purchase();
  const api=await import('../../../src/server/modules/finance/public/supply-posting-prerequisites'),initial=await api.supplyPostingPrerequisites(z.reviewer);
  expect(initial).toMatchObject({available:true,configured:false,canCreateChart:true,canCreateAccount:true,canCreatePeriod:true,canConfirmRate:false,charts:[],accounts:[],periods:[],rates:[]});
  const chartBody={...await zeroSupplyEnvelope(z.reviewer),input:{name:'Actual first Supply chart',baseCurrency:'THB',accountingBasis:'IFRS'}},chartKey=key(),chart=await api.createSupplyPostingChart(z.reviewer,chartBody,chartKey);
  expect(await api.createSupplyPostingChart(z.reviewer,chartBody,chartKey)).toEqual(chart);
  const mapping:Pick<PostingSetupInput,typeof postingFields[number]>={receivableAccountId:'',payableAccountId:'',revenueAccountId:'',inventoryAccountId:'',clearingAccountId:'',cogsAccountId:'',inputTaxAccountId:'',outputTaxAccountId:'',cashAccountId:''};
  const definitions=[['receivableAccountId','asset','receivable'],['payableAccountId','liability','payable'],['revenueAccountId','revenue','none'],['inventoryAccountId','asset','none'],['clearingAccountId','liability','none'],['cogsAccountId','expense','none'],['inputTaxAccountId','asset','none'],['outputTaxAccountId','liability','none'],['cashAccountId','asset','bank']] as const;
  for(const [field,category,controlKind] of definitions){
    const body={...await zeroSupplyEnvelope(z.reviewer),input:{chartId:chart.id,code:key(),name:`Normal first ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},command=key(),account=await api.createSupplyPostingAccount(z.reviewer,body,command);
    mapping[field]=account.id;expect(await api.createSupplyPostingAccount(z.reviewer,body,command)).toEqual(account);
  }
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(z.company).first()).toBeNull();
  const periodBody={...await zeroSupplyEnvelope(z.reviewer),input:{name:'Actual first receiving year',startDate:'2026-01-01',endDate:'2026-12-31'}},period=await api.createSupplyPostingPeriod(z.reviewer,periodBody,key());
  expect(period).toMatchObject({companyId:z.company,state:'open',version:0});
  const rateInput={currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Fictional dated acceptance quote confirmed through the original normal Company approver'};
  await expect(api.confirmSupplyPostingRate(z.reviewer,{...await zeroSupplyEnvelope(z.reviewer),input:rateInput},key())).rejects.toMatchObject({status:403});
  const rateBody={...await zeroSupplyEnvelope(z.manager),input:rateInput},rate=await api.confirmSupplyPostingRate(z.manager,rateBody,key());
  expect(rate).toMatchObject({currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36'});
  expect(await db.prepare('SELECT confirmed_by FROM company_fx_rates WHERE id=?').bind(rate.id).first('confirmed_by')).toBe(z.manager.userId);
  const {commercePostingSetup,initializeCommercePostingSetup}=await import('../../../src/server/modules/commerce/public/posting-setup'),setup=await commercePostingSetup(z.reviewer);
  if(!setup.contextKey)throw Error('Actual nine-account setup context required');
  const input:PostingSetupInput={expectedVersion:0,contextKey:setup.contextKey,chartId:chart.id,currency:'THB',reason:'Original Company preparer deliberately selects all nine actual initial accounts',...mapping};
  vi.stubEnv('NODE_ENV','production');
  try{
    const production=await Promise.allSettled([commercePostingSetup(z.reviewer),initializeCommercePostingSetup(z.reviewer,input,key())]);
    expect(production).toMatchObject([{status:'rejected',reason:{code:'FEATURE_COMING_SOON'}},{status:'rejected',reason:{code:'FEATURE_COMING_SOON'}}]);
    expect(await z.counts()).toEqual([1,9,1,1,0,0,0,0]);
  }finally{vi.stubEnv('NODE_ENV','test');}
  expect(await initializeCommercePostingSetup(z.reviewer,input,key())).toEqual({id:z.company,status:'configured',version:1});
  await zeroSupplyReceipt(z,period.id,input);
  expect(await z.counts()).toEqual([1,9,1,1,1,1,2,1]);
  expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId)).toEqual(['supply']);
  expect(await api.createSupplyPostingChart(z.reviewer,chartBody,chartKey)).toEqual(chart);
  const retained=()=>Promise.all(['commerce_goods_valuations','money_transaction_snapshots','commerce_settings','finance_invoices','finance_journals'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? ORDER BY image`).bind(z.company).all()).results)),before=await retained();
  expect(await api.supplyPostingPrerequisites(z.reviewer)).toMatchObject({configured:true,canCreateChart:false,canCreateAccount:false,canCreatePeriod:true});
  expect(await api.supplyPostingPrerequisites(z.manager)).toMatchObject({configured:true,canConfirmRate:true});
  const next=await api.createSupplyPostingPeriod(z.reviewer,{...await zeroSupplyEnvelope(z.reviewer),input:{name:'Next non-overlap normal period',startDate:'2027-01-01',endDate:'2027-01-31'}},key());
  expect(next).toMatchObject({state:'open',version:0});
  const laterRate=await api.confirmSupplyPostingRate(z.manager,{...await zeroSupplyEnvelope(z.manager),input:{currency:'THB',effectiveDate:'2026-09-03',currencyPerUsd:'37',note:'Next fictional dated quote approved through the original Company owner'}},key());
  expect(laterRate.id).not.toBe(rate.id);
  expect(await retained()).toEqual(before);
  expect(await z.counts()).toEqual([1,9,2,2,1,1,2,1]);
  // Corrupted-import diagnostic AFTER the entire normal positive workflow.
  // Retained actual history may never advertise or accept a new mapping.
  await db.prepare('DELETE FROM commerce_settings WHERE company_id=?').bind(z.company).run();
  const orphan=await commercePostingSetup(z.reviewer);
  await expect(initializeCommercePostingSetup(z.reviewer,{...input,contextKey:orphan.contextKey},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_HISTORY_REQUIRES_REVIEW'});
  expect(orphan).toMatchObject({available:true,configured:false,canConfigure:false,configuration:null});
  expect(await api.supplyPostingPrerequisites(z.reviewer)).toMatchObject({configured:false,canCreateChart:false,canCreateAccount:false,canCreatePeriod:true,unavailableReason:'history-requires-review'});
});

it('zero-ledger Supply prerequisite commands retain current Company authority purchase gates reference context and exact command bodies',async()=>{
  const z=await zeroLedgerSupplyCompany(),api=await import('../../../src/server/modules/finance/public/supply-posting-prerequisites'),chartInput={name:'Initial guarded Supply chart',baseCurrency:'THB',accountingBasis:'IFRS'},emptyEnvelope={expectedVersion:0 as const,contextKey:'0'.repeat(64)};
  expect(await api.supplyPostingPrerequisites(z.reviewer)).toMatchObject({available:false,contextKey:null,charts:[],accounts:[],periods:[],rates:[]});
  await expect(api.createSupplyPostingChart(z.reviewer,{...emptyEnvelope,input:chartInput},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  expect(await z.counts()).toEqual([0,0,0,0,0,0,0,0]);await z.purchase();
  const initial=await zeroSupplyEnvelope(z.reviewer);
  vi.stubEnv('NODE_ENV','production');
  try{
    const production=await Promise.allSettled([api.supplyPostingPrerequisites(z.reviewer),api.createSupplyPostingChart(z.reviewer,{...initial,input:chartInput},key())]);
    expect(production).toMatchObject([{status:'rejected',reason:{code:'FEATURE_COMING_SOON'}},{status:'rejected',reason:{code:'FEATURE_COMING_SOON'}}]);
    expect(await z.counts()).toEqual([0,0,0,0,0,0,0,0]);
  }finally{vi.stubEnv('NODE_ENV','test');}
  for(const actor of [z.buyer,{...z.reviewer,branchId:z.branch},{...z.reviewer,departmentId:z.department}]){
    const command=key();await expect(api.createSupplyPostingChart(actor,{...initial,input:chartInput},command)).rejects.toMatchObject({status:403});expect(await postingCommandImage(command)).toEqual([[],[],[]]);
  }
  const command=key(),body={...initial,input:chartInput},chart=await api.createSupplyPostingChart(z.reviewer,body,command);
  await expect(api.createSupplyPostingChart(z.reviewer,{...body,input:{...chartInput,name:'Different original chart request'}},command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  const accountInput={chartId:chart.id,code:key(),name:'Current native bank',category:'asset' as const,currency:'THB',isControl:true,controlKind:'bank' as const};
  await expect(api.createSupplyPostingAccount(z.reviewer,{...initial,input:accountInput},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_CONTEXT_CHANGED'});
  await expect(api.createSupplyPostingAccount(z.reviewer,{...await zeroSupplyEnvelope(z.reviewer),input:{...accountInput,chartId:f.chart}},key())).rejects.toMatchObject({code:'CHART_NOT_FOUND'});
  const current=await zeroSupplyEnvelope(z.reviewer);
  await db.prepare('UPDATE assignments SET version=version+1 WHERE id=?').bind(z.reviewer.assignmentId).run();
  await expect(api.createSupplyPostingAccount(z.reviewer,{...current,input:accountInput},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_CONTEXT_CHANGED'});
  const writeEnvelope=await zeroSupplyEnvelope(z.reviewer),approveEnvelope=await zeroSupplyEnvelope(z.manager);
  await z.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...z.grant,state:'draining',version:2}});
  for(const run of [
    ()=>api.createSupplyPostingChart(z.reviewer,{...writeEnvelope,input:{...chartInput,name:'New draining chart'}},key()),
    ()=>api.createSupplyPostingAccount(z.reviewer,{...writeEnvelope,input:accountInput},key()),
    ()=>api.createSupplyPostingPeriod(z.reviewer,{...writeEnvelope,input:{name:'New draining period',startDate:'2026-01-01',endDate:'2026-12-31'}},key()),
    ()=>api.confirmSupplyPostingRate(z.manager,{...approveEnvelope,input:{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'New draining quote'}},key()),
  ])await expect(run()).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
  expect(await api.supplyPostingPrerequisites(z.reviewer)).toMatchObject({available:true,canCreateChart:false,canCreateAccount:false,canCreatePeriod:false,canConfirmRate:false});
  // Committed receipt replay retains the original body despite changed source
  // catalogue, while current authority still has to exist.
  expect(await api.createSupplyPostingChart(z.reviewer,body,command)).toEqual(chart);
  await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(z.reviewer.assignmentId).run();
  await expect(api.createSupplyPostingChart(z.reviewer,body,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
  expect(await z.counts()).toEqual([1,0,0,0,0,0,0,0]);
});

it('zero-ledger Supply prerequisites roll back original chart account period and rate effects on deferred source authority or purchase loss',async()=>{
  const z=await zeroLedgerSupplyCompany();await z.purchase();
  const api=await import('../../../src/server/modules/finance/public/supply-posting-prerequisites');
  const chart=await api.createSupplyPostingChart(z.reviewer,{...await zeroSupplyEnvelope(z.reviewer),input:{name:'Original selected prerequisite chart',baseCurrency:'THB',accountingBasis:'IFRS'}},key());
  const image=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','assignments'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? ORDER BY image`).bind(z.company).all()).results));
  for(const kind of ['chart','account','rate','period'] as const){
    if(kind==='period')await z.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...z.grant,endsAt:Date.now()+1500,version:2,reason:'Actual purchase deadline during deferred prerequisite effects',evidence:'test:zero-ledger-prerequisite-clock'}});
    const actor=kind==='rate'?z.manager:z.reviewer,context=await zeroSupplyEnvelope(actor),command=key(),fault=`zero_prerequisite_${key().replaceAll('-','')}`,before=await image();
    const effect=kind==='account'?`UPDATE finance_charts SET name='Deferred unreviewed chart',version=version+1 WHERE id='${chart.id}'`:kind==='period'?'PERFORM pg_sleep(1.7)':`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${actor.assignmentId}'`;
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      const run=kind==='chart'?()=>api.createSupplyPostingChart(actor,{...context,input:{name:'Uncommitted late authority chart',baseCurrency:'THB',accountingBasis:'IFRS'}},command):kind==='account'?()=>api.createSupplyPostingAccount(actor,{...context,input:{chartId:chart.id,code:key(),name:'Uncommitted original parent account',category:'asset',currency:'THB',isControl:false,controlKind:'none'}},command):kind==='period'?()=>api.createSupplyPostingPeriod(actor,{...context,input:{name:'Uncommitted expired period',startDate:'2026-01-01',endDate:'2026-12-31'}},command):()=>api.confirmSupplyPostingRate(actor,{...context,input:{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Uncommitted actual approver quote'}},command);
      await expect(run()).rejects.toMatchObject({code:kind==='account'?'COMMERCE_POSTING_SOURCE_CHANGED':kind==='period'?'PACKAGE_CAPABILITY_CHANGED':'QUEUED_AUTHORITY_CHANGED'});
      expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);
      expect(await image()).toEqual(before);expect(await postingCommandImage(command)).toEqual([[],[],[]]);
    }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
  expect(await z.counts()).toEqual([1,0,0,0,0,0,0,0]);
});

it('zero-ledger Supply sees normal period and quote writers before prerequisite snapshots without masked retries',async()=>{
  const z=await zeroLedgerSupplyCompany();await z.purchase();
  const api=await import('../../../src/server/modules/finance/public/supply-posting-prerequisites'),{createPeriod}=await import('../../../src/server/modules/finance/public/finance'),{confirmManualRate}=await import('../../../src/server/modules/finance/public/company-money'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics');
  for(const kind of ['period','rate'] as const){
    const context=await zeroSupplyEnvelope(z.reviewer),command=key(),fault=`zero_source_writer_${key().replaceAll('-','')}`;
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      const measured=await measureReads(async()=>{
        const writer=kind==='period'?createPeriod(z.reviewer,{name:'Normal first period writer',startDate:'2026-01-01',endDate:'2026-12-31'},command):confirmManualRate(z.manager,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Normal actual approver quote writer'},command);
        let staged=false;
        for(let i=0;i<100;i++){staged=!!await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called');if(staged)break;await new Promise(resolve=>setTimeout(resolve,10));}
        expect(staged).toBe(true);
        await expect(api.createSupplyPostingChart(z.reviewer,{...context,input:{name:'Captured before the original normal writer',baseCurrency:'THB',accountingBasis:'IFRS'}},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_CONTEXT_CHANGED'});
        await writer;
      });
      expect({deadlockFailures:measured.metrics.deadlockFailures??0,serializationFailures:measured.metrics.serializationFailures??0,transactionRetries:measured.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
    }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
  expect(await z.counts()).toEqual([0,0,1,1,0,0,0,0]);
});

// Supplier Money appendix: original financial/worker references are explicitly
// imported by expenseFixture; every new payable, batch, bank and Money fact below
// is created by its normal owner. No historical Money is deleted or synthesized.
async function supplierMoneyImage(){
 const queries=[
  ...['finance_supplier_batches','finance_invoices','finance_payments','finance_allocations','finance_journals','finance_book_postings','finance_financial_events','finance_bank_statements','finance_bank_transactions','money_transaction_snapshots','company_fx_rates','company_money_settings','company_currency_scales','assignments','company_package_grants'].map(table=>`SELECT to_jsonb(r)::text image FROM ${table} r WHERE r.company_id=? ORDER BY image`),
  'SELECT to_jsonb(r)::text image FROM finance_supplier_instructions r JOIN finance_supplier_batches b ON b.id=r.batch_id WHERE b.company_id=? ORDER BY image',
  'SELECT to_jsonb(r)::text image FROM finance_journal_lines r JOIN finance_journals j ON j.id=r.journal_id WHERE j.company_id=? ORDER BY image',
  'SELECT to_jsonb(r)::text image FROM finance_payment_reversals r JOIN finance_payments p ON p.id=r.payment_id WHERE p.company_id=? ORDER BY image',
  "SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.authority_json::jsonb->>'companyId'=? ORDER BY image",
  "SELECT to_jsonb(r)::text image FROM audit_events r WHERE r.authority_json::jsonb->>'companyId'=? ORDER BY image",
  "SELECT to_jsonb(r)::text image FROM outbox_events r WHERE r.payload_json::jsonb->>'companyId'=? ORDER BY image",
 ];
 return Promise.all(queries.map(async sql=>(await db.prepare(sql).bind(f.company).all<{image:string}>()).results.map(row=>row.image)));
}
async function supplierMoneyRows(){return (await db.prepare("SELECT source_type,source_id,amount_kind,transaction_date::text,currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback,original_source_type,original_source_id,original_amount_kind FROM money_transaction_snapshots WHERE company_id=? ORDER BY source_type,source_id,amount_kind").bind(f.company).all<Record<string,unknown>>()).results;}
async function supplierMoneySource(mode:'disabled'|'shadow'|'enforced'='enforced',purchased=true){
 const native=await import('../../../src/server/modules/finance/public/finance'),money=await import('../../../src/server/modules/finance/public/company-money');
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-supplier-money-provider'}),obligations:async()=>{throw Error('SUPPLIER_MONEY_FIXTURE_CANNOT_INACTIVATE');}}),grant={companyId:f.company,packageId:'finance',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Actual normal Supplier Money owner',evidence:'test:supplier-money-owner'};
 if(purchased)await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT',mode);
 const rate=await money.confirmManualRate(f.manager,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Actual independently confirmed fictional Supplier payment quote'},key());
 const party=await saveMaster(f.owner,'parties',null,{kind:'supplier',code:key(),name:'Actual independent Supplier Money source'},key()),invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:String(party.id),kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-01',dueDate:'2026-09-03',currency:'THB',subtotal:'100.01',tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key()),invoiceId=String(invoice.id);
 await assign('invoice',invoiceId,'draft',f.manager);await postInvoice(f.manager,invoiceId,{expectedVersion:1,periodId:f.period,entryDate:'2026-09-01'},key());
 const batch=await createSupplierBatch(f.reviewer,{bankAccountId:f.accounts.cash,plannedDate:'2026-09-03',items:[{invoiceId,expectedVersion:2,amount:'100.01',beneficiaryName:'Original reviewed beneficiary',beneficiaryAccount:'FICTITIOUS-SUPPLIER-MONEY'}]},key()),batchId=String(batch.id);
 await decideSupplierBatch(f.manager,batchId,{expectedVersion:1,reason:'A different approver reviewed original Supplier and exact amount'},key(),'approve');await exportSupplierBatch(f.payer,batchId,{expectedVersion:2},key());
 const externalId=key(),statement=await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:'2026-09-30',opening:'100.01',closing:'0',sourceHash:key(),rows:[{externalId,transactionDate:'2026-09-03',description:'Actual independent exact Supplier bank observation',amount:'-100.01',currency:'THB'}]},key()),bankId=String(await db.prepare('SELECT id FROM finance_bank_transactions WHERE statement_id=?').bind(statement.id).first('id')),instructionId=String((await supplierBatchWorkspace(f.payer,batchId)).items[0].id),command=key(),body={expectedVersion:2,bankTransactionId:bankId,bankVersion:1,periodId:f.period,bankReference:externalId,evidence:'Actual Treasury confirmed exact original Supplier bank observation',attachmentSetIds:[] as string[]};
 return {native,money,store,grant,rate,party,invoiceId,batchId,instructionId,bankId,command,body};
}
it('captures native Supplier instruction Money in all original purchase modes and reverses the exact immutable quote without rewriting Supplier history',async()=>{
 for(const mode of ['disabled','shadow','enforced'] as const){
  if(mode!=='disabled')f=await expenseFixture();
  const s=await supplierMoneySource(mode),confirmed=await confirmSupplierInstruction(f.payer,s.instructionId,s.body,s.command),paymentId=String(confirmed.paymentId),original=(await supplierMoneyRows()).find(row=>row.source_type==='finance_supplier_instruction'&&row.source_id===s.instructionId);
  expect(confirmed).toEqual({id:s.instructionId,status:'confirmed',version:3,paymentId:`supplier-payment-${s.command}`,journalId:`journal-${s.command}`});
  expect(original).toEqual({source_type:'finance_supplier_instruction',source_id:s.instructionId,amount_kind:'settled',transaction_date:'2026-09-03',currency:'THB',currency_scale:2,amount_units:'10001',usd_units:'278',fx_rate_id:s.rate.id,used_latest_fallback:false,original_source_type:null,original_source_id:null,original_amount_kind:null});
  expect((await supplierMoneyRows()).filter(row=>row.source_type==='finance_payment'&&row.source_id===paymentId)).toEqual([]);
  const receipt=await db.prepare('SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?').bind(s.command).first('image'),event=await db.prepare('SELECT to_jsonb(e)::text image FROM finance_financial_events e WHERE e.journal_id=?').bind(String(confirmed.journalId)).first('image');expect(event).not.toBeNull();
  await s.money.confirmManualRate(f.manager,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'40',note:'Later lawful quote must not rerate the original Supplier transaction'},key());
  const body={expectedVersion:1,periodId:f.period,entryDate:'2026-09-04',reason:'Independent correction of actual original Supplier allocation'},command=key(),reversed=await s.native.reversePayment(f.manager,paymentId,body,command),reversal=(await supplierMoneyRows()).find(row=>row.source_type==='finance_payment_reversal'&&row.source_id===reversed.reversalJournalId);
  expect(reversed).toEqual({id:paymentId,status:'voided',version:2,reversalJournalId:`payment-reversal-${command}`});expect(reversal).toEqual({...original,source_type:'finance_payment_reversal',source_id:reversed.reversalJournalId,amount_kind:'gross',transaction_date:'2026-09-04',amount_units:'-10001',usd_units:'-278',original_source_type:'finance_supplier_instruction',original_source_id:s.instructionId,original_amount_kind:'settled'});
  expect((await supplierMoneyRows()).find(row=>row.source_type==='finance_supplier_instruction'&&row.source_id===s.instructionId)).toEqual(original);
  expect(await db.prepare('SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?').bind(s.command).first('image')).toBe(receipt);expect(await db.prepare('SELECT to_jsonb(e)::text image FROM finance_financial_events e WHERE e.journal_id=?').bind(String(confirmed.journalId)).first('image')).toBe(event);
  expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(s.invoiceId).first()).toEqual({status:'posted',outstanding_minor:10001,base_outstanding_minor:10001});expect(await db.prepare('SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?').bind(s.bankId).first()).toEqual({match_status:'unmatched',matched_type:null,matched_id:null});
  const stable=await supplierMoneyImage();expect(await confirmSupplierInstruction(f.payer,s.instructionId,s.body,s.command)).toEqual(confirmed);expect(await s.native.reversePayment(f.manager,paymentId,body,command)).toEqual(reversed);expect(await supplierMoneyImage()).toEqual(stable);
 }
});
it('rolls back native Supplier confirmation Money receipt and accounting effects after actual immediate source or receipt faults and deferred quote or IAM faults',async()=>{
 const s=await supplierMoneySource(),probes=[
  {name:'source',deferred:false,sql:`UPDATE finance_supplier_instructions SET beneficiary_name=beneficiary_name||' immediate fault' WHERE id='${s.instructionId}'`,code:'SUPPLIER_MONEY_SOURCE_CHANGED'},
  {name:'receipt',deferred:false,sql:`UPDATE command_receipts SET request_hash=repeat('0',64) WHERE command_id='${s.command}'`,code:'IDEMPOTENCY_CONFLICT'},
  {name:'quote',deferred:true,sql:`INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) SELECT 'supplier-fault-${s.command}',company_id,currency,effective_date,currency_per_usd+1,confirmed_by,confirmed_at+1,'Labelled late-source fault, never a positive quote' FROM company_fx_rates WHERE id='${s.rate.id}'`,code:'SUPPLIER_MONEY_SOURCE_CHANGED'},
  {name:'iam',deferred:true,sql:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.payer.assignmentId}'`,code:'FINANCE_ACCESS_CHANGED'},
 ];
 for(const probe of probes){
  const name=`supplier_money_${probe.name}_${key().replaceAll('-','')}`,before=await supplierMoneyImage();
  await db.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}');${probe.sql};RETURN NEW;END $$;CREATE ${probe.deferred?'CONSTRAINT ':''}TRIGGER ${name} AFTER INSERT ON money_transaction_snapshots ${probe.deferred?'DEFERRABLE INITIALLY DEFERRED ':''}FOR EACH ROW WHEN(NEW.company_id='${f.company}' AND NEW.source_type='finance_supplier_instruction' AND NEW.source_id='${s.instructionId}') EXECUTE FUNCTION ${name}()`);
  try{await expect(confirmSupplierInstruction(f.payer,s.instructionId,s.body,s.command)).rejects.toMatchObject({code:probe.code});expect(await db.prepare(`SELECT is_called FROM ${name}`).first('is_called')).toBe(true);expect(await supplierMoneyImage()).toEqual(before);expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(s.command).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${name} ON money_transaction_snapshots;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`);}
 }
 const confirmed=await confirmSupplierInstruction(f.payer,s.instructionId,s.body,s.command),stable=await supplierMoneyImage();expect(await confirmSupplierInstruction(f.payer,s.instructionId,s.body,s.command)).toEqual(confirmed);expect(await supplierMoneyImage()).toEqual(stable);expect((await supplierMoneyRows()).filter(row=>row.source_type==='finance_supplier_instruction'&&row.source_id===s.instructionId)).toHaveLength(1);
});
it('preserves lawful Supplier settlement after audited expiry while current purchase denial and deferred reversal source loss remain atomic',async()=>{
 const s=await supplierMoneySource(),prior=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='finance');if(!prior)throw Error('ACTUAL_FINANCE_PURCHASE_REQUIRED');
 await s.store.apply({expectedVersion:prior.version,idempotencyKey:key(),grant:{...prior,endsAt:Date.now()-1,version:prior.version+1,reason:'Audited actual Finance expiry retains existing approved Supplier duties',evidence:'test:supplier-money-expiry'}});
 const confirmed=await confirmSupplierInstruction(f.payer,s.instructionId,s.body,s.command),paymentId=String(confirmed.paymentId),command=key(),body={expectedVersion:1,periodId:f.period,entryDate:'2026-09-04',reason:'Independent correction retains source-bound settling authority'},name=`supplier_money_reverse_${key().replaceAll('-','')}`,before=await supplierMoneyImage();
 await db.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}');UPDATE finance_supplier_instructions SET evidence=evidence||' deferred source fault' WHERE id='${s.instructionId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON money_transaction_snapshots DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.company_id='${f.company}' AND NEW.source_type='finance_payment_reversal' AND NEW.source_id='payment-reversal-${command}') EXECUTE FUNCTION ${name}()`);
 try{await expect(s.native.reversePayment(f.manager,paymentId,body,command)).rejects.toMatchObject({code:'SUPPLIER_MONEY_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${name}`).first('is_called')).toBe(true);expect(await supplierMoneyImage()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${name} ON money_transaction_snapshots;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`);}
 const reversed=await s.native.reversePayment(f.manager,paymentId,body,command),stable=await supplierMoneyImage();expect(await s.native.reversePayment(f.manager,paymentId,body,command)).toEqual(reversed);expect(await supplierMoneyImage()).toEqual(stable);
 // A separate legacy Company has normal original owner-created AP, an
 // approved/exported batch and imported bank evidence, but no purchase. The
 // new enforced call must deny it without minting a quote/Money/receipt effect.
 const original=f;f=await expenseFixture();try{const denied=await supplierMoneySource('disabled',false);expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const clean=await supplierMoneyImage();await expect(confirmSupplierInstruction(f.payer,denied.instructionId,denied.body,denied.command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await supplierMoneyImage()).toEqual(clean);}finally{f=original;}

});
it('waits for the actual disabled invoice-planning payment receipt before Supplier Money reads and completes both normal owners without hidden retries',async()=>{
 const s=await supplierMoneySource('disabled'),{measureReads}=await import('../../../src/server/modules/runtime/adapters/runtime/read-metrics'),bank=await s.native.createAccount(f.reviewer,{chartId:f.chart,code:key(),name:'Actual original controlled payment bank',category:'asset',currency:'THB',isControl:true,controlKind:'bank'},key()),body={partyId:String(s.party.id),kind:'payment' as const,paymentNumber:key(),paymentDate:'2026-09-03',currency:'THB',amount:'100.01',bankAccount:String(bank.id),settlementInvoiceId:s.invoiceId},oldKey=key(),name=`supplier_money_old_writer_${key().replaceAll('-','')}`;
 await db.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${name} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${oldKey}') EXECUTE FUNCTION ${name}()`);
 try{
  const observed=await measureReads(async()=>{let done=false;const old=Promise.allSettled([s.native.createPayment(f.reviewer,body,oldKey).finally(()=>{done=true;})]);let atReceipt=false;for(let attempt=0;attempt<100;attempt++){atReceipt=!!await db.prepare(`SELECT is_called FROM ${name}`).first('is_called');if(atReceipt||done)break;await new Promise(resolve=>setTimeout(resolve,5));}if(!atReceipt){const [result]=await old;if(result.status==='rejected')throw result.reason;throw Error('Actual disabled invoice-planning payment did not reach its native receipt');}expect(done).toBe(false);const current=Promise.allSettled([confirmSupplierInstruction(f.payer,s.instructionId,s.body,s.command)]);let waiting=false;for(let attempt=0;attempt<100&&!waiting&&!done;attempt++){waiting=!!await db.prepare("SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='command_receipts'::regclass AND mode='ShareRowExclusiveLock' AND NOT granted LIMIT 1").first();if(!waiting)await new Promise(resolve=>setTimeout(resolve,5));}const [[ordinary],[supplier]]=await Promise.all([old,current]);return {waiting,ordinary,supplier};});
  expect(observed.result.waiting).toBe(true);expect(observed.result.ordinary.status).toBe('fulfilled');expect(observed.result.supplier.status).toBe('fulfilled');if(observed.result.ordinary.status!=='fulfilled')throw observed.result.ordinary.reason;if(observed.result.supplier.status!=='fulfilled')throw observed.result.supplier.reason;
  expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});expect(observed.result.ordinary.value).toMatchObject({status:'draft',settlementInvoiceId:s.invoiceId});expect(observed.result.supplier.value).toMatchObject({status:'confirmed',paymentId:`supplier-payment-${s.command}`});
  const stable=await supplierMoneyImage();expect(await s.native.createPayment(f.reviewer,body,oldKey)).toEqual(observed.result.ordinary.value);expect(await confirmSupplierInstruction(f.payer,s.instructionId,s.body,s.command)).toEqual(observed.result.supplier.value);expect(await supplierMoneyImage()).toEqual(stable);
 }finally{await db.exec(`DROP TRIGGER ${name} ON command_receipts;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`);}
});


it('rolls back an immediate Supplier reversal receipt authority Company mutation and recovers the exact original command without duplicate Money',async()=>{
 const s=await supplierMoneySource(),confirmed=await confirmSupplierInstruction(f.payer,s.instructionId,s.body,s.command),paymentId=String(confirmed.paymentId),command=key(),body={expectedVersion:1,periodId:f.period,entryDate:'2026-09-04',reason:'Independent correction retains exact native receipt Company'},fault=`supplier_receipt_company_${key().replaceAll('-','')}`;
 expect((await financePurchaseControls(f.manager))?.['finance.banking']?.settleExisting).toBe(true);
 const commandImage=async()=>(await Promise.all([
  db.prepare('SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=? ORDER BY image').bind(command).all<{image:string}>(),
  db.prepare('SELECT to_jsonb(r)::text image FROM audit_events r WHERE r.command_id=? ORDER BY image').bind(command).all<{image:string}>(),
  db.prepare('SELECT to_jsonb(r)::text image FROM outbox_events r WHERE r.command_id=? ORDER BY image').bind(command).all<{image:string}>(),
 ])).map(result=>result.results.map(row=>row.image)),before=await supplierMoneyImage(),beforeCommand=await commandImage();
 // Adversarial negative metadata fault only: leave request_hash/result_json and
 // all normal business inputs unchanged; never synthesize a payment or Money.
 await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');UPDATE command_receipts SET authority_json=jsonb_set(authority_json::jsonb,'{companyId}',to_jsonb('unrelated-adversarial-receipt-company'::text))::text WHERE command_id=NEW.command_id;RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${command}' AND NEW.operation='finance.payment.reverse') EXECUTE FUNCTION ${fault}()`);
 try{
  await expect(s.native.reversePayment(f.manager,paymentId,body,command)).rejects.toMatchObject({code:'SUPPLIER_MONEY_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);
  expect(await supplierMoneyImage()).toEqual(before);expect(await commandImage()).toEqual(beforeCommand);expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();
 }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
 const reversed=await s.native.reversePayment(f.manager,paymentId,body,command);expect(reversed).toEqual({id:paymentId,status:'voided',version:2,reversalJournalId:`payment-reversal-${command}`});expect(await db.prepare("SELECT authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE command_id=?").bind(command).first('company_id')).toBe(f.company);
 expect((await supplierMoneyRows()).filter(row=>row.source_type==='finance_payment_reversal'&&row.source_id===reversed.reversalJournalId)).toHaveLength(1);const stable=await supplierMoneyImage(),stableCommand=await commandImage();expect(await s.native.reversePayment(f.manager,paymentId,body,command)).toEqual(reversed);expect(await supplierMoneyImage()).toEqual(stable);expect(await commandImage()).toEqual(stableCommand);
});
