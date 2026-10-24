import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import {fulfillmentSchema} from '@erpx/server-commerce/public/commerce';
import {settlementSchema} from '@erpx/server-commerce/public/commerce';
import { refundConfirmationSchema } from "@erpx/server-finance/public/refunds";
import { afterEach,beforeAll, beforeEach, expect, it, vi } from "vitest";
import {readFile} from "node:fs/promises";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import { deliveryEvidence } from "../../support/helpers/delivery-evidence";
import {completeCommerceReturn,decideCommerceReturn,fulfillOrder,invoiceFulfillment,saveMaster,settleInvoice,saveOrder,transitionOrder,orderSchema} from '@erpx/server-commerce/public/commerce';
import {getOrder} from '@erpx/server-commerce/public/commerce-order-read';
import {importBankStatement,postInvoice,postJournal} from '@erpx/server-finance/public/finance';
import { approveRefund, confirmRefundInstruction, exportRefundInstructions, prepareRefundInstruction, refundWorkspace } from "@erpx/server-finance/public/refunds";
import {confirmManualRate} from '@erpx/server-finance/public/company-money';
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import { countSchema, saveCount, transitionCount } from "@erpx/server-inventory/public/inventory-counts";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {
  addCartLine,
  changeEcommerce,
  checkoutCart,
  createCart,
  createChannel,
  createListing,
  ecommerceDetail,
  ecommerceWorkspace,
} from "@erpx/server-ecommerce/public/ecommerce";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: ecommerceSettlementDatabase ?? env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let ecommerceSettlementDatabase:import("@erpx/server-runtime/public/runtime/types").SqlDatabase|undefined;
// Declared clean scanner for isolated media-pipeline evidence. Native File
// storage, sanitized image processing, owner bind/review/revoke are unchanged.
vi.mock("@erpx/server-files/public/attachments/scanner",()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'Declared Ecommerce native test scanner',definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'Declared Ecommerce native test scanner',definitionUpdatedAt:Date.now()})}));
afterEach(()=>{vi.unstubAllEnvs();ecommerceSettlementDatabase=undefined;});
let fixture: Awaited<ReturnType<typeof expenseFixture>>,
  owner: WorkContext,
  reviewer: WorkContext,
  warehouse: string,
  product: string,
  party: string;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
});

