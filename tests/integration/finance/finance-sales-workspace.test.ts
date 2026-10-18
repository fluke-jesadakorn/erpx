import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { requestCommerceReturn, decideCommerceReturn, completeCommerceReturn, returnRequestSchema, saveMaster, saveOrder, orderSchema, transitionOrder, fulfillOrder, fulfillmentSchema, invoiceFulfillment, getOrder, settleInvoice } from "../../../src/server/modules/commerce/public/commerce";
import { postInvoice, postJournal, reversePayment, getInvoice } from "../../../src/server/modules/finance/public/finance";
import { deliveryEvidence } from "../../support/helpers/delivery-evidence";
import { assignFlowPrimary, resolveFlow } from "../../../src/server/modules/workflow/public/flow-routing";
import { invoiceSalesCompletion, prepareSalesDefaults, decideSalesDefaults, salesWorkspace, customerReceiptDetail, customerReceiptExport, createSalesInvoiceDraft } from "../../../src/server/modules/finance/public/sales";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  f.head.role = "sales_manager"; f.head.branchId = null; f.head.departmentId = null; f.head.assignmentId = await f.grant(f.head, "sales_manager");
  f.manager.branchId = null; f.manager.departmentId = null; f.payer.branchId = null; f.payer.departmentId = null;
  await db.prepare("UPDATE finance_accounts SET control_kind='bank',is_control=1 WHERE id=?").bind(f.accounts.cash).run();
  await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,'ar-sales','Trade receivables','asset','THB',1,'receivable',1,1)").bind(`${f.company}-ar`,f.chart,f.company).run();
  await db.prepare("UPDATE finance_accounts SET category='liability',control_kind='none' WHERE id=?").bind(f.accounts.tax).run();
});
async function assign(kind: "order" | "invoice", id: string, stepKey: string, userId: string) {
  const flow = await resolveFlow(db, f.owner, kind, id);
  await assignFlowPrimary(db, f.owner, kind, id, { stepKey, userId, reason: "Assign sales workspace test responsibility", saveDefault: false, expectedVersion: flow.version, recordVersion: flow.recordVersion }, key());
}
async function defaults() {
  const input = { chartId: f.chart, currency: "THB", receivableAccountId: `${f.company}-ar`, revenueAccountId: f.accounts.revenue, taxAccountId: f.accounts.tax, reason: "Reviewed sales account defaults for customer invoicing" };
  const edition = await prepareSalesDefaults(f.owner, input, key());
  await expect(decideSalesDefaults(f.owner, String(edition.id), { expectedVersion: 1, action: "approve", reason: "Independently reviewed active accounts" }, key())).rejects.toMatchObject({ code: "INDEPENDENT_APPROVER_REQUIRED" });
  await decideSalesDefaults(f.manager, String(edition.id), { expectedVersion: 1, action: "approve", reason: "Independently reviewed active accounts" }, key());
}
it("completes and invoices services without warehouses, inventory or cost journals; receipts preserve partial allocations", async () => {
  await defaults();
  await db.prepare("DELETE FROM commerce_settings WHERE company_id=?").bind(f.company).run();
  const customer = await saveMaster(f.owner, "parties", null, { kind: "customer", code: key(), name: "Installation customer" }, key());
  const service = await saveMaster(f.owner, "products", null, { sku: key(), name: "Installation", price: "100", itemType: "service", taxBps: 700 }, key());
  const quoteKey = key(), input = orderSchema.parse({ kind: "quote", partyId: customer.id, orderDate: "2026-09-02", dueDate: "2026-09-10", lines: [{ productId: service.id, quantity: "2", price: "100", taxBps: 700 }] });
  const quote = await saveOrder(f.owner, null, input, quoteKey);
  expect(await saveOrder(f.owner, null, input, quoteKey)).toEqual(quote);
  expect((await getOrder(f.owner, String(quote.id))).order.number).toBe("QT-2026-000001");
  await transitionOrder(f.owner, String(quote.id), { expectedVersion: 1, action: "submit" }, key());
  await transitionOrder(f.head, String(quote.id), { expectedVersion: 2, action: "approve" }, key());
  await assign("order", String(quote.id), "confirmed", f.owner.userId);
  const conversion = await transitionOrder(f.owner, String(quote.id), { expectedVersion: 3, action: "convert" }, key()), orderId = String(conversion.salesId);
  await transitionOrder(f.owner, orderId, { expectedVersion: 1, action: "submit" }, key());
  await transitionOrder(f.head, orderId, { expectedVersion: 2, action: "approve" }, key());
  await assign("order", orderId, "confirmed", f.owner.userId);
  const lineId = String((await getOrder(f.owner, orderId)).lines[0].id), completionKey = key();
  const completionInput = fulfillmentSchema.parse({ expectedVersion: 3, date: "2026-09-03", completionMode: "service", evidenceNote: "Customer accepted installation phase one", lines: [{ lineId, quantity: "1" }] });
  const first = await fulfillOrder(f.owner, orderId, completionInput, completionKey);
  expect(await fulfillOrder(f.owner, orderId, completionInput, completionKey)).toEqual(first);
  await expect(fulfillOrder(f.owner, orderId, completionInput, key())).rejects.toMatchObject({ status: 409 });
  const second = await fulfillOrder(f.owner, orderId, fulfillmentSchema.parse({ ...completionInput, expectedVersion: 4, date: "2026-09-04" }), key());
  expect(await db.prepare("SELECT COUNT(*) n FROM commerce_stock WHERE product_id=?").bind(service.id).first("n")).toBe(0);
  expect(await db.prepare("SELECT COUNT(*) n FROM commerce_fulfillments WHERE order_id=? AND journal_id IS NOT NULL").bind(orderId).first("n")).toBe(0);
  await expect(saveMaster(f.owner, "products", String(service.id), { expectedVersion: 1, sku: key(), name: "Installation", price: "100", itemType: "stock" }, key())).rejects.toMatchObject({code:"ITEM_TYPE_LOCKED"});
  await assign("order", orderId, "invoicing", f.owner.userId);
  const invoiceKey = key(), invoiceInput = { expectedVersion: 1, date: "2026-09-04", dueDate: "2026-09-10", additionalFulfillments: [{ id: String(second.fulfillmentId), expectedVersion: 1 }] };
  const result = await invoiceFulfillment(f.owner, String(first.fulfillmentId), invoiceInput, invoiceKey), invoiceId = String(result.invoiceId);
  expect(await invoiceFulfillment(f.owner, String(first.fulfillmentId), invoiceInput, invoiceKey)).toEqual(result);
  expect(await db.prepare("SELECT invoice_number,total_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first()).toEqual({ invoice_number: "INV-2026-000001", total_minor: 21400 });
  await assign("invoice", invoiceId, "draft", f.manager.userId);
  await postInvoice(f.manager, invoiceId, { expectedVersion: 1, periodId: f.period, entryDate: "2026-09-04" }, key());
  await assign("invoice", invoiceId, "posted", f.payer.userId);
  const settlementKey = key(), paymentInput = { expectedVersion: 2, date: "2026-09-05", amount: "50", bankAccountId: f.accounts.cash, reference: "Customer bank receipt first installment" };
  const payment = await settleInvoice(f.payer, invoiceId, paymentInput, settlementKey);
  expect(await settleInvoice(f.payer, invoiceId, paymentInput, settlementKey)).toEqual(payment);
  const receiptId = String(await db.prepare("SELECT id FROM finance_customer_receipts WHERE payment_id=?").bind(payment.paymentId).first("id"));
  expect((await customerReceiptDetail(f.owner, receiptId)).snapshot).toMatchObject({ documentNumber: "RC-2026-000001", amountMinor: "5000", allocations: [{ invoiceId, invoiceNumber: "INV-2026-000001", amountMinor: "5000" }] });
  const exported = await customerReceiptExport(f.owner, receiptId);
  await expect(db.prepare("UPDATE finance_customer_receipts SET document_number='rewrite' WHERE id=?").bind(receiptId).run()).rejects.toThrow("CUSTOMER_RECEIPT_IMMUTABLE");
  expect((await customerReceiptExport(f.owner, receiptId)).sha256).toBe(exported.sha256);
  expect(await db.prepare("SELECT outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first("outstanding_minor")).toBe(16400);
  const balance = await db.prepare("SELECT SUM(l.base_debit_minor-l.base_credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=? AND j.status='posted'").bind(`${f.company}-ar`).first("balance");
  expect(balance).toBe(16400);
  const workspace = await salesWorkspace(f.owner, { tab: "invoices" });
  expect(workspace.summary).toEqual([expect.objectContaining({ currency: "THB", awaiting_minor: 16400 })]);
  expect(workspace.received).toEqual([expect.objectContaining({ currency: "THB", received_minor: 5000 })]);
  await reversePayment(f.manager, String(payment.paymentId), { expectedVersion: 1, periodId: f.period, entryDate: "2026-09-06", reason: "Bank evidence independently confirms an incorrect customer receipt" }, key());
  expect((await customerReceiptDetail(f.owner, receiptId)).status).toBe("cancelled");
  expect((await customerReceiptExport(f.owner, receiptId)).sha256).toBe(exported.sha256);
  expect(await db.prepare("SELECT outstanding_minor FROM finance_invoices WHERE id=?").bind(invoiceId).first("outstanding_minor")).toBe(21400);
  const deliveryLine=String((await getOrder(f.owner,orderId)).fulfillmentLines.find(line=>line.fulfillment_id===first.fulfillmentId)!.id);
  const credit=await requestCommerceReturn(f.owner,String(first.fulfillmentId),returnRequestSchema.parse({expectedVersion:2,date:"2026-09-07",reason:"Customer and independent reviewer agreed to credit the completed installation",lines:[{lineId:deliveryLine,quantity:"1"}]}),key());
  const reviewed=await decideCommerceReturn(f.head,String(credit.id),"approve",{expectedVersion:1,reason:"Independent sales manager reviewed the service credit"},key());
  expect(reviewed.stockJournalId).toBeUndefined();
  const creditJournal=String(reviewed.creditJournalId),flow=await resolveFlow(db,f.owner,"journal",creditJournal);
  await assignFlowPrimary(db,f.owner,"journal",creditJournal,{stepKey:"draft",userId:f.manager.userId,reason:"Independent Finance review of service credit",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  await postJournal(f.manager,creditJournal,key(),{expectedVersion:1});
  await completeCommerceReturn(f.manager,String(credit.id),{expectedVersion:2},key());
  expect(await db.prepare("SELECT COUNT(*) n FROM inventory_ledger WHERE product_id=?").bind(service.id).first("n")).toBe(0);

});
it("retains each installment receipt and rejects closed periods and out-of-scope reads", async () => {
  await defaults();
  const customer=await saveMaster(f.owner,"parties",null,{kind:"customer",code:key(),name:"Installment customer"},key());
  const draft=await createSalesInvoiceDraft(f.owner,{partyId:String(customer.id),invoiceDate:"2026-09-02",dueDate:"2026-09-10",currency:"THB",completionConfirmed:true,completionNote:"Customer confirmed the installation was completed",lines:[{description:"Installation",quantity:"1",unitPrice:"100",taxBps:700}]},key()),id=String(draft.id);
  await assign("invoice",id,"draft",f.manager.userId);
  await db.prepare("UPDATE finance_periods SET state='closed' WHERE id=?").bind(f.period).run();
  await expect(postInvoice(f.manager,id,{expectedVersion:1,periodId:f.period,entryDate:"2026-09-03"},key())).rejects.toMatchObject({status:409});
  expect((await getInvoice(f.owner,id)).invoice.status).toBe("draft");
  await db.prepare("UPDATE finance_periods SET state='open' WHERE id=?").bind(f.period).run();
  await postInvoice(f.manager,id,{expectedVersion:1,periodId:f.period,entryDate:"2026-09-03"},key());
  await assign("invoice",id,"posted",f.payer.userId);
  const first=await settleInvoice(f.payer,id,{expectedVersion:2,date:"2026-09-04",amount:"33.33",reference:"Customer installment one",bankAccountId:f.accounts.cash},key());
  await expect(settleInvoice(f.payer,id,{expectedVersion:2,date:"2026-09-04",amount:"10",reference:"Stale invoice receipt",bankAccountId:f.accounts.cash},key())).rejects.toMatchObject({status:409});
  await settleInvoice(f.payer,id,{expectedVersion:3,date:"2026-09-05",amount:"73.67",reference:"Customer installment two",bankAccountId:f.accounts.cash},key());
  const receiptRows=(await db.prepare("SELECT id,document_number,amount_minor FROM finance_customer_receipts WHERE company_id=? ORDER BY document_number").bind(f.company).all<{id:string;document_number:string;amount_minor:number}>()).results;
  expect(receiptRows.map(row=>[row.document_number,row.amount_minor])).toEqual([["RC-2026-000001",3333],["RC-2026-000002",7367]]);
  expect((await getInvoice(f.owner,id)).invoice).toMatchObject({status:"paid",outstanding_minor:0});
  const ar=await db.prepare("SELECT SUM(l.base_debit_minor-l.base_credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=? AND j.status='posted'").bind(`${f.company}-ar`).first("balance");expect(ar).toBe(0);
  await expect(customerReceiptDetail({...f.owner,companyId:"another-company"},receiptRows[0].id)).rejects.toMatchObject({code:"RECEIPT_NOT_FOUND"});
  await expect(customerReceiptDetail({...f.owner,branchId:f.branch},receiptRows[0].id)).rejects.toMatchObject({code:"RECEIPT_NOT_FOUND"});
  const searched=await salesWorkspace(f.owner,{tab:"receipts",q:String(draft.number),startDate:"",endDate:""});
  expect(searched.page?.items).toHaveLength(2);expect(searched.received).toEqual([expect.objectContaining({currency:"THB",received_minor:10700})]);
  expect(await db.prepare("SELECT COUNT(*) n FROM finance_customer_receipts WHERE payment_id=?").bind(first.paymentId).first("n")).toBe(1);
});
it("allocates concurrent automatic invoice numbers and replays without creating duplicates", async () => {
  await defaults();
  const customer = await saveMaster(f.owner, "parties", null, { kind: "customer", code: key(), name: "Service customer" }, key());
  const input = { partyId: String(customer.id), invoiceDate: "2026-09-02", dueDate: "2026-09-10", currency: "THB", completionConfirmed: true as const, completionNote: "Completed service confirmed by the customer", lines: [{ description: "Consulting", quantity: "1", unitPrice: "100", taxBps: 700 }] };
  const keys = [key(), key()];
  const results = await Promise.all(keys.map(command => createSalesInvoiceDraft(f.owner, input, command)));
  expect(new Set(results.map(row => row.number)).size).toBe(2);
  expect(await invoiceSalesCompletion(f.owner,String(results[0].id))).toMatchObject({note:input.completionNote,confirmedBy:f.owner.userId,invoiceRevision:1});
  expect(await invoiceSalesCompletion({...f.owner,companyId:"another-company"},String(results[0].id))).toBeNull();
  const sameKey = key(), same = await Promise.all([createSalesInvoiceDraft(f.owner,input,sameKey),createSalesInvoiceDraft(f.owner,input,sameKey)]);
  expect(same[0]).toEqual(same[1]);
  expect(await createSalesInvoiceDraft(f.owner, input, keys[0])).toEqual(results[0]);
  expect(await db.prepare("SELECT COUNT(*) n FROM finance_invoices WHERE company_id=?").bind(f.company).first("n")).toBe(3);
  await expect(createSalesInvoiceDraft({ ...f.owner, branchId: f.branch }, input, key())).rejects.toMatchObject({ code: "COMPANY_SCOPE_REQUIRED" });
});

it("combines equipment and installation, preserving stock cost and itemized source lines", async () => {
  await defaults();
  const warehouse=await saveMaster(f.owner,"warehouses",null,{branchId:f.branch,code:key(),name:"Equipment warehouse"},key());
  const goods=await saveMaster(f.owner,"products",null,{sku:key(),name:"Equipment",price:"100"},key()),service=await saveMaster(f.owner,"products",null,{sku:key(),name:"Installation",price:"50",itemType:"service"},key());
  const customer=await saveMaster(f.owner,"parties",null,{kind:"customer",code:key(),name:"Equipment customer"},key());
  await db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,6000,1)").bind(warehouse.id,goods.id).run();
  const result=await saveOrder(f.owner,null,orderSchema.parse({kind:"sales",partyId:customer.id,warehouseId:warehouse.id,orderDate:"2026-09-02",dueDate:"2026-09-10",lines:[{productId:goods.id,quantity:"2",price:"100",taxBps:700},{productId:service.id,quantity:"1",price:"50",taxBps:700}]}),key()),orderId=String(result.id);
  await transitionOrder(f.owner,orderId,{expectedVersion:1,action:"submit"},key());await transitionOrder(f.head,orderId,{expectedVersion:2,action:"approve"},key());await assign("order",orderId,"confirmed",f.owner.userId);
  const lines=(await getOrder(f.owner,orderId)).lines;
  const delivered=await fulfillOrder(f.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:"2026-09-03",lines:[{lineId:String(lines.find(line=>line.item_type==="stock")!.id),quantity:"1"}],...await deliveryEvidence(db,f.owner,orderId,"Customer signed equipment delivery proof")}),key());
  const completed=await fulfillOrder(f.owner,orderId,fulfillmentSchema.parse({expectedVersion:4,date:"2026-09-03",completionMode:"service",evidenceNote:"Installed and accepted by the customer",lines:[{lineId:String(lines.find(line=>line.item_type==="service")!.id),quantity:"1"}]}),key());
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id=?").bind(goods.id).first()).toEqual({quantity_milli:1000,value_minor:3000});
  expect(await db.prepare("SELECT COUNT(*) n FROM inventory_ledger WHERE product_id=?").bind(service.id).first("n")).toBe(0);
  const journal=String(await db.prepare("SELECT journal_id FROM commerce_fulfillments WHERE id=?").bind(delivered.fulfillmentId).first("journal_id"));
  const flow=await resolveFlow(db,f.owner,"journal",journal);await assignFlowPrimary(db,f.owner,"journal",journal,{stepKey:"draft",userId:f.manager.userId,reason:"Review goods cost independently",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postJournal(f.manager,journal,key(),{expectedVersion:1});
  await assign("order",orderId,"invoicing",f.owner.userId);
  const invoice=await invoiceFulfillment(f.owner,String(delivered.fulfillmentId),{expectedVersion:1,date:"2026-09-03",dueDate:"2026-09-10",useSalesDefaults:true,additionalFulfillments:[{id:String(completed.fulfillmentId),expectedVersion:1}]},key()),invoiceId=String(invoice.invoiceId);
  const detail=await getInvoice(f.owner,invoiceId);expect(detail.lines.map(line=>[line.description,line.quantity_millis,line.total_minor])).toEqual([["Equipment",1000,10700],["Installation",1000,5350]]);
  expect(detail.invoice.total_minor).toBe(16050);
  expect((await salesWorkspace({...f.owner,branchId:f.branch},{tab:"invoices",currency:"THB"})).page?.items.some(row=>row.id===invoiceId)).toBe(true);
  expect((await salesWorkspace({...f.owner,branchId:"other-branch"},{tab:"invoices"})).page?.items.some(row=>row.id===invoiceId)).toBe(false);
  await expect(fulfillOrder({...f.owner,branchId:"other-branch"},orderId,fulfillmentSchema.parse({expectedVersion:5,date:"2026-09-04",lines:[{lineId:String(lines[0].id),quantity:"1"}]}),key())).rejects.toMatchObject({code:"ORDER_NOT_FOUND"});
});


// Sales-only initial service startup: original four cases above remain byte-exact.
import { afterEach } from "vitest";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { packageGrantStore, readCompanyPackageGrants } from "../../../src/server/modules/commercial/public/package-grants";
import { salesDefaultsWorkspace } from "../../../src/server/modules/finance/public/sales";
afterEach(() => vi.unstubAllEnvs());

async function zeroLedgerSalesCompany() {
  // Seed organization and actual native worker/assignment authority only.
  // This Company has NEVER had a chart/account/period/quote/settings row.
  // It is a disposable Company fixture, not normal Company onboarding proof.
  const company=`zero-sales-${key()}`,branch=`${company}-branch`,department=`${company}-department`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero ledger Sales fixture',?)").bind(company,company,company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Native services branch',?,?,?)").bind(branch,branch,company,company,branch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Native Sales team',?,?,?)").bind(department,department,branch,company,branch),
  ]);
  async function actor(name:string,role:string,scopeKind:'holding'|'branch'|'department'='holding'):Promise<WorkContext>{
    const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`;
    await db.batch([
      db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,branch_id,scope_node_id) VALUES(?,?,?,'[]',?,0,9999999999999,?,?,?)").bind(assignmentId,userId,role,company,scopeKind,scopeKind==='holding'?null:branch,scopeKind==='department'?department:scopeKind==='branch'?branch:null),
      db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${userId}`,userId,company,name),
      db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Native worker','THB','fixture',1,1)").bind(`eng-${userId}`,`person-${userId}`,company,branch,department),
    ]);
    const scope:WorkContext['scope']=scopeKind==='department'?{kind:'department',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company,branchId:branch,departmentId:department,departmentSlug:department,departmentName:'Native Sales team'}:scopeKind==='branch'?{kind:'branch',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company,branchId:branch,branchSlug:branch,branchName:'Native service branch'}:{kind:'holding',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company};
    const context:WorkContext={userId,assignmentId,role,companyId:company,branchId:scopeKind==='holding'?null:branch,departmentId:scopeKind==='department'?department:null,holdingId:'h001',holdingSlug:'test',holdingName:'Test',name,email:`${name}@example.test`,assignmentScope:scopeKind,inherited:false,modules:[],assignments:[],workspaces:[],scope};
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
  const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-zero-ledger-Sales-provider'}),obligations:async()=>{throw Error('ZERO_LEDGER_FIXTURE_CANNOT_WITHDRAW');}}),grant={companyId:company,packageId:'sales',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Reviewed isolated zero-ledger services foundation',evidence:'test:zero-ledger-Sales'};
  async function purchase(){await store.apply({expectedVersion:0,idempotencyKey:key(),grant});}
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
  return {company,branch,department,owner,reviewer,manager,salesManager,treasury,counts,purchase,store,grant,actor};
}



