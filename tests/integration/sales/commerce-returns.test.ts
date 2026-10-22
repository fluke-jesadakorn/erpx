import {packageGrantStore,readCompanyPackageGrants} from "../../../src/server/modules/commercial/public/package-grants";
import { fulfillmentSchema } from "../../../src/server/modules/commerce/public/commerce";
import { returnRequestSchema } from "../../../src/server/modules/commerce/public/commerce";
import { afterEach,beforeAll,beforeEach,expect,it,vi } from "vitest";
import { applyMigrations,env } from "../../support/runtime-env";
import { expenseDb as db,expenseFixture,key } from "../../support/helpers/expense-fixture";
import { deliveryEvidence } from "../../support/helpers/delivery-evidence";
import { assignFlowPrimary,resolveFlow } from "../../../src/server/modules/workflow/public/flow-routing";
import { aging,calculateCloseReconciliations,postInvoice,postJournal } from "../../../src/server/modules/finance/public/finance";
import { completeCommerceReturn,decideCommerceReturn,fulfillOrder,getOrder,invoiceFulfillment,orderSchema,requestCommerceReturn,saveMaster,saveOrder,settleInvoice,transitionOrder } from "../../../src/server/modules/commerce/public/commerce";
import { attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet } from "../../support/helpers/attachment-v2";
import {withFinanceFlowAssignment} from "../../../src/server/modules/finance/public/document-purchases";
import {prepareCommerceProjection,assertCommerceFinanceHistory,postIncludedCommerceJournal} from "../../support/helpers/commerce-finance-history";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

afterEach(()=>vi.unstubAllEnvs());
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{enableAttachmentV2ForTest();await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.manager.role="finance_approver";f.manager.branchId=null;f.manager.departmentId=null;f.manager.assignmentId=await f.grant(f.manager,"finance_approver");f.head.role="purchasing_manager";f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,"purchasing_manager");});

async function assign(kind:"order"|"invoice"|"journal",id:string,stepKey:string,userId:string){const flow=await resolveFlow(db,f.owner,kind,id),command=key(),input={stepKey,userId,reason:"Assign return fixture responsibility",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion};if(kind==="order")await assignFlowPrimary(db,f.owner,kind,id,input,command);else await withFinanceFlowAssignment(f.owner,command,tx=>assignFlowPrimary(tx??db,f.owner,kind,id,input,command),{kind,id});}

it("returns a supplier receipt through independent review, stock reversal and debit-note GL",async()=>{
  for(const [suffix,category] of [['inventory','asset'],['clearing','liability']])await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)").bind(`${f.company}-${suffix}`,f.chart,f.company,suffix,suffix,category).run();
  await db.prepare('UPDATE commerce_settings SET inventory_account_id=?,clearing_account_id=? WHERE company_id=?').bind(`${f.company}-inventory`,`${f.company}-clearing`,f.company).run();
  const warehouse=await saveMaster(f.owner,"warehouses",null,{branchId:f.branch,code:key(),name:"Return warehouse"},key()),product=await saveMaster(f.owner,"products",null,{sku:key(),name:"Return product",price:"10"},key()),party=await saveMaster(f.owner,"parties",null,{kind:"supplier",code:key(),name:"Return supplier"},key());
  const order=await saveOrder(f.owner,null,orderSchema.parse({kind:"purchase",partyId:party.id,warehouseId:warehouse.id,orderDate:"2026-09-02",dueDate:"2026-09-03",lines:[{productId:product.id,quantity:"2",price:"10",taxBps:0}]}),key()),orderId=String(order.id);
  await transitionOrder(f.owner,orderId,{action:"submit",expectedVersion:1},key());await transitionOrder(f.head,orderId,{action:"approve",expectedVersion:2},key());await assign("order",orderId,"stock_gl",f.manager.userId);
  let details=await getOrder(f.owner,orderId);const receiptFiles=await seedAttachmentSet(db,f.owner,{targetType:"purchase_order",targetId:orderId,purposeKey:"delivery_note",componentRef:"goods-receipt",expectedTargetVersion:3});const receipt=await fulfillOrder(f.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:"2026-09-02",lines:[{lineId:String(details.lines[0].id),quantity:"2"}],attachmentSetIds:[receiptFiles.setId]}),key());
  expect(await attachmentBindingState(db,receiptFiles.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"purchase_receipt",binding_id:String(receipt.fulfillmentId)});
  const goodsJournal=String((await db.prepare("SELECT journal_id FROM commerce_fulfillments WHERE id=?").bind(receipt.fulfillmentId).first("journal_id")));await assign("journal",goodsJournal,"draft",f.manager.userId);await postJournal(f.manager,goodsJournal,key(),{expectedVersion:1});
  await assign("order",orderId,"invoicing",f.owner.userId);const invoice=await invoiceFulfillment(f.owner,String(receipt.fulfillmentId),{expectedVersion:1,date:"2026-09-02",dueDate:"2026-09-03"},key());await assign("invoice",String(invoice.invoiceId),"draft",f.manager.userId);await postInvoice(f.manager,String(invoice.invoiceId),{expectedVersion:1,periodId:f.period,entryDate:"2026-09-02"},key());
  await assign("invoice",String(invoice.invoiceId),"posted",f.payer.userId);
  const pendingSlip=await seedAttachmentSet(db,f.payer,{targetType:"invoice_payment",targetId:String(invoice.invoiceId),purposeKey:"payment_slip",componentKind:"action",componentRef:"legacy-upload-adapter",expectedTargetVersion:2,sourceState:"quarantined"});
  await expect(settleInvoice(f.payer,String(invoice.invoiceId),{expectedVersion:2,date:"2026-09-03",amount:"1",reference:"Pending scan proof",attachmentSetIds:[pendingSlip.setId]},key())).rejects.toMatchObject({code:"ATTACHMENT_SCAN_PENDING"});
  expect(await attachmentBindingState(db,pendingSlip.setId)).toMatchObject({set_state:"draft",link_state:"draft"});
  const rawSlip=await f.attach(String(invoice.invoiceId),"invoice_payment");
  await expect(settleInvoice(f.payer,String(invoice.invoiceId),{expectedVersion:2,date:"2026-09-03",amount:"1",reference:"Unlinked raw proof",paymentSlipId:rawSlip,attachmentSetIds:[]},key())).rejects.toMatchObject({code:"PAYMENT_SLIP_REQUIRED"});
  const {createSupplierBatch,decideSupplierBatch}=await import("../../../src/server/modules/procurement/public/supplier-payments");
  const reserved=await createSupplierBatch(f.owner,{bankAccountId:f.accounts.cash,plannedDate:'2026-09-03',items:[{invoiceId:String(invoice.invoiceId),expectedVersion:2,amount:'20',beneficiaryName:'Simulated supplier',beneficiaryAccount:'SIMULATED-001'}]},key());
  details=await getOrder(f.owner,orderId);const line=details.fulfillmentLines[0],returnFiles=await seedAttachmentSet(db,f.owner,{targetType:"commerce_fulfillment",targetId:String(receipt.fulfillmentId),purposeKey:"return_condition",componentRef:"return-request",expectedTargetVersion:2}),requested=await requestCommerceReturn(f.owner,String(receipt.fulfillmentId),returnRequestSchema.parse({expectedVersion:2,date:"2026-09-03",reason:"Supplier accepted damaged unit",lines:[{lineId:String(line.id),quantity:"1"}],attachmentSetIds:[returnFiles.setId]}),key());
  expect(await attachmentBindingState(db,returnFiles.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"commerce_return",binding_id:String(requested.id)});
  await expect(decideCommerceReturn(f.owner,String(requested.id),"approve",{expectedVersion:1,reason:"Self approval"},key())).rejects.toMatchObject({code:"INDEPENDENT_APPROVER_REQUIRED"});
  await expect(decideCommerceReturn(f.head,String(requested.id),'approve',{expectedVersion:1,reason:'Cannot return a reserved invoice'},key())).rejects.toMatchObject({code:'CONFLICT'});
  await decideSupplierBatch(f.owner,String(reserved.id),{expectedVersion:1,reason:'Cancel unexported supplier payment before returning goods'},key(),'cancel');
  const approved=await decideCommerceReturn(f.head,String(requested.id),"approve",{expectedVersion:1,reason:"Warehouse evidence and supplier authorization reviewed"},key());
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(warehouse.id,product.id).first()).toEqual({quantity_milli:1000,value_minor:1000});
  for(const journalId of [String(approved.stockJournalId),String(approved.creditJournalId)]){await assign("journal",journalId,"draft",f.manager.userId);await postJournal(f.manager,journalId,key(),{expectedVersion:1});const sums=await db.prepare("SELECT SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?").bind(journalId).first();expect(sums?.debit).toBe(sums?.credit);}
  const completed=await completeCommerceReturn(f.manager,String(requested.id),{expectedVersion:2},key());expect(completed).toMatchObject({status:"posted",version:3});
  expect(await db.prepare("SELECT outstanding_minor,status FROM finance_invoices WHERE id=?").bind(invoice.invoiceId).first()).toEqual({outstanding_minor:1000,status:"partially_paid"});
  await expect(requestCommerceReturn(f.owner,String(receipt.fulfillmentId),returnRequestSchema.parse({expectedVersion:2,date:"2026-09-04",reason:"Exceeds remaining quantity",lines:[{lineId:String(line.id),quantity:"2"}]}),key())).rejects.toMatchObject({code:"OVER_RETURN"});
});

