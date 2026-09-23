import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { expenseFixture, expenseDb as db, key } from "./helpers/expense-fixture";
import { assignFlowPrimary, resolveFlow } from "../src/server/flow-routing";
import { saveExpenseAccounting } from "../src/server/expense-workflow";
import { postExpense, getExpense } from "../src/server/expenses";
import { createInvoice, invoiceSchema, postInvoice } from "../src/server/finance";
import { settleInvoice } from "../src/server/commerce";
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});
it("does not let an Operations flow assignment activate deferred payroll or People work in production",async()=>{
  vi.stubEnv("NODE_ENV","production");
  try {
    const input={stepKey:"draft",userId:f.owner.userId,reason:"Assign pilot work",saveDefault:false,expectedVersion:1,recordVersion:1};
    await expect(assignFlowPrimary(db,f.owner,"payroll","unavailable-payroll",input,key())).rejects.toMatchObject({code:"FEATURE_COMING_SOON"});
    await expect(assignFlowPrimary(db,f.owner,"expense","unavailable-expense",input,key())).rejects.toMatchObject({code:"FEATURE_COMING_SOON"});
  } finally { vi.unstubAllEnvs(); }
});
async function assign(id:string,userId:string,saveDefault=false,stepKey="department_approval") {
  const view=await resolveFlow(db,f.owner,"expense",id);
  return assignFlowPrimary(db,f.owner,"expense",id,{stepKey,userId,saveDefault,reason:"Assign the responsible staff member",expectedVersion:view.version,recordVersion:view.recordVersion},key());
}
it("requires a primary for an ambiguous team; administrators cannot select the claimant or an unrelated user",async()=>{
  await f.grant(f.tie,"finance_preparer");
  await db.prepare("DELETE FROM flow_defaults WHERE company_id=? AND kind='expense'").bind(f.company).run();
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  await f.action(f.head,id,"approve");
  const view=await resolveFlow(db,f.reviewer,"expense",id);
  expect(view.current?.candidates.map(p=>p.userId)).toEqual(expect.arrayContaining([f.reviewer.userId,f.tie.userId]));
  expect(view.canAct).toBe(false);expect(view.blocker).toContain("Select a primary");
  await expect(f.action(f.reviewer,id,"approve")).rejects.toBeDefined();
  await expect(assign(id,f.claimant.userId,false,"accounts_review")).rejects.toMatchObject({code:"FLOW_STAFF_INELIGIBLE"});
  await assign(id,f.reviewer.userId,false,"accounts_review");
  expect((await resolveFlow(db,f.reviewer,"expense",id)).canAct).toBe(true);
  expect((await resolveFlow(db,f.tie,"expense",id)).canAct).toBe(false);
});
it("freezes defaults for existing documents and applies a new revision only to new routes",async()=>{
  await f.grant(f.tie,"finance_preparer");
  await db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'expense','accounts_review',?,?,'Fixture default',1)").bind(f.company,f.reviewer.userId,f.owner.userId).run();
  const first=await f.claim();await f.attach(first);await f.action(f.claimant,first,"submit");
  const second=await f.claim();await assign(second,f.tie.userId,true,"accounts_review");
  const third=await f.claim();
  expect((await resolveFlow(db,f.owner,"expense",first)).steps.find(s=>s.key==="accounts_review")?.primary?.userId).toBe(f.reviewer.userId);
  expect((await resolveFlow(db,f.owner,"expense",third)).steps.find(s=>s.key==="accounts_review")?.primary?.userId).toBe(f.tie.userId);
  expect((await resolveFlow(db,f.owner,"expense",second)).history.at(-1)?.reason).toContain("tie");
});
it("rejects stale reassignment and leaves completed approvals immutable",async()=>{
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  const before=await resolveFlow(db,f.owner,"expense",id);
  await assign(id,f.head.userId);
  await expect(assignFlowPrimary(db,f.owner,"expense",id,{stepKey:"department_approval",userId:f.head.userId,saveDefault:false,reason:"Stale reassignment",expectedVersion:before.version,recordVersion:before.recordVersion},key())).rejects.toMatchObject({code:"VERSION_CONFLICT"});
  await f.action(f.head,id,"approve");
  await expect(assign(id,f.head.userId)).rejects.toMatchObject({code:"FLOW_STEP_LOCKED"});
});
it("keeps approval, GL and payment separate and returns an unposted GL review for correction",async()=>{
  const id=await f.claim();await f.throughAccounts(id);await f.action(f.manager,id,"approve");
  expect((await getExpense(f.manager,id)).journal_id).toBeNull();
  expect((await getExpense(f.payer,id)).can_pay).toBe(0);
  await f.action(f.manager,id,"reject");
  expect((await getExpense(f.claimant,id)).workflow_stage).toBe("draft");
  expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE source_id=?").bind(id).first("n")).toBe(0);
});
it("posts one balanced GL under retries and blocks a revoked primary",async()=>{
  const id=await f.claim();await f.throughAccounts(id);await f.action(f.manager,id,"approve");
  const row=await getExpense(f.manager,id), input={expectedVersion:Number(row.version),periodId:f.period,entryDate:"2026-09-02"}, command=key();
  const result=await postExpense(f.manager,id,input,command);
  expect(await postExpense(f.manager,id,input,command)).toEqual(result);
  const sums=await db.prepare("SELECT SUM(l.debit_minor) debit,SUM(l.credit_minor) credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.source_id=?").bind(id).first();
  expect(sums).toEqual({debit:10700,credit:10700});
  await db.prepare("UPDATE assignments SET revoked_at=1 WHERE company_id=? AND user_id=? AND role='treasury'").bind(f.company,f.payer.userId).run();
  expect((await resolveFlow(db,f.payer,"expense",id)).canAct).toBe(false);
});
it("repairs a revoked reviewer through an audited independent reassignment",async()=>{
  f.tie.role="finance_preparer";f.tie.assignmentId=await f.grant(f.tie,"finance_preparer");
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  await f.action(f.head,id,"approve");
  await assign(id,f.reviewer.userId,false,"accounts_review");
  const recorded=(await resolveFlow(db,f.owner,"expense",id)).steps.filter(step=>step.status==="recorded");
  await db.prepare("UPDATE assignments SET revoked_at=1 WHERE company_id=? AND user_id=? AND role='finance_preparer'").bind(f.company,f.reviewer.userId).run();
  const blocked=await resolveFlow(db,f.owner,"expense",id);
  expect(blocked.blocker).toContain("no longer eligible");
  expect(blocked.canManage).toBe(true);
  await expect(f.action(f.reviewer,id,"approve")).rejects.toBeDefined();
  await expect(assign(id,f.claimant.userId,false,"accounts_review")).rejects.toMatchObject({code:"FLOW_STAFF_INELIGIBLE"});
  await assign(id,f.tie.userId,false,"accounts_review");
  const repaired=await resolveFlow(db,f.tie,"expense",id);
  expect(repaired.canAct).toBe(true);
  expect(repaired.steps.filter(step=>step.status==="recorded")).toEqual(recorded);
  expect(repaired.history.at(-1)?.reason).toContain("Assign the responsible staff member");
  expect((await resolveFlow(db,f.reviewer,"expense",id)).canAct).toBe(false);
  const expense=await getExpense(f.tie,id);
  await saveExpenseAccounting(f.tie,id,{expectedVersion:Number(expense.version),categoryAccountId:f.accounts.expense,tax:"7",periodId:f.period,entryDate:"2026-09-02",note:"Replacement reviewer checked source evidence"},key());
  await f.action(f.tie,id,"approve");
  expect((await resolveFlow(db,f.owner,"expense",id)).current?.key).not.toBe("accounts_review");
});
it("routes standalone invoice posting and partial settlement through different named staff",async()=>{
  for (const [actor,role] of [[f.reviewer,"finance_preparer"],[f.manager,"finance_approver"],[f.payer,"treasury"]] as const) {
    actor.role=role;actor.assignmentId=(await db.prepare("SELECT id FROM assignments WHERE company_id=? AND user_id=? AND role=? AND revoked_at IS NULL").bind(f.company,actor.userId,role).first<{id:string}>())!.id;
  }
  const party=`party-${key()}`;
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'supplier',?,'Supplier','THB',1,1)").bind(party,f.company,party).run();
  const invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party,kind:"payable",invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-03",currency:"THB",subtotal:"100",tax:"7",taxAccountId:f.accounts.tax,debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key());
  const view=await resolveFlow(db,f.owner,"invoice",invoice.id);
  await assignFlowPrimary(db,f.owner,"invoice",invoice.id,{stepKey:"draft",userId:f.manager.userId,reason:"Independent GL accountant",saveDefault:false,expectedVersion:view.version,recordVersion:view.recordVersion},key());
  await expect(postInvoice(f.reviewer,invoice.id,{expectedVersion:1,periodId:f.period,entryDate:"2026-09-02"},key())).rejects.toBeDefined();
  await postInvoice(f.manager,invoice.id,{expectedVersion:1,periodId:f.period,entryDate:"2026-09-02"},key());
  expect((await resolveFlow(db,f.payer,"invoice",invoice.id)).canAct).toBe(true);
  await expect(settleInvoice(f.payer,invoice.id,{expectedVersion:2,date:"2026-09-03",amount:"40",reference:"Missing slip"},key())).rejects.toMatchObject({code:"PAYMENT_SLIP_REQUIRED"});
  const slip=await f.attach(invoice.id,"invoice_payment");
  await settleInvoice(f.payer,invoice.id,{paymentSlipId:slip,expectedVersion:2,date:"2026-09-03",amount:"40",reference:"Actual bank reference"},key());
  const partial=await resolveFlow(db,f.payer,"invoice",invoice.id);expect(partial.current?.key).toBe("posted");expect(partial.canAct).toBe(true);
  await expect(settleInvoice(f.payer,invoice.id,{paymentSlipId:slip,expectedVersion:3,date:"2026-09-03",amount:"67",reference:"Reused slip"},key())).rejects.toMatchObject({status:409});
  const finalSlip=await f.attach(invoice.id,"invoice_payment");
  await settleInvoice(f.payer,invoice.id,{paymentSlipId:finalSlip,expectedVersion:3,date:"2026-09-03",amount:"67",reference:"Remaining bank reference"},key());
  expect((await resolveFlow(db,f.payer,"invoice",invoice.id)).current).toBeNull();
});