async function zeroSalesEnvelope(actor:WorkContext){
  const {salesPostingSetup}=await import('../../../src/server/modules/finance/public/sales-posting-setup');
  const view=await salesPostingSetup(actor);
  expect(view.available).toBe(true);expect(view.contextKey).toMatch(/^[a-f0-9]{64}$/);
  if(!view.contextKey)throw Error('Actual current Sales posting context required');
  return {expectedVersion:0 as const,contextKey:view.contextKey};
}

async function zeroSalesReferences(z:Awaited<ReturnType<typeof zeroLedgerSalesCompany>>){
  const api=await import('../../../src/server/modules/finance/public/sales-posting-setup'),chartBody={...await zeroSalesEnvelope(z.reviewer),input:{name:'Original first Sales service chart',baseCurrency:'THB',accountingBasis:'IFRS'}},chartKey=key(),chart=await api.createSalesPostingChart(z.reviewer,chartBody,chartKey);
  expect(await api.createSalesPostingChart(z.reviewer,chartBody,chartKey)).toEqual(chart);
  const accounts={receivableAccountId:'',revenueAccountId:'',taxAccountId:'',cashAccountId:''};
  for(const [field,category,controlKind] of [['receivableAccountId','asset','receivable'],['revenueAccountId','revenue','none'],['taxAccountId','liability','none'],['cashAccountId','asset','bank']] as const){
    const body={...await zeroSalesEnvelope(z.reviewer),input:{chartId:chart.id,code:key(),name:`Original initial ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},command=key(),row=await api.createSalesPostingAccount(z.reviewer,body,command);
    accounts[field]=row.id;expect(await api.createSalesPostingAccount(z.reviewer,body,command)).toEqual(row);
  }
  const periodBody={...await zeroSalesEnvelope(z.reviewer),input:{name:'Original first service year',startDate:'2026-01-01',endDate:'2026-12-31'}},periodKey=key(),period=await api.createSalesPostingPeriod(z.reviewer,periodBody,periodKey);
  expect(period).toMatchObject({state:'open',version:0});expect(await api.createSalesPostingPeriod(z.reviewer,periodBody,periodKey)).toEqual(period);
  const rateInput={currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Original actual Company approver confirms fictional dated service quote'};
  await expect(api.confirmSalesPostingRate(z.reviewer,{...await zeroSalesEnvelope(z.reviewer),input:rateInput},key())).rejects.toMatchObject({status:403});
  const rateBody={...await zeroSalesEnvelope(z.manager),input:rateInput},rateKey=key(),rate=await api.confirmSalesPostingRate(z.manager,rateBody,rateKey);
  expect(await api.confirmSalesPostingRate(z.manager,rateBody,rateKey)).toEqual(rate);
  expect(await db.prepare('SELECT confirmed_by FROM company_fx_rates WHERE id=?').bind(rate.id).first('confirmed_by')).toBe(z.manager.userId);
  return {api,chart,accounts,period,rate,defaults:{chartId:chart.id,currency:'THB',receivableAccountId:accounts.receivableAccountId,revenueAccountId:accounts.revenueAccountId,taxAccountId:accounts.taxAccountId,reason:'Normal original Company preparer selected the actual service receivable revenue and output VAT'}};
}

async function assignZeroSales(z:Awaited<ReturnType<typeof zeroLedgerSalesCompany>>,kind:'order'|'invoice'|'journal',id:string,stepKey:string,actor:WorkContext){
  const flow=await resolveFlow(db,z.owner,kind,id);
  await assignFlowPrimary(db,z.owner,kind,id,{stepKey,userId:actor.userId,reason:'Original current Sales-only independent responsibility',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
}

async function zeroSalesCommandImage(command:string){
  return Promise.all(['command_receipts','audit_events','outbox_events'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE command_id=? ORDER BY image`).bind(command).all()).results));
}