async function paidReturn(kind:'sales'|'purchase',paid='15',taxBps=0,returnDate='2026-09-04',afterApproval?:()=>Promise<void>) {
  const {settleInvoice}=await import("../../../src/server/modules/commerce/public/commerce");
  for(const [suffix,category] of [['receivable','asset'],['inventory','asset'],['clearing','liability']])await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)").bind(`${f.company}-${suffix}`,f.chart,f.company,suffix,suffix,category).run();
  await db.prepare('UPDATE commerce_settings SET receivable_account_id=?,inventory_account_id=?,clearing_account_id=?,output_tax_account_id=? WHERE company_id=?').bind(`${f.company}-receivable`,`${f.company}-inventory`,`${f.company}-clearing`,f.accounts.tax,f.company).run();
  const warehouse=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Paid-return fixture warehouse'},key());
  const product=await saveMaster(f.owner,'products',null,{sku:key(),name:'Paid-return fixture product',price:'10'},key());
  if(kind==='sales')await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)').bind(warehouse.id,product.id).run();
  const party=await saveMaster(f.owner,'parties',null,{kind:kind==='sales'?'customer':'supplier',code:key(),name:'Refund fixture counterparty'},key());
  f.head.role=kind==='sales'?'sales_manager':'purchasing_manager';f.head.assignmentId=await f.grant(f.head,f.head.role);
  const order=await saveOrder(f.owner,null,orderSchema.parse({kind,partyId:party.id,warehouseId:warehouse.id,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product.id,quantity:'2',price:'10',taxBps}]}),key()),orderId=String(order.id);
  await transitionOrder(f.owner,orderId,{action:'submit',expectedVersion:1},key());
  await transitionOrder(f.head,orderId,{action:'approve',expectedVersion:2},key());
  await afterApproval?.();
  await assign('order',orderId,'stock_gl',f.manager.userId);
  let details=await getOrder(f.owner,orderId);
  const fulfillment=await fulfillOrder(f.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-02',lines:[{lineId:String(details.lines[0].id),quantity:'2'}],...(kind==='sales'?await deliveryEvidence(db,f.owner,orderId):{})}),key());
  const goodsJournal=String(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(fulfillment.fulfillmentId).first('journal_id'));
  await assign('journal',goodsJournal,'draft',f.manager.userId);if(process.env.ERPX_FINANCE_PACKAGE_ENFORCEMENT==='enforced')await postIncludedCommerceJournal(f,String(fulfillment.fulfillmentId),goodsJournal);else await postJournal(f.manager,goodsJournal,key(),{expectedVersion:1});
  await assign('order',orderId,'invoicing',f.owner.userId);
  const invoice=await invoiceFulfillment(f.owner,String(fulfillment.fulfillmentId),{expectedVersion:1,date:'2026-09-02',dueDate:'2026-09-03'},key());
  const invoiceId=String(invoice.invoiceId);await assign('invoice',invoiceId,'draft',f.manager.userId);
  await postInvoice(f.manager,invoiceId,{expectedVersion:1,periodId:f.period,entryDate:'2026-09-02'},key());
  f.payer.branchId=null;f.payer.departmentId=null;
  await assign('invoice',invoiceId,'posted',f.payer.userId);
  const paymentEvidence=kind==='purchase'?await seedAttachmentSet(db,f.payer,{targetType:'invoice_payment',targetId:invoiceId,purposeKey:'payment_slip',componentKind:'action',componentRef:'legacy-upload-adapter',expectedTargetVersion:2}):null;
  const settlement=await settleInvoice(f.payer,invoiceId,{expectedVersion:2,date:'2026-09-03',amount:paid,reference:'Simulated original bank payment',attachmentSetIds:paymentEvidence?[paymentEvidence.setId]:[]},key());
  if(paymentEvidence)expect(await attachmentBindingState(db,paymentEvidence.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'invoice_payment_settlement'});
  details=await getOrder(f.owner,orderId);
  const requestKey=key(),input={expectedVersion:2,date:returnDate,reason:'Customer or supplier accepted the whole return',lines:[{lineId:String(details.fulfillmentLines[0].id),quantity:'2',...(kind==='sales'?{condition:'unknown' as const,conditionNote:'Returned units require independent Quality inspection',qualityDueDate:new Date(Date.parse(returnDate)+86400000).toISOString().slice(0,10)}:{})}]};
  const requested=await requestCommerceReturn(f.owner,String(fulfillment.fulfillmentId),returnRequestSchema.parse(input),requestKey);
  expect(await requestCommerceReturn(f.owner,String(fulfillment.fulfillmentId),returnRequestSchema.parse(input),requestKey)).toEqual(requested);
  await expect(requestCommerceReturn(f.owner,'different-source',returnRequestSchema.parse(input),requestKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  // A later configuration edit must not redirect historical stock reversals.
  await db.prepare('UPDATE commerce_settings SET inventory_account_id=?,cogs_account_id=?,clearing_account_id=? WHERE company_id=?').bind(f.accounts.expense,f.accounts.cash,f.accounts.cash,f.company).run();
  const approved=await decideCommerceReturn(f.head,String(requested.id),'approve',{expectedVersion:1,reason:'Return condition and original evidence reviewed'},key());
  const originalPosting=(await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(goodsJournal).all<{account_id:string;debit_minor:number;credit_minor:number}>()).results;
  const reversalPosting=(await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(approved.stockJournalId).all()).results;
  expect(reversalPosting).toEqual(originalPosting.map(line=>({account_id:line.account_id,debit_minor:line.credit_minor,credit_minor:line.debit_minor})));
  for(const journalId of [String(approved.stockJournalId),String(approved.creditJournalId)]){
    await assign('journal',journalId,'draft',f.manager.userId);await postJournal(f.manager,journalId,key(),{expectedVersion:1});
  }
  const {beginClose}=await import("../../../src/server/modules/finance/public/finance");
  await expect(beginClose(f.owner,f.period,key())).rejects.toMatchObject({code:process.env.ERPX_FINANCE_PACKAGE_ENFORCEMENT==='enforced'?'PACKAGE_REQUIRED':'CLOSE_NOT_READY'});
  const completeKey=key(),completed=await completeCommerceReturn(f.manager,String(requested.id),{expectedVersion:2},completeKey);
  expect(await completeCommerceReturn(f.manager,String(requested.id),{expectedVersion:2},completeKey)).toEqual(completed);
  const refundId=String(completed.refundId);
  expect(completed).toMatchObject({creditedMinor:2000+Math.round(2000*taxBps/10000)-Number(paid)*100,refundMinor:Number(paid)*100});
  expect(await db.prepare('SELECT outstanding_minor,status FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({outstanding_minor:0,status:'paid'});
  const kindOfDebt=kind==='sales'?'receivable':'payable';
  if(process.env.ERPX_FINANCE_PACKAGE_ENFORCEMENT==='enforced')await expect(aging(f.manager,kindOfDebt,'2026-09-02',f.chart)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  else{
  const beforePayment=(await aging(f.manager,kindOfDebt,'2026-09-02',f.chart)).find(i=>i.id===invoiceId);
  expect(beforePayment?.outstanding).toBe((20+Math.round(2000*taxBps/10000)/100).toFixed(2));
  const beforeReturn=(await aging(f.manager,kindOfDebt,'2026-09-03',f.chart)).find(i=>i.id===invoiceId);
  expect(beforeReturn?.outstanding??"0.00").toBe((20+Math.round(2000*taxBps/10000)/100-Number(paid)).toFixed(2));
  expect((await aging(f.manager,kindOfDebt,returnDate,f.chart)).some(i=>i.id===invoiceId)).toBe(false);
  }
  const control=await db.prepare("SELECT CASE WHEN kind='receivable' THEN debit_account_id ELSE credit_account_id END id FROM finance_invoices WHERE id=?").bind(invoiceId).first<string>('id');
  await db.prepare("UPDATE finance_accounts SET is_control=1,control_kind=? WHERE id=?").bind(kindOfDebt,control).run();
  const period=await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first<Record<string,string|number|null>>();
  const reconciliation=await calculateCloseReconciliations(db,f.manager,{...period!,end_date:returnDate});
  expect(reconciliation.rows.find(r=>r.accountId===control)).toMatchObject({sourceMinor:-Number(paid)*100,differenceMinor:0});
  expect(await db.prepare('SELECT COUNT(*) n FROM finance_refund_obligations WHERE return_id=?').bind(requested.id).first('n')).toBe(1);
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first()).toEqual(kind==='sales'?{quantity_milli:2000,value_minor:1200}:{quantity_milli:0,value_minor:0});
  if(kind==='sales'){
    expect(await db.prepare("SELECT status,source_type,source_id,held_quantity_milli,quantity_scope FROM inventory_quality_holds WHERE company_id=? AND source_type='customer_return' AND source_id=?").bind(f.company,`${requested.id}-0`).first()).toEqual({status:'held',source_type:'customer_return',source_id:`${requested.id}-0`,held_quantity_milli:2000,quantity_scope:'partial'});
    expect(await db.prepare('SELECT inventory_quarantined_quantity(?,?) quantity').bind(warehouse.id,product.id).first('quantity')).toBe(2000);
    await expect(db.prepare('UPDATE commerce_return_lines SET condition_note=? WHERE return_id=?').bind('Rewrite condition',requested.id).run()).rejects.toThrow('RETURN_QUALITY_EVIDENCE_IMMUTABLE');
    const holdId=String(await db.prepare("SELECT id FROM inventory_quality_holds WHERE source_type='customer_return' AND source_id=?").bind(`${requested.id}-0`).first('id'));
    const {inspectQualityHold,releaseQualityHold}=await import("../../../src/server/modules/quality/public/quality");
    await inspectQualityHold(f.owner,holdId,{expectedVersion:1,result:'pass',evidence:'Packaging and functional check passed after customer return',measurements:[],attachmentIds:[],attachmentSetIds:[]},key());
    f.tie.role='quality_manager';f.tie.assignmentId=await f.grant(f.tie,'quality_manager');
    await releaseQualityHold(f.tie,holdId,{expectedVersion:2,reason:'Independent review confirms the returned units are resalable'},key());
    expect(await db.prepare('SELECT inventory_quarantined_quantity(?,?) quantity').bind(warehouse.id,product.id).first('quantity')).toBe(0);
    expect((await getOrder(f.owner,orderId)).returnQualityHolds[0]).toMatchObject({id:holdId,status:'released',condition_code:'unknown'});
    const {customerTimeline}=await import("../../../src/server/modules/customer_360/public/customer-timeline");
    const timeline=await customerTimeline(f.owner,String(party.id));
    expect(timeline.items.find(e=>e.source_type==='fulfillment')).toMatchObject({source_id:String(fulfillment.fulfillmentId),href:`/commerce/orders/${orderId}`});
    expect(timeline.items.find(e=>e.source_type==='return')).toMatchObject({source_id:String(requested.id),status:'posted',href:`/commerce/orders/${orderId}`});
    expect(JSON.stringify(timeline)).not.toContain(refundId);
  }
  return {refundId,invoiceId,requested,warehouse,product,settlement,orderId,fulfillmentId:String(fulfillment.fulfillmentId),goodsJournal,approved};
}
it.each(['sales','purchase'] as const)('retains the original VAT and records a signed October %s return in its own tax month',async kind=>{
  const source=await paidReturn(kind,'15',700,'2026-10-02'),actor={...f.owner,branchId:null,departmentId:null};
  const {prepareAccountingProfile,decideAccountingProfile}=await import("../../../src/server/modules/finance/public/accounting-profiles");
  const p=await prepareAccountingProfile(actor,{statutoryChartId:f.chart,standard:'TFRS for NPAEs',fiscalStartMonth:1,effectiveDate:'2026-01-01',eligibilityEvidence:'Fixture entity eligibility reviewed for NPAEs',policyEvidence:'Fixture VAT source mapping and deduction policy reviewed',taxBasis:'current_tax',inventoryPolicy:'moving_average',depreciationPolicy:'straight_line'},key());
  await decideAccountingProfile(f.manager,p.id,'approve',{expectedVersion:1,reason:'Independent NPAEs and statutory ledger approval'},key());
  const {prepareVatEntry,reviewVatEntry,vatWorkspace,prepareVatPackage,prepareVatCorrection,reviewVatCorrection,vatCorrectionSchema,vatCorrectionDetail,approveVatPackage,vatPackageDetail}=await import("../../../src/server/modules/finance/public/finance-tax-register");
  const facts={taxPointDate:'2026-09-02',claimMonth:'2026-09',documentReference:'ORIGINAL-FIXTURE-VAT',treatment:'standard' as const,deductibleBps:10000,legalBasis:'Reviewed VAT for original goods and the referenced return note',taxPointEvidence:'Original goods delivered on September 2; document and journal evidence reviewed',documentEvidence:'Fictional original VAT document retained only in isolated fixture'};
  const original=await prepareVatEntry(actor,{...facts,sourceType:'invoice',sourceId:source.invoiceId,expectedSourceVersion:Number(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(source.invoiceId).first('version'))},key());
  await reviewVatEntry(f.manager,original.id,'approve',{expectedVersion:1,reason:'Reviewed the original full VAT point and source tax lines'},key());
  const note=await prepareVatEntry(actor,{...facts,sourceType:'commerce_return',sourceId:String(source.requested.id),expectedSourceVersion:3,taxPointDate:'2026-10-02',claimMonth:'2026-10',documentReference:'RETURN-FIXTURE-VAT'},key());
  expect(note).toMatchObject({taxMinor:-140,deductibleMinor:-140});
  await reviewVatEntry(f.manager,note.id,'approve',{expectedVersion:1,reason:'Reviewed referenced original, October return and signed VAT lines'},key());
  const sept=await vatWorkspace(f.manager,{filingMonth:'2026-09'}),oct=await vatWorkspace(f.manager,{filingMonth:'2026-10'});
  expect(sept.entries).toEqual(expect.arrayContaining([expect.objectContaining({id:original.id,tax_minor:140})]));
  expect(oct.entries).toEqual(expect.arrayContaining([expect.objectContaining({id:note.id,tax_minor:-140,original_entry_id:original.id})]));
  expect(sept.reconciliation?.ready).toBe(true);expect(oct.reconciliation?.ready).toBe(true);
  const pack=await prepareVatPackage(actor,{chartId:f.chart,filingMonth:'2026-10',filingKind:'original',reason:'Prepare only the October return while preserving September VAT'},key());
  expect(kind==='sales'?pack.result.outputVatMinor:pack.result.inputVatMinor).toBe(-140);
  const previous=(await db.prepare('SELECT * FROM finance_tax_entries WHERE id IN (?,?) ORDER BY id').bind(original.id,note.id).all()).results;
  const correctionInput=vatCorrectionSchema.parse({...facts,expectedEntryVersion:2,expectedSourceVersion:Number(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(source.invoiceId).first('version')),documentReference:'CORRECTED-ORIGINAL-FIXTURE',treatment:kind==='purchase'?'allocated':'standard',deductibleBps:kind==='purchase'?5000:10000,reason:'Reviewed correction must update the original and its linked return together'});
  await expect(prepareVatCorrection(actor,original.id,{...correctionInput,taxPointDate:'2026-11-01',claimMonth:'2026-11'},key())).rejects.toMatchObject({code:'RETURN_TAX_DATE_INVALID'});
  let correction=await prepareVatCorrection(actor,original.id,correctionInput,key());expect(correction.entryCount).toBe(2);
  if(kind==='purchase'){
    // A later return revision must stale the whole family, even with equal GL.
    await db.prepare('UPDATE commerce_returns SET version=version+1 WHERE id=?').bind(source.requested.id).run();
    await expect(reviewVatCorrection(f.manager,correction.id,'approve',{expectedVersion:1,reason:'The referenced return changed after family preparation'},key())).rejects.toMatchObject({code:'VAT_CORRECTION_STALE'});
    await reviewVatCorrection(f.manager,correction.id,'reject',{expectedVersion:1,reason:'Retain stale family evidence and prepare all current return revisions'},key());
    correction=await prepareVatCorrection(actor,original.id,correctionInput,key());
  }
  const detail=await vatCorrectionDetail(f.manager,correction.id);
  expect(detail.entries.map(e=>[e.previous.id,e.replacement.tax_minor,e.replacement.deductible_minor])).toEqual([[original.id,140,kind==='purchase'?70:140],[note.id,-140,kind==='purchase'?-70:-140]]);
  await expect(db.batch([
    db.prepare("UPDATE finance_tax_entries SET status='approved',version=2,reviewed_by=?,review_reason='Incomplete family review fixture',reviewed_at=1 WHERE id=?").bind(f.manager.userId,detail.entries[0].replacement.id),
    db.prepare("UPDATE finance_vat_corrections SET status='approved',version=2,reviewed_by=?,review_reason='Incomplete family review fixture',reviewed_at=1 WHERE id=?").bind(f.manager.userId,correction.id),
  ])).rejects.toThrow('VAT_CORRECTION_NOT_ATOMIC');
  expect((await vatCorrectionDetail(f.manager,correction.id)).entries.every(e=>e.replacement.status==='prepared')).toBe(true);
  await reviewVatCorrection(f.manager,correction.id,'approve',{expectedVersion:1,reason:'Independent review of original and signed return deduction replacements'},key());
  expect((await db.prepare('SELECT * FROM finance_tax_entries WHERE id IN (?,?) ORDER BY id').bind(original.id,note.id).all()).results).toEqual(previous);
  const currentSept=await vatWorkspace(f.manager,{filingMonth:'2026-09'}),currentOct=await vatWorkspace(f.manager,{filingMonth:'2026-10'});
  expect(currentSept.entries).toHaveLength(1);expect(currentOct.entries).toHaveLength(1);
  expect(currentOct.entries[0]).toMatchObject({original_entry_id:currentSept.entries[0].id,deductible_minor:kind==='purchase'?-70:-140});
  expect(currentOct.reconciliation).toMatchObject({ready:true,pendingCorrections:0});
  await expect(approveVatPackage(f.manager,pack.id,{expectedVersion:1,reason:'Earlier return package is stale after the linked correction'},key())).rejects.toMatchObject({code:'VAT_PACKAGE_STALE'});
  expect((await vatPackageDetail(f.manager,pack.id)).snapshot.register[0].id).toBe(note.id);
});
async function approveObligation(id:string) {
 const {approveRefund}=await import("../../../src/server/modules/finance/public/refunds");
 const input={expectedVersion:1,beneficiaryName:'Simulated counterparty',beneficiaryAccount:'TEST-BANK-123',reason:'Checked original invoice, return and beneficiary independently'};
 await expect(approveRefund(f.manager,id,input,key())).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});
 await expect(approveRefund(f.owner,id,input,key())).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});
 f.head.role='finance_approver';f.head.assignmentId=await f.grant(f.head,'finance_approver');
 await approveRefund(f.head,id,input,key());
}
async function prepare(id:string,amount:string) {
 const {prepareRefundInstruction}=await import("../../../src/server/modules/finance/public/refunds");
 const version=Number(await db.prepare('SELECT version FROM finance_refund_obligations WHERE id=?').bind(id).first('version'));
 return prepareRefundInstruction(f.payer,id,{expectedVersion:version,amount,plannedDate:'2026-09-05',bankAccount:f.accounts.cash,cashAccountId:f.accounts.cash},key());
}
async function bankEvidence(amount:string) {
 const {importBankStatement}=await import("../../../src/server/modules/finance/public/finance");
 const statement=await importBankStatement(f.owner,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:'2026-09-05',opening:'100',closing:(100+Number(amount)).toFixed(2),sourceHash:key(),rows:[{externalId:key(),transactionDate:'2026-09-05',description:'Simulation bank evidence',amount,currency:'THB'}]},key());
 const bank=await db.prepare('SELECT * FROM finance_bank_transactions WHERE statement_id=?').bind(statement.id).first<{id:string;version:number}>();
 expect(bank).toBeTruthy();return bank!;
}
it.each(['sales','purchase'] as const)('separates invoice credit from %s cash refund and reconciles partial/final payments exactly once',async(kind)=>{
 const {approveRefund,prepareRefundInstruction,exportRefundInstructions,confirmRefundInstruction,refundWorkspace,refundWork,recoverRefundExport}=await import("../../../src/server/modules/finance/public/refunds");
 const source=await paidReturn(kind),id=source.refundId;
 await expect(prepare(id,'15')).rejects.toBeDefined();
 await approveObligation(id);
 const before=await refundWorkspace(f.payer,id);
 expect(before.owner).toMatchObject({amount_minor:1500,outstanding_minor:1500,status:'approved',direction:kind==='sales'?'outgoing':'incoming'});
 await expect(prepareRefundInstruction(f.head,id,{expectedVersion:2,amount:'1',plannedDate:'2026-09-05',bankAccount:f.accounts.cash,cashAccountId:f.accounts.cash},key())).rejects.toMatchObject({code:'INDEPENDENT_PAYER_REQUIRED'});
 expect((await refundWork(f.payer)).find(row=>row.id===id)).toMatchObject({queue:'todo',canAct:true});
 const {listStaffNotifications}=await import("../../../src/server/modules/collaboration/public/staff-notifications");
 expect((await listStaffNotifications(db,f.payer)).some(note=>note.record_type==='refund'&&note.record_id===id)).toBe(true);
 expect((await listStaffNotifications(db,{...f.payer,role:'employee'})).some(note=>note.record_id===id)).toBe(false);
 await expect(refundWorkspace({...f.payer,companyId:'unrelated-company'},id)).rejects.toMatchObject({status:404});
 await expect(refundWorkspace({...f.payer,branchId:'unrelated-branch'},id)).rejects.toMatchObject({status:404});
 const first=await prepare(id,'6'),instructionId=String(first.id),exportKey=key(),exportInput={instructions:[{id:instructionId,expectedVersion:1}]};
 const {calendarData}=await import("../../../src/server/modules/work_projection/public/calendar/index");
 const calendar=await calendarData(db,f.payer,{date:'2026-09-05',view:'day',layers:'refunds',mode:'mine'});
 expect(calendar.sources).toEqual([{source:'refunds',status:'ready'}]);
 expect(calendar.items.map(item=>item.sourceId)).toContain(instructionId);
 expect((await calendarData(db,{...f.payer,role:'employee'},{date:'2026-09-05',view:'day',layers:'refunds'})).items).toEqual([]);
 const exported=await exportRefundInstructions(f.payer,exportInput,exportKey);
 expect(await exportRefundInstructions(f.payer,exportInput,exportKey)).toEqual(exported);
 expect(await recoverRefundExport(f.payer,instructionId)).toMatchObject({content:exported.content,sha256:exported.sha256});
 const {createHash}=await import('node:crypto');expect(exported.sha256).toBe(createHash('sha256').update(String(exported.content)).digest('hex'));
 expect((await refundWorkspace(f.payer,id)).owner.outstanding_minor).toBe(1500);
 const refundBeforeConfirm=await refundWorkspace(f.payer,id),proof=await seedAttachmentSet(db,f.payer,{targetType:'finance_refund_obligation',targetId:id,componentKind:'action',componentRef:`refund-instruction:${instructionId}`,purposeKey:'refund_proof',expectedTargetVersion:Number(refundBeforeConfirm.owner.version)});
 const bank=await bankEvidence(kind==='sales'?'-6':'6'),confirmation={expectedVersion:2,bankTransactionId:bank.id,bankVersion:bank.version,periodId:f.period,bankReference:'SIMULATED-BANK-REF-1',evidence:'Imported statement amount and beneficiary checked',attachmentSetIds:[proof.setId]};
 await expect(confirmRefundInstruction(f.head,instructionId,confirmation,key())).rejects.toMatchObject({code:'INDEPENDENT_PAYER_REQUIRED'});
 await db.prepare("UPDATE finance_periods SET state='closed' WHERE id=?").bind(f.period).run();
 await expect(confirmRefundInstruction(f.payer,instructionId,confirmation,key())).rejects.toMatchObject({code:'PERIOD_CLOSED'});
 expect((await refundWorkspace(f.payer,id)).owner.outstanding_minor).toBe(1500);
 await db.prepare("UPDATE finance_periods SET state='open' WHERE id=?").bind(f.period).run();
 const confirmKey=key(),confirmed=await confirmRefundInstruction(f.payer,instructionId,confirmation,confirmKey);
 expect(await confirmRefundInstruction(f.payer,instructionId,confirmation,confirmKey)).toEqual(confirmed);
 expect(await attachmentBindingState(db,proof.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'finance_refund_confirmation'});
 expect((await refundWorkspace(f.payer,id)).owner.outstanding_minor).toBe(900);
 await expect(confirmRefundInstruction(f.payer,instructionId,{...confirmation,expectedVersion:3},key())).rejects.toMatchObject({code:'REFUND_NOT_EXPORTED'});
 const second=await prepare(id,'9');await expect(prepare(id,'0.01')).rejects.toMatchObject({status:409});
 await exportRefundInstructions(f.payer,{instructions:[{id:String(second.id),expectedVersion:1}]},key());
 await expect(confirmRefundInstruction(f.payer,String(second.id),{...confirmation,expectedVersion:2},key())).rejects.toMatchObject({code:'BANK_EVIDENCE_MISMATCH'});
 const finalBank=await bankEvidence(kind==='sales'?'-9':'9');await confirmRefundInstruction(f.payer,String(second.id),{...confirmation,bankTransactionId:finalBank.id,bankVersion:finalBank.version,bankReference:'SIMULATED-BANK-REF-2',attachmentSetIds:[]},key());
 const done=await refundWorkspace(f.payer,id);expect(done.owner).toMatchObject({status:'settled',outstanding_minor:0});expect(done.instructions.every(row=>row.status==='confirmed')).toBe(true);
 expect((await refundWork(f.payer)).find(row=>row.id===id)?.queue).toBe('completed');
 const sums=await db.prepare('SELECT SUM(l.debit_minor-l.credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status=\'posted\' AND l.account_id=?').bind(f.company,done.owner.balance_account_id).first('balance');expect(sums).toBe(0);
 expect(await db.prepare("SELECT COUNT(*) n FROM finance_bank_transactions WHERE company_id=? AND match_status='matched'").bind(f.company).first('n')).toBe(2);
 // Reapproval cannot mutate the beneficiary after export or settlement.
 await expect(approveRefund(f.head,id,{expectedVersion:Number(done.owner.version),beneficiaryName:'Changed',beneficiaryAccount:'OTHER',reason:'Must not overwrite approved instructions'},key())).rejects.toMatchObject({status:409});
});
it('holds uncertain payments, requires independent failure review, and serializes competing replacement instructions',async()=>{
 const {recordRefundException,exportRefundInstructions,prepareRefundInstruction,refundWorkspace}=await import("../../../src/server/modules/finance/public/refunds");
 const {refundId:id}=await paidReturn('sales','20');await approveObligation(id);
 const first=await prepare(id,'20');await exportRefundInstructions(f.payer,{instructions:[{id:String(first.id),expectedVersion:1}]},key());
 await expect(recordRefundException(f.payer,String(first.id),{expectedVersion:2,outcome:'cancelled',evidence:'Cannot cancel an exported transfer'},key())).rejects.toMatchObject({code:'EXPORTED_REFUND_REQUIRES_REVIEW'});
 await recordRefundException(f.payer,String(first.id),{expectedVersion:2,outcome:'unknown',evidence:'Bank timed out after submission; outcome unknown'},key());
 await expect(prepare(id,'20')).rejects.toMatchObject({status:409});
 const {refundWork}=await import("../../../src/server/modules/finance/public/refunds");
 expect((await refundWork(f.manager)).find(item=>item.id===id)).toMatchObject({canAct:true,responsibleQueue:'Independent Finance approver'});
 const {beginClose,closePeriod}=await import("../../../src/server/modules/finance/public/finance");
 await expect(beginClose(f.owner,f.period,key())).rejects.toMatchObject({code:'CLOSE_NOT_READY'});
 await expect(recordRefundException(f.payer,String(first.id),{expectedVersion:3,outcome:'failed',evidence:'Payer cannot independently review failure'},key())).rejects.toMatchObject({status:403});
 await recordRefundException(f.manager,String(first.id),{expectedVersion:3,outcome:'failed',evidence:'Independent bank statement and rejection reference confirm no transfer'},key());
 const owner=(await refundWorkspace(f.payer,id)).owner,input={expectedVersion:Number(owner.version),amount:'20',plannedDate:'2026-09-05',bankAccount:f.accounts.cash,cashAccountId:f.accounts.cash};
 const results=await Promise.allSettled([prepareRefundInstruction(f.payer,id,input,key()),prepareRefundInstruction(f.payer,id,input,key())]);
 expect(results.filter(result=>result.status==='fulfilled')).toHaveLength(1);
 const current=await refundWorkspace(f.payer,id);expect(current.owner.outstanding_minor).toBe(2000);expect(current.instructions.filter(row=>row.status==='prepared')).toHaveLength(1);
 const pending=current.instructions.find(row=>row.status==='prepared')!;
 await recordRefundException(f.payer,String(pending.id),{expectedVersion:1,outcome:'cancelled',evidence:'Cancel unused instruction before any bank submission'},key());
 expect((await refundWorkspace(f.payer,id)).instructions.filter(row=>row.status==='prepared')).toHaveLength(0);
 expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE company_id=? AND source_type='return_refund'").bind(f.company).first('n')).toBe(0);
 const closing=await beginClose(f.owner,f.period,key());
 await bankEvidence('-1');
 await expect(closePeriod(f.head,f.period,{expectedVersion:Number(closing.version),proof:'Late bank data must block final close',attachmentSetIds:[]},key())).rejects.toMatchObject({status:409});
});