async function stockFixture() {
  f.manager.role="finance_approver"; f.manager.branchId=null; f.manager.departmentId=null;
  f.manager.assignmentId=await f.grant(f.manager,"finance_approver");
  await db.prepare("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,status,created_at,updated_at) VALUES(?,?,?,'5400','Stock variance','expense','THB','active',1,1)").bind(`variance-${key()}`,f.company,f.chart).run();
  const {saveMaster}=await import("../src/server/commerce");
  const warehouse=await saveMaster(f.owner,"warehouses",null,{branchId:f.branch,code:key(),name:"Test warehouse"},key());
  const product=await saveMaster(f.owner,"products",null,{sku:key(),name:"Test product",price:"10"},key());
  return {warehouseId:String(warehouse.id),productId:String(product.id)};
}
async function choose(kind:"stock_count"|"order"|"invoice",id:string,stepKey:string,userId:string) {
  const view=await resolveFlow(db,f.owner,kind,id);
  await assignFlowPrimary(db,f.owner,kind,id,{stepKey,userId,reason:"Assign eligible test responsibility",saveDefault:false,expectedVersion:view.version,recordVersion:view.recordVersion},key());
}
it("keeps an active company account eligible to submit its own draft before an HR engagement exists",async()=>{
  const externalOwner=await f.member("external-owner","holding_owner",10,f.accountsDepartment,false);
  const {saveMaster,saveOrder,orderSchema}=await import("../src/server/commerce");
  const warehouse=await saveMaster(externalOwner,"warehouses",null,{branchId:f.branch,code:key(),name:"External owner warehouse"},key());
  const product=await saveMaster(externalOwner,"products",null,{sku:key(),name:"External owner product",price:"10"},key());
  const customer=await saveMaster(externalOwner,"parties",null,{kind:"customer",code:key(),name:"External owner customer"},key());
  const order=await saveOrder(externalOwner,null,orderSchema.parse({kind:"sales",partyId:customer.id,warehouseId:warehouse.id,orderDate:"2026-09-02",dueDate:"2026-09-03",lines:[{productId:product.id,quantity:"1",price:"10",taxBps:0}]}),key());
  const flow=await resolveFlow(db,externalOwner,"order",String(order.id));
  expect(flow.current?.primary).toMatchObject({userId:externalOwner.userId,name:externalOwner.name});
  expect(flow.canAct).toBe(true);
  expect(flow.blocker).toBeNull();
});
it("requires stock approval before balanced GL and posts the counted stock only once",async()=>{
  const {saveCount,transitionCount}=await import("../src/server/inventory-counts");
  const stock=await stockFixture();
  const created=await saveCount(f.owner,null,{warehouseId:stock.warehouseId,countDate:"2026-09-02",reason:"Physical count verification",lines:[{productId:stock.productId,countedQuantity:"10",unitCost:"10"}]},key());
  const id=String(created.id);
  await transitionCount(f.owner,id,"submit",{expectedVersion:1},key());
  await choose("stock_count",id,"submitted",f.manager.userId);
  await expect(transitionCount(f.manager,id,"post",{expectedVersion:2,reason:"Cannot skip approval"},key())).rejects.toBeDefined();
  await transitionCount(f.manager,id,"approve",{expectedVersion:2,reason:"Physical evidence checked"},key());
  expect(await db.prepare("SELECT COUNT(*) n FROM commerce_stock WHERE product_id=?").bind(stock.productId).first("n")).toBe(0);
  const command=key(),input={expectedVersion:3,reason:"Debit credit valuation checked"};
  const posted=await transitionCount(f.manager,id,"post",input,command);
  expect(await transitionCount(f.manager,id,"post",input,command)).toEqual(posted);
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id=?").bind(stock.productId).first()).toEqual({quantity_milli:10000,value_minor:10000});
  expect((await resolveFlow(db,f.manager,"stock_count",id)).current).toBeNull();
});
it("hands purchase receipt to accounting before drafting and posting its payable",async()=>{
  const {saveMaster,saveOrder,orderSchema,transitionOrder,getOrder,fulfillOrder,invoiceFulfillment}=await import("../src/server/commerce");
  const {postJournal}=await import("../src/server/finance");
  const stock=await stockFixture();
  f.head.role="purchasing_manager";f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,"purchasing_manager");
  const party=await saveMaster(f.owner,"parties",null,{kind:"supplier",code:key(),name:"Test supplier"},key());
  const order=await saveOrder(f.owner,null,orderSchema.parse({kind:"purchase",partyId:party.id,warehouseId:stock.warehouseId,orderDate:"2026-09-02",dueDate:"2026-09-03",lines:[{productId:stock.productId,quantity:"2",price:"10",taxBps:0}]}),key());
  const id=String(order.id);
  await transitionOrder(f.owner,id,{action:"submit",expectedVersion:1},key());
  await transitionOrder(f.head,id,{action:"approve",expectedVersion:2},key());
  await choose("order",id,"stock_gl",f.manager.userId);
  const details=await getOrder(f.owner,id);
  const receipt=await fulfillOrder(f.owner,id,{expectedVersion:3,date:"2026-09-02",lines:[{lineId:String(details.lines[0].id),quantity:"2"}]},key());
  const journal=await db.prepare("SELECT j.id,j.status FROM finance_journals j JOIN commerce_fulfillments f ON f.journal_id=j.id WHERE f.id=?").bind(receipt.fulfillmentId).first<{id:string;status:string}>();
  expect(journal?.status).toBe("draft");
  expect((await resolveFlow(db,f.manager,"order",id)).current?.key).toBe("stock_gl");
  await expect(invoiceFulfillment(f.owner,String(receipt.fulfillmentId),{expectedVersion:1,date:"2026-09-02",dueDate:"2026-09-03"},key())).rejects.toBeDefined();
  const journalFlow=await resolveFlow(db,f.owner,"journal",journal!.id);
  await assignFlowPrimary(db,f.owner,"journal",journal!.id,{stepKey:"draft",userId:f.manager.userId,reason:"Keep source order and journal assignment consistent",saveDefault:false,expectedVersion:journalFlow.version,recordVersion:journalFlow.recordVersion},key());
  await postJournal(f.manager,journal!.id,key(),{expectedVersion:1});
  await choose("order",id,"invoicing",f.owner.userId);
  const invoice=await invoiceFulfillment(f.owner,String(receipt.fulfillmentId),{expectedVersion:1,date:"2026-09-02",dueDate:"2026-09-03"},key());
  expect(await db.prepare("SELECT status FROM finance_invoices WHERE id=?").bind(invoice.invoiceId).first("status")).toBe("draft");
  await choose("invoice",String(invoice.invoiceId),"draft",f.manager.userId);
  await postInvoice(f.manager,String(invoice.invoiceId),{expectedVersion:1,periodId:f.period,entryDate:"2026-09-02"},key());
  expect((await resolveFlow(db,f.payer,"invoice",String(invoice.invoiceId))).current?.key).toBe("posted");
});

