import { fulfillmentSchema } from "../../../src/server/modules/commerce/public/commerce";
import { settlementSchema } from "../../../src/server/modules/commerce/public/commerce";
import { refundConfirmationSchema } from "../../../src/server/modules/finance/public/refunds";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import { deliveryEvidence } from "../../support/helpers/delivery-evidence";
import { completeCommerceReturn, decideCommerceReturn, fulfillOrder, getOrder, invoiceFulfillment, saveMaster, settleInvoice } from "../../../src/server/modules/commerce/public/commerce";
import { importBankStatement, postInvoice, postJournal } from "../../../src/server/modules/finance/public/finance";
import { approveRefund, confirmRefundInstruction, exportRefundInstructions, prepareRefundInstruction, refundWorkspace } from "../../../src/server/modules/finance/public/refunds";
import { confirmManualRate } from "../../../src/server/modules/finance/public/company-money";
import { assignFlowPrimary, resolveFlow } from "../../../src/server/modules/workflow/public/flow-routing";
import { countSchema, saveCount, transitionCount } from "../../../src/server/modules/inventory/public/inventory-counts";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import {
  addCartLine,
  changeEcommerce,
  checkoutCart,
  createCart,
  createChannel,
  createListing,
  ecommerceDetail,
  ecommerceWorkspace,
} from "../../../src/server/modules/ecommerce/public/ecommerce";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let fixture: Awaited<ReturnType<typeof expenseFixture>>,
  owner: WorkContext,
  reviewer: WorkContext,
  warehouse: string,
  product: string,
  party: string;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});