it('keeps tax in the full paid credit and creates exactly the gross refund obligation',async()=>{
 const {refundWorkspace}=await import("../../../src/server/modules/finance/public/refunds");
 const source=await paidReturn('sales','21.40',700),data=await refundWorkspace(f.manager,source.refundId);
 expect(data.owner).toMatchObject({amount_minor:2140,outstanding_minor:2140,status:'pending'});
 const note=await db.prepare('SELECT r.subtotal_minor,r.tax_minor,r.total_minor,r.credited_minor,r.refund_minor FROM commerce_returns r WHERE id=?').bind(source.requested.id).first();
 expect(note).toEqual({subtotal_minor:2000,tax_minor:140,total_minor:2140,credited_minor:0,refund_minor:2140});
 const tax=await db.prepare("SELECT SUM(l.debit_minor-l.credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted' AND l.account_id=?").bind(f.company,f.accounts.tax).first('balance');
 expect(tax).toBe(0);
});


it.each(['sales','purchase'] as const)('keeps September %s balances after a full October return',async kind=>{
  const source=await paidReturn(kind,'15',0,'2026-10-04'),debtKind=kind==='sales'?'receivable':'payable';
  expect((await aging(f.manager,debtKind,'2026-09-30',f.chart)).find(row=>row.id===source.invoiceId)?.outstanding).toBe('5.00');
  const period=await db.prepare('SELECT * FROM finance_periods WHERE id=?').bind(f.period).first<Record<string,string|number|null>>();
  const september=await calculateCloseReconciliations(db,f.manager,{...period!,end_date:'2026-09-30'});
  expect(september.rows.find(row=>row.kind===debtKind)).toMatchObject({sourceMinor:500,ledgerMinor:500,differenceMinor:0});
});


it.each(['sales','purchase'] as const)('retains a real %s-only order through drained goods, invoice, settlement, return, credit and refund obligation with original evidence',async kind=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const packageId=kind==='sales'?'sales':'supply',store=packageGrantStore(db,{authorize:async()=>({userId:'fictional-commerce-return-provider'}),obligations:async()=>{throw new Error('NO_INACTIVE_CUTOVER_IN_RETURN_FIXTURE');}}),grant={companyId:f.company,packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Isolated actual Commerce return owner purchase',evidence:'test:commerce-drained-return'};await store.apply({expectedVersion:0,idempotencyKey:key(),grant});
 const result=await paidReturn(kind,'15',0,'2026-09-04',async()=>{await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});});expect(result.requested.id).toBeTruthy();expect((await readCompanyPackageGrants(db,f.company)).map(value=>[value.packageId,value.state])).toEqual([[packageId,'draining']]);expect(await db.prepare('SELECT status,outstanding_minor FROM finance_refund_obligations WHERE id=?').bind(result.refundId).first()).toMatchObject({status:'pending',outstanding_minor:1500});
});

it.each(['sales','purchase'] as const)('uses only actual %s owner Finance sources for independent posting role entry and retained Files while both package boundaries are enforced',async kind=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const packageId=kind==='sales'?'sales':'supply',store=packageGrantStore(db,{authorize:async()=>({userId:'fictional-included-finance-review'}),obligations:async()=>{throw new Error('No owner inactive witness is invented');}}),grant={companyId:f.company,packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Actual retained Sales Supply Finance handoff',evidence:'test:commerce-finance-source'};await store.apply({expectedVersion:0,idempotencyKey:key(),grant});
 let book!:Awaited<ReturnType<typeof prepareCommerceProjection>>;
 const result=await paidReturn(kind,'15',0,'2026-09-04',async()=>{book=await prepareCommerceProjection(f,kind);await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');});
 await assertCommerceFinanceHistory(f,kind,result,book);
},60_000);

it('withdraws actual Sales only after goods return quality independent credits cash refund and reviewed secondary books settle while held source history survives',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const scope={companyId:f.company,packageId:'sales',packageVersion:1},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-actual-sales-withdrawal-provider'}),obligations:async()=>{throw Error('ONLY_NORMAL_SOURCE_REVIEW_CAN_INACTIVATE');}}),grant={...scope,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Actual isolated Sales return source purchase',evidence:'test:sales-withdrawal-delivery-money'};await store.apply({expectedVersion:0,idempotencyKey:key(),grant});let book!:Awaited<ReturnType<typeof prepareCommerceProjection>>;
 const result=await paidReturn('sales','15',0,'2026-09-04',async()=>{book=await prepareCommerceProjection(f,'sales');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});}),{withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),{packageGrantRevision}=await import('../../../src/server/modules/commercial/public/package-grants'),observation=()=>withOwnerPackageObligations(db,scope,tx=>readOwnerPackageObligations(db,tx,{...scope,now:Date.now()}));
 const open=await observation();expect(open.observations.find(row=>row.moduleId==='finance')?.outstanding).toBeGreaterThan(0);const grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==='sales')! ,request={companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'All actual source money stock and provider duties independently reviewed',evidence:'test:sales-four-owner-real-goods'}}]},reviewStore=packageGrantStore(db,{authorize:async()=>({userId:'isolated-actual-sales-withdrawal-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)}),review=(apply=false)=>withOwnerPackageObligations(db,scope,tx=>apply?reviewStore.batch.applyIn(tx,request,async()=>{}):reviewStore.batch.previewIn(tx,request));await expect(review()).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await assertCommerceFinanceHistory(f,'sales',result,book);const closed=await observation();expect(closed.observations).toEqual(expect.arrayContaining(['sales','crm','inventory','finance'].map(moduleId=>expect.objectContaining({moduleId,outstanding:0}))));expect(closed.total).toBe(0);const actualHold=await db.prepare("SELECT id,source_type FROM inventory_quality_holds WHERE company_id=? AND source_type='customer_return' ORDER BY id LIMIT 1").bind(f.company).first<{id:string;source_type:string}>();if(!actualHold)throw Error('ACTUAL_RETURN_QUARANTINE_HISTORY_REQUIRED');await db.prepare("UPDATE inventory_quality_holds SET source_type='manual' WHERE id=?").bind(actualHold.id).run();expect((await observation()).observations.find(row=>row.moduleId==='inventory')?.outstanding).toBeGreaterThan(0);await expect(review()).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await db.prepare('UPDATE inventory_quality_holds SET source_type=? WHERE id=?').bind(actualHold.source_type,actualHold.id).run();expect((await observation()).total).toBe(0);expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['sales']);const history=await db.prepare('SELECT count(*) count FROM attachments WHERE legal_hold=1').first('count'),applied=await review(true);expect(await review(true)).toEqual(applied);expect(await db.prepare('SELECT count(*) count FROM attachments WHERE legal_hold=1').first('count')).toBe(history);expect(await db.prepare('SELECT status,outstanding_minor FROM finance_refund_obligations WHERE id=?').bind(result.refundId).first()).toEqual({status:'settled',outstanding_minor:0});expect(await db.prepare("SELECT state FROM company_package_grants WHERE company_id=? AND package_id='sales'").bind(f.company).first('state')).toBe('inactive');const {attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service'),heldLink=await db.prepare("SELECT link.id FROM attachment_links link JOIN attachments source ON source.id=link.attachment_id WHERE source.company_id=? AND source.legal_hold=1 AND link.state='bound' LIMIT 1").bind(f.company).first<string>('id');if(!heldLink)throw Error('ACTUAL_HELD_SALES_JOURNAL_FILE_REQUIRED');expect(await(await attachmentContent(db,env.H001_FILES,f.manager,heldLink,'original')).object.text()).toBe('%PDF-1.7');
},60_000);

it.each(['sales','purchase'] as const)('fresh %s private document bytes keep actual original order receipt return and independent Finance sources during package-only draining',async(kind)=>{
 const source=await paidReturn(kind),{attachmentContent,bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service'),orderType=kind==='sales'?'commerce_order':'purchase_order',orderVersion=Number(await db.prepare('SELECT version FROM commerce_orders WHERE id=?').bind(source.orderId).first('version')),orderFile=await seedAttachmentSet(db,f.owner,{targetType:orderType,targetId:source.orderId,purposeKey:kind==='sales'?'customer_brief':'order_document',componentKind:'record',expectedTargetVersion:orderVersion});await bindRecordAttachmentSet(db,f.owner,orderFile.setId,orderVersion,key());
 const fulfillmentVersion=Number(await db.prepare('SELECT version FROM commerce_fulfillments WHERE id=?').bind(source.fulfillmentId).first('version')),returnId=String(source.requested.id),returnVersion=Number(await db.prepare('SELECT version FROM commerce_returns WHERE id=?').bind(returnId).first('version')),fulfillmentFile=await seedAttachmentSet(db,f.owner,{targetType:'commerce_fulfillment',targetId:source.fulfillmentId,purposeKey:'return_condition',componentRef:'return-request',expectedTargetVersion:fulfillmentVersion}),returnFile=await seedAttachmentSet(db,f.owner,{targetType:'commerce_return',targetId:returnId,purposeKey:'return_document',componentRef:'return-review',expectedTargetVersion:returnVersion}),documents=[orderFile,fulfillmentFile,returnFile];
 for(const [index,file] of documents.entries()){const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,`Exact ${kind} private original document ${index}`,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();}
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');await expect(attachmentContent(db,env.H001_FILES,f.owner,orderFile.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const packageId=kind==='sales'?'sales':'supply',grant={companyId:f.company,packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Only actual native Commerce private source history',evidence:'test:commerce-private-source'},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-commerce-private-provider'}),obligations:async()=>{throw Error('COMMERCE_FILE_FIXTURE_CANNOT_INACTIVATE');}});await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});
 const partyId=String(await db.prepare('SELECT party_id FROM commerce_orders WHERE id=?').bind(source.orderId).first('party_id')),accountId=String(await db.prepare('SELECT account_id FROM finance_journal_lines WHERE journal_id=? ORDER BY id LIMIT 1').bind(source.goodsJournal).first('account_id')),accountName=await db.prepare('SELECT name FROM finance_accounts WHERE id=?').bind(accountId).first('name');let consumed=0;const files:import('../../../src/server/modules/runtime/public/runtime/types').FileStore={get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;},put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
 for(const [index,file] of documents.entries()){expect(await(await attachmentContent(db,files,f.owner,file.linkId,'original')).object.text()).toBe(`Exact ${kind} private original document ${index}`);await expect(db.transaction(tx=>attachmentContent(tx,files,f.owner,file.linkId,'original'))).rejects.toMatchObject({code:'COMMERCE_FILE_READ_TRANSACTION_REQUIRED'});for(const fault of ['order','party','finance','iam'] as const){const trigger=`commerce_byte_${key().replaceAll('-','')}`,effect=fault==='order'?`UPDATE commerce_orders SET due_date='2026-12-31' WHERE id='${source.orderId}'`:fault==='party'?`UPDATE finance_parties SET name=name||' unexpected source edit' WHERE id='${partyId}'`:fault==='finance'?`UPDATE finance_accounts SET name=name||' unexpected source edit' WHERE id='${accountId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}'`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{await expect(attachmentContent(db,files,f.owner,file.linkId,'original')).rejects.toMatchObject({code:fault==='iam'?'ATTACHMENT_FILE_READ_CHANGED':'COMMERCE_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(await db.prepare('SELECT due_date FROM commerce_orders WHERE id=?').bind(source.orderId).first('due_date')).toBe('2026-09-03');expect(await db.prepare('SELECT name FROM finance_parties WHERE id=?').bind(partyId).first('name')).toBe('Refund fixture counterparty');expect(await db.prepare('SELECT name FROM finance_accounts WHERE id=?').bind(accountId).first('name')).toBe(accountName);expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first()).toEqual({revoked_at:null,version:1});}expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(file.linkId).first('n'))).toBe(1);expect(await(await attachmentContent(db,files,f.owner,file.linkId,'original')).object.text()).toBe(`Exact ${kind} private original document ${index}`);expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});}
 expect(consumed).toBe(18);expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual([packageId]);await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.owner.assignmentId).run();await expect(attachmentContent(db,files,f.owner,orderFile.linkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(consumed).toBe(18);expect(await db.prepare("SELECT reason FROM attachment_access_events WHERE link_id=? AND outcome='denied' ORDER BY created_at DESC LIMIT 1").bind(orderFile.linkId).first('reason')).toBe('ATTACHMENT_TARGET_NOT_FOUND');
});
it('fresh neutral product evidence uses existing Stock history and retains the original actual POS catalog fallback without granting Sales or Supply management',async()=>{
 const {attachmentContent,bindRecordAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service'),product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Neutral shared product private source',price:'10'},key())).id),spec=await seedAttachmentSet(db,f.owner,{targetType:'product',targetId:product,purposeKey:'specification',componentKind:'record',expectedTargetVersion:1});await bindRecordAttachmentSet(db,f.owner,spec.setId,1,key());const image=await seedAttachmentSet(db,f.owner,{targetType:'product',targetId:product,purposeKey:'product_image',componentKind:'record',expectedTargetVersion:2,mediaType:'image/png'});await bindRecordAttachmentSet(db,f.owner,image.setId,2,key());for(const file of [spec,image]){const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Retained exact neutral product private source',{httpMetadata:{contentType:file===image?'image/png':'application/pdf'}});}
 const ordinaryWarehouse=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Ordinary Sales source remains separately purchased'},key()),ordinaryCustomer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Ordinary separately purchased Sales customer'},key()),ordinarySale=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:ordinaryCustomer.id,warehouseId:ordinaryWarehouse.id,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product,quantity:'1',price:'10',taxBps:0}]}),key());
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(attachmentContent(db,env.H001_FILES,f.owner,spec.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const grant={companyId:f.company,packageId:'pos',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Only original included Stock product reference',evidence:'test:product-included-stock'},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-product-files-provider'}),obligations:async()=>{throw Error('PRODUCT_FILE_FIXTURE_CANNOT_INACTIVATE');}});await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});expect(await(await attachmentContent(db,env.H001_FILES,f.owner,spec.linkId,'original')).object.text()).toBe('Retained exact neutral product private source');await expect(getOrder(f.owner,String(ordinarySale.id))).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const {countsPage}=await import('../../../src/server/modules/inventory/public/inventory-counts');await expect(countsPage(f.owner,{})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const cashier={...f.reviewer,role:'pos_cashier',modules:['pos'],branchId:null,departmentId:null};cashier.assignmentId=await f.grant(cashier,'pos_cashier');await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind('["pos"]',cashier.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,cashier,image.linkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});await db.prepare("INSERT INTO pos_catalog(product_id,company_id,kind) VALUES(?,?,'stock')").bind(product,f.company).run();expect(await(await attachmentContent(db,env.H001_FILES,cashier,image.linkId,'original')).object.text()).toBe('Retained exact neutral product private source');const trigger=`product_pos_byte_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE pos_catalog SET category='Changed during private read',version=version+1 WHERE product_id='${product}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${image.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{await expect(attachmentContent(db,env.H001_FILES,cashier,image.linkId,'original')).rejects.toMatchObject({code:'COMMERCE_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(await db.prepare('SELECT category,version FROM pos_catalog WHERE product_id=?').bind(product).first()).toEqual({category:'',version:1});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['pos']);
});


// Sales-only Goods mapping: all original cases/assertions above are unchanged.
import type {WorkContext} from '../../../src/server/modules/identity_access/public/authorization';

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

async function actualSalesServiceBeforeGoods(noVat=false){
  const z=await zeroLedgerSalesCompany();await z.purchase();const refs=await zeroSalesReferences(z),api=refs.api;
  const draft=await api.prepareSalesPostingDefaults(z.reviewer,{...await zeroSalesEnvelope(z.reviewer),input:{...refs.defaults,taxAccountId:noVat?undefined:refs.defaults.taxAccountId}},key());
  const approved=await api.decideSalesPostingDefaults(z.manager,String(draft.id),{contextKey:(await zeroSalesEnvelope(z.manager)).contextKey,input:{expectedVersion:1,action:'approve',reason:'Original independent reviewer approves actual service defaults before the goods extension'}},key());
  const customer=await saveMaster(z.owner,'parties',null,{kind:'customer',code:key(),name:'Actual pre-existing service customer'},key()),service=await saveMaster(z.owner,'products',null,{sku:key(),name:'Actual pre-existing installed service',itemType:'service',price:'100',taxBps:noVat?0:700},key()),order=await saveOrder(z.owner,null,orderSchema.parse({kind:'sales',partyId:customer.id,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId:service.id,quantity:'1',price:'100',taxBps:noVat?0:700}]}),key()),orderId=String(order.id);
  await transitionOrder(z.owner,orderId,{expectedVersion:1,action:'submit'},key());await transitionOrder(z.salesManager,orderId,{expectedVersion:2,action:'approve'},key());await assignZeroSales(z,'order',orderId,'confirmed',z.owner);
  const lineId=String((await getOrder(z.owner,orderId)).lines[0].id),completion=await fulfillOrder(z.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-03',completionMode:'service',evidenceNote:'Original customer accepted the actual pre-existing service',lines:[{lineId,quantity:'1'}]}),key());
  await assignZeroSales(z,'order',orderId,'invoicing',z.owner);
  const invoiced=await invoiceFulfillment(z.owner,String(completion.fulfillmentId),{expectedVersion:1,date:'2026-09-04',dueDate:'2026-09-10'},key()),invoiceId=String(invoiced.invoiceId);
  await assignZeroSales(z,'invoice',invoiceId,'draft',z.manager);await postInvoice(z.manager,invoiceId,{expectedVersion:1,periodId:refs.period.id,entryDate:'2026-09-04'},key());await assignZeroSales(z,'invoice',invoiceId,'posted',z.treasury);
  const paid=await settleInvoice(z.treasury,invoiceId,{expectedVersion:2,date:'2026-09-05',amount:'50',bankAccountId:refs.accounts.cashAccountId,reference:'Actual service customer payment predates the Goods mapping'},key());
  const mapping={receivableAccountId:refs.accounts.receivableAccountId,revenueAccountId:refs.accounts.revenueAccountId,outputTaxAccountId:refs.accounts.taxAccountId,cashAccountId:refs.accounts.cashAccountId,payableAccountId:'',inventoryAccountId:'',clearingAccountId:'',cogsAccountId:'',inputTaxAccountId:''};
  for(const [field,category,controlKind] of [['payableAccountId','liability','payable'],['inventoryAccountId','asset','none'],['clearingAccountId','liability','none'],['cogsAccountId','expense','none'],['inputTaxAccountId','asset','none']] as const){
    const account=await api.createSalesPostingAccount(z.reviewer,{...await zeroSalesEnvelope(z.reviewer),input:{chartId:refs.chart.id,code:key(),name:`Actual initial Goods ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key());mapping[field]=account.id;
  }
  const serviceHistory=()=>Promise.all([...['commerce_orders','finance_charts','finance_accounts','finance_sales_defaults','finance_invoices','finance_journals','finance_customer_receipts','finance_financial_events','money_transaction_snapshots'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? ORDER BY image`).bind(z.company).all()).results),db.prepare('SELECT to_jsonb(r)::text image FROM commerce_fulfillments r JOIN commerce_orders o ON o.id=r.order_id WHERE o.company_id=? ORDER BY image').bind(z.company).all().then(rows=>rows.results),db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? ORDER BY image').bind(z.company).all().then(rows=>rows.results)]);
  expect(await db.prepare('SELECT count(*) n FROM commerce_settings WHERE company_id=?').bind(z.company).first('n')).toBe(0);
  expect(await db.prepare('SELECT count(*) n FROM commerce_warehouses WHERE company_id=?').bind(z.company).first('n')).toBe(0);
  return {z,refs,mapping,approved,orderId,invoiceId,fulfillmentId:String(completion.fulfillmentId),paymentId:String(paid.paymentId),serviceHistory};
}

async function salesGoodsInput(s:Awaited<ReturnType<typeof actualSalesServiceBeforeGoods>>){
  const {salesGoodsPostingSetup}=await import('../../../src/server/modules/commerce/public/sales-goods-posting-setup'),view=await salesGoodsPostingSetup(s.z.reviewer);
  expect(view.available).toBe(true);if(!view.contextKey)throw Error('Actual current Sales Goods setup context required');
  return {expectedVersion:0 as const,contextKey:view.contextKey,expectedSalesDefaults:view.expectedSalesDefaults,chartId:s.refs.chart.id,currency:'THB',reason:'Original Company reviewer extends compatible approved service defaults to initial Goods posting',...s.mapping};
}

it('Sales-only Goods mapping extends actual posted service history without buying Supply or rebinding its original accounts',async()=>{
  const api=await import('../../../src/server/modules/commerce/public/sales-goods-posting-setup');
  for(const noVat of [false,true]){
    const s=await actualSalesServiceBeforeGoods(noVat),view=await api.salesGoodsPostingSetup(s.z.reviewer),before=await s.serviceHistory(),body=await salesGoodsInput(s),command=key();
    expect(view).toMatchObject({available:true,configured:false,canConfigure:true,requiresApprovedDefaults:true,expectedSalesDefaults:{id:s.approved.id,version:2}});
    expect(await api.initializeSalesGoodsPostingSetup(s.z.reviewer,body,command)).toEqual({id:s.z.company,status:'configured',version:1});
    expect(await api.initializeSalesGoodsPostingSetup(s.z.reviewer,body,command)).toEqual({id:s.z.company,status:'configured',version:1});
    expect(await s.serviceHistory()).toEqual(before);
    expect(await db.prepare('SELECT tax_account_id FROM finance_sales_defaults WHERE id=?').bind(s.approved.id).first('tax_account_id')).toBe(noVat?null:s.mapping.outputTaxAccountId);
    const mapping=await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(s.z.company).first('image');
    await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,{...body,reason:'A different original reviewed command body'},command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
    await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,{...await salesGoodsInput(s),inventoryAccountId:s.mapping.inputTaxAccountId},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});
    expect(await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(s.z.company).first('image')).toBe(mapping);
    expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
    expect(await api.salesGoodsPostingSetup(s.z.reviewer)).toMatchObject({configured:true,canConfigure:false});
    await s.z.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...s.z.grant,state:'draining',version:2}});
    expect(await api.initializeSalesGoodsPostingSetup(s.z.reviewer,body,command)).toMatchObject({status:'configured'});
    await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,body,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
    await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(s.z.reviewer.assignmentId).run();
    await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,body,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
  }
});

it('Sales-only Goods mapping binds strict purchase current Company authority releases and exact reviewed defaults rather than arbitrary history',async()=>{
  const api=await import('../../../src/server/modules/commerce/public/sales-goods-posting-setup'),unlicensed=await zeroLedgerSalesCompany(),placeholder={expectedVersion:0 as const,contextKey:'0'.repeat(64),expectedSalesDefaults:null,chartId:f.chart,currency:'THB',reason:'Valid shaped initial mapping cannot acquire purchase authority',receivableAccountId:f.accounts.cash,payableAccountId:f.accounts.payable,revenueAccountId:f.accounts.revenue,inventoryAccountId:f.accounts.expense,clearingAccountId:f.accounts.payable,cogsAccountId:f.accounts.expense,inputTaxAccountId:f.accounts.tax,outputTaxAccountId:f.accounts.tax,cashAccountId:f.accounts.cash};
  expect(await api.salesGoodsPostingSetup(unlicensed.reviewer)).toMatchObject({available:false,contextKey:null,configuration:null,charts:[],accounts:[]});
  await expect(api.initializeSalesGoodsPostingSetup(unlicensed.reviewer,placeholder,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  const s=await actualSalesServiceBeforeGoods(),before=await s.serviceHistory(),body=await salesGoodsInput(s);
  for(const actor of [s.z.salesManager,await s.z.actor('goods-branch-preparer','finance_preparer','branch'),await s.z.actor('goods-department-preparer','finance_preparer','department')]){
    expect(await api.salesGoodsPostingSetup(actor)).toMatchObject({available:false,configuration:null,charts:[],accounts:[]});
    const command=key();await expect(api.initializeSalesGoodsPostingSetup(actor,body,command)).rejects.toMatchObject({status:403});expect(await zeroSalesCommandImage(command)).toEqual([[],[],[]]);
  }
  vi.stubEnv('NODE_ENV','production');
  try{await expect(api.salesGoodsPostingSetup(s.z.reviewer)).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,body,key())).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});}finally{vi.stubEnv('NODE_ENV','test');}
  for(const invalid of [{...body,payableAccountId:s.mapping.cashAccountId},{...body,payableAccountId:f.accounts.payable}])await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,invalid,key())).rejects.toMatchObject({code:'COMMERCE_POSTING_ACCOUNTS_INVALID'});
  for(const expectedSalesDefaults of [null,{id:String(s.approved.id),version:1}])await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,{...body,expectedSalesDefaults},key())).rejects.toMatchObject({code:'SALES_GOODS_POSTING_DEFAULTS_CHANGED'});
  expect(await s.serviceHistory()).toEqual(before);
  const alternate=await s.refs.api.createSalesPostingAccount(s.z.reviewer,{...await zeroSalesEnvelope(s.z.reviewer),input:{chartId:s.refs.chart.id,code:key(),name:'Different native reviewed receivable',category:'asset',currency:'THB',isControl:true,controlKind:'receivable'}},key());
  await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,body,key())).rejects.toMatchObject({code:'SALES_GOODS_POSTING_CONTEXT_CHANGED'});
  await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,{...await salesGoodsInput(s),receivableAccountId:alternate.id},key())).rejects.toMatchObject({code:'SALES_GOODS_POSTING_DEFAULTS_CHANGED'});
  const afterAlternate=await s.serviceHistory(),stale=await salesGoodsInput(s);await db.prepare('UPDATE assignments SET version=version+1 WHERE id=?').bind(s.z.reviewer.assignmentId).run();
  await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,stale,key())).rejects.toMatchObject({code:'SALES_GOODS_POSTING_CONTEXT_CHANGED'});
  expect(await s.serviceHistory()).toEqual(afterAlternate);
  // Imported orphan economics are a NEGATIVE fixture, never normal initial
  // stock acquisition or a successful Sales-only Goods workflow.
  const warehouse=await saveMaster(s.z.owner,'warehouses',null,{branchId:s.z.branch,code:key(),name:'Diagnostic imported legacy stock'},key()),product=await saveMaster(s.z.owner,'products',null,{sku:key(),name:'Diagnostic imported orphan stock',price:'1'},key());
  await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,1000,100,1)').bind(warehouse.id,product.id).run();
  expect(await api.salesGoodsPostingSetup(s.z.reviewer)).toMatchObject({available:true,configured:false,canConfigure:false,unavailableReason:'goods-history-requires-review'});
  await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,await salesGoodsInput(s),key())).rejects.toMatchObject({code:'SALES_GOODS_POSTING_HISTORY_REQUIRES_REVIEW'});
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(s.z.company).first()).toBeNull();
});

it('Sales-only Goods mapping rolls back late approved-default reference service-source authority and real purchase changes',async()=>{
  const s=await actualSalesServiceBeforeGoods(),api=await import('../../../src/server/modules/commerce/public/sales-goods-posting-setup');
  for(const kind of ['defaults','account','service','relation','authority','purchase'] as const){
    if(kind==='purchase')await s.z.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...s.z.grant,endsAt:Date.now()+1500,version:2,reason:'Actual Sales deadline during deferred initial Goods mapping',evidence:'test:goods-mapping-purchase-clock'}});
    const body=await salesGoodsInput(s),command=key(),fault=`goods_mapping_${key().replaceAll('-','')}`,before=await s.serviceHistory(),effect=kind==='defaults'?`UPDATE finance_sales_defaults SET reason=reason||' deferred change',version=version+1 WHERE id='${s.approved.id}'`:kind==='account'?`UPDATE finance_accounts SET name=name||' deferred change',version=version+1 WHERE id='${s.mapping.inventoryAccountId}'`:kind==='service'?`UPDATE commerce_orders SET due_date='2026-12-30' WHERE id='${s.orderId}'`:kind==='relation'?`UPDATE commerce_fulfillments SET invoice_id=NULL WHERE id='${s.fulfillmentId}'`:kind==='authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${s.z.reviewer.assignmentId}'`:'PERFORM pg_sleep(1.7)';
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,body,command)).rejects.toMatchObject({code:kind==='authority'?'QUEUED_AUTHORITY_CHANGED':kind==='purchase'?'PACKAGE_CAPABILITY_CHANGED':'SALES_GOODS_POSTING_SOURCE_CHANGED'});
      expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await s.serviceHistory()).toEqual(before);expect(await zeroSalesCommandImage(command)).toEqual([[],[],[]]);expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(s.z.company).first()).toBeNull();
    }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
});