it("cancels an approved order independently but requires a return after stock moved",async()=>{
  const {saveMaster,saveOrder,orderSchema,transitionOrder,getOrder,fulfillOrder}=await import("../src/server/commerce");
  const stock=await stockFixture();
  f.head.role="purchasing_manager";f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,"purchasing_manager");
  const party=await saveMaster(f.owner,"parties",null,{kind:"supplier",code:key(),name:"Cancellation supplier"},key());
  async function approvedOrder(){
    const created=await saveOrder(f.owner,null,orderSchema.parse({kind:"purchase",partyId:party.id,warehouseId:stock.warehouseId,orderDate:"2026-09-02",dueDate:"2026-09-03",lines:[{productId:stock.productId,quantity:"2",price:"10",taxBps:0}]}),key());
    const id=String(created.id);
    await transitionOrder(f.owner,id,{action:"submit",expectedVersion:1},key());
    await transitionOrder(f.head,id,{action:"approve",expectedVersion:2},key());
    return id;
  }
  const cancellable=await approvedOrder();
  await expect(transitionOrder(f.owner,cancellable,{action:"cancel",expectedVersion:3},key())).rejects.toMatchObject({code:"INDEPENDENT_CANCELLER_REQUIRED"});
  const cancelled=await transitionOrder(f.head,cancellable,{action:"cancel",expectedVersion:3},key());
  expect(cancelled).toMatchObject({state:"cancelled",version:4});
  const received=await approvedOrder(),details=await getOrder(f.owner,received);
  await fulfillOrder(f.owner,received,{expectedVersion:3,date:"2026-09-02",lines:[{lineId:String(details.lines[0].id),quantity:"1"}]},key());
  await expect(transitionOrder(f.head,received,{action:"cancel",expectedVersion:4},key())).rejects.toMatchObject({code:"ORDER_REVERSAL_REQUIRED"});
});