it('Sales-only service startup creates its empty ledger and independently reviewed defaults before quote service invoice and receipt without Supply or full Finance',async()=>{
  const z=await zeroLedgerSalesCompany();await z.purchase();
  const originalView=await salesDefaultsWorkspace(z.reviewer);
  expect(originalView).toMatchObject({editions:[],usableDefaults:[],issues:[],options:{charts:[],accounts:[]}});
  const api=await import('../../../src/server/modules/finance/public/sales-posting-setup');
  expect(await api.salesPostingSetup(z.reviewer)).toMatchObject({available:true,charts:[],accounts:[],periods:[],rates:[],defaults:[],canCreateChart:true,canCreateAccount:true,canCreatePeriod:true,canPrepareDefaults:true,canConfirmRate:false});
  expect(await api.salesPostingDocumentReadiness(z.salesManager,'quote')).toEqual({ready:false,reason:'sales-defaults-required'});
  const refs=await zeroSalesReferences(z);
  for(const invalid of [{...refs.defaults,receivableAccountId:refs.accounts.revenueAccountId},{...refs.defaults,taxAccountId:refs.accounts.cashAccountId},{...refs.defaults,receivableAccountId:`${f.company}-ar`}])await expect(api.prepareSalesPostingDefaults(z.reviewer,{...await zeroSalesEnvelope(z.reviewer),input:invalid},key())).rejects.toMatchObject({code:'SALES_ACCOUNTS_INVALID'});
  const body={...await zeroSalesEnvelope(z.reviewer),input:refs.defaults},prepareKey=key(),edition=await api.prepareSalesPostingDefaults(z.reviewer,body,prepareKey);
  expect(edition).toMatchObject({status:'draft',version:1});expect(await api.prepareSalesPostingDefaults(z.reviewer,body,prepareKey)).toEqual(edition);
  // A genuinely authorized owner preparing the same source must still obtain a
  // different reviewer; permission aliases do not waive the original rule.
  const own=await api.prepareSalesPostingDefaults(z.owner,{...await zeroSalesEnvelope(z.owner),input:{...refs.defaults,reason:'Original owner preparation still requires an independent actual reviewer'}},key());
  expect((await api.salesPostingSetup(z.owner)).defaults.find(row=>row.id===own.id)).toMatchObject({canReview:false});
  expect((await api.salesPostingSetup(z.manager)).defaults.find(row=>row.id===edition.id)).toMatchObject({canReview:true});
  await expect(api.decideSalesPostingDefaults(z.owner,String(own.id),{contextKey:(await zeroSalesEnvelope(z.owner)).contextKey,input:{expectedVersion:1,action:'approve',reason:'Same original person cannot independently approve'}},key())).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});
  const decision={contextKey:(await zeroSalesEnvelope(z.manager)).contextKey,input:{expectedVersion:1,action:'approve' as const,reason:'Original independent Company Finance approver checked current native accounts'}},reviewKey=key(),approved=await api.decideSalesPostingDefaults(z.manager,String(edition.id),decision,reviewKey);
  expect(approved).toMatchObject({status:'approved',version:2});expect(await api.decideSalesPostingDefaults(z.manager,String(edition.id),decision,reviewKey)).toEqual(approved);
  expect((await api.salesPostingSetup(z.reviewer)).defaults).toEqual(expect.arrayContaining([expect.objectContaining({id:edition.id,status:'approved',usable:true})]));
  expect(await api.salesPostingDocumentReadiness(z.salesManager,'quote')).toEqual({ready:true,reason:null});
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(z.company).first()).toBeNull();
  const customer=await saveMaster(z.owner,'parties',null,{kind:'customer',code:key(),name:'Actual Sales-only service customer'},key()),service=await saveMaster(z.owner,'products',null,{sku:key(),name:'Actual Sales-only installation',price:'100',itemType:'service',taxBps:700},key()),quoteBody=orderSchema.parse({kind:'quote',partyId:customer.id,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId:service.id,quantity:'1',price:'100',taxBps:700}]}),quoteKey=key(),quote=await saveOrder(z.owner,null,quoteBody,quoteKey);
  const {salesEditorOptions}=await import('../../../src/server/modules/commerce/public/sales-editor'),editor=await salesEditorOptions(z.salesManager,'quote');
  expect(Object.keys(editor).sort()).toEqual(['currency','nodes','parties','priceListItems','priceLists','products','warehouses']);
  expect(editor).toMatchObject({currency:'THB',nodes:[],warehouses:[],products:[expect.objectContaining({id:service.id,item_type:'service'})],parties:[expect.objectContaining({id:customer.id,kind:'customer'})]});
  expect(Object.keys(editor.products[0]).sort()).toEqual(['id','item_type','name','price_minor','sku','status','tax_bps']);
  expect(await saveOrder(z.owner,null,quoteBody,quoteKey)).toEqual(quote);
  await transitionOrder(z.owner,String(quote.id),{expectedVersion:1,action:'submit'},key());await transitionOrder(z.salesManager,String(quote.id),{expectedVersion:2,action:'approve'},key());await assignZeroSales(z,'order',String(quote.id),'confirmed',z.owner);
  const conversion=await transitionOrder(z.owner,String(quote.id),{expectedVersion:3,action:'convert'},key()),orderId=String(conversion.salesId);
  await transitionOrder(z.owner,orderId,{expectedVersion:1,action:'submit'},key());await transitionOrder(z.salesManager,orderId,{expectedVersion:2,action:'approve'},key());await assignZeroSales(z,'order',orderId,'confirmed',z.owner);
  const lineId=String((await getOrder(z.owner,orderId)).lines[0].id),completeBody=fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-03',completionMode:'service',evidenceNote:'Customer accepted the actual installed service',lines:[{lineId,quantity:'1'}]}),completeKey=key(),completion=await fulfillOrder(z.owner,orderId,completeBody,completeKey);
  expect(await fulfillOrder(z.owner,orderId,completeBody,completeKey)).toEqual(completion);
  expect(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(completion.fulfillmentId).first()).toMatchObject({journal_id:null});
  expect(await db.prepare('SELECT count(*) n FROM commerce_stock WHERE product_id=?').bind(service.id).first('n')).toBe(0);
  expect(await db.prepare('SELECT count(*) n FROM inventory_ledger WHERE product_id=?').bind(service.id).first('n')).toBe(0);
  await assignZeroSales(z,'order',orderId,'invoicing',z.owner);
  const invoiceBody={expectedVersion:1,date:'2026-09-04',dueDate:'2026-09-10'},invoiceKey=key(),invoiced=await invoiceFulfillment(z.owner,String(completion.fulfillmentId),invoiceBody,invoiceKey),invoiceId=String(invoiced.invoiceId);
  expect(await invoiceFulfillment(z.owner,String(completion.fulfillmentId),invoiceBody,invoiceKey)).toEqual(invoiced);
  await assignZeroSales(z,'invoice',invoiceId,'draft',z.manager);await postInvoice(z.manager,invoiceId,{expectedVersion:1,periodId:refs.period.id,entryDate:'2026-09-04'},key());
  expect((await getInvoice(z.reviewer,invoiceId)).invoice).toMatchObject({status:'posted',total_minor:10700,outstanding_minor:10700});
  const {financeProductDocumentEntry}=await import('../../../src/server/modules/finance/public/document-purchases');
  expect(await financeProductDocumentEntry(db,{companyId:z.company,branchId:null,departmentId:null,userId:z.reviewer.userId,assignmentId:z.reviewer.assignmentId,assignmentVersion:Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(z.reviewer.assignmentId).first('version')),role:z.reviewer.role},{kind:'invoice',id:invoiceId})).toEqual({capability:'finance.sales-posting',purchasePurpose:'history'});
  await assignZeroSales(z,'invoice',invoiceId,'posted',z.treasury);
  const receiptBody={expectedVersion:2,date:'2026-09-05',amount:'50',bankAccountId:refs.accounts.cashAccountId,reference:'Actual original Treasury service customer receipt'},receiptKey=key(),payment=await settleInvoice(z.treasury,invoiceId,receiptBody,receiptKey);
  expect(await settleInvoice(z.treasury,invoiceId,receiptBody,receiptKey)).toEqual(payment);
  const receiptId=String(await db.prepare('SELECT id FROM finance_customer_receipts WHERE payment_id=?').bind(payment.paymentId).first('id')),receipt=await customerReceiptDetail(z.reviewer,receiptId),exported=await customerReceiptExport(z.reviewer,receiptId);
  expect(receipt.snapshot).toMatchObject({documentNumber:'RC-2026-000001',amountMinor:'5000',allocations:[{invoiceId,invoiceNumber:'INV-2026-000001',amountMinor:'5000'}]});
  expect((await getInvoice(z.reviewer,invoiceId)).invoice.outstanding_minor).toBe(5700);
  const {getPayment}=await import('../../../src/server/modules/finance/public/finance'),{salesCustomerReceiptProductEntry}=await import('../../../src/server/modules/finance/public/sales-receipt-sources'),receiptScope={companyId:z.company,branchId:null,departmentId:null,userId:z.manager.userId,assignmentId:z.manager.assignmentId,assignmentVersion:1,role:z.manager.role,sessionId:key()};
  expect(await salesCustomerReceiptProductEntry(db,receiptScope,receiptId)).toEqual({capability:'finance.sales-posting',purchasePurpose:'history'});
  expect(await getPayment(z.manager,String(payment.paymentId))).toMatchObject({payment:{status:'settled'},sourcePurchase:{capability:'finance.sales-posting',settleExisting:true},invoices:[],bankTransactions:[],fxAccounts:[]});
  const reversalBody={expectedVersion:1,periodId:refs.period.id,entryDate:'2026-09-06',reason:'Actual independent original reviewer corrects the misidentified customer bank receipt'};
  await expect(reversePayment(z.treasury,String(payment.paymentId),reversalBody,key())).rejects.toMatchObject({code:'INDEPENDENT_CORRECTION_REQUIRED'});
  const reversalImage=()=>Promise.all(['commerce_orders','finance_payments','finance_invoices','finance_journals','finance_customer_receipts','finance_financial_events','money_transaction_snapshots'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? ORDER BY image`).bind(z.company).all()).results));
  // Real post-scan evidence starts on the ordinary settled payment journal.
  // The deferred faults are source-invalid diagnostics, not fabricated valid
  // reversal history. A genuine normal reversal proves retained Files below.
  vi.stubEnv('ERPX_ATTACHMENTS_V2','true');
  const fileApi=await import('../../../src/server/modules/files/public/attachments/service'),fileFixtures=await import('../../support/helpers/attachment-v2'),settledJournalId=String(await db.prepare('SELECT journal_id FROM finance_payments WHERE id=?').bind(payment.paymentId).first('journal_id')),settledJournalVersion=Number(await db.prepare('SELECT version FROM finance_journals WHERE id=?').bind(settledJournalId).first('version')),ordinaryFile=await fileFixtures.seedAttachmentSet(db,z.reviewer,{targetType:'finance_journal',targetId:settledJournalId,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:settledJournalVersion});
  const ordinaryObjectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(ordinaryFile.sourceId).first('object_key'));await env.H001_FILES.put(ordinaryObjectKey,'%PDF-1.7',{httpMetadata:{contentType:'application/pdf'}});
  const fileTables=['commerce_fulfillments','finance_payments','finance_journals','finance_payment_reversals','finance_allocations','finance_customer_receipts','money_transaction_snapshots','upload_intents','attachments','attachment_sets','attachment_links','attachment_events','attachment_access_events','command_receipts','audit_events','outbox_events'],fileImage=()=>Promise.all(fileTables.map(async table=>(await db.prepare(`SELECT row_to_json(r)::text image FROM ${table} r ORDER BY row_to_json(r)::text`).all()).results));
  for(const kind of ['settled-status','source-link'] as const){
    const command=key(),fault=`sales_ordinary_file_${key().replaceAll('-','')}`,before=await fileImage(),effect=kind==='settled-status'?`UPDATE finance_payments SET status='voided' WHERE id='${payment.paymentId}'`:`UPDATE commerce_fulfillments SET invoice_id=NULL WHERE id='${completion.fulfillmentId}'`;
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      let failure:unknown;try{await fileApi.bindRecordAttachmentSet(db,z.reviewer,ordinaryFile.setId,settledJournalVersion,command);}catch(error){failure=error;}
      expect(failure).toBeInstanceOf(Error);if(!(failure instanceof Error))throw Error('Actual ordinary-source File bind must fail after its deferred source change');
      expect(failure.cause??failure).toMatchObject({code:'23514',detail:expect.stringContaining('finance-document-owner')});
      expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await fileImage()).toEqual(before);
    }finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
  await fileApi.bindRecordAttachmentSet(db,z.reviewer,ordinaryFile.setId,settledJournalVersion,key());expect(await fileFixtures.attachmentBindingState(db,ordinaryFile.setId)).toMatchObject({set_state:'bound',link_state:'bound'});
  expect(await(await fileApi.attachmentContent(db,env.H001_FILES,z.reviewer,ordinaryFile.linkId,'original')).object.text()).toBe('%PDF-1.7');
  for(const kind of ['owner','authority'] as const){
    const command=key(),fault=`sales_reverse_${key().replaceAll('-','')}`,before=await reversalImage(),effect=kind==='owner'?`UPDATE commerce_orders SET kind='purchase' WHERE id='${orderId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${z.manager.assignmentId}'`;
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      await expect(reversePayment(z.manager,String(payment.paymentId),reversalBody,command)).rejects.toMatchObject({code:kind==='owner'?'SALES_RECEIPT_SOURCE_CHANGED':'FINANCE_ACCESS_CHANGED'});
      expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await reversalImage()).toEqual(before);expect(await zeroSalesCommandImage(command)).toEqual([[],[],[]]);
      expect(await db.prepare('SELECT 1 FROM finance_payment_reversals WHERE payment_id=?').bind(payment.paymentId).first()).toBeNull();
    }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
  const reversalKey=key(),reversed=await reversePayment(z.manager,String(payment.paymentId),reversalBody,reversalKey);
  expect(await reversePayment(z.manager,String(payment.paymentId),reversalBody,reversalKey)).toEqual(reversed);
  expect(await(await fileApi.attachmentContent(db,env.H001_FILES,z.reviewer,ordinaryFile.linkId,'original')).object.text()).toBe('%PDF-1.7');
  const retainedJournalId=String(reversed.reversalJournalId),retainedVersion=Number(await db.prepare('SELECT version FROM finance_journals WHERE id=?').bind(retainedJournalId).first('version')),retainedFile=await fileFixtures.seedAttachmentSet(db,z.reviewer,{targetType:'finance_journal',targetId:retainedJournalId,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:retainedVersion}),retainedObjectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(retainedFile.sourceId).first('object_key'));await env.H001_FILES.put(retainedObjectKey,'%PDF-1.7',{httpMetadata:{contentType:'application/pdf'}});
  await fileApi.bindRecordAttachmentSet(db,z.reviewer,retainedFile.setId,retainedVersion,key());expect(await(await fileApi.attachmentContent(db,env.H001_FILES,z.reviewer,retainedFile.linkId,'original')).object.text()).toBe('%PDF-1.7');

  await expect(reversePayment(z.manager,String(payment.paymentId),{...reversalBody,reason:'Different original reversal body cannot borrow the committed command key'},reversalKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  const retainedPayment=await getPayment(z.manager,String(payment.paymentId));
  expect(retainedPayment).toMatchObject({payment:{status:'voided'},sourcePurchase:{capability:'finance.sales-posting',settleExisting:false}});
  const recordTargets=[{kind:'payment' as const,id:String(payment.paymentId)},{kind:'journal' as const,id:String(retainedPayment.payment.journal_id)},{kind:'journal' as const,id:String(reversed.reversalJournalId)}];
  for(const target of recordTargets)expect(await financeProductDocumentEntry(db,receiptScope,target)).toEqual({capability:'finance.sales-posting',purchasePurpose:'history'});
  expect(await salesCustomerReceiptProductEntry(db,receiptScope,receiptId)).toEqual({capability:'finance.sales-posting',purchasePurpose:'history'});
  expect((await customerReceiptDetail(z.reviewer,receiptId)).status).toBe('cancelled');expect((await customerReceiptExport(z.reviewer,receiptId)).sha256).toBe(exported.sha256);
  expect((await getInvoice(z.reviewer,invoiceId)).invoice.outstanding_minor).toBe(10700);
  expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId)).toEqual(['sales']);
  expect(await db.prepare('SELECT count(*) n FROM commerce_settings WHERE company_id=?').bind(z.company).first('n')).toBe(0);
  expect(await db.prepare('SELECT count(*) n FROM commerce_warehouses WHERE company_id=?').bind(z.company).first('n')).toBe(0);
  await z.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...z.grant,state:'draining',version:2}});
  const draining=await api.salesPostingSetup(z.manager),open=draining.defaults.find(row=>row.id===own.id);
  expect(open).toMatchObject({canApprove:false,canReject:true});
  const drainingContext=await zeroSalesEnvelope(z.manager);
  await expect(api.decideSalesPostingDefaults(z.manager,String(own.id),{contextKey:drainingContext.contextKey,input:{expectedVersion:1,action:'approve',reason:'New activation must not be introduced while Sales drains'}},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
  expect(await api.decideSalesPostingDefaults(z.manager,String(own.id),{contextKey:drainingContext.contextKey,input:{expectedVersion:1,action:'reject',reason:'Independent original reviewer closes the remaining actual draft'}},key())).toMatchObject({status:'rejected',version:2});
  expect(await api.prepareSalesPostingDefaults(z.reviewer,body,prepareKey)).toEqual(edition);
  expect(await api.decideSalesPostingDefaults(z.manager,String(edition.id),decision,reviewKey)).toEqual(approved);
  await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(z.manager.assignmentId).run();
  expect(await salesCustomerReceiptProductEntry(db,receiptScope,receiptId)).toBeNull();
  for(const target of recordTargets)expect(await financeProductDocumentEntry(db,receiptScope,target)).toBeNull();
  await expect(customerReceiptDetail(z.manager,receiptId)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
  await expect(customerReceiptExport(z.manager,receiptId)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
  for(const evidence of [ordinaryFile,retainedFile])await expect(fileApi.attachmentContent(db,env.H001_FILES,z.manager,evidence.linkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});
  expect(await(await env.H001_FILES.get(ordinaryObjectKey))!.text()).toBe('%PDF-1.7');expect(await(await env.H001_FILES.get(retainedObjectKey))!.text()).toBe('%PDF-1.7');

});

// A separate original legacy fixture supplies the same-Company no-purchase
// history negative. Its seeded ledger is NOT the zero-ledger positive above.
async function legacyServiceReceiptWithoutPurchase(){
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');
  try{
    await defaults();
    const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Original legacy service customer'},key()),service=await saveMaster(f.owner,'products',null,{sku:key(),name:'Original legacy service',price:'10',itemType:'service',taxBps:0},key()),quote=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId:service.id,quantity:'1',price:'10',taxBps:0}]}),key());
    await transitionOrder(f.owner,String(quote.id),{expectedVersion:1,action:'submit'},key());await transitionOrder(f.head,String(quote.id),{expectedVersion:2,action:'approve'},key());await assign('order',String(quote.id),'confirmed',f.owner.userId);
    const converted=await transitionOrder(f.owner,String(quote.id),{expectedVersion:3,action:'convert'},key()),orderId=String(converted.salesId);
    await transitionOrder(f.owner,orderId,{expectedVersion:1,action:'submit'},key());await transitionOrder(f.head,orderId,{expectedVersion:2,action:'approve'},key());await assign('order',orderId,'confirmed',f.owner.userId);
    const lineId=String((await getOrder(f.owner,orderId)).lines[0].id),completed=await fulfillOrder(f.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-03',completionMode:'service',evidenceNote:'Original legacy customer confirmed the service',lines:[{lineId,quantity:'1'}]}),key());
    await assign('order',orderId,'invoicing',f.owner.userId);
    const invoiced=await invoiceFulfillment(f.owner,String(completed.fulfillmentId),{expectedVersion:1,date:'2026-09-03',dueDate:'2026-09-10'},key()),invoiceId=String(invoiced.invoiceId);
    await assign('invoice',invoiceId,'draft',f.manager.userId);await postInvoice(f.manager,invoiceId,{expectedVersion:1,periodId:f.period,entryDate:'2026-09-03'},key());await assign('invoice',invoiceId,'posted',f.payer.userId);
    const paid=await settleInvoice(f.payer,invoiceId,{expectedVersion:2,date:'2026-09-04',amount:'10',bankAccountId:f.accounts.cash,reference:'Original normal legacy customer receipt'},key()),receiptId=String(await db.prepare('SELECT id FROM finance_customer_receipts WHERE payment_id=?').bind(paid.paymentId).first('id'));
    expect((await customerReceiptDetail(f.owner,receiptId)).status).toBe('issued');
    return {receiptId,paymentId:String(paid.paymentId)};
  }finally{vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');}
}

it('Sales-only service startup retains Company authority exact bodies current references release and draining history',async()=>{
  const z=await zeroLedgerSalesCompany(),api=await import('../../../src/server/modules/finance/public/sales-posting-setup'),chartInput={name:'Original guarded Sales chart',baseCurrency:'THB',accountingBasis:'IFRS'},empty={expectedVersion:0 as const,contextKey:'0'.repeat(64)},scope={companyId:z.company,branchId:null,departmentId:null,userId:z.reviewer.userId,assignmentId:z.reviewer.assignmentId,assignmentVersion:1,role:z.reviewer.role,sessionId:key()};
  expect(await api.salesPostingSetup(z.reviewer)).toMatchObject({available:false,contextKey:null,charts:[],accounts:[],defaults:[]});
  expect(await api.salesPostingSetupProductEntry(db,scope)).toBeNull();
  const {salesEditorOptions}=await import('../../../src/server/modules/commerce/public/sales-editor');
  expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');
  try{expect(await salesEditorOptions(f.head,'quote')).toMatchObject({currency:'THB',nodes:[]});}finally{vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');}
  await expect(salesEditorOptions(f.head,'quote')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  const {receiptId:legacyReceipt,paymentId:legacyPayment}=await legacyServiceReceiptWithoutPurchase();
  expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);
  await expect(customerReceiptDetail(f.owner,legacyReceipt)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await expect(customerReceiptExport(f.owner,legacyReceipt)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  const {salesCustomerReceiptProductEntry}=await import('../../../src/server/modules/finance/public/sales-receipt-sources');
  expect(await salesCustomerReceiptProductEntry(db,{companyId:f.company,branchId:null,departmentId:null,userId:f.owner.userId,assignmentId:f.owner.assignmentId,assignmentVersion:Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first('version')),role:f.owner.role,sessionId:key()},legacyReceipt)).toBeNull();
  await expect(api.createSalesPostingChart(z.reviewer,{...empty,input:chartInput},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await expect(prepareSalesDefaults(f.owner,{chartId:f.chart,currency:'THB',receivableAccountId:`${f.company}-ar`,revenueAccountId:f.accounts.revenue,taxAccountId:f.accounts.tax,reason:'Original default route must retain actual package authority'},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await z.purchase();const initial=await zeroSalesEnvelope(z.reviewer);
  await expect(reversePayment(z.manager,legacyPayment,{expectedVersion:1,periodId:f.period,entryDate:'2026-09-05',reason:'A purchased Company cannot reverse another Company customer receipt'},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  const selectedBefore=(await db.prepare('SELECT to_jsonb(a)::text image FROM assignments a WHERE company_id=? ORDER BY id').bind(z.company).all()).results;
  expect(await api.salesPostingSetupProductEntry(db,scope)).toMatchObject({revision:expect.stringMatching(/^[a-f0-9]{64}$/),purchasePurpose:'history'});
  expect((await db.prepare('SELECT to_jsonb(a)::text image FROM assignments a WHERE company_id=? ORDER BY id').bind(z.company).all()).results).toEqual(selectedBefore);
  for(const actor of [z.salesManager,await z.actor('branch-preparer','finance_preparer','branch'),await z.actor('department-preparer','finance_preparer','department')]){
    const command=key();expect(await api.salesPostingSetup(actor)).toMatchObject({available:false,contextKey:null,charts:[],accounts:[],defaults:[]});
    await expect(api.createSalesPostingChart(actor,{...initial,input:chartInput},command)).rejects.toMatchObject({status:403});expect(await zeroSalesCommandImage(command)).toEqual([[],[],[]]);
    expect(await api.salesPostingSetupProductEntry(db,{...scope,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,role:actor.role})).toBeNull();
  }
  vi.stubEnv('NODE_ENV','production');
  try{
    expect(await api.salesPostingSetupProductEntry(db,scope)).toBeNull();
    await expect(api.salesPostingSetup(z.reviewer)).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
    await expect(api.createSalesPostingChart(z.reviewer,{...initial,input:chartInput},key())).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
  }finally{vi.stubEnv('NODE_ENV','test');}
  const command=key(),body={...initial,input:chartInput},chart=await api.createSalesPostingChart(z.reviewer,body,command);
  await expect(api.createSalesPostingChart(z.reviewer,{...body,input:{...chartInput,name:'A different original chart body'}},command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  const accountInput={chartId:chart.id,code:key(),name:'Native initial Sales bank',category:'asset' as const,currency:'THB',isControl:true,controlKind:'bank' as const};
  await expect(api.createSalesPostingAccount(z.reviewer,{...initial,input:accountInput},key())).rejects.toMatchObject({code:'SALES_POSTING_CONTEXT_CHANGED'});
  await expect(api.createSalesPostingAccount(z.reviewer,{...await zeroSalesEnvelope(z.reviewer),input:{...accountInput,chartId:f.chart}},key())).rejects.toMatchObject({code:'CHART_NOT_FOUND'});
  const current=await zeroSalesEnvelope(z.reviewer);await db.prepare('UPDATE assignments SET version=version+1 WHERE id=?').bind(z.reviewer.assignmentId).run();
  await expect(api.createSalesPostingAccount(z.reviewer,{...current,input:accountInput},key())).rejects.toMatchObject({code:'SALES_POSTING_CONTEXT_CHANGED'});
  expect(await api.salesPostingSetupProductEntry(db,scope)).toBeNull();
  const writeContext=await zeroSalesEnvelope(z.reviewer),approveContext=await zeroSalesEnvelope(z.manager);
  await z.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...z.grant,state:'draining',version:2}});
  for(const run of [()=>api.createSalesPostingChart(z.reviewer,{...writeContext,input:{...chartInput,name:'New draining chart'}},key()),()=>api.createSalesPostingAccount(z.reviewer,{...writeContext,input:accountInput},key()),()=>api.createSalesPostingPeriod(z.reviewer,{...writeContext,input:{name:'New draining year',startDate:'2026-01-01',endDate:'2026-12-31'}},key()),()=>api.confirmSalesPostingRate(z.manager,{...approveContext,input:{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'New draining quote'}},key())])await expect(run()).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
  expect(await api.salesPostingSetup(z.reviewer)).toMatchObject({available:true,canCreateChart:false,canCreateAccount:false,canCreatePeriod:false,canConfirmRate:false,canPrepareDefaults:false});
  expect(await api.createSalesPostingChart(z.reviewer,body,command)).toEqual(chart);
  await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(z.reviewer.assignmentId).run();
  expect(await api.salesPostingSetupProductEntry(db,{...scope,assignmentVersion:2})).toBeNull();
  await expect(api.createSalesPostingChart(z.reviewer,body,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
  expect(await z.counts()).toEqual([1,0,0,0,0,0,0,0]);
});

it('Sales-only service startup rolls back original prerequisite and reviewed-default commands on late current authority source or purchase loss',async()=>{
  const z=await zeroLedgerSalesCompany();await z.purchase();const refs=await zeroSalesReferences(z),api=refs.api;
  let reviewedDraftId:string|undefined;
  const image=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','finance_sales_defaults','assignments'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? ORDER BY image`).bind(z.company).all()).results));
  for(const kind of ['chart','account','rate','defaults','review','period'] as const){
    const draft=kind==='review'?await api.prepareSalesPostingDefaults(z.reviewer,{...await zeroSalesEnvelope(z.reviewer),input:refs.defaults},key()):null;
    if(draft)reviewedDraftId=String(draft.id);
    if(kind==='period')await z.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...z.grant,endsAt:Date.now()+1500,version:2,reason:'Actual Sales purchase expires while original period command effects are deferred',evidence:'test:sales-zero-ledger-deadline'}});
    const actor=kind==='rate'||kind==='review'?z.manager:z.reviewer,context=await zeroSalesEnvelope(actor),command=key(),fault=`sales_setup_${key().replaceAll('-','')}`,before=await image(),source=kind==='account'||kind==='defaults'||kind==='review',effect=kind==='account'?`UPDATE finance_charts SET name='Unreviewed deferred chart title',version=version+1 WHERE id='${refs.chart.id}'`:kind==='defaults'||kind==='review'?`UPDATE finance_accounts SET status='inactive',version=version+1 WHERE id='${refs.accounts.revenueAccountId}'`:kind==='period'?'PERFORM pg_sleep(1.7)':`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${actor.assignmentId}'`;
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      const run=kind==='chart'?()=>api.createSalesPostingChart(actor,{...context,input:{name:'Uncommitted late-authority service chart',baseCurrency:'THB',accountingBasis:'IFRS'}},command):kind==='account'?()=>api.createSalesPostingAccount(actor,{...context,input:{chartId:refs.chart.id,code:key(),name:'Uncommitted original chart child',category:'asset',currency:'THB',isControl:false,controlKind:'none'}},command):kind==='rate'?()=>api.confirmSalesPostingRate(actor,{...context,input:{currency:'THB',effectiveDate:'2026-09-02',currencyPerUsd:'37',note:'Uncommitted actual Company approver quote'}},command):kind==='defaults'?()=>api.prepareSalesPostingDefaults(actor,{...context,input:refs.defaults},command):kind==='review'?()=>api.decideSalesPostingDefaults(actor,String(draft?.id),{contextKey:context.contextKey,input:{expectedVersion:1,action:'approve',reason:'Original independent review must roll back with its account source'}},command):()=>api.createSalesPostingPeriod(actor,{...context,input:{name:'Uncommitted next Sales period',startDate:'2027-01-01',endDate:'2027-12-31'}},command);
      await expect(run()).rejects.toMatchObject({code:source?'SALES_POSTING_SOURCE_CHANGED':kind==='period'?'PACKAGE_CAPABILITY_CHANGED':'QUEUED_AUTHORITY_CHANGED'});
      expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await image()).toEqual(before);expect(await zeroSalesCommandImage(command)).toEqual([[],[],[]]);
    }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
  if(!reviewedDraftId)throw Error('Actual original reviewed draft must have been prepared');
  expect(await db.prepare('SELECT status,version,reviewed_by FROM finance_sales_defaults WHERE id=?').bind(reviewedDraftId).first()).toEqual({status:'draft',version:1,reviewed_by:null});
});