it('Sales-only Goods mapping sees actual normal account writers before snapshots and serializes normal competing initializers without retry masking',async()=>{
  const s=await actualSalesServiceBeforeGoods(),api=await import('../../../src/server/modules/commerce/public/sales-goods-posting-setup'),{createAccount}=await import('../../../src/server/modules/finance/public/finance'),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),body=await salesGoodsInput(s),writerKey=key(),fault=`goods_writer_${key().replaceAll('-','')}`;
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${writerKey}') EXECUTE FUNCTION ${fault}()`);
  try{
    const measured=await measureReads(async()=>{
      const writer=createAccount(s.z.reviewer,{chartId:s.refs.chart.id,code:key(),name:'Normal original account writer before mapping snapshot',category:'asset',currency:'THB',isControl:false,controlKind:'none'},writerKey);let staged=false;
      for(let i=0;i<100;i++){staged=!!await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called');if(staged)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(staged).toBe(true);
      await expect(api.initializeSalesGoodsPostingSetup(s.z.reviewer,body,key())).rejects.toMatchObject({code:'SALES_GOODS_POSTING_CONTEXT_CHANGED'});await writer;
    });
    expect({deadlockFailures:measured.metrics.deadlockFailures??0,serializationFailures:measured.metrics.serializationFailures??0,transactionRetries:measured.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
  }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  const current=await salesGoodsInput(s),commands=[key(),key()],observed=await measureReads(()=>Promise.allSettled(commands.map(command=>api.initializeSalesGoodsPostingSetup(s.z.reviewer,current,command))));
  expect(observed.result.filter(row=>row.status==='fulfilled')).toHaveLength(1);expect(observed.result.filter(row=>row.status==='rejected')).toHaveLength(1);
  const winner=observed.result.findIndex(row=>row.status==='fulfilled'),result=observed.result[winner],rejection=observed.result.find(row=>row.status==='rejected');if(!result||result.status!=='fulfilled'||!rejection||rejection.status!=='rejected')throw Error('Exactly one actual initializer must commit');
  expect(rejection.reason).toMatchObject({code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});
  const replay=await measureReads(()=>Promise.all([api.initializeSalesGoodsPostingSetup(s.z.reviewer,current,commands[winner]),api.initializeSalesGoodsPostingSetup(s.z.reviewer,current,commands[winner])]));expect(replay.result).toEqual([result.value,result.value]);
  for(const reading of [observed,replay])expect({deadlockFailures:reading.metrics.deadlockFailures??0,serializationFailures:reading.metrics.serializationFailures??0,transactionRetries:reading.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
  expect(await zeroSalesCommandImage(commands[1-winner])).toEqual([[],[],[]]);
});


it('Sales-only Company service refund retains its actual original receipt cash without Goods mapping before preparing an instruction',async()=>{
  const s=await actualSalesServiceBeforeGoods();
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(s.z.company).first()).toBeNull();
  expect(await db.prepare('SELECT count(*)::int n FROM commerce_warehouses WHERE company_id=?').bind(s.z.company).first('n')).toBe(0);
  expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
  expect(await db.prepare('SELECT p.bank_account,p.status,c.payment_id FROM finance_payments p JOIN finance_customer_receipts c ON c.payment_id=p.id AND c.company_id=p.company_id WHERE p.id=? AND p.company_id=?').bind(s.paymentId,s.z.company).first()).toMatchObject({bank_account:s.refs.accounts.cashAccountId,status:'settled',payment_id:s.paymentId});
  const detail=await getOrder(s.z.owner,s.orderId),requested=await requestCommerceReturn(s.z.owner,s.fulfillmentId,returnRequestSchema.parse({expectedVersion:2,date:'2026-09-06',reason:'Actual customer accepted credit of the completed Company service',lines:[{lineId:String(detail.fulfillmentLines[0].id),quantity:'1'}]}),key()),approved=await decideCommerceReturn(s.z.salesManager,String(requested.id),'approve',{expectedVersion:1,reason:'Different actual Sales manager independently reviews the service return'},key());
  expect(approved.stockJournalId).toBeUndefined();await assignZeroSales(s.z,'journal',String(approved.creditJournalId),'draft',s.z.manager);await postJournal(s.z.manager,String(approved.creditJournalId),key(),{expectedVersion:1});
  const completionKey=key(),completed=await completeCommerceReturn(s.z.manager,String(requested.id),{expectedVersion:2},completionKey);expect(completed).toMatchObject({refundMinor:5000,status:'posted'});expect(await completeCommerceReturn(s.z.manager,String(requested.id),{expectedVersion:2},completionKey)).toEqual(completed);
  const refunds=await import('../../../src/server/modules/finance/public/refunds'),reviewer=await s.z.actor('independent-service-refund-reviewer','finance_approver'),refundId=String(completed.refundId);
  await refunds.approveRefund(reviewer,refundId,{expectedVersion:1,beneficiaryName:'Original actual service customer',beneficiaryAccount:'FICTIONAL-CUSTOMER-ACCOUNT',reason:'Independent original invoice credit and actual settled receipt reviewed'},key());
  const current=await refunds.refundWorkspace(s.z.treasury,refundId);
  expect(current.owner).toMatchObject({status:'approved',amount_minor:5000,outstanding_minor:5000,branch_id:null,department_id:null});
  expect.soft(current.accounts.map(row=>row.id)).toEqual([s.refs.accounts.cashAccountId]);
  const prepared=await refunds.prepareRefundInstruction(s.z.treasury,refundId,{expectedVersion:Number(current.owner.version),amount:'50',plannedDate:'2026-09-07',bankAccount:s.refs.accounts.cashAccountId,cashAccountId:s.refs.accounts.cashAccountId},key());
  expect(prepared).toMatchObject({obligationId:refundId,status:'prepared',version:1});
  expect(await db.prepare('SELECT cash_account_id,amount_minor FROM finance_refund_instructions WHERE id=?').bind(prepared.id).first()).toEqual({cash_account_id:s.refs.accounts.cashAccountId,amount_minor:5000});
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(s.z.company).first()).toBeNull();
});

// Ignored candidate: append only after the frozen 415d9b test and coherent backend.
// Positive source business is always created by original owner commands. SQL
// changes below are explicitly adverse fixtures and never establish eligibility.
async function companyServiceCashFixture(twoBanks=false){
  const s=await actualSalesServiceBeforeGoods(),finance=await import('../../../src/server/modules/finance/public/finance');
  const banks=[s.refs.accounts.cashAccountId],payments=[s.paymentId];
  let reversedBank:string|null=null,reversedPayment:string|null=null;
  async function createBank(name:string){
    return finance.createAccount(s.z.reviewer,{chartId:s.refs.chart.id,code:key(),name,category:'asset',currency:'THB',isControl:true,controlKind:'bank'},key());
  }
  if(twoBanks){
    for(const [name,amount] of [['Second actual original partial receipt bank','25'],['Actually reversed original receipt bank','10']] as const){
      const bank=await createBank(name),version=Number(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(s.invoiceId).first('version'));
      const payment=await settleInvoice(s.z.treasury,s.invoiceId,{expectedVersion:version,date:'2026-09-05',amount,bankAccountId:bank.id,reference:name},key());
      if(amount==='10'){
        const reverseKey=key(),body={expectedVersion:1,periodId:s.refs.period.id,entryDate:'2026-09-06',reason:'Independent actual reviewer corrects the third original receipt before crediting the service'};
        const result=await finance.reversePayment(s.z.manager,String(payment.paymentId),body,reverseKey);
        expect(await finance.reversePayment(s.z.manager,String(payment.paymentId),body,reverseKey)).toEqual(result);
        expect(await db.prepare('SELECT status FROM finance_payments WHERE id=?').bind(payment.paymentId).first('status')).toBe('voided');
        reversedBank=bank.id;reversedPayment=String(payment.paymentId);
      }else{banks.push(bank.id);payments.push(String(payment.paymentId));}
    }
  }
  const detail=await getOrder(s.z.owner,s.orderId),returned=await requestCommerceReturn(s.z.owner,s.fulfillmentId,returnRequestSchema.parse({expectedVersion:2,date:'2026-09-06',reason:'Actual Company service customer independently accepts the whole credit',lines:[{lineId:String(detail.fulfillmentLines[0].id),quantity:'1'}]}),key());
  const reviewed=await decideCommerceReturn(s.z.salesManager,String(returned.id),'approve',{expectedVersion:1,reason:'Different actual Sales manager reviews the original completed service'},key());
  expect(reviewed.stockJournalId).toBeUndefined();await assignZeroSales(s.z,'journal',String(reviewed.creditJournalId),'draft',s.z.manager);await postJournal(s.z.manager,String(reviewed.creditJournalId),key(),{expectedVersion:1});
  const completed=await completeCommerceReturn(s.z.manager,String(returned.id),{expectedVersion:2},key()),refundId=String(completed.refundId),refunds=await import('../../../src/server/modules/finance/public/refunds');
  expect(completed.refundMinor).toBe(twoBanks?7500:5000);
  const reviewer=await s.z.actor('independent-cash-source-reviewer','finance_approver');
  await refunds.approveRefund(reviewer,refundId,{expectedVersion:1,beneficiaryName:'Original fictional service customer',beneficiaryAccount:'FICTIONAL-SERVICE-REFUND',reason:'Independent original credit and settled receipt review'},key());
  const input=async(cashAccountId=banks[0],amount='1')=>({expectedVersion:Number((await refunds.getRefund(s.z.treasury,refundId)).version),amount,plannedDate:'2026-09-07',bankAccount:cashAccountId,cashAccountId});
  return {...s,refundId,refunds,banks,payments,reversedBank,reversedPayment,createBank,input};
}

async function companyServiceCashImage(s:Awaited<ReturnType<typeof companyServiceCashFixture>>,command:string){
  return {history:await s.serviceHistory(),financial:await Promise.all(['finance_payments','finance_allocations','finance_refund_obligations','finance_refund_instructions','money_transaction_snapshots'].map(async table=>(await db.prepare(`SELECT to_jsonb(row)::text image FROM ${table} row WHERE company_id=? ORDER BY image`).bind(s.z.company).all()).results)),authority:await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(s.z.treasury.assignmentId).first(),command:await zeroSalesCommandImage(command)};
}

async function rejectsInvalidServiceCash(run:()=>Promise<unknown>){
  // The new closed constructor refuses an unavailable initial selection before
  // mutation. This is the original CONFLICT outcome, not a SQL23514 claim.
  await expect(run()).rejects.toMatchObject({status:409,code:'CONFLICT'});
}

it('Company Service cash uses both actual partial-receipt banks and excludes reversed unrelated foreign and invalid account sources',async()=>{
  const s=await companyServiceCashFixture(true),finance=await import('../../../src/server/modules/finance/public/finance');
  expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
  expect((await s.refunds.refundWorkspace(s.z.treasury,s.refundId)).accounts.map(row=>String(row.id)).sort()).toEqual([...s.banks].sort());
  expect(s.reversedBank).not.toBeNull();expect(s.reversedPayment).not.toBeNull();
  const unrelated=await s.createBank('Actual same-Company bank without an original invoice receipt'),foreign=await zeroLedgerSalesCompany();await foreign.purchase();
  const foreignChart=await finance.createChart(foreign.reviewer,{name:'Actual unrelated Company chart',baseCurrency:'THB',accountingBasis:'IFRS'},key()),foreignBank=await finance.createAccount(foreign.reviewer,{chartId:foreignChart.id,code:key(),name:'Actual unrelated Company bank',category:'asset',currency:'THB',isControl:true,controlKind:'bank'},key());
  for(const bank of [unrelated.id,foreignBank.id,s.reversedBank]){
    if(!bank)throw Error('Actual reversed source bank required');
    const command=key(),body=await s.input(bank),before=await companyServiceCashImage(s,command);
    await rejectsInvalidServiceCash(()=>s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,body,command));
    expect(await companyServiceCashImage(s,command)).toEqual(before);
  }
  const account=s.banks[0],initialVersion=Number(await db.prepare('SELECT version FROM finance_accounts WHERE id=?').bind(account).first('version'));
  await finance.changeAccountStatus(s.z.reviewer,s.refs.chart.id,account,{status:'inactive',expectedVersion:initialVersion,reason:'Actual original owner retires this receipt bank'},key());
  expect((await s.refunds.refundWorkspace(s.z.treasury,s.refundId)).accounts.map(row=>String(row.id))).toEqual([s.banks[1]]);
  const inactiveCommand=key(),inactiveBody=await s.input(account),inactiveBefore=await companyServiceCashImage(s,inactiveCommand);await rejectsInvalidServiceCash(()=>s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,inactiveBody,inactiveCommand));expect(await companyServiceCashImage(s,inactiveCommand)).toEqual(inactiveBefore);
  await finance.changeAccountStatus(s.z.reviewer,s.refs.chart.id,account,{status:'active',expectedVersion:initialVersion+1,reason:'Actual original owner reactivates the receipt bank after review'},key());
  const original=await db.prepare('SELECT status,currency,chart_id,control_kind,category,version FROM finance_accounts WHERE id=?').bind(account).first<{status:string;currency:string;chart_id:string;control_kind:string;category:string;version:number}>();
  if(!original)throw Error('Actual original receipt bank required');
  const otherChart=await finance.createChart(s.z.reviewer,{name:'Actual different chart is not the refund chart',baseCurrency:'THB',accountingBasis:'IFRS'},key());
  // Diagnostic adverse source mutations only. No synthetic settled receipt or
  // journal is inserted; the original row is restored exactly after each test.
  for(const patch of ["currency='USD'",`chart_id='${otherChart.id}'`,"control_kind='none'","category='liability'"]){
    await db.exec(`UPDATE finance_accounts SET ${patch} WHERE id='${account}'`);
    try{
      const view=await s.refunds.refundWorkspace(s.z.treasury,s.refundId);expect(view.accounts.map(row=>String(row.id))).toEqual([s.banks[1]]);
      const command=key(),body=await s.input(account),before=await companyServiceCashImage(s,command);
      await rejectsInvalidServiceCash(()=>s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,body,command));expect(await companyServiceCashImage(s,command)).toEqual(before);
    }finally{await db.prepare('UPDATE finance_accounts SET status=?,currency=?,chart_id=?,control_kind=?,category=?,version=? WHERE id=?').bind(original.status,original.currency,original.chart_id,original.control_kind,original.category,original.version,account).run();}
  }
  // A real receipt whose current bank disagrees with its captured journal does
  // not authorize that bank, even though the bank itself is otherwise valid.
  await db.prepare('UPDATE finance_payments SET bank_account=? WHERE id=?').bind(unrelated.id,s.payments[0]).run();
  try{
    expect((await s.refunds.refundWorkspace(s.z.treasury,s.refundId)).accounts.map(row=>String(row.id))).toEqual([s.banks[1]]);
    const command=key(),body=await s.input(unrelated.id),before=await companyServiceCashImage(s,command);await rejectsInvalidServiceCash(()=>s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,body,command));expect(await companyServiceCashImage(s,command)).toEqual(before);
  }finally{await db.prepare('UPDATE finance_payments SET bank_account=? WHERE id=?').bind(account,s.payments[0]).run();}
  const firstKey=key(),firstBody=await s.input(s.banks[0],'20'),first=await s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,firstBody,firstKey);
  expect(await s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,firstBody,firstKey)).toEqual(first);
  const replayBefore=await companyServiceCashImage(s,firstKey);await db.prepare("UPDATE finance_accounts SET status='inactive' WHERE id=?").bind(account).run();
  try{expect(await s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,firstBody,firstKey)).toEqual(first);}finally{await db.prepare('UPDATE finance_accounts SET status=? WHERE id=?').bind(original.status,account).run();}
  expect(await companyServiceCashImage(s,firstKey)).toEqual(replayBefore);
  const secondKey=key(),secondBody=await s.input(s.banks[1],'25'),second=await s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,secondBody,secondKey);expect(await s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,secondBody,secondKey)).toEqual(second);
  expect((await db.prepare('SELECT cash_account_id,amount_minor FROM finance_refund_instructions WHERE obligation_id=? ORDER BY cash_account_id').bind(s.refundId).all()).results).toEqual([{cash_account_id:s.banks[0],amount_minor:2000},{cash_account_id:s.banks[1],amount_minor:2500}].sort((a,b)=>a.cash_account_id.localeCompare(b.cash_account_id)));
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(s.z.company).first()).toBeNull();
});