it("enforces a versioned customer credit limit when sales approval creates exposure",async()=>{
  const {saveMaster,saveOrder,orderSchema,transitionOrder}=await import("../src/server/commerce");
  const stock=await stockFixture();
  f.head.role="sales_manager";f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,"sales_manager");
  const party=await saveMaster(f.owner,"parties",null,{kind:"customer",code:key(),name:"Credit customer",creditLimit:"15"},key());
  const order=await saveOrder(f.owner,null,orderSchema.parse({kind:"sales",partyId:party.id,warehouseId:stock.warehouseId,orderDate:"2026-09-02",dueDate:"2026-09-03",lines:[{productId:stock.productId,quantity:"2",price:"10",taxBps:0}]}),key());
  const id=String(order.id);
  await transitionOrder(f.owner,id,{action:"submit",expectedVersion:1},key());
  await expect(transitionOrder(f.head,id,{action:"approve",expectedVersion:2},key())).rejects.toBeDefined();
  await saveMaster(f.owner,"parties",String(party.id),{kind:"customer",code:String((await db.prepare("SELECT code FROM finance_parties WHERE id=?").bind(party.id).first("code"))),name:"Credit customer",paymentTermsDays:30,creditLimit:"30",status:"active",expectedVersion:1},key());
  await expect(transitionOrder(f.head,id,{action:"approve",expectedVersion:2},key())).resolves.toMatchObject({state:"confirmed"});
});