async function assignOwnerStep(kind:"order"|"invoice"|"journal",id:string,stepKey:string,userId:string){
 const flow=await resolveFlow(db,owner,kind,id);
 await assignFlowPrimary(db,owner,kind,id,{stepKey,userId,reason:'Independent owner handoff for checkout evidence',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
}

it("collects a Sales-backed checkout and completes its reviewed return and bank refund with original stock and FX evidence",async()=>{
 fixture.manager.branchId=null;fixture.manager.departmentId=null;fixture.payer.branchId=null;fixture.payer.departmentId=null;
 await db.prepare("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'5400','Declared stock count variance','expense','THB',1,1)").bind(`${fixture.company}-count-variance`,fixture.company,fixture.chart).run();
 // Use a native counted lot source so the Ecommerce adapter must retain the
 // exact fulfilled identity as well as the historical financial snapshots.
 product=String((await saveMaster(fixture.owner,'products',null,{sku:key(),name:'Lot-tracked online item',price:'20',trackingMode:'lot'},key())).id);
 const counted=await saveCount(fixture.owner,null,countSchema.parse({warehouseId:warehouse,countDate:'2026-09-21',reason:'Count the declared initial labelled online stock through its native owner.',lines:[{productId:product,countedQuantity:'2',unitCost:'6',allocations:[{lotCode:'ONLINE-ORIGINAL',countedQuantity:'2',unitCost:'6'}]}]}),key());
 await transitionCount(fixture.owner,String(counted.id),'submit',{expectedVersion:1,attachmentSetIds:[]},key());
 const countFlow=await resolveFlow(db,fixture.owner,'stock_count',String(counted.id));await assignFlowPrimary(db,fixture.owner,'stock_count',String(counted.id),{stepKey:'submitted',userId:fixture.manager.userId,expectedVersion:countFlow.version,recordVersion:2,reason:'Independent Finance reviews the labelled opening stock.',saveDefault:false},key());
 await transitionCount(fixture.manager,String(counted.id),'approve',{expectedVersion:2,attachmentSetIds:[],reason:'The physical lot count and original 12.00 THB value are independently verified.'},key());
 const approvedCountFlow=await resolveFlow(db,fixture.owner,'stock_count',String(counted.id));await assignFlowPrimary(db,fixture.owner,'stock_count',String(counted.id),{stepKey:'approved',userId:fixture.manager.userId,expectedVersion:approvedCountFlow.version,recordVersion:3,reason:'The reviewer posts the independently approved inventory result.',saveDefault:false},key());
 await transitionCount(fixture.manager,String(counted.id),'post',{expectedVersion:3,attachmentSetIds:[],reason:'Post the independently approved original lot valuation.'},key());
 const lotId=String(await db.prepare("SELECT id FROM inventory_lots WHERE product_id=? AND lot_code='ONLINE-ORIGINAL'").bind(product).first('id'));
 const checkout=await submittedCheckout('2'),checkoutId=String(checkout.id),orderId=String(checkout.salesOrderId);
 const approveKey=key(),approveInput={action:'approve-order',expectedVersion:1,reason:'Confirm exact customer order and source stock',expiresAt:Date.now()+3600000};
 await changeEcommerce(reviewer,'checkout',checkoutId,approveInput,approveKey);
 await expect(changeEcommerce(reviewer,'checkout',checkoutId,approveInput,approveKey)).resolves.toMatchObject({status:'confirmed',salesOrderVersion:4});
 await assignStock(orderId);
 const order=await getOrder(owner,orderId),delivered=await fulfillOrder(owner,orderId,fulfillmentSchema.parse({expectedVersion:4,date:'2026-09-22',lines:[{lineId:String(order.lines[0].id),quantity:'2',allocations:[{lotId,quantity:'2'}]}],...(await deliveryEvidence(db,owner,orderId))}),key());
 const fulfillmentId=String(delivered.fulfillmentId),goodsJournal=String(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(fulfillmentId).first('journal_id'));
 await assignOwnerStep('journal',goodsJournal,'draft',fixture.manager.userId);await postJournal(fixture.manager,goodsJournal,key(),{expectedVersion:1});
 await assignOwnerStep('order',orderId,'invoicing',owner.userId);
 const invoice=await invoiceFulfillment(owner,fulfillmentId,{expectedVersion:1,date:'2026-09-22',dueDate:'2026-09-28'},key()),invoiceId=String(invoice.invoiceId);
 await assignOwnerStep('invoice',invoiceId,'draft',fixture.manager.userId);await postInvoice(fixture.manager,invoiceId,{expectedVersion:1,periodId:fixture.period,entryDate:'2026-09-22'},key());
 await assignOwnerStep('invoice',invoiceId,'posted',fixture.payer.userId);
 const payKey=key(),payInput={expectedVersion:2,date:'2026-09-22',amount:'50',reference:'Fictional customer transfer after verified receipt'};
 await settleInvoice(fixture.payer,invoiceId,settlementSchema.parse(payInput),payKey);await expect(settleInvoice(fixture.payer,invoiceId,settlementSchema.parse(payInput),payKey)).resolves.toMatchObject({id:orderId,outstandingMinor:0});
 expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'paid',outstanding_minor:0,base_outstanding_minor:0});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:0,value_minor:0});
 const original=await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_type='finance_invoice' AND source_id=? AND amount_kind='total'").bind(invoiceId).first<{amount_units:string;usd_units:string;fx_rate_id:string}>();
 await confirmManualRate(fixture.manager,{currency:'THB',effectiveDate:'2026-09-23',currencyPerUsd:'50',note:'Later synthetic quote must not rewrite original invoice/stock evidence'},key());
 const detail=await getOrder(owner,orderId),returnKey=key(),returnInput={action:'request-return',expectedVersion:2,reason:'Customer returned one unopened item',fulfillmentId,fulfillmentVersion:2,date:'2026-09-23',lines:[{lineId:String(detail.fulfillmentLines[0].id),quantity:'1',allocations:[{lotId,quantity:'1'}],condition:'unopened',conditionNote:'Sealed original packaging is independently checked',qualityDueDate:'2026-09-24'}]};
 await expect(changeEcommerce(owner,'checkout',checkoutId,{...returnInput,lines:[{...returnInput.lines[0],allocations:[{lotId:'not-the-fulfilled-lot',quantity:'1'}]}]},key())).rejects.toMatchObject({code:'RETURN_LOT_INVALID'});
 expect(await db.prepare('SELECT COUNT(*) n FROM ecommerce_return_links WHERE checkout_id=?').bind(checkoutId).first('n')).toBe(0);
 expect(await db.prepare('SELECT version FROM commerce_fulfillments WHERE id=?').bind(fulfillmentId).first('version')).toBe(2);
 const requested=await changeEcommerce(owner,'checkout',checkoutId,returnInput,returnKey),returnId=String(requested.commerceReturnId);
 await expect(changeEcommerce(owner,'checkout',checkoutId,returnInput,returnKey)).resolves.toEqual(requested);
 await expect(decideCommerceReturn(owner,returnId,'approve',{expectedVersion:1,reason:'Requester self review'},key())).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});
 const approved=await decideCommerceReturn(reviewer,returnId,'approve',{expectedVersion:1,reason:'Independent original delivery, condition and financial credit reviewed'},key());
 for(const journal of [approved.stockJournalId,approved.creditJournalId]){await assignOwnerStep('journal',String(journal),'draft',fixture.manager.userId);await postJournal(fixture.manager,String(journal),key(),{expectedVersion:1});}
 const completeKey=key(),completed=await completeCommerceReturn(fixture.manager,returnId,{expectedVersion:2},completeKey);
 await expect(completeCommerceReturn(fixture.manager,returnId,{expectedVersion:2},completeKey)).resolves.toEqual(completed);
 expect(completed).toMatchObject({status:'posted',refundMinor:2500,creditedMinor:0});
 const reversal=await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id,original_source_type,original_source_id FROM money_transaction_snapshots WHERE source_type='commerce_return' AND source_id=? AND amount_kind='credit_total'").bind(returnId).first<{amount_units:string;usd_units:string;fx_rate_id:string;original_source_type:string;original_source_id:string}>();
 expect(reversal).toMatchObject({amount_units:'-2500',fx_rate_id:original!.fx_rate_id,original_source_type:'finance_invoice',original_source_id:invoiceId});
 expect(Number(reversal!.usd_units)).toBe(-Math.round(Number(original!.usd_units)/2));
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:1000,value_minor:600});
 expect(await db.prepare('SELECT a.lot_id,a.quantity_milli,a.value_minor,a.fulfillment_allocation_id FROM commerce_return_lot_allocations a JOIN commerce_return_lines l ON l.id=a.return_line_id WHERE l.return_id=?').bind(returnId).first()).toEqual({lot_id:lotId,quantity_milli:1000,value_minor:600,fulfillment_allocation_id:detail.lotAllocations[0].id});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM inventory_lot_balances WHERE warehouse_id=? AND lot_id=?').bind(warehouse,lotId).first()).toEqual({quantity_milli:1000,value_minor:600});
 expect(await db.prepare("SELECT h.lot_id,h.held_quantity_milli FROM inventory_quality_holds h JOIN commerce_return_lot_allocations a ON a.id=h.source_id JOIN commerce_return_lines l ON l.id=a.return_line_id WHERE h.source_type='customer_return' AND l.return_id=?").bind(returnId).first()).toEqual({lot_id:lotId,held_quantity_milli:1000});
 expect(await db.prepare('SELECT inventory_quarantined_quantity(?,?) q').bind(warehouse,product).first('q')).toBe(1000);
 const refundId=String(completed.refundId);
 await approveRefund(reviewer,refundId,{expectedVersion:1,reason:'Review return beneficiary and exact gross amount',beneficiaryName:'Fictional online customer',beneficiaryAccount:'SIMULATED-Ecommerce-REFUND'},key());
 const instruction=await prepareRefundInstruction(fixture.payer,refundId,{expectedVersion:2,amount:'25',plannedDate:'2026-09-23',bankAccount:fixture.accounts.cash,cashAccountId:fixture.accounts.cash},key());
 await exportRefundInstructions(fixture.payer,{instructions:[{id:String(instruction.id),expectedVersion:1}]},key());
 const statement=await importBankStatement(fixture.payer,{bankAccount:fixture.accounts.cash,statementNumber:key(),statementDate:'2026-09-23',opening:'100',closing:'75',sourceHash:key(),rows:[{transactionDate:'2026-09-23',externalId:'SIMULATED-Ecommerce-REFUND',description:'Fictional exact customer refund',amount:'-25',currency:'THB'}]},key());
 const bank=(await db.prepare('SELECT id,version FROM finance_bank_transactions WHERE statement_id=?').bind(statement.id).first<{id:string;version:number}>())!;
 const confirmationKey=key(),confirmation={expectedVersion:2,bankTransactionId:bank.id,bankVersion:bank.version,periodId:fixture.period,bankReference:'SIMULATED-Ecommerce-REFUND',evidence:'Exact independent bank statement confirms the approved refund'};
 await confirmRefundInstruction(fixture.payer,String(instruction.id),refundConfirmationSchema.parse(confirmation),confirmationKey);
 await expect(confirmRefundInstruction(fixture.payer,String(instruction.id),refundConfirmationSchema.parse(confirmation),confirmationKey)).resolves.toMatchObject({status:'confirmed'});
 expect((await refundWorkspace(fixture.payer,refundId)).owner).toMatchObject({status:'settled',outstanding_minor:0});
 const after=await ecommerceDetail(owner,checkoutId);
 expect(after.returnLinks).toHaveLength(1);expect(after.order.returns).toMatchObject([{id:returnId,status:'posted',refund_minor:2500}]);
 const sums=await db.prepare("SELECT SUM(l.debit_minor) debit,SUM(l.credit_minor) credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted'").bind(fixture.company).first();expect(sums!.debit).toBe(sums!.credit);
 await expect(db.prepare('DELETE FROM ecommerce_return_links WHERE checkout_id=?').bind(checkoutId).run()).rejects.toThrow('ECOMMERCE_HISTORY_IMMUTABLE');
});