it('Company Service cash preparation rolls back deferred receipt account owner IAM and inactive purchase facts while actual expiry permits settlement',async()=>{
  const s=await companyServiceCashFixture();
  for(const kind of ['receipt','account','owner','authority','purchase'] as const){
    // Inactivation here is an explicitly adverse native fact mutation, not a
    // normal Commercial withdrawal while an actual refund remains unsettled.
    const command=key(),body=await s.input(),before=await companyServiceCashImage(s,command),fault=`service_cash_${key().replaceAll('-','')}`,effect=kind==='receipt'?`UPDATE finance_payments SET bank_account='${s.mapping.receivableAccountId}' WHERE id='${s.paymentId}'`:kind==='account'?`UPDATE finance_accounts SET name=name||' deferred change',version=version+1 WHERE id='${s.banks[0]}'`:kind==='owner'?`UPDATE commerce_orders SET due_date='2026-12-30' WHERE id='${s.orderId}'`:kind==='authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${s.z.treasury.assignmentId}'`:`UPDATE company_package_grants SET state='inactive',version=version+1 WHERE company_id='${s.z.company}' AND package_id='sales'`;
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      const preparation=s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,body,command);
      if(kind==='purchase'){
        // The original1047 schema/audit defense rejects this unaudited fact
        // first. This is not proof of the later capability fence or a normal
        // Commercial withdrawal, and no matching audit is fabricated.
        await expect(preparation,'actual deferred inactive fact hits the original audit defense').rejects.toMatchObject({code:'23514',message:'PACKAGE_HISTORY_CURRENT_AUDIT_REQUIRED'});
      }else await expect(preparation,`actual deferred ${kind} must refuse the whole preparation`).rejects.toMatchObject({code:kind==='authority'?'FINANCE_ACCESS_CHANGED':'FINANCE_REFUND_CASH_SOURCE_CHANGED'});
      expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called'),`actual deferred ${kind} trigger ran`).toBe(true);expect(await companyServiceCashImage(s,command),`actual deferred ${kind} whole rollback`).toEqual(before);
    }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
  expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='sales'").bind(s.z.company).first()).toEqual({state:'active',version:1});
  // Original expiry is draining, which deliberately retains settle-existing.
  // Record native times on both sides of the real deadline, not only JS time.
  const endsAt=Date.now()+2500;await s.z.store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...s.z.grant,version:2,endsAt,reason:'Actual Sales expiry retains this original unsettled refund',evidence:'test:service-cash-expiry-settlement'}});
  const command=key(),body=await s.input(),fault=`service_cash_expiry_${key().replaceAll('-','')}`;
  await db.exec(`CREATE SEQUENCE ${fault};CREATE SEQUENCE ${fault}_after;CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM setval('${fault}',floor(extract(epoch from clock_timestamp())*1000)::bigint);PERFORM pg_sleep(2.7);PERFORM setval('${fault}_after',floor(extract(epoch from clock_timestamp())*1000)::bigint);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try{
    const prepared=await s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,body,command);expect(prepared).toMatchObject({obligationId:s.refundId,status:'prepared',version:1});
    expect(Number(await db.prepare(`SELECT last_value::text value FROM ${fault}`).first('value')),'native effects began before actual expiry').toBeLessThan(endsAt);expect(Number(await db.prepare(`SELECT last_value::text value FROM ${fault}_after`).first('value')),'native effects finished after actual expiry').toBeGreaterThan(endsAt);
    const committed=await companyServiceCashImage(s,command);expect(await s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,body,command)).toEqual(prepared);expect(await companyServiceCashImage(s,command)).toEqual(committed);
  }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault};DROP SEQUENCE ${fault}_after`);}
});

async function anotherCompanyServiceReceipt(s:Awaited<ReturnType<typeof companyServiceCashFixture>>){
  const customer=await saveMaster(s.z.owner,'parties',null,{kind:'customer',code:key(),name:'Unrelated actual customer for normal receipt writer'},key()),service=await saveMaster(s.z.owner,'products',null,{sku:key(),name:'Unrelated actual completed service for normal reversal writer',itemType:'service',price:'10',taxBps:0},key()),order=await saveOrder(s.z.owner,null,orderSchema.parse({kind:'sales',partyId:customer.id,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId:service.id,quantity:'1',price:'10',taxBps:0}]}),key()),orderId=String(order.id);
  await transitionOrder(s.z.owner,orderId,{expectedVersion:1,action:'submit'},key());await transitionOrder(s.z.salesManager,orderId,{expectedVersion:2,action:'approve'},key());await assignZeroSales(s.z,'order',orderId,'confirmed',s.z.owner);
  const line=String((await getOrder(s.z.owner,orderId)).lines[0].id),fulfilled=await fulfillOrder(s.z.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-03',completionMode:'service',evidenceNote:'Actual unrelated service completed normally',lines:[{lineId:line,quantity:'1'}]}),key());await assignZeroSales(s.z,'order',orderId,'invoicing',s.z.owner);
  const invoiced=await invoiceFulfillment(s.z.owner,String(fulfilled.fulfillmentId),{expectedVersion:1,date:'2026-09-04',dueDate:'2026-09-10'},key()),invoiceId=String(invoiced.invoiceId);await assignZeroSales(s.z,'invoice',invoiceId,'draft',s.z.manager);await postInvoice(s.z.manager,invoiceId,{expectedVersion:1,periodId:s.refs.period.id,entryDate:'2026-09-04'},key());await assignZeroSales(s.z,'invoice',invoiceId,'posted',s.z.treasury);
  const paid=await settleInvoice(s.z.treasury,invoiceId,{expectedVersion:2,date:'2026-09-05',amount:'5',bankAccountId:s.banks[0],reference:'Normal unrelated receipt has not been credited and can legitimately be reversed'},key());
  return {paymentId:String(paid.paymentId),partyId:String(customer.id)};
}

async function overlapCompanyServiceCash(s:Awaited<ReturnType<typeof companyServiceCashFixture>>,writer:(command:string)=>Promise<unknown>){
  const {measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),writerKey=key(),cashKey=key(),body=await s.input(),fault=`service_cash_writer_${key().replaceAll('-','')}`;
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${writerKey}') EXECUTE FUNCTION ${fault}()`);
  try{
    const observed=await measureReads(async()=>{
      const originalWriter=writer(writerKey);let staged=false;
      for(let attempt=0;attempt<100;attempt++){staged=!!await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called');if(staged)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(staged).toBe(true);
      const [written,prepared]=await Promise.all([originalWriter,s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,body,cashKey)]);return {written,prepared};
    });
    expect(observed.result.prepared).toMatchObject({obligationId:s.refundId,status:'prepared',version:1});expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
    expect(await s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,body,cashKey)).toEqual(observed.result.prepared);
  }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
}

it('Sales-only Company Service cash source waits before normal same-Company account chart and independent receipt reversal writers without retries',async()=>{
  const s=await companyServiceCashFixture(),finance=await import('../../../src/server/modules/finance/public/finance'),unrelated=await anotherCompanyServiceReceipt(s);
  await overlapCompanyServiceCash(s,command=>finance.createAccount(s.z.reviewer,{chartId:s.refs.chart.id,code:key(),name:'Normal actual account writer overlapping Service cash capture',category:'asset',currency:'THB',isControl:false,controlKind:'none'},command));
  await overlapCompanyServiceCash(s,command=>finance.createChart(s.z.reviewer,{name:'Normal actual chart writer overlapping Service cash capture',baseCurrency:'THB',accountingBasis:'IFRS'},command));
  // Reverse an uncredited actual receipt from a DIFFERENT invoice in this same
  // Company. The original credited-source reversal refusal is not bypassed.
  await overlapCompanyServiceCash(s,command=>finance.reversePayment(s.z.manager,unrelated.paymentId,{expectedVersion:1,periodId:s.refs.period.id,entryDate:'2026-09-06',reason:'Different rightful Finance approver reverses the original unrelated receipt'},command));
  expect(await db.prepare('SELECT status FROM finance_payments WHERE id=?').bind(unrelated.paymentId).first('status')).toBe('voided');expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
});

it('separately declared Sales plus Finance Company serializes the same Service cash constructor against a normal generic payment writer',async()=>{
  const s=await companyServiceCashFixture(),finance=await import('../../../src/server/modules/finance/public/finance');
  // This is an explicit additional concurrency control, NOT Sales-only package
  // acceptance and NOT a prerequisite imposed on the Sales-only cases above.
  await s.z.store.apply({expectedVersion:0,idempotencyKey:key(),grant:{...s.z.grant,packageId:'finance',version:1,reason:'Separately declared full Banking writer overlap control',evidence:'test:service-cash-generic-writer'}});
  expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId).sort()).toEqual(['finance','sales']);
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(s.z.company).first()).toBeNull();
  const party=await db.prepare('SELECT party_id FROM finance_invoices WHERE id=?').bind(s.invoiceId).first<string>('party_id');if(!party)throw Error('Actual original same-Company customer required');
  // Full Banking must not bypass the same genuine no-mapping Service cash
  // constructor. A real deferred source change still rolls back the command.
  const probe=key(),probeBody=await s.input(),probeBefore=await companyServiceCashImage(s,probe),fault=`service_cash_full_${key().replaceAll('-','')}`;
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');UPDATE finance_accounts SET name=name||' deferred source edit',version=version+1 WHERE id='${s.banks[0]}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${probe}') EXECUTE FUNCTION ${fault}()`);
  try{await expect(s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,probeBody,probe)).rejects.toMatchObject({code:'FINANCE_REFUND_CASH_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await companyServiceCashImage(s,probe)).toEqual(probeBefore);}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  await overlapCompanyServiceCash(s,command=>finance.createPayment(s.z.reviewer,{partyId:party,kind:'receipt',paymentNumber:`OVERLAP-${key()}`,paymentDate:'2026-09-07',currency:'THB',amount:'1',bankAccount:s.banks[0]},command));
  expect(await db.prepare('SELECT count(*)::int n FROM finance_payments WHERE company_id=? AND status=\'draft\'').bind(s.z.company).first('n')).toBe(1);
});