it('Sales-only service startup sees actual normal period and rate writers before snapshots with no masked retries',async()=>{
  const z=await zeroLedgerSalesCompany();await z.purchase();const api=await import('../../../src/server/modules/finance/public/sales-posting-setup'),{createPeriod}=await import('../../../src/server/modules/finance/public/finance'),{confirmManualRate}=await import('../../../src/server/modules/finance/public/company-money'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics');
  for(const kind of ['period','rate'] as const){
    const context=await zeroSalesEnvelope(z.reviewer),command=key(),fault=`sales_setup_writer_${key().replaceAll('-','')}`;
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      const measured=await measureReads(async()=>{
        const writer=kind==='period'?createPeriod(z.reviewer,{name:'Normal first Sales period writer',startDate:'2026-01-01',endDate:'2026-12-31'},command):confirmManualRate(z.manager,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Normal original Company approver writes actual Sales quote'},command);
        let staged=false;for(let i=0;i<100;i++){staged=!!await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called');if(staged)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(staged).toBe(true);
        await expect(api.createSalesPostingChart(z.reviewer,{...context,input:{name:'Captured before the original normal reference writer',baseCurrency:'THB',accountingBasis:'IFRS'}},key())).rejects.toMatchObject({code:'SALES_POSTING_CONTEXT_CHANGED'});await writer;
      });
      expect({deadlockFailures:measured.metrics.deadlockFailures??0,serializationFailures:measured.metrics.serializationFailures??0,transactionRetries:measured.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
    }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
  expect(await z.counts()).toEqual([0,0,1,1,0,0,0,0]);
});

// Additive Sales-host service catalogue: normal original owners; no Stock/Finance seed.
async function readySalesServiceCatalogue(){
 const z=await zeroLedgerSalesCompany();await z.purchase();const refs=await zeroSalesReferences(z),context=await zeroSalesEnvelope(z.reviewer),edition=await refs.api.prepareSalesPostingDefaults(z.reviewer,{...context,input:refs.defaults},key());
 await refs.api.decideSalesPostingDefaults(z.manager,String(edition.id),{contextKey:(await zeroSalesEnvelope(z.manager)).contextKey,input:{expectedVersion:1,action:'approve',reason:'Independent original approver enables the actual service-only catalogue'}},key());return {z,refs};
}
it('Sales-only service catalogue native reader and original product writer enable the first quotation without Supply stock balances or Commerce9 setup',async()=>{
 const {z}=await readySalesServiceCatalogue(),api=await import('../../../src/server/modules/commerce/public/sales-service-products');
 expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(z.company).first()).toBeNull();expect((await api.salesServiceProducts(z.owner))).toMatchObject({available:true,currency:'THB',canCreate:true,products:[],unavailableReason:null});expect((await api.salesServiceProducts(z.salesManager))).toMatchObject({available:true,canCreate:false,products:[]});
 const input=api.salesServiceProductSchema.parse({itemType:'service',sku:key(),name:'Actual first Sales-host service',unit:'visit',price:'100',taxBps:700}),command=key();await expect(api.createSalesServiceProduct(z.salesManager,input,key())).rejects.toMatchObject({status:403});const saved=await api.createSalesServiceProduct(z.owner,input,command);expect(saved).toEqual({id:`products-${command}`,version:1});expect(await api.createSalesServiceProduct(z.owner,input,command)).toEqual(saved);
 const view=await api.salesServiceProducts(z.owner);expect(view.products).toEqual([{id:saved.id,sku:input.sku,name:input.name,unit:'visit',priceMinor:10000,taxBps:700,status:'active',version:1}]);expect(Object.keys(view).sort()).toEqual(['available','canCreate','companyId','contextKey','currency','products','unavailableReason']);expect(await db.prepare('SELECT operation FROM command_receipts WHERE command_id=?').bind(command).first('operation')).toBe('commerce.products.create');expect(await db.prepare("SELECT currency,amount_units::text FROM money_transaction_snapshots WHERE company_id=? AND source_type='commerce_product_revision' AND source_id=? AND amount_kind='unit_price'").bind(z.company,`${saved.id}:1`).first()).toEqual({currency:'THB',amount_units:'10000'});
 const customer=await saveMaster(z.owner,'parties',null,{kind:'customer',code:key(),name:'Actual first service customer'},key()),quote=await saveOrder(z.salesManager,null,orderSchema.parse({kind:'quote',partyId:customer.id,orderDate:'2026-10-17',dueDate:'2026-10-20',lines:[{productId:saved.id,quantity:'1',price:'100',taxBps:700}]}),key());expect((await getOrder(z.salesManager,String(quote.id))).lines[0]).toMatchObject({product_id:saved.id,item_type:'service',quantity_milli:1000});
 expect(await db.prepare('SELECT count(*) n FROM commerce_stock WHERE product_id=?').bind(saved.id).first('n')).toBe(0);expect(await db.prepare('SELECT count(*) n FROM inventory_ledger WHERE product_id=?').bind(saved.id).first('n')).toBe(0);expect(await db.prepare('SELECT count(*) n FROM commerce_warehouses WHERE company_id=?').bind(z.company).first('n')).toBe(0);
});
it('Sales service catalogue preserves strict source schema original permissions exact replay and draining history',async()=>{
 const {z}=await readySalesServiceCatalogue(),api=await import('../../../src/server/modules/commerce/public/sales-service-products'),input=api.salesServiceProductSchema.parse({itemType:'service',sku:key(),name:'Retained original service',price:'10'});
 const absent=await zeroLedgerSalesCompany(),absentCommand=key();expect(await api.salesServiceProducts(absent.owner)).toMatchObject({available:false,canCreate:false,products:[],unavailableReason:'purchase-required'});await expect(api.createSalesServiceProduct(absent.owner,input,absentCommand)).rejects.toMatchObject({status:403});expect(await zeroSalesCommandImage(absentCommand)).toEqual([[],[],[]]);expect(await absent.counts()).toEqual([0,0,0,0,0,0,0,0]);
 for(const raw of [{...input,itemType:'stock'},{...input,trackingMode:'serial'},{...input,expectedVersion:1},{...input,companyId:'foreign'}])await expect(api.createSalesServiceProduct(z.owner,raw,key())).rejects.toBeInstanceOf(Error);
 const command=key(),saved=await api.createSalesServiceProduct(z.owner,input,command),before=await zeroSalesCommandImage(command);await expect(api.createSalesServiceProduct(z.owner,{...input,name:'Changed body'},command)).rejects.toMatchObject({status:409});
 // Original authorized master amendment archives this service; replay retains
 // its original version-one price capture without replacing that history.
 await saveMaster(z.owner,'products',saved.id,{...input,status:'archived',expectedVersion:1},key());await z.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...z.grant,state:'draining',version:2}});
 expect(await api.createSalesServiceProduct(z.owner,input,command)).toEqual(saved);expect(await zeroSalesCommandImage(command)).toEqual(before);expect(await api.salesServiceProducts(z.owner)).toMatchObject({canCreate:false,products:[{id:saved.id,status:'archived',version:2}]});await expect(api.createSalesServiceProduct(z.owner,{...input,sku:key()},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
});
it('Sales service catalogue rolls back born-receipt corruption and deferred source mutation with original Money audit and master effects',async()=>{
 for(const born of [true,false]){
  const {z,refs}=await readySalesServiceCatalogue(),api=await import('../../../src/server/modules/commerce/public/sales-service-products'),command=key(),fault=`sales_catalogue_${key().replaceAll('-','')}`,input=api.salesServiceProductSchema.parse({itemType:'service',sku:key(),name:'Never committed corrupted service',price:'25'}),image=async()=>Promise.all([db.prepare('SELECT to_jsonb(p)::text image FROM commerce_products p WHERE company_id=? ORDER BY id').bind(z.company).all().then(result=>result.results),db.prepare("SELECT to_jsonb(m)::text image FROM money_transaction_snapshots m WHERE company_id=? AND source_type='commerce_product_revision' ORDER BY source_id").bind(z.company).all().then(result=>result.results),zeroSalesCommandImage(command),db.prepare('SELECT to_jsonb(a)::text image FROM finance_accounts a WHERE id=?').bind(refs.accounts.revenueAccountId).all().then(result=>result.results)]),before=await image();
  const effect=born?`NEW.request_hash:='wrong-but-nonempty';RETURN NEW`:`UPDATE finance_accounts SET status='inactive',version=version+1 WHERE id='${refs.accounts.revenueAccountId}';RETURN NEW`;
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${effect};END $$;CREATE ${born?'TRIGGER':'CONSTRAINT TRIGGER'} ${fault} ${born?'BEFORE':'AFTER'} INSERT ON command_receipts ${born?'':'DEFERRABLE INITIALLY DEFERRED'} FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try{await expect(api.createSalesServiceProduct(z.owner,input,command)).rejects.toMatchObject({code:born?'IDEMPOTENCY_CONFLICT':'SALES_SERVICE_PRODUCT_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await image()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
 }
});

it('Sales service catalogue Department original history remains readable while product preparation stays Company or Branch only',async()=>{
 const {z}=await readySalesServiceCatalogue(),api=await import('../../../src/server/modules/commerce/public/sales-service-products'),department=await z.actor('department-catalogue-sales','sales_manager','department'),input=api.salesServiceProductSchema.parse({itemType:'service',sku:key(),name:'Native Company service visible to Department Sales',price:'12.50'}),created=await api.createSalesServiceProduct(z.owner,input,key()),before=await api.salesServiceProducts(z.owner),read=await api.salesServiceProducts(department);
 expect(read).toMatchObject({companyId:z.company,available:true,canCreate:false,currency:'THB',products:[{id:created.id,name:input.name}],unavailableReason:null});expect(read.products).toEqual(before.products);expect(read.contextKey).toMatch(/^[a-f0-9]{64}$/);const denied=key();await expect(api.createSalesServiceProduct(department,{...input,sku:key()},denied)).rejects.toMatchObject({status:403,code:'SALES_SERVICE_SCOPE_REQUIRED'});expect(await zeroSalesCommandImage(denied)).toEqual([[],[],[]]);expect((await api.salesServiceProducts(z.owner)).products).toEqual(before.products);
});

it('Sales service catalogue concurrent actual read and original creation retain coherent native history with no hidden retries',async()=>{
 const {z}=await readySalesServiceCatalogue(),api=await import('../../../src/server/modules/commerce/public/sales-service-products'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),input=api.salesServiceProductSchema.parse({itemType:'service',sku:key(),name:'Actual concurrently prepared service catalogue item',price:'12.50'}),command=key(),measured=await measureReads(()=>Promise.all([api.salesServiceProducts(z.owner),api.createSalesServiceProduct(z.owner,input,command)])),[read,created]=measured.result;
 expect(read).toMatchObject({available:true,currency:'THB',canCreate:true,unavailableReason:null});expect(created).toEqual({id:`products-${command}`,version:1});expect([[],[created.id]]).toContainEqual(read.products.map(product=>product.id));
 expect({deadlockFailures:measured.metrics.deadlockFailures??0,serializationFailures:measured.metrics.serializationFailures??0,transactionRetries:measured.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
 const before=await zeroSalesCommandImage(command);expect(await api.createSalesServiceProduct(z.owner,input,command)).toEqual(created);expect(await zeroSalesCommandImage(command)).toEqual(before);expect((await api.salesServiceProducts(z.owner)).products).toEqual([{id:created.id,sku:input.sku,name:input.name,unit:'each',priceMinor:1250,taxBps:0,status:'active',version:1}]);expect(await db.prepare("SELECT count(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='commerce_product_revision' AND source_id=? AND amount_kind='unit_price'").bind(z.company,`${created.id}:1`).first('n')).toBe(1);
});