it("keeps stale, cross-scope and concurrent checkout commands from leaving Sales or return side effects",async()=>{
 const checkout=await submittedCheckout(),id=String(checkout.id),orderId=String(checkout.salesOrderId);
 const hidden={...owner,branchId:'other-branch',departmentId:null};
 await expect(ecommerceDetail(hidden,id)).rejects.toMatchObject({status:404});
 expect((await ecommerceWorkspace(hidden)).checkouts).toHaveLength(0);
 await expect(changeEcommerce(reviewer,'checkout',id,{action:'approve-order',expectedVersion:99,reason:'Wrong parent source version'},key())).rejects.toMatchObject({code:'ECOMMERCE_SOURCE_CHANGED'});
 expect((await getOrder(owner,orderId)).order).toMatchObject({state:'submitted',version:2});
 const results=await Promise.allSettled([key(),key()].map(k=>changeEcommerce(reviewer,'checkout',id,{action:'approve-order',expectedVersion:1,reason:'Competing independent approval',expiresAt:Date.now()+3600000},k)));
 expect(results.filter(x=>x.status==='fulfilled')).toHaveLength(1);
 expect(await db.prepare('SELECT COUNT(*) n FROM inventory_order_reservations WHERE order_line_id=?').bind(`${orderId}-line-0`).first('n')).toBe(1);
 expect((await getOrder(owner,orderId)).order).toMatchObject({state:'confirmed',version:4});
 await expect(createCart(owner,{channelId:'unknown',partyId:party,warehouseId:warehouse,currency:'USD'},key())).rejects.toMatchObject({code:'ECOMMERCE_CURRENCY_MAPPING_REQUIRED'});
});
beforeEach(async () => {
  fixture = await expenseFixture();
  for(const [suffix,category] of [['receivable','asset'],['inventory','asset'],['clearing','liability']])await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)").bind(`${fixture.company}-${suffix}`,fixture.chart,fixture.company,suffix,suffix,category).run();
  await db.prepare('UPDATE commerce_settings SET receivable_account_id=?,inventory_account_id=?,clearing_account_id=? WHERE company_id=?').bind(`${fixture.company}-receivable`,`${fixture.company}-inventory`,`${fixture.company}-clearing`,fixture.company).run();
  owner = { ...fixture.owner, modules: ["ecommerce"] };
  reviewer = {
    ...fixture.tie,
    assignmentId: await fixture.grant(fixture.tie, "holding_owner"),
    role: "holding_owner",
    modules: ["ecommerce"],
  };
  warehouse = String(
    (
      await saveMaster(
        fixture.owner,
        "warehouses",
        null,
        { branchId: fixture.branch, code: key(), name: "Online warehouse" },
        key(),
      )
    ).id,
  );
  product = String(
    (
      await saveMaster(
        fixture.owner,
        "products",
        null,
        { sku: key(), name: "Online item", price: "20" },
        key(),
      )
    ).id,
  );
  party = String(
    (
      await saveMaster(
        fixture.owner,
        "parties",
        null,
        {
          kind: "customer",
          code: key(),
          name: "Online customer",
          taxId: key(),
          currency: "THB",
          paymentTermsDays: 7,
          creditLimit: "10000",
        },
        key(),
      )
    ).id,
  );
  await db
    .prepare(
      "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)",
    )
    .bind(warehouse, product)
    .run();
});