it('Sales-only Company service refund exports then imports fresh bank evidence and independently confirms without a Goods mapping or Banking purchase',async()=>{
  const s=await companyServiceCashFixture(),bankOwner=await import('../../../src/server/modules/finance/public/refunds');
  expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
  const prepared=await s.refunds.prepareRefundInstruction(s.z.treasury,s.refundId,await s.input(s.banks[0],'50'),key()),instructionId=String(prepared.id),exportKey=key(),exportInput={instructions:[{id:instructionId,expectedVersion:1}]};
  const exported=await s.refunds.exportRefundInstructions(s.z.treasury,exportInput,exportKey);expect(await s.refunds.exportRefundInstructions(s.z.treasury,exportInput,exportKey)).toEqual(exported);
  expect(await db.prepare('SELECT status,version FROM finance_refund_instructions WHERE id=?').bind(instructionId).first()).toEqual({status:'exported',version:2});
  const bankInput={expectedVersion:2,bankAccount:s.banks[0],statementNumber:`FICTIONAL-SERVICE-${key()}`,statementDate:'2026-09-07',opening:'100',closing:'50',sourceHash:key(),rows:[{externalId:`FICTIONAL-SERVICE-REFUND-${key()}`,transactionDate:'2026-09-07',description:'Actual bank observation for the exported original Company service refund',amount:'-50',currency:'THB'}]},importKey=key();
  const bankImage=()=>Promise.all(['finance_bank_statements','finance_bank_transactions'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? ORDER BY image`).bind(s.z.company).all()).results)),before=await bankImage();
  // The reviewed named owner imports exact instruction-linked bank observations
  // through the original Bank application; no generic Banking right is added.
  let imported:Awaited<ReturnType<typeof bankOwner.importRefundBankEvidence>>;
  try{imported=await bankOwner.importRefundBankEvidence(s.z.treasury,instructionId,bankInput,importKey);}catch(error){expect(await bankImage()).toEqual(before);expect(await zeroSalesCommandImage(importKey)).toEqual([[],[],[]]);throw error;}
  expect(imported).toEqual({id:`statement-${importKey}`,imported:1,status:'imported',refundInstructionId:instructionId,refundObligationId:s.refundId,bankTransactionId:`bankrow-${importKey}-0`});expect(imported).not.toHaveProperty('__refundInstructionBankEvidence');
  expect(await bankOwner.importRefundBankEvidence(s.z.treasury,instructionId,bankInput,importKey)).toEqual(imported);
  const bank=await db.prepare('SELECT id,version,amount_minor,currency,match_status FROM finance_bank_transactions WHERE statement_id=? AND company_id=?').bind(`statement-${importKey}`,s.z.company).first<{id:string;version:number;amount_minor:number;currency:string;match_status:string}>();
  expect(bank).toEqual({id:`bankrow-${importKey}-0`,version:1,amount_minor:-5000,currency:'THB',match_status:'unmatched'});if(!bank)throw Error('Actual fresh bank evidence required');
  const confirmer=await s.z.actor('different-normal-service-refund-confirmer','treasury'),confirmation={expectedVersion:2,bankTransactionId:bank.id,bankVersion:bank.version,periodId:s.refs.period.id,bankReference:'FICTIONAL-SERVICE-REFUND-CONFIRMED',evidence:'Different actual Treasury officer checked the original exported instruction and fresh native bank movement',attachmentSetIds:[]},confirmKey=key();
  const confirmed=await s.refunds.confirmRefundInstruction(confirmer,instructionId,confirmation,confirmKey);expect(await s.refunds.confirmRefundInstruction(confirmer,instructionId,confirmation,confirmKey)).toEqual(confirmed);
  expect(await db.prepare('SELECT status,outstanding_minor FROM finance_refund_obligations WHERE id=?').bind(s.refundId).first()).toEqual({status:'settled',outstanding_minor:0});expect(await db.prepare('SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?').bind(bank.id).first()).toEqual({match_status:'matched',matched_type:'payment',matched_id:confirmed.paymentId});
  expect(await db.prepare('SELECT kind,status,amount_minor,bank_account FROM finance_payments WHERE id=?').bind(confirmed.paymentId).first()).toEqual({kind:'refund',status:'settled',amount_minor:5000,bank_account:s.banks[0]});
  expect(await db.prepare('SELECT SUM(debit_minor-credit_minor)::int net FROM finance_journal_lines WHERE journal_id=?').bind(confirmed.journalId).first('net')).toBe(0);
  expect(await db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(s.z.company).first()).toBeNull();expect(await db.prepare('SELECT count(*)::int n FROM commerce_warehouses WHERE company_id=?').bind(s.z.company).first('n')).toBe(0);expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
});

// Ignored additive candidate. Preserve current27 and first named positive;
// append/run only after Root reviews the first successful bank boundary.
async function serviceRefundBankBody(s:Awaited<ReturnType<typeof companyServiceCashFixture>>,instructionId:string,amount:string){
  const version=Number(await db.prepare('SELECT version FROM finance_refund_instructions WHERE id=?').bind(instructionId).first('version'));
  return {expectedVersion:version,bankAccount:s.banks[0],statementNumber:`FICTIONAL-NAMED-${key()}`,statementDate:'2026-09-07',opening:'100',closing:(100-Number(amount)).toFixed(2),sourceHash:key(),rows:[{externalId:`FICTIONAL-NAMED-ROW-${key()}`,transactionDate:'2026-09-07',description:'Actual bank observation for the exact original exported instruction',amount:`-${amount}`,currency:'THB'}]};
}

async function serviceRefundWholeBankImage(s:Awaited<ReturnType<typeof companyServiceCashFixture>>,command:string){
  return {ownerFinanceMoneyAuthorityCommand:await companyServiceCashImage(s,command),bank:await Promise.all(['finance_bank_statements','finance_bank_transactions'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? ORDER BY image`).bind(s.z.company).all()).results))};
}

it('Company service partial refund bank confirmation rejects late remaining-owner and bank-version drift and exact replays survive a later sibling settlement',async()=>{
  const s=await companyServiceCashFixture(),owner=await import('../../../src/server/modules/finance/public/refunds'),instructions:string[]=[];
  for(let index=0;index<2;index++)instructions.push(String((await owner.prepareRefundInstruction(s.z.treasury,s.refundId,await s.input(s.banks[0],'10'),key())).id));
  await owner.exportRefundInstructions(s.z.treasury,{instructions:instructions.map(id=>({id,expectedVersion:1}))},key());
  const imports:{instructionId:string;body:Awaited<ReturnType<typeof serviceRefundBankBody>>;command:string;result:Awaited<ReturnType<typeof owner.importRefundBankEvidence>>}[]=[];
  for(const instructionId of instructions){const body=await serviceRefundBankBody(s,instructionId,'10'),command=key(),result=await owner.importRefundBankEvidence(s.z.treasury,instructionId,body,command);imports.push({instructionId,body,command,result});}
  const first=imports[0],second=imports[1];if(!first||!second)throw Error('Two actual original exported instruction sources required');
  const confirmer=await s.z.actor('independent-partial-bank-confirmer','treasury'),confirmation=(item:(typeof imports)[number])=>({expectedVersion:2,bankTransactionId:item.result.bankTransactionId,bankVersion:1,periodId:s.refs.period.id,bankReference:`FICTIONAL-PARTIAL-${item.instructionId}`,evidence:'Different actual Treasury officer checked the exact exported source and fresh bank evidence',attachmentSetIds:[]});
  const initial=await db.prepare('SELECT outstanding_minor,base_outstanding_minor,version FROM finance_refund_obligations WHERE id=?').bind(s.refundId).first();expect(initial).toMatchObject({outstanding_minor:5000,base_outstanding_minor:5000});
  for(const kind of ['remaining-owner','bank-version'] as const){
    const command=key(),body=confirmation(first),before=await serviceRefundWholeBankImage(s,command),fault=`service_bank_final_${key().replaceAll('-','')}`,effect=kind==='remaining-owner'?`UPDATE finance_refund_obligations SET outstanding_minor=outstanding_minor-1,version=version+1 WHERE id='${s.refundId}'`:`UPDATE finance_bank_transactions SET version=version+1 WHERE id='${first.result.bankTransactionId}'`;
    // Adverse deferred native mutation only. It does not manufacture a valid
    // receipt, payment, match, grant or positive bank source.
    await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{
      await expect(owner.confirmRefundInstruction(confirmer,first.instructionId,body,command),`actual deferred ${kind} must roll back whole native confirmation`).rejects.toMatchObject({code:'REFUND_BANK_EVIDENCE_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await serviceRefundWholeBankImage(s,command)).toEqual(before);
    }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  }
  const firstKey=key(),firstBody=confirmation(first),firstResult=await owner.confirmRefundInstruction(confirmer,first.instructionId,firstBody,firstKey);
  expect(await db.prepare('SELECT status,outstanding_minor FROM finance_refund_obligations WHERE id=?').bind(s.refundId).first()).toEqual({status:'approved',outstanding_minor:4000});
  const secondKey=key(),secondBody=confirmation(second),secondResult=await owner.confirmRefundInstruction(confirmer,second.instructionId,secondBody,secondKey);expect(secondResult.paymentId).not.toBe(firstResult.paymentId);
  expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_refund_obligations WHERE id=?').bind(s.refundId).first()).toEqual({status:'approved',outstanding_minor:3000,base_outstanding_minor:3000});
  const afterSibling=await serviceRefundWholeBankImage(s,firstKey),firstImport=await serviceRefundWholeBankImage(s,first.command);
  expect(await owner.confirmRefundInstruction(confirmer,first.instructionId,firstBody,firstKey)).toEqual(firstResult);expect(await owner.importRefundBankEvidence(s.z.treasury,first.instructionId,first.body,first.command)).toEqual(first.result);
  expect(await serviceRefundWholeBankImage(s,firstKey)).toEqual(afterSibling);expect(await serviceRefundWholeBankImage(s,first.command)).toEqual(firstImport);expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
});

it('separate Finance control cannot consume instruction-scoped refund bank evidence through a different normal generic payment confirmation',async()=>{
  const s=await companyServiceCashFixture(),owner=await import('../../../src/server/modules/finance/public/refunds'),finance=await import('../../../src/server/modules/finance/public/finance');
  // Explicit adversarial full-Banking control, not a Sales-only prerequisite.
  await s.z.store.apply({expectedVersion:0,idempotencyKey:key(),grant:{...s.z.grant,packageId:'finance',version:1,reason:'Separate actual generic Bank cross-consumer control',evidence:'test:refund-bank-cross-consumer'}});
  const instruction=await owner.prepareRefundInstruction(s.z.treasury,s.refundId,await s.input(s.banks[0],'50'),key()),instructionId=String(instruction.id);await owner.exportRefundInstructions(s.z.treasury,{instructions:[{id:instructionId,expectedVersion:1}]},key());
  const evidence=await owner.importRefundBankEvidence(s.z.treasury,instructionId,await serviceRefundBankBody(s,instructionId,'50'),key());
  const supplier=await finance.createParty(s.z.reviewer,{kind:'supplier',code:key(),name:'Actual unrelated generic supplier',currency:'THB',paymentTermsDays:30,creditLimit:'0'},key()),invoice=await finance.createInvoice(s.z.reviewer,{partyId:String(supplier.id),kind:'payable',invoiceNumber:`FICTIONAL-OTHER-BILL-${key()}`,invoiceDate:'2026-09-04',dueDate:'2026-09-10',currency:'THB',subtotal:'50',tax:'0',debitAccountId:s.mapping.cogsAccountId,creditAccountId:s.mapping.payableAccountId},key()),invoiceId=String(invoice.id);
  await assignZeroSales(s.z,'invoice',invoiceId,'draft',s.z.manager);await postInvoice(s.z.manager,invoiceId,{expectedVersion:1,periodId:s.refs.period.id,entryDate:'2026-09-04'},key());
  const payment=await finance.createPayment(s.z.reviewer,{partyId:String(supplier.id),kind:'payment',paymentNumber:`FICTIONAL-OTHER-PAY-${key()}`,paymentDate:'2026-09-06',currency:'THB',amount:'50',bankAccount:s.banks[0]},key()),paymentId=String(payment.id);
  await finance.approvePayment(s.z.manager,paymentId,{expectedVersion:1,proof:'Different actual Finance approver checked the unrelated supplier obligation'},key());await finance.releasePayment(s.z.treasury,paymentId,{expectedVersion:2,proof:'Original independent Treasury releases the unrelated actual payment'},key());
  expect(await db.prepare('SELECT status,version,bank_account,amount_minor FROM finance_payments WHERE id=?').bind(paymentId).first()).toEqual({status:'released',version:3,bank_account:s.banks[0],amount_minor:5000});expect(await db.prepare('SELECT currency,amount_minor,version,match_status FROM finance_bank_transactions WHERE id=?').bind(evidence.bankTransactionId).first()).toEqual({currency:'THB',amount_minor:-5000,version:1,match_status:'unmatched'});
  const command=key(),before=await serviceRefundWholeBankImage(s,command),body={expectedVersion:3,periodId:s.refs.period.id,bankTransactionId:evidence.bankTransactionId,bankVersion:1,bankReference:'FICTIONAL-WRONG-INSTRUCTION-CONSUMER',proof:'Must refuse evidence reserved to the different original refund instruction',allocations:[{invoiceId,amount:'50'}]};
  // All generic payment source facts are real/valid. The only invalid boundary
  // is consuming a marked bank row owned by the other refund instruction.
  await expect(finance.confirmPayment(s.z.treasury,paymentId,body,command)).rejects.toMatchObject({code:'REFUND_BANK_PROVENANCE_REQUIRED'});expect(await serviceRefundWholeBankImage(s,command)).toEqual(before);
  expect(await db.prepare('SELECT status,version FROM finance_refund_instructions WHERE id=?').bind(instructionId).first()).toEqual({status:'exported',version:2});
});

// Ignored native candidates. No active append or execution until Root's guard
// and UI epoch is coherent. All business sources use original owner commands.
async function exportedServiceBankInstruction(s:Awaited<ReturnType<typeof companyServiceCashFixture>>){
  const owner=await import('../../../src/server/modules/finance/public/refunds'),prepared=await owner.prepareRefundInstruction(s.z.treasury,s.refundId,await s.input(s.banks[0],'50'),key()),id=String(prepared.id);await owner.exportRefundInstructions(s.z.treasury,{instructions:[{id,expectedVersion:1}]},key());return {owner,id,body:await serviceRefundBankBody(s,id,'50')};
}

it('scoped service refund bank import rejects unknown authority fields stale versions and unrelated real bank accounts without effects',async()=>{
  const s=await companyServiceCashFixture(),{owner,id,body}=await exportedServiceBankInstruction(s),unrelated=await s.createBank('Actual unrelated bank cannot supply instruction provenance');
  for(const [label,input] of [['top-level-source',{...body,refundInstructionId:id}],['private-marker',{...body,__refundInstructionBankEvidence:{instructionId:id,companyId:s.z.company}}],['row-source',{...body,rows:[{...body.rows[0],refundInstructionId:id}]}]] as const){
    const command=key(),before=await serviceRefundWholeBankImage(s,command);await expect(owner.importRefundBankEvidence(s.z.treasury,id,input,command),label).rejects.toMatchObject({name:'ZodError',issues:expect.arrayContaining([expect.objectContaining({code:'unrecognized_keys'})])});expect(await serviceRefundWholeBankImage(s,command),label).toEqual(before);
  }
  for(const [label,input,code] of [['stale',{...body,expectedVersion:1},'STALE_VERSION'],['unrelated-account',{...body,bankAccount:unrelated.id},'REFUND_BANK_EVIDENCE_MISMATCH']] as const){
    const command=key(),before=await serviceRefundWholeBankImage(s,command);await expect(owner.importRefundBankEvidence(s.z.treasury,id,input,command),label).rejects.toMatchObject({code});expect(await serviceRefundWholeBankImage(s,command),label).toEqual(before);
  }
  const finance=await import('../../../src/server/modules/finance/public/finance'),forged={...body,refundInstructionId:id,refundInstructionSource:'invented-client-source',__refundInstructionBankEvidence:{instructionId:id,companyId:s.z.company}},command=key(),before=await serviceRefundWholeBankImage(s,command);
  // Extra transport labels cannot grant the original generic Banking purchase
  // or mint the named bankService's server-owned provenance marker.
  await expect(finance.importBankStatement(s.z.treasury,forged,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await serviceRefundWholeBankImage(s,command)).toEqual(before);
});

it('scoped service refund bank observations bind real amount currency and planned statement dates before committing any Bank rows',async()=>{
  const s=await companyServiceCashFixture(),{owner,id,body}=await exportedServiceBankInstruction(s);
  for(const [label,amount,currency,transactionDate,closing] of [['wrong-amount','-49','THB','2026-09-07','51'],['wrong-direction','50','THB','2026-09-07','150'],['wrong-currency','-50','USD','2026-09-07','50'],['before-planned','-50','THB','2026-09-06','50'],['after-statement','-50','THB','2026-09-08','50']] as const){
    const input={...body,closing,rows:[{...body.rows[0],amount,currency,transactionDate}]},command=key(),before=await serviceRefundWholeBankImage(s,command);await expect(owner.importRefundBankEvidence(s.z.treasury,id,input,command),label).rejects.toMatchObject({code:'REFUND_BANK_EVIDENCE_MISMATCH'});expect(await serviceRefundWholeBankImage(s,command),label).toEqual(before);
  }
  const command=key(),before=await serviceRefundWholeBankImage(s,command);await expect(owner.importRefundBankEvidence(s.z.treasury,id,{...body,closing:'49'},command)).rejects.toMatchObject({code:'STATEMENT_UNBALANCED'});expect(await serviceRefundWholeBankImage(s,command)).toEqual(before);
});

it('Company service Bank import retains current Company authority and releases while a separate unpurchased legacy mapped Sales refund stays denied',async()=>{
  const s=await companyServiceCashFixture(),{owner,id,body}=await exportedServiceBankInstruction(s),branch=await s.z.actor('real-branch-bank-reader','treasury','branch'),foreign=await zeroLedgerSalesCompany();
  vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');
  for(const [label,actor,error] of [['no-pay',s.z.reviewer,{status:403}],['branch',branch,{status:403,code:'COMPANY_TREASURY_REQUIRED'}],['other-Company',foreign.treasury,{status:404,code:'REFUND_INSTRUCTION_NOT_FOUND'}]] as const){
    const command=key(),before=await serviceRefundWholeBankImage(s,command);await expect(owner.importRefundBankEvidence(actor,id,body,command),label).rejects.toMatchObject(error);expect(await serviceRefundWholeBankImage(s,command),label).toEqual(before);
  }
  const nodeEnv=process.env.NODE_ENV;vi.stubEnv('NODE_ENV','production');
  try{const command=key(),before=await serviceRefundWholeBankImage(s,command);await expect(owner.importRefundBankEvidence(s.z.treasury,id,body,command)).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});expect(await serviceRefundWholeBankImage(s,command)).toEqual(before);}finally{vi.stubEnv('NODE_ENV',nodeEnv);}
  // Separate original seeded legacy mapped/Goods Company: its real normal
  // return, approval, instruction and export predate any purchase enforcement.
  // This proves the new import stays strict in disabled mode, not a zero-ledger
  // Service no-purchase or successful normal withdrawal claim.
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');
  const legacy=await paidReturn('sales');await approveObligation(legacy.refundId);
  const prepared=await prepare(legacy.refundId,'15'),legacyId=String(prepared.id);
  await owner.exportRefundInstructions(f.payer,{instructions:[{id:legacyId,expectedVersion:1}]},key());
  expect(f.company).not.toBe(s.z.company);expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);
  const command=key(),image=async()=>({company:await Promise.all(['finance_refund_obligations','finance_refund_instructions','finance_payments','finance_allocations','finance_journals','finance_financial_events','money_transaction_snapshots','finance_bank_statements','finance_bank_transactions'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? ORDER BY image`).bind(f.company).all()).results)),journalLines:(await db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? ORDER BY image').bind(f.company).all()).results,command:await zeroSalesCommandImage(command)}),before=await image();
  await expect(owner.importRefundBankEvidence(f.payer,legacyId,{expectedVersion:2,bankAccount:f.accounts.cash,statementNumber:key(),statementDate:'2026-09-05',opening:'100',closing:'85',sourceHash:key(),rows:[{externalId:key(),transactionDate:'2026-09-05',amount:'-15',currency:'THB'}]},command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  expect(await image()).toEqual(before);expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);
});

it('actual pay-only Company authority can confirm exact scoped evidence while competing new import keys create only one original Bank statement',async()=>{
  const s=await companyServiceCashFixture(),{owner,id,body}=await exportedServiceBankInstruction(s),role=`custom_native_pay_only_${key()}`;
  // Declared isolated authority fixture, not normal IAM role onboarding: real
  // native published facts are read afresh; no actor/IAM object is forged and
  // no installed role, MFA/session or permission set is modified.
  await db.batch([db.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES(?,?,'Native pay-only Company fixture',false,1,'active')").bind(s.z.company,role),db.prepare("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES(?,?,1,?,'[]','[\"holding\"]','declared-native-fixture',1)").bind(s.z.company,role,JSON.stringify(['iam.self','finance.read','finance.pay']))]);
  const payOnly=await s.z.actor('native-pay-only-confirmer',role),{hasPermission}=await import('../../../src/lib/identity_access/permissions');expect(hasPermission(payOnly,'finance.read')).toBe(true);expect(hasPermission(payOnly,'finance.pay')).toBe(true);expect(hasPermission(payOnly,'finance.write')).toBe(false);
  const deniedKey=key(),deniedBefore=await serviceRefundWholeBankImage(s,deniedKey);await expect(owner.importRefundBankEvidence(payOnly,id,body,deniedKey)).rejects.toMatchObject({status:403});expect(await serviceRefundWholeBankImage(s,deniedKey)).toEqual(deniedBefore);
  const keys=[key(),key()],{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),competing=await measureReads(()=>Promise.allSettled(keys.map(command=>owner.importRefundBankEvidence(s.z.treasury,id,body,command))));
  const winner=competing.result.findIndex(result=>result.status==='fulfilled'),value=competing.result[winner],loser=competing.result.find(result=>result.status==='rejected');expect(competing.result.filter(result=>result.status==='fulfilled')).toHaveLength(1);expect(competing.result.filter(result=>result.status==='rejected')).toHaveLength(1);if(!value||value.status!=='fulfilled'||!loser||loser.status!=='rejected')throw Error('One actual import and one duplicate refusal required');expect(loser.reason).toMatchObject({code:'REFUND_BANK_EVIDENCE_ALREADY_IMPORTED'});
  expect({deadlockFailures:competing.metrics.deadlockFailures??0,serializationFailures:competing.metrics.serializationFailures??0,transactionRetries:competing.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});expect(await zeroSalesCommandImage(keys[1-winner])).toEqual([[],[],[]]);
  expect(await db.prepare('SELECT count(*)::int n FROM finance_bank_statements WHERE company_id=?').bind(s.z.company).first('n')).toBe(1);expect(await db.prepare('SELECT count(*)::int n FROM finance_bank_transactions WHERE company_id=?').bind(s.z.company).first('n')).toBe(1);
  const imported=value.value,winningKey=keys[winner],committed=await serviceRefundWholeBankImage(s,winningKey);expect(Object.keys(imported).sort()).toEqual(['bankTransactionId','id','imported','refundInstructionId','refundObligationId','status'].sort());expect(await owner.importRefundBankEvidence(s.z.treasury,id,body,winningKey)).toEqual(imported);expect(await serviceRefundWholeBankImage(s,winningKey)).toEqual(committed);
  await expect(owner.importRefundBankEvidence(s.z.treasury,id,{...body,sourceHash:key()},winningKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});expect(await serviceRefundWholeBankImage(s,winningKey)).toEqual(committed);
  const confirmation={expectedVersion:2,bankTransactionId:imported.bankTransactionId,bankVersion:1,periodId:s.refs.period.id,bankReference:'FICTIONAL-PAY-ONLY-CONFIRM',evidence:'Actual published pay-only Company role independently confirms the original scoped bank observation',attachmentSetIds:[]},confirmKey=key(),confirmed=await owner.confirmRefundInstruction(payOnly,id,confirmation,confirmKey);expect(await owner.confirmRefundInstruction(payOnly,id,confirmation,confirmKey)).toEqual(confirmed);expect(await db.prepare('SELECT status,outstanding_minor FROM finance_refund_obligations WHERE id=?').bind(s.refundId).first()).toEqual({status:'settled',outstanding_minor:0});
});

// Ignored named-constructor candidate. Do not activate/run until the reviewed
// owner implementation and native source epoch are released by Root.
// This is a verified current-period physical variance, not opening equity,
// acquisition, supplier receiving or a seeded positive Stock balance.
it('Sales-only first physical stock count posts verified current-period variance through independent original Stock authority',async()=>{
  const s=await actualSalesServiceBeforeGoods(),goods=await import('../../../src/server/modules/commerce/public/sales-goods-posting-setup'),inventory=await import('../../../src/server/modules/inventory/public/inventory-counts'),initial=await import('../../../src/server/modules/inventory/public/initial-physical-stock-count');
  const variance=await s.refs.api.createSalesPostingAccount(s.z.reviewer,{...await zeroSalesEnvelope(s.z.reviewer),input:{chartId:s.refs.chart.id,code:'5400',name:'Verified current-period physical stock variance',category:'expense',currency:'THB',isControl:false,controlKind:'none'}},key());
  await goods.initializeSalesGoodsPostingSetup(s.z.reviewer,await salesGoodsInput(s),key());
  expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
  expect(await db.prepare('SELECT category,currency,status,control_kind FROM finance_accounts WHERE id=? AND company_id=?').bind(variance.id,s.z.company).first()).toEqual({category:'expense',currency:'THB',status:'active',control_kind:'none'});
  expect(await db.prepare('SELECT state FROM finance_periods WHERE id=? AND company_id=?').bind(s.refs.period.id,s.z.company).first('state')).toBe('open');
  const warehouse=await saveMaster(s.z.owner,'warehouses',null,{branchId:s.z.branch,code:key(),name:'Actual first physical-count warehouse'},key()),product=await saveMaster(s.z.owner,'products',null,{sku:key(),name:'Actually counted initial stock item',itemType:'stock',price:'10',taxBps:700},key());
  const preparer=await s.z.actor('initial-physical-count-preparer','warehouse_manager','branch');
  expect(inventory.canPrepareCounts(preparer)).toBe(true);expect(inventory.canApproveCounts(s.z.manager)).toBe(true);expect(preparer.userId).not.toBe(s.z.manager.userId);
  const absence=()=>Promise.all(['commerce_stock','commerce_stock_moves','inventory_adjustment_moves','inventory_order_reservations','inventory_quality_holds'].map(table=>db.prepare(`SELECT count(*)::int n FROM ${table} WHERE warehouse_id=? AND product_id=?`).bind(warehouse.id,product.id).first('n')));
  expect(await absence()).toEqual([0,0,0,0,0]);expect(await db.prepare('SELECT count(*)::int n FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=? AND l.product_id=?').bind(warehouse.id,product.id).first('n')).toBe(0);
  const genericKey=key(),command=key(),input=inventory.countSchema.parse({warehouseId:String(warehouse.id),countDate:'2026-09-06',reason:'Physical count verifies two existing units at six THB each in the current period; no purchase or opening-equity claim',lines:[{productId:String(product.id),countedQuantity:'2',unitCost:'6'}]}),image=async(commandKey=command)=>({count:(await db.prepare('SELECT to_jsonb(r)::text image FROM inventory_counts r WHERE company_id=? ORDER BY image').bind(s.z.company).all()).results,journals:(await db.prepare('SELECT to_jsonb(r)::text image FROM finance_journals r WHERE company_id=? ORDER BY image').bind(s.z.company).all()).results,absent:await absence(),command:await zeroSalesCommandImage(commandKey)}),before=await image(genericKey);
  // Generic manual Count management remains Supply-only before and after
  // this exact initial-source inclusion. A failed generic key has no receipt.
  await expect(inventory.saveCount(preparer,null,input,genericKey)).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await image(genericKey)).toEqual(before);
  const workspace=await initial.initialPhysicalStockCountWorkspace(preparer,{warehouseId:String(warehouse.id),productId:String(product.id),countDate:input.countDate});
  if(typeof workspace.contextKey!=='string'||!/^([a-f0-9]{64})$/.test(workspace.contextKey))throw Error('Actual current initial physical Stock context required');
  const envelope={expectedVersion:0 as const,contextKey:workspace.contextKey,input},created=await initial.prepareInitialPhysicalStockCount(preparer,envelope,command);
  expect(created).toMatchObject({status:'draft',version:1});expect(await initial.prepareInitialPhysicalStockCount(preparer,envelope,command)).toEqual(created);const id=String(created.id);
  await initial.transitionInitialPhysicalStockCount(preparer,id,'submit',{expectedVersion:1,attachmentSetIds:[]},key());
  async function assignReview(stepKey:'submitted'|'approved',recordVersion:number){
    const flow=await initial.initialPhysicalStockCountFlow(s.z.owner,id);expect(flow.recordVersion).toBe(recordVersion);expect(flow.canManage).toBe(true);
    const input={stepKey,userId:s.z.manager.userId,reason:'Independent original Company Stock approver checks the actual physical variance and verified cost',saveDefault:false as const,expectedVersion:flow.version,recordVersion};
    const flowImage=async(commandKey:string)=>({owner:await image(commandKey),routes:await Promise.all(['flow_routes','flow_events','flow_defaults'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? AND kind='stock_count' ORDER BY image`).bind(s.z.company).all()).results)),notifications:(await db.prepare('SELECT to_jsonb(r)::text image FROM staff_notifications r WHERE company_id=? ORDER BY image').bind(s.z.company).all()).results});
    if(stepKey==='submitted'){
      // Approving Stock never creates the original Company organization.manage
      // authority to assign its primary. The real owner performs that UI action.
      expect((await initial.initialPhysicalStockCountFlow(s.z.manager,id)).canManage).toBe(false);
      const deniedKey=key(),deniedBefore=await flowImage(deniedKey);await expect(initial.assignInitialPhysicalStockCountFlow(s.z.manager,id,input,deniedKey)).rejects.toMatchObject({status:403,code:'FLOW_ADMIN_REQUIRED'});expect(await flowImage(deniedKey)).toEqual(deniedBefore);
      const defaultKey=key(),defaultBefore=await flowImage(defaultKey);await expect(initial.assignInitialPhysicalStockCountFlow(s.z.owner,id,{...input,saveDefault:true},defaultKey)).rejects.toMatchObject({name:'ZodError'});expect(await flowImage(defaultKey)).toEqual(defaultBefore);
    }
    const assignKey=key(),assigned=await initial.assignInitialPhysicalStockCountFlow(s.z.owner,id,input,assignKey);expect(assigned).toMatchObject({id,version:(flow.version||1)+1});
    const assignedFlow=await initial.initialPhysicalStockCountFlow(s.z.manager,id);expect(assignedFlow.current?.primary?.userId).toBe(s.z.manager.userId);expect(assignedFlow.canAct).toBe(true);
    const assignedImage=await flowImage(assignKey);expect(await initial.assignInitialPhysicalStockCountFlow(s.z.owner,id,input,assignKey)).toEqual(assigned);expect(await flowImage(assignKey)).toEqual(assignedImage);
    if(stepKey==='submitted'){const staleKey=key(),staleBefore=await flowImage(staleKey);await expect(initial.assignInitialPhysicalStockCountFlow(s.z.owner,id,input,staleKey)).rejects.toMatchObject({status:409,code:'VERSION_CONFLICT'});expect(await flowImage(staleKey)).toEqual(staleBefore);}
  }
  await assignReview('submitted',2);await expect(initial.transitionInitialPhysicalStockCount(preparer,id,'approve',{expectedVersion:2,reason:'Original preparer cannot independently approve their own physical count',attachmentSetIds:[]},key())).rejects.toMatchObject({status:403});
  await initial.transitionInitialPhysicalStockCount(s.z.manager,id,'approve',{expectedVersion:2,reason:'Independent actual Stock approver verifies the physical quantity and six THB unit cost',attachmentSetIds:[]},key());await assignReview('approved',3);
  const postKey=key(),postInput={expectedVersion:3,reason:'Post only the approved current-period physical variance using the actual mapped Inventory account and expense5400',attachmentSetIds:[]},posted=await initial.transitionInitialPhysicalStockCount(s.z.manager,id,'post',postInput,postKey);expect(posted).toMatchObject({id,status:'posted',version:4});
  const detail=await inventory.countDetail(s.z.manager,id),journalId=String(detail.count.journal_id);
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first()).toEqual({quantity_milli:2000,value_minor:1200});
  expect(await db.prepare('SELECT company_id,chart_id,period_id,status,source_type,source_id,is_opening FROM finance_journals WHERE id=?').bind(journalId).first()).toEqual({company_id:s.z.company,chart_id:s.refs.chart.id,period_id:s.refs.period.id,status:'posted',source_type:'stock_count',source_id:id,is_opening:0});
  expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(journalId).all()).results).toEqual([{account_id:s.mapping.inventoryAccountId,debit_minor:1200,credit_minor:0},{account_id:variance.id,debit_minor:0,credit_minor:1200}].sort((a,b)=>a.account_id.localeCompare(b.account_id)));
  const settled={post:await image(postKey),create:await image()};expect(await initial.transitionInitialPhysicalStockCount(s.z.manager,id,'post',postInput,postKey)).toEqual(posted);expect(await initial.prepareInitialPhysicalStockCount(preparer,envelope,command)).toEqual(created);expect({post:await image(postKey),create:await image()}).toEqual(settled);
  const manualKey=key(),manualBefore=await image(manualKey);await expect(inventory.saveCount(preparer,null,input,manualKey)).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await image(manualKey)).toEqual(manualBefore);
  // A new command may not reuse the captured pre-stock context to reset the
  // actual already-posted quantity/value. No existing balance is SQL-seeded.
  const repeatKey=key(),repeatBefore=await image(repeatKey);await expect(initial.prepareInitialPhysicalStockCount(preparer,envelope,repeatKey)).rejects.toMatchObject({status:409,code:'INITIAL_PHYSICAL_STOCK_HISTORY'});expect(await image(repeatKey)).toEqual(repeatBefore);
  // Explicit adverse native authority fixture, not role onboarding or a new
  // grant: replay cannot revive a genuinely revoked selected assignment.
  await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(preparer.assignmentId).run();
  const revokedBefore=await image();await expect(initial.prepareInitialPhysicalStockCount(preparer,envelope,command)).rejects.toMatchObject({status:403,code:'QUEUED_AUTHORITY_CHANGED'});expect(await image()).toEqual(revokedBefore);expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
});