it("keeps batched Work equal to detail-authorized flows across roles and scopes", async () => {
  const { otherFlowWork } = await import("../src/server/flow-work");
  const { withPolicyReadSnapshot } = await import("../src/server/iam-read-snapshot");
  const { workOrderRows, getOrder } = await import("../src/server/commerce");
  const { workInvoiceRows, getInvoice } = await import("../src/server/finance");
  const { workJournalRows, journalDetail } = await import("../src/server/ledger");
  const { workPayrollRows, getPayrollRun } = await import("../src/server/people-payroll");
  const party = `party-${key()}`;
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'supplier',?,'Work parity supplier','THB',1,1)").bind(party,f.company,party).run();
  await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party,kind:"payable",invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-03",currency:"THB",subtotal:"100",tax:"7",taxAccountId:f.accounts.tax,debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key());
  const {saveMaster,saveOrder,orderSchema} = await import("../src/server/commerce");
  const stock = await stockFixture();
  const supplier = await saveMaster(f.owner,"parties",null,{kind:"supplier",code:key(),name:"Batch parity supplier"},key());
  await saveOrder(f.owner,null,orderSchema.parse({kind:"purchase",partyId:supplier.id,warehouseId:stock.warehouseId,orderDate:"2026-09-02",dueDate:"2026-09-03",lines:[{productId:stock.productId,quantity:"2",price:"10",taxBps:0}]}),key());
  for (const actor of [f.owner, f.claimant, f.reviewer, f.payer, { ...f.owner, branchId: f.branch }]) {
    const reference = await otherFlowWork(actor, true);
    expect(await withPolicyReadSnapshot(() => otherFlowWork(actor))).toEqual(reference);
    for (const [list, detail] of [[workOrderRows,getOrder],[workInvoiceRows,getInvoice],[workJournalRows,journalDetail],[workPayrollRows,getPayrollRun]] as const) {
      let rows;
      try { rows = await list(actor); } catch (error) {
        expect(error).toMatchObject({status:403}); continue;
      }
      for (const row of rows) await expect(detail(actor,String(row.id))).resolves.toBeDefined();
    }
  }
});