async function publishedListing() {
  const channel = await createChannel(
    owner,
    {
      name: "Web shop",
      channelType: "web",
      configuration: { storefront: "sandbox" },
    },
    key(),
  );
  await expect(
    changeEcommerce(
      owner,
      "channel",
      String(channel.id),
      {
        action: "activate",
        expectedVersion: 1,
        reason: "Self review is forbidden",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await changeEcommerce(
    reviewer,
    "channel",
    String(channel.id),
    {
      action: "activate",
      expectedVersion: 1,
      reason: "Channel configuration reviewed",
    },
    key(),
  );
  const listing = await createListing(
    owner,
    {
      channelId: channel.id,
      productId: product,
      title: "Published item",
      currency: "THB",
      price: "25.00",
    },
    key(),
  );
  await changeEcommerce(
    reviewer,
    "listing",
    String(listing.id),
    {
      action: "publish",
      expectedVersion: 1,
      reason: "Price and product reviewed",
    },
    key(),
  );
  return { channel, listing };
}

async function submittedCheckout(quantity = "1") {
  const { channel, listing } = await publishedListing();
  const cart = await createCart(
    owner,
    {
      channelId: channel.id,
      partyId: party,
      warehouseId: warehouse,
      currency: "THB",
    },
    key(),
  );
  const line = await addCartLine(
    owner,
    {
      cartId: cart.id,
      expectedVersion: 1,
      listingId: listing.id,
      listingVersion: 2,
      quantity,
    },
    key(),
  );
  const checkoutKey = key();
  const checkout = await checkoutCart(
    owner,
    {
      cartId: cart.id,
      expectedVersion: line.version,
      orderDate: "2026-09-21",
      dueDate: "2026-09-28",
    },
    checkoutKey,
  );
  expect(
    await checkoutCart(
      owner,
      {
        cartId: cart.id,
        expectedVersion: line.version,
        orderDate: "2026-09-21",
        dueDate: "2026-09-28",
      },
      checkoutKey,
    ),
  ).toEqual(checkout);
  return checkout;
}

async function assignStock(orderId: string) {
  const flow = await resolveFlow(db, owner, "order", orderId);
  await assignFlowPrimary(
    db,
    owner,
    "order",
    orderId,
    {
      stepKey: flow.current!.key,
      userId: owner.userId,
      reason: "Assign E-commerce fulfillment owner",
      saveDefault: false,
      expectedVersion: flow.version,
      recordVersion: flow.recordVersion,
    },
    key(),
  );
}

it("reviews channels and listings, locks checkout into Sales, and reserves shared stock", async () => {
  const checkout = await submittedCheckout("2");
  const result = await changeEcommerce(
    reviewer,
    "checkout",
    String(checkout.id),
    {
      action: "approve-order",
      expectedVersion: 1,
      reason: "Order and stock reviewed",
      expiresAt: Date.now() + 3600000,
    },
    key(),
  );
  expect(result).toMatchObject({ status: "confirmed", salesOrderVersion: 4 });
  expect(
    await db
      .prepare(
        "SELECT r.quantity_milli FROM inventory_order_reservations r JOIN commerce_order_lines l ON l.id=r.order_line_id WHERE l.order_id=?",
      )
      .bind(checkout.salesOrderId)
      .first("quantity_milli"),
  ).toBe(2000);
  const detail = await ecommerceDetail(owner, String(checkout.id));
  expect(detail.order.order).toMatchObject({ state: "confirmed", version: 4 });
});

it("fulfills only reserved stock and syncs immutable delivery evidence", async () => {
  const checkout = await submittedCheckout();
  await changeEcommerce(
    reviewer,
    "checkout",
    String(checkout.id),
    {
      action: "approve-order",
      expectedVersion: 1,
      reason: "Ready to fulfill",
      expiresAt: Date.now() + 3600000,
    },
    key(),
  );
  await assignStock(String(checkout.salesOrderId));
  const order = await getOrder(owner, String(checkout.salesOrderId));
  await fulfillOrder(
    owner,
    String(checkout.salesOrderId),
    {
      expectedVersion: Number(order.order.version),
      date: "2026-09-22",
      lines: [{ lineId: String(order.lines[0].id), quantity: "1", allocations: [] }],
      ...(await deliveryEvidence(db, owner, String(checkout.salesOrderId))),
    },
    key(),
  );
  const synced = await changeEcommerce(
    owner,
    "checkout",
    String(checkout.id),
    {
      action: "sync-fulfillment",
      expectedVersion: 2,
      reason: "Delivery created by Sales",
    },
    key(),
  );
  expect(synced.status).toBe("fulfilled");
  const detail = await ecommerceDetail(owner, String(checkout.id));
  expect(detail.fulfillmentLinks).toHaveLength(1);
  expect(
    await db
      .prepare(
        "SELECT quantity_milli FROM commerce_stock WHERE warehouse_id=? AND product_id=?",
      )
      .bind(warehouse, product)
      .first("quantity_milli"),
  ).toBe(1000);
});

it("rejects stale carts and refuses a return before the Sales fulfillment is invoiced", async () => {
  const checkout = await submittedCheckout();
  await expect(
    createListing(
      owner,
      {
        channelId: "missing",
        productId: product,
        title: "Invalid listing",
        currency: "THB",
        price: "1",
      },
      key(),
    ),
  ).rejects.toBeTruthy();
  await changeEcommerce(
    reviewer,
    "checkout",
    String(checkout.id),
    {
      action: "approve-order",
      expectedVersion: 1,
      reason: "Ready to fulfill",
      expiresAt: Date.now() + 3600000,
    },
    key(),
  );
  await assignStock(String(checkout.salesOrderId));
  const order = await getOrder(owner, String(checkout.salesOrderId));
  await fulfillOrder(
    owner,
    String(checkout.salesOrderId),
    {
      expectedVersion: Number(order.order.version),
      date: "2026-09-22",
      lines: [{ lineId: String(order.lines[0].id), quantity: "1", allocations: [] }],
      ...(await deliveryEvidence(db, owner, String(checkout.salesOrderId))),
    },
    key(),
  );
  const delivered = await getOrder(owner, String(checkout.salesOrderId));
  await expect(
    changeEcommerce(
      owner,
      "checkout",
      String(checkout.id),
      {
        action: "request-return",
        expectedVersion: 2,
        reason: "Customer requested return",
        fulfillmentId: delivered.fulfillments[0].id,
        fulfillmentVersion: Number(delivered.fulfillments[0].version),
        date: "2026-09-23",
        lines: [{ lineId: delivered.fulfillmentLines[0].id, quantity: "1" }],
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "INVOICED_FULFILLMENT_NOT_FOUND" });
});