// Ignored ONE owner-lifecycle regression. Root must release CAS/native after
// review. Reads are the real source-leased Sales/Supply Inventory owners;
// no synthetic obligation reader, status repair or Stock liquidation.
it('the actual Sales Inventory owner retains an initial physical Count draft and settles only its original posted decision without borrowing Supply or liquidating Stock',async()=>{
  const s=await actualSalesServiceBeforeGoods(),goods=await import('../../../src/server/modules/commerce/public/sales-goods-posting-setup'),initial=await import('../../../src/server/modules/inventory/public/initial-physical-stock-count'),inventory=await import('../../../src/server/modules/inventory/public/inventory-counts');
  const {withPackageSettlementSources}=await import('../../../src/server/modules/commercial/public/package-settlement-sources'),{lockSalesPackageObligationSources}=await import('../../../src/server/modules/sales/public/package-obligations'),{lockSalesInventoryPackageObligationSources,salesInventoryPackageObligations}=await import('../../../src/server/modules/inventory/public/sales-package-obligations'),{lockSupplyInventoryPackageObligationSources,supplyInventoryPackageObligations}=await import('../../../src/server/modules/inventory/public/supply-package-obligations');
  const variance=await s.refs.api.createSalesPostingAccount(s.z.reviewer,{...await zeroSalesEnvelope(s.z.reviewer),input:{chartId:s.refs.chart.id,code:'5400',name:'Actual Count lifecycle current-period physical variance',category:'expense',currency:'THB',isControl:false,controlKind:'none'}},key());
  await goods.initializeSalesGoodsPostingSetup(s.z.reviewer,await salesGoodsInput(s),key());
  const warehouse=await saveMaster(s.z.owner,'warehouses',null,{branchId:s.z.branch,code:key(),name:'Actual original Count lifecycle warehouse'},key()),product=await saveMaster(s.z.owner,'products',null,{sku:key(),name:'Actual original Count lifecycle stock item',itemType:'stock',price:'10',taxBps:700},key()),preparer=await s.z.actor('initial-stock-lifecycle-preparer','warehouse_manager','branch');
  expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
  const selection={warehouseId:String(warehouse.id),productId:String(product.id),countDate:'2026-09-06'},input={warehouseId:selection.warehouseId,countDate:selection.countDate,reason:'Verified two actual physical units at six THB in the current period, not opening equity or supplier acquisition',lines:[{productId:selection.productId,countedQuantity:'2',unitCost:'6',allocations:[]}]},stock=()=>db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first();
  const observe=async()=>{
    const sales=await withPackageSettlementSources(db,s.z.company,[lock=>lockSalesPackageObligationSources(db,lock),lock=>lockSalesInventoryPackageObligationSources(db,lock)],transaction=>salesInventoryPackageObligations(db,transaction,{companyId:s.z.company,packageId:'sales',packageVersion:1,moduleId:'inventory',now:Date.now()}));
    const supply=await withPackageSettlementSources(db,s.z.company,[lock=>lockSupplyInventoryPackageObligationSources(db,lock)],transaction=>supplyInventoryPackageObligations(db,transaction,{companyId:s.z.company,packageId:'supply',packageVersion:1,moduleId:'inventory',now:Date.now()}));
    // Collect BOTH genuine owner results before asserting either. A migration
    // regression must expose lost Sales ownership and accidental Supply debt.
    return {sales,supply};
  };
  const before=await observe();expect({sales:before.sales.outstanding,supply:before.supply.outstanding}).toEqual({sales:0,supply:0});expect(await stock()).toBeNull();
  const genericKey=key(),genericImage=await zeroSalesCommandImage(genericKey);await expect(inventory.saveCount(preparer,null,inventory.countSchema.parse(input),genericKey)).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await zeroSalesCommandImage(genericKey)).toEqual(genericImage);expect(await db.prepare('SELECT count(*)::int n FROM inventory_counts WHERE company_id=?').bind(s.z.company).first('n')).toBe(0);
  const workspace=await initial.initialPhysicalStockCountWorkspace(preparer,selection);if(typeof workspace.contextKey!=='string'||!/^[a-f0-9]{64}$/.test(workspace.contextKey))throw Error('Actual current initial Stock context required');
  const createKey=key(),body={expectedVersion:0 as const,contextKey:workspace.contextKey,input},created=await initial.prepareInitialPhysicalStockCount(preparer,body,createKey),id=String(created.id);expect(created).toMatchObject({id:`count-${createKey}`,status:'draft',version:1});
  const draft=await observe();expect({sales:draft.sales.outstanding,supply:draft.supply.outstanding},'Both actual source-leased owner observations for the normal named draft').toEqual({sales:1,supply:0});expect(draft.sales.revision).not.toBe(before.sales.revision);expect(await stock()).toBeNull();
  await initial.transitionInitialPhysicalStockCount(preparer,id,'submit',{expectedVersion:1,attachmentSetIds:[]},key());
  async function assign(stepKey:'submitted'|'approved',recordVersion:number){const flow=await initial.initialPhysicalStockCountFlow(s.z.owner,id);expect(flow.recordVersion).toBe(recordVersion);expect(flow.canManage).toBe(true);await initial.assignInitialPhysicalStockCountFlow(s.z.owner,id,{stepKey,userId:s.z.manager.userId,expectedVersion:flow.version,recordVersion,reason:'Actual original Company authority assigns a different independent Stock reviewer',saveDefault:false},key());}
  await assign('submitted',2);await initial.transitionInitialPhysicalStockCount(s.z.manager,id,'approve',{expectedVersion:2,reason:'Independent review of actual physical units and verified six THB unit cost',attachmentSetIds:[]},key());await assign('approved',3);
  const postKey=key(),postBody={expectedVersion:3,reason:'Post the approved current-period physical variance with the real mapped Inventory and expense5400 accounts',attachmentSetIds:[]},posted=await initial.transitionInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey);expect(posted).toMatchObject({id,status:'posted',version:4});
  const detail=await inventory.countDetail(s.z.manager,id),journalId=String(detail.count.journal_id);expect(await stock()).toEqual({quantity_milli:2000,value_minor:1200});expect(await db.prepare('SELECT company_id,chart_id,period_id,status,source_type,source_id,is_opening FROM finance_journals WHERE id=?').bind(journalId).first()).toEqual({company_id:s.z.company,chart_id:s.refs.chart.id,period_id:s.refs.period.id,status:'posted',source_type:'stock_count',source_id:id,is_opening:0});expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(journalId).all()).results).toEqual([{account_id:s.mapping.inventoryAccountId,debit_minor:1200,credit_minor:0},{account_id:variance.id,debit_minor:0,credit_minor:1200}].sort((a,b)=>a.account_id.localeCompare(b.account_id)));
  const settled=await observe();expect({sales:settled.sales.outstanding,supply:settled.supply.outstanding}).toEqual({sales:0,supply:0});expect(settled.sales.revision).not.toBe(draft.sales.revision);expect(await stock()).toEqual({quantity_milli:2000,value_minor:1200});
  // Exact original terminal receipts, not the visible status alone, settle
  // this Count decision. Retained positive Stock is still an actual asset.
  expect(await initial.transitionInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey)).toEqual(posted);expect(await initial.prepareInitialPhysicalStockCount(preparer,body,createKey)).toEqual(created);const replay=await observe();expect({sales:replay.sales.outstanding,supply:replay.supply.outstanding,salesRevision:replay.sales.revision,supplyRevision:replay.supply.revision}).toEqual({sales:0,supply:0,salesRevision:settled.sales.revision,supplyRevision:settled.supply.revision});expect(await stock()).toEqual({quantity_milli:2000,value_minor:1200});expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
});