async function assignOwnerStep(kind:"order"|"invoice"|"journal",id:string,stepKey:string,userId:string){
 const flow=await resolveFlow(db,owner,kind,id);
 await assignFlowPrimary(db,owner,kind,id,{stepKey,userId,reason:'Independent owner handoff for checkout evidence',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
}

it("collects a Sales-backed checkout and completes its reviewed return and bank refund with original stock and FX evidence",async()=>{
 const store=packageGrantStore(db,{authorize:async()=>({userId:'fictional-ecommerce-owner-provider'}),obligations:async()=>{throw new Error('NO_INACTIVE_CUTOVER_IN_ECOMMERCE_FIXTURE');}});for(const packageId of ['sales','sales-ecommerce'])await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:fixture.company,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Reviewed isolated Ecommerce owner purchase fixture',evidence:'test:commerce-composed-ecommerce'}});

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
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
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
 await ecommercePurchase('sales-ecommerce','draining');
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
 const beforeRefund=await ecommerceSettlementObservation();expect(beforeRefund.observations.find(row=>row.moduleId==='finance')?.outstanding).toBeGreaterThan(0);expect(beforeRefund.observations.find(row=>row.moduleId==='inventory')?.outstanding).toBeGreaterThan(0);
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
 const retained=await ecommerceSettlementObservation();expect(retained.observations.find(row=>row.moduleId==='finance')?.outstanding).toBe(0);expect(retained.observations.find(row=>row.moduleId==='inventory')?.outstanding).toBeGreaterThan(0);
 const {inspectQualityHold,releaseQualityHold,qualityInspectionSchema}=await import('@erpx/server-quality/public/quality'),inspector={...fixture.reviewer,role:'quality_inspector',branchId:null,departmentId:null,assignmentId:await fixture.grant(fixture.reviewer,'quality_inspector')},qualityManager={...fixture.head,role:'quality_manager',branchId:null,departmentId:null,assignmentId:await fixture.grant(fixture.head,'quality_manager')},qualityId=String(await db.prepare("SELECT h.id FROM inventory_quality_holds h JOIN commerce_return_lot_allocations allocation ON allocation.id=h.source_id JOIN commerce_return_lines line ON line.id=allocation.return_line_id WHERE h.source_type='customer_return' AND line.return_id=?").bind(returnId).first('id'));
 await inspectQualityHold(inspector,qualityId,qualityInspectionSchema.parse({expectedVersion:1,result:'pass',evidence:'Independent inspection confirms the actual returned unopened original lot'}),key());await releaseQualityHold(qualityManager,qualityId,{expectedVersion:2,reason:'Third person reviewed the actual returned lot and original passing inspection'},key());
 await changeEcommerce(owner,'checkout',checkoutId,{action:'sync-fulfillment',expectedVersion:Number(after.checkout.version),reason:'Retain the final actual fulfilled Sales order and reviewed return identities'},key());const native=await db.prepare('SELECT cart.channel_id,line.listing_id FROM ecommerce_checkouts checkout JOIN ecommerce_carts cart ON cart.id=checkout.cart_id JOIN ecommerce_cart_lines line ON line.cart_id=cart.id WHERE checkout.id=?').bind(checkoutId).first<{channel_id:string;listing_id:string}>();if(!native)throw Error('ACTUAL_RETURNED_ECOMMERCE_SOURCE_REQUIRED');await retireNativeStorefront(native.channel_id,native.listing_id);expect((await ecommerceSettlementObservation()).total).toBe(0);const withdrawal=await ecommerceWithdrawal();await ecommerceWithdrawalReview(withdrawal);await ecommerceWithdrawalReview(withdrawal,true);expect((await readCompanyPackageGrants(db,fixture.company)).find(row=>row.packageId==='sales')?.state).toBe('active');expect((await ecommerceDetail(owner,checkoutId)).returnLinks).toHaveLength(1);
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

it('retains an independently rejected native listing without publishing or deleting the original price capture',async()=>{
 const channel=await createChannel(owner,{name:'Declared independently reviewed rejection channel',channelType:'web',configuration:{}},key());await changeEcommerce(reviewer,'channel',String(channel.id),{action:'activate',expectedVersion:1,reason:'Independent channel source review'},key());const listing=await createListing(owner,{channelId:String(channel.id),productId:product,title:'Declared rejected native listing',currency:'THB',price:'20'},key()),input={action:'reject',expectedVersion:1,reason:'Independent review rejects the retained original listing terms'},command=key(),result=await changeEcommerce(reviewer,'listing',String(listing.id),input,command);expect(result).toMatchObject({id:listing.id,status:'rejected',version:2});expect(await changeEcommerce(reviewer,'listing',String(listing.id),input,command)).toEqual(result);expect(await db.prepare('SELECT status,price_minor,reviewed_by FROM ecommerce_listings WHERE id=?').bind(listing.id).first()).toEqual({status:'rejected',price_minor:2000,reviewed_by:reviewer.userId});expect(await db.prepare("SELECT amount_units::text amount FROM money_transaction_snapshots WHERE source_type='ecommerce_listing' AND source_id=? AND amount_kind='unit_price'").bind(listing.id).first('amount')).toBe('2000');
});

async function ecommercePurchase(packageId:'sales'|'sales-ecommerce'='sales-ecommerce',state:'active'|'draining'='active',endsAt:number|null=null,companyId=fixture.company){
 const prior=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId===packageId);
 return packageGrantStore(db,{authorize:async()=>({userId:'isolated-ecommerce-provider'}),obligations:async()=>{throw Error('ECOMMERCE_FIXTURE_CANNOT_INACTIVATE');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(prior?.version??0)+1,reason:'Isolated actual Ecommerce owner purchase',evidence:'test:ecommerce-purchase'}});
}
it('requires the actual Ecommerce add-on and retains exact history and existing settlement while draining',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const input={name:'Exact independently reviewed online channel',channelType:'web',configuration:{}},command=key();
 await expect(createChannel(owner,input,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(ecommerceWorkspace(owner)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await ecommercePurchase('sales');await expect(createChannel(owner,input,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await ecommercePurchase();const created=await createChannel(owner,input,command),id=String(created.id),activate={action:'activate',expectedVersion:1,reason:'Independent exact channel review'},activateKey=key();await changeEcommerce(reviewer,'channel',id,activate,activateKey);
 const checkout=await submittedCheckout(),checkoutId=String(checkout.id);await ecommercePurchase('sales-ecommerce','draining');
 await expect(createChannel(owner,{...input,name:'Forbidden new storefront'},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await expect(createCart(owner,{channelId:id,partyId:party,warehouseId:warehouse,currency:'THB'},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await expect(changeEcommerce(reviewer,'checkout',checkoutId,{action:'approve-order',expectedVersion:1,reason:'A new reservation cannot start while draining'},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 expect(await createChannel(owner,input,command)).toEqual(created);expect(await changeEcommerce(reviewer,'channel',id,activate,activateKey)).toMatchObject({status:'active',version:2});await expect(createChannel(owner,{...input,name:'Changed exact captured request'},command)).rejects.toThrow();
 const synced=await changeEcommerce(owner,'checkout',checkoutId,{action:'sync-fulfillment',expectedVersion:1,reason:'Retain the current original Sales source while draining'},key());expect(synced).toMatchObject({status:'submitted',version:2});expect((await ecommerceDetail(owner,checkoutId)).checkout).toMatchObject({version:2});
 const retired=await changeEcommerce(reviewer,'channel',id,{action:'retire',expectedVersion:2,reason:'Stop new publication while retaining original channel history'},key());expect(retired).toMatchObject({status:'retired',version:3});expect(await createChannel(owner,input,command)).toEqual(created);
 expect((await readCompanyPackageGrants(db,fixture.company)).map(row=>row.packageId)).toEqual(['sales','sales-ecommerce']);
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),owner.assignmentId).run();await expect(createChannel(owner,input,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});await expect(ecommerceWorkspace(owner)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
});
it('rejects arbitrary supplied Ecommerce transactions and current assignment module withdrawal before native Sales effects',async()=>{
 const {channel,listing}=await publishedListing(),cart=await createCart(owner,{channelId:channel.id,partyId:party,warehouseId:warehouse,currency:'THB'},key()),line=await addCartLine(owner,{cartId:cart.id,expectedVersion:1,listingId:listing.id,listingVersion:2,quantity:'1'},key());
 await ecommercePurchase('sales');await ecommercePurchase();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const input={cartId:cart.id,expectedVersion:line.version,orderDate:'2026-09-21',dueDate:'2026-09-28'};
 await expect(db.transaction(tx=>checkoutCart(owner,input,key(),tx))).rejects.toMatchObject({code:'ECOMMERCE_COMPOSITION_REQUIRED'});
 await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind('["crm"]',owner.assignmentId).run();await expect(checkoutCart(owner,input,key())).rejects.toMatchObject({code:'ECOMMERCE_ACCESS_CHANGED'});expect(await db.prepare('SELECT 1 FROM ecommerce_checkouts WHERE cart_id=?').bind(cart.id).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM commerce_orders WHERE id=?').bind(`order-ecommerce-order-${cart.id}`).first()).toBeNull();
});
it('rolls back native Ecommerce and child Sales receipts after late selected-module authority loss',async()=>{
 const {channel,listing}=await publishedListing(),cart=await createCart(owner,{channelId:channel.id,partyId:party,warehouseId:warehouse,currency:'THB'},key()),line=await addCartLine(owner,{cartId:cart.id,expectedVersion:1,listingId:listing.id,listingVersion:2,quantity:'1'},key());await ecommercePurchase('sales');await ecommercePurchase();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 const command=key(),fault=`ecommerce_authority_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET modules_json='["crm"]' WHERE id='${owner.assignmentId}';RETURN NEW;END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 const input={cartId:cart.id,expectedVersion:line.version,orderDate:'2026-09-21',dueDate:'2026-09-28'};
 try{await expect(checkoutCart(owner,input,command)).rejects.toMatchObject({code:'ECOMMERCE_ACCESS_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${fault} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 expect(await db.prepare('SELECT modules_json FROM assignments WHERE id=?').bind(owner.assignmentId).first('modules_json')).toBe('[]');expect(await db.prepare('SELECT 1 FROM ecommerce_checkouts WHERE cart_id=?').bind(cart.id).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM commerce_orders WHERE id=?').bind(`order-ecommerce-order-${cart.id}`).first()).toBeNull();expect(await db.prepare('SELECT version FROM ecommerce_carts WHERE id=?').bind(cart.id).first('version')).toBe(2);
 for(const receipt of [command,`ecommerce-order-${cart.id}`,`ecommerce-submit-${cart.id}`])for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(receipt).first()).toBeNull();expect(await checkoutCart(owner,input,command)).toMatchObject({status:'submitted',version:1});
});
it('rolls back native Ecommerce writes and money captures after late add-on expiry',async()=>{
 await ecommercePurchase('sales');const endsAt=Date.now()+3500;await ecommercePurchase('sales-ecommerce','active',endsAt);vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const command=key(),fault=`ecommerce_expiry_${key().replaceAll('-','')}`,id=`ecommerce-channel-${command}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(GREATEST(0,(${endsAt}-floor(extract(epoch FROM clock_timestamp())*1000)::bigint)::numeric/1000)+0.05);RETURN NEW;END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(createChannel(owner,{name:'Expires during exact owner completion',channelType:'web',configuration:{}},command)).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${fault} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 expect(await db.prepare('SELECT 1 FROM ecommerce_channels WHERE id=?').bind(id).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
});
it('waits for a real committed Ecommerce writer before the first source snapshot without retry masking',async()=>{
 const {channel}=await publishedListing();await ecommercePurchase('sales');await ecommercePurchase();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const command=key(),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics');
 const observed=await committedSourceVisibility(db,tx=>tx.prepare("UPDATE ecommerce_channels SET status='retired',version=version+1 WHERE id=?").bind(channel.id).run(),()=>measureReads(async()=>{try{await createListing(owner,{channelId:channel.id,productId:product,title:'Cannot use the former channel snapshot',currency:'THB',price:'20'},command);return null;}catch(error){return error;}}));
 expect(observed.result).toMatchObject({status:409});expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);expect(await db.prepare('SELECT 1 FROM ecommerce_listings WHERE id=?').bind(`ecommerce-listing-${command}`).first()).toBeNull();
});
it('commits two Companies complete Ecommerce owner entries without hidden deadlock retries',async()=>{
 const other=await expenseFixture(),otherOwner={...other.owner,modules:['ecommerce']};for(const company of [fixture.company,other.company]){await ecommercePurchase('sales','active',null,company);await ecommercePurchase('sales-ecommerce','active',null,company);}vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const {measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),observed=await measureReads(()=>Promise.all([owner,otherOwner].map(actor=>createChannel(actor,{name:'Independent Company Ecommerce source lease',channelType:'web',configuration:{}},key()))));
 expect(observed.result).toHaveLength(2);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);for(const [index,company] of [fixture.company,other.company].entries())expect(await db.prepare('SELECT company_id FROM ecommerce_channels WHERE id=?').bind(observed.result[index].id).first('company_id')).toBe(company);
});

const ecommerceSettlementScope=()=>({companyId:fixture.company,packageId:'sales-ecommerce',packageVersion:1});
async function ecommerceSettlementObservation(){const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');return withOwnerPackageObligations(db,ecommerceSettlementScope(),tx=>readOwnerPackageObligations(db,tx,{...ecommerceSettlementScope(),now:Date.now()}));}
async function ecommerceWithdrawal(){const {packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,fixture.company),current=grants.find(row=>row.packageId==='sales-ecommerce');if(!current)throw Error('ORIGINAL_ECOMMERCE_GRANT_REQUIRED');return {companyId:fixture.company,expectedRevision:packageGrantRevision(fixture.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Actual native Ecommerce sources and included owner settlement independently reviewed',evidence:'test:ecommerce-four-owner-withdrawal'}}]};}
async function ecommerceWithdrawalReview(request:Awaited<ReturnType<typeof ecommerceWithdrawal>>,apply=false,after?:(tx:import("@erpx/server-runtime/public/runtime/types").SqlDatabase)=>Promise<void>){const {withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-ecommerce-settlement-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});return withOwnerPackageObligations(db,ecommerceSettlementScope(),async tx=>{const result=apply?await store.batch.applyIn(tx,request,async()=>{}):await store.batch.previewIn(tx,request);if(after)await after(tx);return result;});}
async function retireNativeStorefront(channelId:string,listingId:string){await changeEcommerce(reviewer,'listing',listingId,{action:'retire',expectedVersion:2,reason:'Stop original publication and retain the independently captured listing price'},key());await changeEcommerce(reviewer,'channel',channelId,{action:'retire',expectedVersion:2,reason:'Retain the original stopped channel configuration and history'},key());}

it('reads exact four-owner Ecommerce obligations without borrowing unrelated Sales pricing or CRM duties',async()=>{
 await ecommercePurchase('sales');await ecommercePurchase();const {savePriceList}=await import('@erpx/server-sales/public/sales-pricing');await savePriceList(fixture.owner,{name:'Unrelated base Sales price book',currency:'THB',customerId:null,maxAutoDiscountBps:0,validFrom:'2026-09-21',validUntil:'2026-12-31',items:[{productId:product,price:'25',taxBps:0}]},key());const observation=await ecommerceSettlementObservation();expect(observation.observations.map(row=>row.moduleId)).toEqual(['ecommerce','sales','inventory','finance']);expect(observation.total).toBe(0);expect(observation.observations.every(row=>/^[a-f0-9]{64}$/.test(row.revision))).toBe(true);expect(JSON.stringify(observation)).not.toMatch(/authority_json|recipient_email|configuration/);
});

it('withdraws only the Ecommerce add-on after normal cart abandonment and independent native retirement and retains exact captured history',async()=>{
 await ecommercePurchase('sales');await ecommercePurchase();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const {channel,listing}=await publishedListing(),cart=await createCart(owner,{channelId:channel.id,partyId:party,warehouseId:warehouse,currency:'THB'},key()),line=await addCartLine(owner,{cartId:cart.id,expectedVersion:1,listingId:listing.id,listingVersion:2,quantity:'1'},key());await ecommercePurchase('sales-ecommerce','draining');const request=await ecommerceWithdrawal();expect((await ecommerceSettlementObservation()).observations.find(row=>row.moduleId==='ecommerce')?.outstanding).toBeGreaterThan(0);await expect(ecommerceWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(ecommerceWithdrawalReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');const body={action:'abandon',expectedVersion:Number(line.version),reason:'The original customer abandoned the retained open cart'},command=key(),abandoned=await changeEcommerce(owner,'cart',String(cart.id),body,command);expect(abandoned).toMatchObject({status:'abandoned',version:3});await retireNativeStorefront(String(channel.id),String(listing.id));expect((await ecommerceSettlementObservation()).total).toBe(0);const history=(await db.prepare('SELECT to_jsonb(row)::text value FROM ecommerce_cart_lines row WHERE cart_id=?').bind(cart.id).all()).results;await ecommerceWithdrawalReview(request);const result=await ecommerceWithdrawalReview(request,true);expect(await ecommerceWithdrawalReview(request,true)).toEqual(result);expect(await changeEcommerce(owner,'cart',String(cart.id),body,command)).toEqual(abandoned);expect((await db.prepare('SELECT to_jsonb(row)::text value FROM ecommerce_cart_lines row WHERE cart_id=?').bind(cart.id).all()).results).toEqual(history);await expect(changeEcommerce(owner,'cart',String(cart.id),body,key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});expect((await ecommerceWorkspace(owner)).purchase).toMatchObject({newWork:false,settleExisting:false,history:true});expect((await readCompanyPackageGrants(db,fixture.company)).find(row=>row.packageId==='sales')?.state).toBe('active');const quote=await saveOrder(fixture.owner,null,orderSchema.parse({kind:'quote',partyId:party,warehouseId:warehouse,orderDate:'2026-09-21',dueDate:'2026-09-28',lines:[{productId:product,quantity:'1',price:'25',taxBps:0}]}),key());expect((await getOrder(fixture.owner,String(quote.id))).order).toMatchObject({state:'draft'});
});

it('settles actual retained Ecommerce child Sales cancellation before inactive without granting full Finance or Supply',async()=>{
 await ecommercePurchase('sales');await ecommercePurchase();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const checkout=await submittedCheckout(),id=String(checkout.id),orderId=String(checkout.salesOrderId);await ecommercePurchase('sales-ecommerce','draining');const request=await ecommerceWithdrawal();expect((await ecommerceSettlementObservation()).observations.find(row=>row.moduleId==='sales')?.outstanding).toBeGreaterThan(0);await expect(ecommerceWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await transitionOrder(reviewer,orderId,{action:'return',expectedVersion:2},key());await transitionOrder(fixture.owner,orderId,{action:'cancel',expectedVersion:3},key());const synchronized={action:'sync-fulfillment',expectedVersion:1,reason:'The actual original Sales order was cancelled through its owner'},syncKey=key();await changeEcommerce(owner,'checkout',id,synchronized,syncKey);const native=await db.prepare('SELECT cart.channel_id,line.listing_id FROM ecommerce_checkouts checkout JOIN ecommerce_carts cart ON cart.id=checkout.cart_id JOIN ecommerce_cart_lines line ON line.cart_id=cart.id WHERE checkout.id=?').bind(id).first<{channel_id:string;listing_id:string}>();if(!native)throw Error('ACTUAL_ECOMMERCE_NATIVE_SOURCE_REQUIRED');await retireNativeStorefront(native.channel_id,native.listing_id);expect((await ecommerceSettlementObservation()).total).toBe(0);await ecommerceWithdrawalReview(request);await ecommerceWithdrawalReview(request,true);expect((await ecommerceDetail(owner,id)).checkout).toMatchObject({status:'cancelled',version:2});expect(await changeEcommerce(owner,'checkout',id,synchronized,syncKey)).toMatchObject({status:'cancelled',version:2});expect((await readCompanyPackageGrants(db,fixture.company)).map(row=>row.packageId)).toEqual(['sales','sales-ecommerce']);
});

it('rolls back a late native Ecommerce obligation with the normal Commercial inactive receipt after final owner effects',async()=>{
 await ecommercePurchase('sales');await ecommercePurchase();await ecommercePurchase('sales-ecommerce','draining');const request=await ecommerceWithdrawal(),command=key();await expect(ecommerceWithdrawalReview(request,true,async tx=>{ecommerceSettlementDatabase=tx;try{await createChannel(owner,{name:'Late actual native Ecommerce obligation',channelType:'web',configuration:{}},command);}finally{ecommerceSettlementDatabase=undefined;}})).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='sales-ecommerce'").bind(fixture.company).first()).toEqual({state:'draining',version:2});expect(await db.prepare('SELECT 1 FROM ecommerce_channels WHERE id=?').bind(`ecommerce-channel-${command}`).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM company_package_change_sets WHERE id=?').bind(request.idempotencyKey).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect((await ecommerceSettlementObservation()).total).toBe(0);
});

it('observes an actual committed Ecommerce obligation before the withdrawal first snapshot without retry masking',async()=>{
 await ecommercePurchase('sales');await ecommercePurchase();await ecommercePurchase('sales-ecommerce','draining');const request=await ecommerceWithdrawal(),command=key(),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),observed=await committedSourceVisibility(db,async tx=>{ecommerceSettlementDatabase=tx;try{await createChannel(owner,{name:'Actual concurrently committed Ecommerce obligation',channelType:'web',configuration:{}},command);}finally{ecommerceSettlementDatabase=undefined;}},()=>measureReads(async()=>{try{await ecommerceWithdrawalReview(request);return null;}catch(error){return error;}}));expect(observed.result).toMatchObject({message:'PACKAGE_OBLIGATIONS_REMAIN'});expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);expect(await db.prepare('SELECT status FROM ecommerce_channels WHERE id=?').bind(`ecommerce-channel-${command}`).first('status')).toBe('draft');
});

it('requires minted exact Ecommerce sources and refuses forged terminal labels or unsupported withdrawal scopes',async()=>{
 const {ecommercePackageObligations}=await import('@erpx/server-ecommerce/public/package-obligations'),{withOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),input={...ecommerceSettlementScope(),moduleId:'ecommerce',now:Date.now()};await expect(db.transaction(tx=>ecommercePackageObligations(db,tx,input))).rejects.toThrow('ECOMMERCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED');expect(()=>withOwnerPackageObligations(db,{...ecommerceSettlementScope(),packageVersion:2},async()=>{})).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');await expect(withOwnerPackageObligations(db,ecommerceSettlementScope(),tx=>ecommercePackageObligations(db,tx,{...input,moduleId:'sales'}))).rejects.toThrow('ECOMMERCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED');await expect(withOwnerPackageObligations(db,ecommerceSettlementScope(),tx=>ecommercePackageObligations(db,tx,{...input,companyId:'other-company'}))).rejects.toThrow('ECOMMERCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED');await ecommercePurchase('sales');await ecommercePurchase();const channel=await createChannel(owner,{name:'Forged retirement must remain unresolved',channelType:'web',configuration:{}},key());await db.prepare("UPDATE ecommerce_channels SET status='retired',version=version+1 WHERE id=?").bind(channel.id).run();await ecommercePurchase('sales-ecommerce','draining');expect((await ecommerceSettlementObservation()).total).toBeGreaterThan(0);await expect(ecommerceWithdrawalReview(await ecommerceWithdrawal())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
});

it('withdraws actual Ecommerce public media while draining then retains held private bytes and exact reviewed revocation history after addon inactive',async()=>{
 await db.batch([owner,reviewer].map(actor=>db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind('["ecommerce"]',actor.assignmentId)));
 const {createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,drainAttachmentScans,bindRecordAttachmentSet,createAttachmentPublication,approveAttachmentPublication,revokeAttachmentPublication,withdrawAttachmentLink,attachmentContent,listAttachmentSets}=await import('@erpx/server-files/public/attachments/service'),{seedAttachmentSet}=await import('../../support/helpers/attachment-v2'),{controlAccount}=await import('../../support/helpers/engineering-workplace-platform-attachment-owners');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');const {channel,listing}=await publishedListing();await controlAccount(owner);await controlAccount(reviewer);vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const input={targetType:'ecommerce_listing',targetId:String(listing.id),purposeKey:'listing_media',componentKind:'record' as const,expectedTargetVersion:2};await expect(createAttachmentSet(db,owner,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await ecommercePurchase('sales');await expect(createAttachmentSet(db,owner,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await ecommercePurchase();const image=await readFile('public/icon-192.png'),set=await createAttachmentSet(db,owner,input,key()),reservation=await reserveAttachmentFile(db,owner,set.id,{filename:'declared-ecommerce-icon.png',mediaType:'image/png',bytes:image.byteLength},key()),upload=await completeAttachmentUpload(db,env.H001_FILES,owner,reservation.uploadIntentId,Uint8Array.from(image).buffer);expect(await drainAttachmentScans(db,env.H001_FILES,upload.sourceId)).toBe(true);await bindRecordAttachmentSet(db,owner,set.id,2,key());const publication=await createAttachmentPublication(db,env.H001_FILES,owner,{sourceLinkId:reservation.attachmentLinkId,targetType:'ecommerce_listing',targetId:String(listing.id),altText:'Declared actual Ecommerce product image',ordinal:0},key());if(!publication)throw Error('ORIGINAL_ECOMMERCE_PUBLICATION_REQUIRED');await expect(approveAttachmentPublication(db,owner,String(publication.id),key())).rejects.toMatchObject({code:'INDEPENDENT_PUBLICATION_REVIEW'});await approveAttachmentPublication(db,reviewer,String(publication.id),key());await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(upload.sourceId).run();const staged=await seedAttachmentSet(db,owner,{...input,expectedTargetVersion:3,mediaType:'image/png'});await ecommercePurchase('sales-ecommerce','draining');await expect(createAttachmentSet(db,owner,{...input,expectedTargetVersion:3},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await createAttachmentSet(db,owner,{...input,purposeKey:'compliance_document',expectedTargetVersion:3},key());await withdrawAttachmentLink(db,owner,staged.linkId,'Stop this actual staged listing media while retaining its File history',key());await changeEcommerce(reviewer,'listing',String(listing.id),{action:'retire',expectedVersion:3,reason:'Stop the actual listing while retaining independently reviewed native public media'},key());await changeEcommerce(reviewer,'channel',String(channel.id),{action:'retire',expectedVersion:2,reason:'Stop the actual native Ecommerce channel'},key());const request=await ecommerceWithdrawal();expect((await ecommerceSettlementObservation()).observations.find(row=>row.moduleId==='ecommerce')?.outstanding).toBeGreaterThan(0);await expect(ecommerceWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');const reason='Independent reviewer revokes actual Ecommerce public media during draining',command=key(),revoked=await revokeAttachmentPublication(db,reviewer,String(publication.id),reason,command);expect(revoked).toMatchObject({state:'revoked'});expect((await ecommerceSettlementObservation()).total).toBe(0);await ecommerceWithdrawalReview(request);await ecommerceWithdrawalReview(request,true);expect(await revokeAttachmentPublication(db,reviewer,String(publication.id),reason,command)).toMatchObject({state:'revoked'});await expect(revokeAttachmentPublication(db,reviewer,String(publication.id),reason,key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});await expect(revokeAttachmentPublication(db,reviewer,String(publication.id),reason+' changed',command)).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});expect(await listAttachmentSets(db,owner,{targetType:'ecommerce_listing',targetId:String(listing.id)})).not.toHaveLength(0);const retained=await attachmentContent(db,env.H001_FILES,owner,reservation.attachmentLinkId,'original');expect(new Uint8Array(await retained.object.arrayBuffer())).toEqual(new Uint8Array(image));expect(await db.prepare('SELECT legal_hold FROM attachments WHERE id=?').bind(upload.sourceId).first('legal_hold')).toBe(1);await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),reviewer.assignmentId).run();await expect(revokeAttachmentPublication(db,reviewer,String(publication.id),reason,command)).rejects.toMatchObject({code:'ATTACHMENT_TARGET_NOT_FOUND'});
});

it('rolls back actual Ecommerce File binding and its immutable receipt after a deferred native listing source change',async()=>{
 await db.batch([owner,reviewer].map(actor=>db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind('["ecommerce"]',actor.assignmentId)));
 const {seedAttachmentSet,attachmentBindingState}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet}=await import('@erpx/server-files/public/attachments/service');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');const {listing}=await publishedListing(),file=await seedAttachmentSet(db,owner,{targetType:'ecommerce_listing',targetId:String(listing.id),purposeKey:'compliance_document',componentKind:'record',expectedTargetVersion:2});await ecommercePurchase('sales');await ecommercePurchase();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const command=key(),fault=`ecommerce_file_source_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE ecommerce_listings SET title='Changed after actual File owner persistence' WHERE id='${listing.id}';RETURN NEW;END;$$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);try{await expect(bindRecordAttachmentSet(db,owner,file.setId,2,command)).rejects.toMatchObject({code:'ECOMMERCE_ATTACHMENT_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}expect(await db.prepare('SELECT title,version FROM ecommerce_listings WHERE id=?').bind(listing.id).first()).toEqual({title:'Published item',version:2});expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:'draft',link_state:'draft'});for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();await bindRecordAttachmentSet(db,owner,file.setId,2,command);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:'bound',link_state:'bound'});expect(await db.prepare('SELECT version FROM ecommerce_listings WHERE id=?').bind(listing.id).first('version')).toBe(3);
});

it('buffers real Ecommerce private bytes until deferred listing and current IAM checks complete without an escaped lazy object',async()=>{
 const {seedAttachmentSet}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');
 await db.batch([owner,reviewer].map(actor=>db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind('["ecommerce"]',actor.assignmentId)));
 const {listing}=await publishedListing(),file=await seedAttachmentSet(db,owner,{targetType:'ecommerce_listing',targetId:String(listing.id),purposeKey:'compliance_document',componentKind:'record',expectedTargetVersion:2}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes='Actual Ecommerce retained private listing evidence';await env.H001_FILES.put(objectKey,bytes);await ecommercePurchase('sales');await ecommercePurchase();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await bindRecordAttachmentSet(db,owner,file.setId,2,key());await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();
 let consumed=0;const files:import("@erpx/server-runtime/public/runtime/types").FileStore={put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args),get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;}};
 const baseline=Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(file.linkId).first('n'));
 for(const kind of ['source','iam'] as const){const fault=`ecommerce_read_${key().replaceAll('-','')}`,effect=kind==='source'?`UPDATE ecommerce_listings SET title='Late private Ecommerce source' WHERE id='${listing.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${owner.assignmentId}'`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
  try{await expect(attachmentContent(db,files,owner,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
  expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(file.linkId).first('n'))).toBe(baseline);expect(await db.prepare('SELECT title,version FROM ecommerce_listings WHERE id=?').bind(listing.id).first()).toEqual({title:'Published item',version:3});expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(owner.assignmentId).first('revoked_at')).toBeNull();
 }
 expect(consumed).toBe(2);const result=await attachmentContent(db,files,owner,file.linkId,'original');expect(consumed).toBe(3);expect(await result.object.text()).toBe(bytes);expect(await new Response(result.object.body).text()).toBe(bytes);expect(await db.prepare('SELECT legal_hold,source_state FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,source_state:'available'});
 await expect(db.transaction(tx=>attachmentContent(tx,files,owner,file.linkId,'original'))).rejects.toMatchObject({code:'ECOMMERCE_FILE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(3);
});

it('reads two Companies actual private listing evidence with source writers and zero deadlock serialization or retry masking',async()=>{
 const {seedAttachmentSet}=await import('../../support/helpers/attachment-v2'),{attachmentContent}=await import('@erpx/server-files/public/attachments/service'),other=await expenseFixture(),otherActor={...other.owner,modules:['ecommerce']},otherReviewer={...other.tie,role:'holding_owner',assignmentId:await other.grant(other.tie,'holding_owner'),modules:['ecommerce']};vi.stubEnv('ERPX_ATTACHMENTS_V2','true');
 const otherProduct=await saveMaster(other.owner,'products',null,{sku:key(),name:'Actual independent Company online reference',price:'12'},key()),firstChannel=await createChannel(owner,{name:'First private evidence channel',channelType:'web',configuration:{storefront:'sandbox'}},key()),secondChannel=await createChannel(otherActor,{name:'Second private evidence channel',channelType:'web',configuration:{storefront:'sandbox'}},key());
 await changeEcommerce(reviewer,'channel',String(firstChannel.id),{action:'activate',expectedVersion:1,reason:'Original independent first Company channel review'},key());await changeEcommerce(otherReviewer,'channel',String(secondChannel.id),{action:'activate',expectedVersion:1,reason:'Original independent second Company channel review'},key());
 const firstListing=await createListing(owner,{channelId:firstChannel.id,productId:product,title:'First actual private listing',currency:'THB',price:'20'},key()),secondListing=await createListing(otherActor,{channelId:secondChannel.id,productId:otherProduct.id,title:'Second actual private listing',currency:'THB',price:'12'},key());
 await db.batch([owner,otherActor].map(actor=>db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind('["ecommerce"]',actor.assignmentId)));
 const cases=await Promise.all([{actor:owner,id:String(firstListing.id),company:fixture.company},{actor:otherActor,id:String(secondListing.id),company:other.company}].map(async item=>({...item,file:await seedAttachmentSet(db,item.actor,{targetType:'ecommerce_listing',targetId:item.id,purposeKey:'compliance_document',componentKind:'record',expectedTargetVersion:1}),bytes:`Private evidence of actual Company ${item.company}`})));
 for(const item of cases){await ecommercePurchase('sales','active',null,item.company);await ecommercePurchase('sales-ecommerce','active',null,item.company);const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(item.file.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,item.bytes);}
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const fault=`ecommerce_read_two_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(.12);UPDATE ecommerce_listings SET title=title WHERE id=CASE WHEN NEW.link_id='${cases[0].file.linkId}' THEN '${cases[0].id}' ELSE '${cases[1].id}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN ('${cases[0].file.linkId}','${cases[1].file.linkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{const {measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),measured=await measureReads(()=>Promise.all(cases.map(item=>attachmentContent(db,env.H001_FILES,item.actor,item.file.linkId,'original'))));expect(measured.result).toHaveLength(2);expect(measured.metrics.deadlockFailures??0).toBe(0);expect(measured.metrics.serializationFailures??0).toBe(0);expect(measured.metrics.transactionRetries??0).toBe(0);for(let index=0;index<cases.length;index++){expect(await measured.result[index].object.text()).toBe(cases[index].bytes);expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND company_id=? AND actor_id=? AND outcome='allowed'").bind(cases[index].file.linkId,cases[index].company,cases[index].actor.userId).first('n'))).toBe(1);}}
 finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
});

it('retains only a truthful native Ecommerce denial audit and rolls back denied evidence after late listing or authority changes',async()=>{
 await db.batch([owner,reviewer].map(actor=>db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind('["ecommerce"]',actor.assignmentId)));
 const {seedAttachmentSet}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');const {listing}=await publishedListing(),file=await seedAttachmentSet(db,owner,{targetType:'ecommerce_listing',targetId:String(listing.id),purposeKey:'compliance_document',componentKind:'record',expectedTargetVersion:2});await bindRecordAttachmentSet(db,owner,file.setId,2,key());
 await (await import('../../support/helpers/private-file-unavailable-audits')).retainedUnavailableFileAudits(db,env.H001_FILES,owner,file.linkId,{targetType:'ecommerce_listing',targetId:String(listing.id),purposeKey:'compliance_document',componentKind:'record',expectedTargetVersion:3});
 const reader=fixture.claimant;await expect(attachmentContent(db,env.H001_FILES,reader,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FORBIDDEN'});
 const original=(await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied'").bind(file.linkId,reader.userId).all()).results;expect(original).toHaveLength(1);expect(original[0].reason).toBe('ATTACHMENT_FORBIDDEN');expect(await db.prepare("SELECT 1 FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(file.linkId).first()).toBeNull();
 for(const change of ['source','authority'] as const){const fault=`ecommerce_denied_${key().replaceAll('-','')}`,effect=change==='source'?`UPDATE ecommerce_listings SET version=version+1 WHERE id='${listing.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${reader.assignmentId}'`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fault}()`);
  try{await expect(attachmentContent(db,env.H001_FILES,reader,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
  expect((await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied'").bind(file.linkId,reader.userId).all()).results).toEqual(original);expect(await db.prepare('SELECT version FROM ecommerce_listings WHERE id=?').bind(listing.id).first('version')).toBe(3);expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(reader.assignmentId).first('revoked_at')).toBeNull();
 }
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(reader.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,reader,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_TARGET_NOT_FOUND'});
 const invalidRetained=(await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied' ORDER BY id").bind(file.linkId,reader.userId).all()).results;expect(invalidRetained).toHaveLength(2);expect(invalidRetained.some(event=>event.reason==='ATTACHMENT_TARGET_NOT_FOUND')).toBe(true);
 const invalidVersion=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(reader.assignmentId).first('version'),invalidFault=`ecommerce_invalid_denial_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${invalidFault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='${reader.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${invalidFault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${invalidFault}()`);
 try{await expect(attachmentContent(db,env.H001_FILES,reader,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${invalidFault} ON attachment_access_events;DROP FUNCTION ${invalidFault}()`);}
 expect(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(reader.assignmentId).first('version')).toBe(invalidVersion);expect((await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied' ORDER BY id").bind(file.linkId,reader.userId).all()).results).toEqual(invalidRetained);

});