// Ignored source-concurrency candidates only. Root must release coherent
// native owner/schema before activation. SQL below is timing instrumentation,
// never a positive stock, account, source or grant fixture.
async function initialPhysicalStockConcurrencyFixture(){
  const s=await actualSalesServiceBeforeGoods(),goods=await import('../../../src/server/modules/commerce/public/sales-goods-posting-setup'),initial=await import('../../../src/server/modules/inventory/public/initial-physical-stock-count');
  await s.refs.api.createSalesPostingAccount(s.z.reviewer,{...await zeroSalesEnvelope(s.z.reviewer),input:{chartId:s.refs.chart.id,code:'5400',name:'Current-period independently verified Stock variance',category:'expense',currency:'THB',isControl:false,controlKind:'none'}},key());
  await goods.initializeSalesGoodsPostingSetup(s.z.reviewer,await salesGoodsInput(s),key());
  const warehouseCode=key(),sku=key(),warehouse=await saveMaster(s.z.owner,'warehouses',null,{branchId:s.z.branch,code:warehouseCode,name:'Actual concurrent first Stock warehouse'},key()),product=await saveMaster(s.z.owner,'products',null,{sku,name:'Actual concurrent first Stock product',itemType:'stock',price:'10',taxBps:700},key()),preparer=await s.z.actor('initial-stock-concurrent-preparer','warehouse_manager','branch'),selection={warehouseId:String(warehouse.id),productId:String(product.id),countDate:'2026-09-06'};
  const workspace=await initial.initialPhysicalStockCountWorkspace(preparer,selection);if(typeof workspace.contextKey!=='string'||!/^[a-f0-9]{64}$/.test(workspace.contextKey))throw Error('Actual current initial Stock context required');
  const envelope={expectedVersion:0 as const,contextKey:workspace.contextKey,input:{warehouseId:selection.warehouseId,countDate:selection.countDate,reason:'Verified actual first physical count at two units and six THB per unit',lines:[{productId:selection.productId,countedQuantity:'2',unitCost:'6',allocations:[]}]}};
  const state=async(command:string)=>({counts:(await db.prepare('SELECT to_jsonb(r)::text image FROM inventory_counts r WHERE company_id=? ORDER BY image').bind(s.z.company).all()).results,lines:(await db.prepare('SELECT to_jsonb(l)::text image FROM inventory_count_lines l JOIN inventory_counts c ON c.id=l.count_id WHERE c.company_id=? ORDER BY image').bind(s.z.company).all()).results,sources:(await db.prepare('SELECT to_jsonb(r)::text image FROM inventory_initial_physical_stock_sources r WHERE company_id=? ORDER BY image').bind(s.z.company).all()).results,stock:await db.prepare('SELECT to_jsonb(r)::text image FROM commerce_stock r WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first(),journals:(await db.prepare("SELECT to_jsonb(r)::text image FROM finance_journals r WHERE company_id=? AND source_type='stock_count' ORDER BY image").bind(s.z.company).all()).results,command:await zeroSalesCommandImage(command)});
  expect(await db.prepare('SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first()).toBeNull();expect((await readCompanyPackageGrants(db,s.z.company)).map(row=>row.packageId)).toEqual(['sales']);
  return {s,initial,warehouse,product,warehouseCode,sku,preparer,selection,envelope,state};
}

it.each(['warehouse','product'] as const)('initial physical Stock waits for an actual same-Company %s writer then rejects its stale selected context without retry masking',async kind=>{
  const c=await initialPhysicalStockConcurrencyFixture(),writerKey=key(),command=key(),before=await c.state(command),fault=`initial_stock_writer_${key().replaceAll('-','')}`,table=kind==='warehouse'?'commerce_warehouses':'commerce_products',id=kind==='warehouse'?String(c.warehouse.id):String(c.product.id),originalVersion=Number(await db.prepare(`SELECT version FROM ${table} WHERE id=? AND company_id=?`).bind(id,c.s.z.company).first('version'));
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${writerKey}') EXECUTE FUNCTION ${fault}()`);
  try{
    const {measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),observed=await measureReads(async()=>{
      let writerFinished=false;
      const writer=saveMaster(c.s.z.owner,kind==='warehouse'?'warehouses':'products',id,kind==='warehouse'?{branchId:c.s.z.branch,code:c.warehouseCode,name:'Actual normal warehouse writer committed a revised name',expectedVersion:originalVersion}:{sku:c.sku,name:'Actual normal product writer committed a revised name',itemType:'stock',price:'10',taxBps:700,expectedVersion:originalVersion},writerKey).finally(()=>{writerFinished=true;}),writerOutcome=Promise.allSettled([writer]);
      let staged=false;for(let attempt=0;attempt<100;attempt++){staged=!!await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called');if(staged||writerFinished)break;await new Promise(resolve=>setTimeout(resolve,10));}
      if(!staged){const [written]=await writerOutcome;if(written.status==='rejected')throw written.reason;throw Error('Actual normal writer must reach the held native receipt before constructor starts');}
      expect(writerFinished).toBe(false);
      const [written,prepared]=await Promise.all([writerOutcome,Promise.allSettled([c.initial.prepareInitialPhysicalStockCount(c.preparer,c.envelope,command)])]);return {written:written[0],prepared:prepared[0]};
    });
    expect(observed.result.written.status).toBe('fulfilled');expect(observed.result.prepared.status).toBe('rejected');if(observed.result.prepared.status!=='rejected')throw Error('The actual pre-writer selection must become stale');expect(observed.result.prepared.reason).toMatchObject({status:409,code:'INITIAL_PHYSICAL_STOCK_CONTEXT_CHANGED'});
    expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
    expect(Number(await db.prepare(`SELECT version FROM ${table} WHERE id=? AND company_id=?`).bind(id,c.s.z.company).first('version'))).toBe(originalVersion+1);expect(await c.state(command)).toEqual(before);
    const refreshed=await c.initial.initialPhysicalStockCountWorkspace(c.preparer,c.selection);expect(refreshed.contextKey).toMatch(/^[a-f0-9]{64}$/);expect(refreshed.contextKey).not.toBe(c.envelope.contextKey);
  }finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
});

it('two actual same-context new keys create exactly one initial physical Stock Count and source without retry masking',async()=>{
  const c=await initialPhysicalStockConcurrencyFixture(),keys=[key(),key()],{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics'),observed=await measureReads(()=>Promise.allSettled(keys.map(command=>c.initial.prepareInitialPhysicalStockCount(c.preparer,c.envelope,command))));
  expect(observed.result.filter(row=>row.status==='fulfilled')).toHaveLength(1);expect(observed.result.filter(row=>row.status==='rejected')).toHaveLength(1);const winner=observed.result.findIndex(row=>row.status==='fulfilled'),successful=observed.result[winner],failed=observed.result.find(row=>row.status==='rejected');if(!successful||successful.status!=='fulfilled'||!failed||failed.status!=='rejected')throw Error('One actual initial Count and one refusal are required');expect(failed.reason).toMatchObject({status:409,code:'INITIAL_PHYSICAL_STOCK_HISTORY'});
  expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
  expect(await db.prepare('SELECT count(*)::int n FROM inventory_counts WHERE company_id=?').bind(c.s.z.company).first('n')).toBe(1);expect(await db.prepare('SELECT count(*)::int n FROM inventory_initial_physical_stock_sources WHERE company_id=?').bind(c.s.z.company).first('n')).toBe(1);expect(await db.prepare('SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(c.warehouse.id,c.product.id).first()).toBeNull();expect(await zeroSalesCommandImage(keys[1-winner])).toEqual([[],[],[]]);
  const committed=await c.state(keys[winner]);expect(await c.initial.prepareInitialPhysicalStockCount(c.preparer,c.envelope,keys[winner])).toEqual(successful.value);expect(await c.state(keys[winner])).toEqual(committed);
});

it('ordinary Sales journal Files retain the captured native owner through deferred source loss and exact-key retry',async()=>{
  // Reuse the declared legacy goods/Stock fixture. This proves the included
  // Finance/File fence, not normal initial Stock acquisition or zero-ledger setup.
  const source=await paidReturn('sales'),{requireFinanceDocumentHistory}=await import('../../../src/server/modules/finance/public/document-purchases'),{getInvoice}=await import('../../../src/server/modules/finance/public/finance'),{bindRecordAttachmentSet,attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service');
  const grant={companyId:f.company,packageId:'sales',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Actual ordinary Sales journal retained Files source',evidence:'test:ordinary-sales-source-finality'},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-ordinary-sales-files-provider'}),obligations:async()=>{throw Error('ONLY_REAL_OWNER_REVIEW_CAN_INACTIVATE');}});
  await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
  const invoice=await getInvoice(f.owner,source.invoiceId),journalId=String(invoice.invoice.journal_id),version=Number(await db.prepare('SELECT version FROM finance_journals WHERE id=?').bind(journalId).first('version'));
  // This is the server-derived ordinary proof; no input flag chooses its branch.
  expect(await requireFinanceDocumentHistory(db,f.owner,'journal',journalId)).toMatchObject({capability:'finance.sales-posting',commerceProof:'ordinary',additionalCapabilities:[]});
  const file=await seedAttachmentSet(db,f.owner,{targetType:'finance_journal',targetId:journalId,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:version}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes='%PDF-1.7';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});
  const command=key(),fault=`ordinary_sales_file_${key().replaceAll('-','')}`,tables=['commerce_orders','commerce_fulfillments','commerce_returns','finance_invoices','finance_payments','finance_allocations','finance_journals','finance_journal_lines','finance_book_postings','finance_financial_events','money_transaction_snapshots','inventory_counts','inventory_count_lines','commerce_stock','attachment_sets','attachment_links','attachment_events','attachment_access_events','attachments','command_assertions','command_receipts','audit_events','outbox_events','assignments','company_package_grants'],image=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r ORDER BY image`).all()).results)),before=await image();
  // Labelled adversarial deferred mutation of a real owner FK. It cannot be
  // upgraded to another source proof after capture. No private marker is forged.
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');UPDATE commerce_fulfillments SET invoice_id=NULL,version=version+1 WHERE id='${source.fulfillmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try{
    let failure:unknown;try{await bindRecordAttachmentSet(db,f.owner,file.setId,version,command);}catch(error){failure=error;}
    expect(failure).toBeInstanceOf(Error);expect((failure as {cause?:unknown}).cause??failure).toMatchObject({code:'23514',detail:expect.stringContaining('finance-document-owner')});expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await image()).toEqual(before);
  }finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  const bound=await bindRecordAttachmentSet(db,f.owner,file.setId,version,command),committed=await image();expect(await bindRecordAttachmentSet(db,f.owner,file.setId,version,command)).toEqual(bound);expect(await image()).toEqual(committed);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:'bound',link_state:'bound'});expect(await(await attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')).object.text()).toBe(bytes);expect(await requireFinanceDocumentHistory(db,f.owner,'journal',journalId)).toMatchObject({capability:'finance.sales-posting',commerceProof:'ordinary',additionalCapabilities:[]});expect((await readCompanyPackageGrants(db,f.company)).map(row=>[row.packageId,row.state])).toEqual([['sales','draining']]);
});

it('ordinary Purchase journal Files retain the captured native owner through deferred source loss and exact-key retry',async()=>{
  // Reuse the declared legacy goods/Stock fixture. This proves the included
  // Finance/File fence, not normal initial Stock acquisition or zero-ledger setup.
  const source=await paidReturn('purchase'),{requireFinanceDocumentHistory}=await import('../../../src/server/modules/finance/public/document-purchases'),{getInvoice}=await import('../../../src/server/modules/finance/public/finance'),{bindRecordAttachmentSet,attachmentContent}=await import('../../../src/server/modules/files/public/attachments/service');
  const grant={companyId:f.company,packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Actual ordinary Purchase journal retained Files source',evidence:'test:ordinary-purchase-source-finality'},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-ordinary-purchase-files-provider'}),obligations:async()=>{throw Error('ONLY_REAL_OWNER_REVIEW_CAN_INACTIVATE');}});
  await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
  const invoice=await getInvoice(f.owner,source.invoiceId),journalId=String(invoice.invoice.journal_id),version=Number(await db.prepare('SELECT version FROM finance_journals WHERE id=?').bind(journalId).first('version'));
  // This is the server-derived ordinary proof; no input flag chooses its branch.
  // The original Supplier correction/reversal history tests remain unchanged;
  // this separate ordinary source may never upgrade to that retained proof.
  expect(await requireFinanceDocumentHistory(db,f.owner,'journal',journalId)).toMatchObject({capability:'finance.inventory-valuation',commerceProof:'ordinary',additionalCapabilities:[]});
  const file=await seedAttachmentSet(db,f.owner,{targetType:'finance_journal',targetId:journalId,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:version}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes='%PDF-1.7';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});
  const command=key(),fault=`ordinary_purchase_file_${key().replaceAll('-','')}`,tables=['commerce_orders','commerce_fulfillments','commerce_returns','finance_invoices','finance_payments','finance_allocations','finance_journals','finance_journal_lines','finance_book_postings','finance_financial_events','money_transaction_snapshots','inventory_counts','inventory_count_lines','commerce_stock','attachment_sets','attachment_links','attachment_events','attachment_access_events','attachments','command_assertions','command_receipts','audit_events','outbox_events','assignments','company_package_grants'],image=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r ORDER BY image`).all()).results)),before=await image();
  // Labelled adversarial deferred mutation of a real owner FK. It cannot be
  // upgraded to another source proof after capture. No private marker is forged.
  await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');UPDATE commerce_fulfillments SET invoice_id=NULL,version=version+1 WHERE id='${source.fulfillmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try{
    let failure:unknown;try{await bindRecordAttachmentSet(db,f.owner,file.setId,version,command);}catch(error){failure=error;}
    expect(failure).toBeInstanceOf(Error);expect((failure as {cause?:unknown}).cause??failure).toMatchObject({code:'23514',detail:expect.stringContaining('finance-document-owner')});expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await image()).toEqual(before);
  }finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
  const bound=await bindRecordAttachmentSet(db,f.owner,file.setId,version,command),committed=await image();expect(await bindRecordAttachmentSet(db,f.owner,file.setId,version,command)).toEqual(bound);expect(await image()).toEqual(committed);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:'bound',link_state:'bound'});expect(await(await attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')).object.text()).toBe(bytes);expect(await requireFinanceDocumentHistory(db,f.owner,'journal',journalId)).toMatchObject({capability:'finance.inventory-valuation',commerceProof:'ordinary',additionalCapabilities:[]});expect((await readCompanyPackageGrants(db,f.company)).map(row=>[row.packageId,row.state])).toEqual([['supply','draining']]);
});


it('Company Service refund proof keeps every original predicate and restores caller planning settings',async()=>{
 const s=await companyServiceCashFixture(false),{createHash}=await import('node:crypto');
 await db.transaction(async tx=>{
  const before=await tx.prepare("SELECT current_setting('join_collapse_limit') value").first<string>('value');
  if(before===null)throw Error('NATIVE_CALLER_JOIN_SETTING_REQUIRED');
  const definition=await tx.prepare("SELECT p.prosrc,p.proconfig FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public' AND p.proname='finance_company_service_refund_source_valid' AND p.proargtypes='25'::oidvector").first<{prosrc:string;proconfig:string[]|null}>();
  if(!definition)throw Error('NATIVE_SERVICE_REFUND_PROOF_REQUIRED');
  expect(createHash('sha256').update(definition.prosrc).digest('hex')).toBe('0c9cacad4733184cca59060fc3db98cf9edbd0ab1b4f2e28caeb8ef0e0c3e51d');
  expect(definition.proconfig).toEqual(['join_collapse_limit=1']);
  expect(await tx.prepare('SELECT public.finance_company_service_refund_source_valid(?) valid').bind(s.refundId).first<boolean>('valid')).toBe(true);
  expect(await tx.prepare("SELECT current_setting('join_collapse_limit') value").first<string>('value')).toBe(before);
  expect(await tx.prepare('SELECT public.finance_company_service_refund_source_valid(?) valid').bind(`missing-service-refund-${key()}`).first<boolean>('valid')).toBe(false);
  expect(await tx.prepare("SELECT current_setting('join_collapse_limit') value").first<string>('value')).toBe(before);
 });
});
