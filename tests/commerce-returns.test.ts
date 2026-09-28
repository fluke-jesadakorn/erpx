import { beforeAll,beforeEach,expect,it,vi } from "vitest";
import { applyMigrations,env } from "./runtime-env";
import { expenseDb as db,expenseFixture,key } from "./helpers/expense-fixture";
import { deliveryEvidence } from "./helpers/delivery-evidence";
import { assignFlowPrimary,resolveFlow } from "../src/server/flow-routing";
import { postInvoice,postJournal } from "../src/server/finance";
import { completeCommerceReturn,decideCommerceReturn,fulfillOrder,getOrder,invoiceFulfillment,orderSchema,requestCommerceReturn,saveMaster,saveOrder,settleInvoice,transitionOrder } from "../src/server/commerce";
import { attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet } from "./helpers/attachment-v2";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{enableAttachmentV2ForTest();await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.manager.role="finance_approver";f.manager.branchId=null;f.manager.departmentId=null;f.manager.assignmentId=await f.grant(f.manager,"finance_approver");f.head.role="purchasing_manager";f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,"purchasing_manager");});

async function assign(kind:"order"|"invoice"|"journal",id:string,stepKey:string,userId:string){const flow=await resolveFlow(db,f.owner,kind,id);await assignFlowPrimary(db,f.owner,kind,id,{stepKey,userId,reason:"Assign return fixture responsibility",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());}

it("returns a supplier receipt through independent review, stock reversal and debit-note GL",async()=>{
  const warehouse=await saveMaster(f.owner,"warehouses",null,{branchId:f.branch,code:key(),name:"Return warehouse"},key()),product=await saveMaster(f.owner,"products",null,{sku:key(),name:"Return product",price:"10"},key()),party=await saveMaster(f.owner,"parties",null,{kind:"supplier",code:key(),name:"Return supplier"},key());
  const order=await saveOrder(f.owner,null,orderSchema.parse({kind:"purchase",partyId:party.id,warehouseId:warehouse.id,orderDate:"2026-09-02",dueDate:"2026-09-03",lines:[{productId:product.id,quantity:"2",price:"10",taxBps:0}]}),key()),orderId=String(order.id);
  await transitionOrder(f.owner,orderId,{action:"submit",expectedVersion:1},key());await transitionOrder(f.head,orderId,{action:"approve",expectedVersion:2},key());await assign("order",orderId,"stock_gl",f.manager.userId);
  let details=await getOrder(f.owner,orderId);const receiptFiles=await seedAttachmentSet(db,f.owner,{targetType:"purchase_order",targetId:orderId,purposeKey:"delivery_note",componentRef:"goods-receipt",expectedTargetVersion:3});const receipt=await fulfillOrder(f.owner,orderId,{expectedVersion:3,date:"2026-09-02",lines:[{lineId:String(details.lines[0].id),quantity:"2"}],attachmentSetIds:[receiptFiles.setId]},key());
  expect(await attachmentBindingState(db,receiptFiles.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"purchase_receipt",binding_id:String(receipt.fulfillmentId)});
  const goodsJournal=String((await db.prepare("SELECT journal_id FROM commerce_fulfillments WHERE id=?").bind(receipt.fulfillmentId).first("journal_id")));await assign("journal",goodsJournal,"draft",f.manager.userId);await postJournal(f.manager,goodsJournal,key(),{expectedVersion:1});
  await assign("order",orderId,"invoicing",f.owner.userId);const invoice=await invoiceFulfillment(f.owner,String(receipt.fulfillmentId),{expectedVersion:1,date:"2026-09-02",dueDate:"2026-09-03"},key());await assign("invoice",String(invoice.invoiceId),"draft",f.manager.userId);await postInvoice(f.manager,String(invoice.invoiceId),{expectedVersion:1,periodId:f.period,entryDate:"2026-09-02"},key());
  await assign("invoice",String(invoice.invoiceId),"posted",f.payer.userId);
  const pendingSlip=await seedAttachmentSet(db,f.payer,{targetType:"invoice_payment",targetId:String(invoice.invoiceId),purposeKey:"payment_slip",componentKind:"action",componentRef:"legacy-upload-adapter",expectedTargetVersion:2,sourceState:"quarantined"});
  await expect(settleInvoice(f.payer,String(invoice.invoiceId),{expectedVersion:2,date:"2026-09-03",amount:"1",reference:"Pending scan proof",attachmentSetIds:[pendingSlip.setId]},key())).rejects.toMatchObject({code:"ATTACHMENT_SCAN_PENDING"});
  expect(await attachmentBindingState(db,pendingSlip.setId)).toMatchObject({set_state:"draft",link_state:"draft"});
  const rawSlip=await f.attach(String(invoice.invoiceId),"invoice_payment");
  await expect(settleInvoice(f.payer,String(invoice.invoiceId),{expectedVersion:2,date:"2026-09-03",amount:"1",reference:"Unlinked raw proof",paymentSlipId:rawSlip,attachmentSetIds:[]},key())).rejects.toMatchObject({code:"PAYMENT_SLIP_REQUIRED"});
  const {createSupplierBatch,decideSupplierBatch}=await import('../src/server/supplier-payments');
  const reserved=await createSupplierBatch(f.owner,{bankAccountId:f.accounts.cash,plannedDate:'2026-09-03',items:[{invoiceId:String(invoice.invoiceId),expectedVersion:2,amount:'20',beneficiaryName:'Simulated supplier',beneficiaryAccount:'SIMULATED-001'}]},key());
  details=await getOrder(f.owner,orderId);const line=details.fulfillmentLines[0],returnFiles=await seedAttachmentSet(db,f.owner,{targetType:"commerce_fulfillment",targetId:String(receipt.fulfillmentId),purposeKey:"return_condition",componentRef:"return-request",expectedTargetVersion:2}),requested=await requestCommerceReturn(f.owner,String(receipt.fulfillmentId),{expectedVersion:2,date:"2026-09-03",reason:"Supplier accepted damaged unit",lines:[{lineId:String(line.id),quantity:"1"}],attachmentSetIds:[returnFiles.setId]},key());
  expect(await attachmentBindingState(db,returnFiles.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"commerce_return",binding_id:String(requested.id)});
  await expect(decideCommerceReturn(f.owner,String(requested.id),"approve",{expectedVersion:1,reason:"Self approval"},key())).rejects.toMatchObject({code:"INDEPENDENT_APPROVER_REQUIRED"});
  await expect(decideCommerceReturn(f.head,String(requested.id),'approve',{expectedVersion:1,reason:'Cannot return a reserved invoice'},key())).rejects.toMatchObject({code:'CONFLICT'});
  await decideSupplierBatch(f.owner,String(reserved.id),{expectedVersion:1,reason:'Cancel unexported supplier payment before returning goods'},key(),'cancel');
  const approved=await decideCommerceReturn(f.head,String(requested.id),"approve",{expectedVersion:1,reason:"Warehouse evidence and supplier authorization reviewed"},key());
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(warehouse.id,product.id).first()).toEqual({quantity_milli:1000,value_minor:1000});
  for(const journalId of [String(approved.stockJournalId),String(approved.creditJournalId)]){await assign("journal",journalId,"draft",f.manager.userId);await postJournal(f.manager,journalId,key(),{expectedVersion:1});const sums=await db.prepare("SELECT SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?").bind(journalId).first();expect(sums?.debit).toBe(sums?.credit);}
  const completed=await completeCommerceReturn(f.manager,String(requested.id),{expectedVersion:2},key());expect(completed).toMatchObject({status:"posted",version:3});
  expect(await db.prepare("SELECT outstanding_minor,status FROM finance_invoices WHERE id=?").bind(invoice.invoiceId).first()).toEqual({outstanding_minor:1000,status:"partially_paid"});
  await expect(requestCommerceReturn(f.owner,String(receipt.fulfillmentId),{expectedVersion:2,date:"2026-09-04",reason:"Exceeds remaining quantity",lines:[{lineId:String(line.id),quantity:"2"}]},key())).rejects.toMatchObject({code:"OVER_RETURN"});
});

async function paidReturn(kind:'sales'|'purchase',paid='15',taxBps=0) {
  const {settleInvoice}=await import('../src/server/commerce');
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
  await assign('order',orderId,'stock_gl',f.manager.userId);
  let details=await getOrder(f.owner,orderId);
  const fulfillment=await fulfillOrder(f.owner,orderId,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:String(details.lines[0].id),quantity:'2'}],...(kind==='sales'?await deliveryEvidence(db,f.owner,orderId):{})},key());
  const goodsJournal=String(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(fulfillment.fulfillmentId).first('journal_id'));
  await assign('journal',goodsJournal,'draft',f.manager.userId);await postJournal(f.manager,goodsJournal,key(),{expectedVersion:1});
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
  const requestKey=key(),input={expectedVersion:2,date:'2026-09-04',reason:'Customer or supplier accepted the whole return',lines:[{lineId:String(details.fulfillmentLines[0].id),quantity:'2',...(kind==='sales'?{condition:'unknown' as const,conditionNote:'Returned units require independent Quality inspection',qualityDueDate:'2026-09-05'}:{})}]};
  const requested=await requestCommerceReturn(f.owner,String(fulfillment.fulfillmentId),input,requestKey);
  expect(await requestCommerceReturn(f.owner,String(fulfillment.fulfillmentId),input,requestKey)).toEqual(requested);
  await expect(requestCommerceReturn(f.owner,'different-source',input,requestKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  // A later configuration edit must not redirect historical stock reversals.
  await db.prepare('UPDATE commerce_settings SET inventory_account_id=?,cogs_account_id=?,clearing_account_id=? WHERE company_id=?').bind(f.accounts.expense,f.accounts.cash,f.accounts.cash,f.company).run();
  const approved=await decideCommerceReturn(f.head,String(requested.id),'approve',{expectedVersion:1,reason:'Return condition and original evidence reviewed'},key());
  const originalPosting=(await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(goodsJournal).all<{account_id:string;debit_minor:number;credit_minor:number}>()).results;
  const reversalPosting=(await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(approved.stockJournalId).all()).results;
  expect(reversalPosting).toEqual(originalPosting.map(line=>({account_id:line.account_id,debit_minor:line.credit_minor,credit_minor:line.debit_minor})));
  for(const journalId of [String(approved.stockJournalId),String(approved.creditJournalId)]){
    await assign('journal',journalId,'draft',f.manager.userId);await postJournal(f.manager,journalId,key(),{expectedVersion:1});
  }
  const {beginClose}=await import('../src/server/finance');
  await expect(beginClose(f.owner,f.period,key())).rejects.toMatchObject({code:'CLOSE_NOT_READY'});
  const completeKey=key(),completed=await completeCommerceReturn(f.manager,String(requested.id),{expectedVersion:2},completeKey);
  expect(await completeCommerceReturn(f.manager,String(requested.id),{expectedVersion:2},completeKey)).toEqual(completed);
  const refundId=String(completed.refundId);
  expect(completed).toMatchObject({creditedMinor:2000+Math.round(2000*taxBps/10000)-Number(paid)*100,refundMinor:Number(paid)*100});
  expect(await db.prepare('SELECT outstanding_minor,status FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({outstanding_minor:0,status:'paid'});
  expect(await db.prepare('SELECT COUNT(*) n FROM finance_refund_obligations WHERE return_id=?').bind(requested.id).first('n')).toBe(1);
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first()).toEqual(kind==='sales'?{quantity_milli:2000,value_minor:1200}:{quantity_milli:0,value_minor:0});
  if(kind==='sales'){
    expect(await db.prepare("SELECT status,source_type,source_id,held_quantity_milli,quantity_scope FROM inventory_quality_holds WHERE company_id=? AND source_type='customer_return' AND source_id=?").bind(f.company,`${requested.id}-0`).first()).toEqual({status:'held',source_type:'customer_return',source_id:`${requested.id}-0`,held_quantity_milli:2000,quantity_scope:'partial'});
    expect(await db.prepare('SELECT inventory_quarantined_quantity(?,?) quantity').bind(warehouse.id,product.id).first('quantity')).toBe(2000);
    await expect(db.prepare('UPDATE commerce_return_lines SET condition_note=? WHERE return_id=?').bind('Rewrite condition',requested.id).run()).rejects.toThrow('RETURN_QUALITY_EVIDENCE_IMMUTABLE');
    const holdId=String(await db.prepare("SELECT id FROM inventory_quality_holds WHERE source_type='customer_return' AND source_id=?").bind(`${requested.id}-0`).first('id'));
    const {inspectQualityHold,releaseQualityHold}=await import('../src/server/quality');
    await inspectQualityHold(f.owner,holdId,{expectedVersion:1,result:'pass',evidence:'Packaging and functional check passed after customer return',measurements:[],attachmentIds:[]},key());
    f.tie.role='quality_manager';f.tie.assignmentId=await f.grant(f.tie,'quality_manager');
    await releaseQualityHold(f.tie,holdId,{expectedVersion:2,reason:'Independent review confirms the returned units are resalable'},key());
    expect(await db.prepare('SELECT inventory_quarantined_quantity(?,?) quantity').bind(warehouse.id,product.id).first('quantity')).toBe(0);
    expect((await getOrder(f.owner,orderId)).returnQualityHolds[0]).toMatchObject({id:holdId,status:'released',condition_code:'unknown'});
    const {customerTimeline}=await import('../src/server/customer-timeline');
    const timeline=await customerTimeline(f.owner,String(party.id));
    expect(timeline.items.find(e=>e.source_type==='fulfillment')).toMatchObject({source_id:String(fulfillment.fulfillmentId),href:`/commerce/orders/${orderId}`});
    expect(timeline.items.find(e=>e.source_type==='return')).toMatchObject({source_id:String(requested.id),status:'posted',href:`/commerce/orders/${orderId}`});
    expect(JSON.stringify(timeline)).not.toContain(refundId);
  }
  return {refundId,invoiceId,requested,warehouse,product,settlement};
}
async function approveObligation(id:string) {
 const {approveRefund}=await import('../src/server/refunds');
 const input={expectedVersion:1,beneficiaryName:'Simulated counterparty',beneficiaryAccount:'TEST-BANK-123',reason:'Checked original invoice, return and beneficiary independently'};
 await expect(approveRefund(f.manager,id,input,key())).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});
 await expect(approveRefund(f.owner,id,input,key())).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});
 f.head.role='finance_approver';f.head.assignmentId=await f.grant(f.head,'finance_approver');
 await approveRefund(f.head,id,input,key());
}
async function prepare(id:string,amount:string) {
 const {prepareRefundInstruction}=await import('../src/server/refunds');
 const version=Number(await db.prepare('SELECT version FROM finance_refund_obligations WHERE id=?').bind(id).first('version'));
 return prepareRefundInstruction(f.payer,id,{expectedVersion:version,amount,plannedDate:'2026-09-05',bankAccount:f.accounts.cash,cashAccountId:f.accounts.cash},key());
}
async function bankEvidence(amount:string) {
 const {importBankStatement}=await import('../src/server/finance');
 const statement=await importBankStatement(f.owner,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:'2026-09-05',opening:'100',closing:(100+Number(amount)).toFixed(2),sourceHash:key(),rows:[{externalId:key(),transactionDate:'2026-09-05',description:'Simulation bank evidence',amount,currency:'THB'}]},key());
 const bank=await db.prepare('SELECT * FROM finance_bank_transactions WHERE statement_id=?').bind(statement.id).first<{id:string;version:number}>();
 expect(bank).toBeTruthy();return bank!;
}
it.each(['sales','purchase'] as const)('separates invoice credit from %s cash refund and reconciles partial/final payments exactly once',async(kind)=>{
 const {approveRefund,prepareRefundInstruction,exportRefundInstructions,confirmRefundInstruction,refundWorkspace,refundWork,recoverRefundExport}=await import('../src/server/refunds');
 const source=await paidReturn(kind),id=source.refundId;
 await expect(prepare(id,'15')).rejects.toBeDefined();
 await approveObligation(id);
 const before=await refundWorkspace(f.payer,id);
 expect(before.owner).toMatchObject({amount_minor:1500,outstanding_minor:1500,status:'approved',direction:kind==='sales'?'outgoing':'incoming'});
 await expect(prepareRefundInstruction(f.head,id,{expectedVersion:2,amount:'1',plannedDate:'2026-09-05',bankAccount:f.accounts.cash,cashAccountId:f.accounts.cash},key())).rejects.toMatchObject({code:'INDEPENDENT_PAYER_REQUIRED'});
 expect((await refundWork(f.payer)).find(row=>row.id===id)).toMatchObject({queue:'todo',canAct:true});
 const {listStaffNotifications}=await import('../src/server/staff-notifications');
 expect((await listStaffNotifications(db,f.payer)).some(note=>note.record_type==='refund'&&note.record_id===id)).toBe(true);
 expect((await listStaffNotifications(db,{...f.payer,role:'employee'})).some(note=>note.record_id===id)).toBe(false);
 await expect(refundWorkspace({...f.payer,companyId:'unrelated-company'},id)).rejects.toMatchObject({status:404});
 await expect(refundWorkspace({...f.payer,branchId:'unrelated-branch'},id)).rejects.toMatchObject({status:404});
 const first=await prepare(id,'6'),instructionId=String(first.id),exportKey=key(),exportInput={instructions:[{id:instructionId,expectedVersion:1}]};
 const {calendarData}=await import('../src/server/calendar');
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
 const {recordRefundException,exportRefundInstructions,prepareRefundInstruction,refundWorkspace}=await import('../src/server/refunds');
 const {refundId:id}=await paidReturn('sales','20');await approveObligation(id);
 const first=await prepare(id,'20');await exportRefundInstructions(f.payer,{instructions:[{id:String(first.id),expectedVersion:1}]},key());
 await expect(recordRefundException(f.payer,String(first.id),{expectedVersion:2,outcome:'cancelled',evidence:'Cannot cancel an exported transfer'},key())).rejects.toMatchObject({code:'EXPORTED_REFUND_REQUIRES_REVIEW'});
 await recordRefundException(f.payer,String(first.id),{expectedVersion:2,outcome:'unknown',evidence:'Bank timed out after submission; outcome unknown'},key());
 await expect(prepare(id,'20')).rejects.toMatchObject({status:409});
 const {refundWork}=await import('../src/server/refunds');
 expect((await refundWork(f.manager)).find(item=>item.id===id)).toMatchObject({canAct:true,responsibleQueue:'Independent Finance approver'});
 const {beginClose,closePeriod}=await import('../src/server/finance');
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
 await expect(closePeriod(f.head,f.period,{expectedVersion:Number(closing.version),proof:'Late bank data must block final close'},key())).rejects.toMatchObject({status:409});
});

it('keeps tax in the full paid credit and creates exactly the gross refund obligation',async()=>{
 const {refundWorkspace}=await import('../src/server/refunds');
 const source=await paidReturn('sales','21.40',700),data=await refundWorkspace(f.manager,source.refundId);
 expect(data.owner).toMatchObject({amount_minor:2140,outstanding_minor:2140,status:'pending'});
 const note=await db.prepare('SELECT r.subtotal_minor,r.tax_minor,r.total_minor,r.credited_minor,r.refund_minor FROM commerce_returns r WHERE id=?').bind(source.requested.id).first();
 expect(note).toEqual({subtotal_minor:2000,tax_minor:140,total_minor:2140,credited_minor:0,refund_minor:2140});
 const tax=await db.prepare("SELECT SUM(l.debit_minor-l.credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted' AND l.account_id=?").bind(f.company,f.accounts.tax).first('balance');
 expect(tax).toBe(0);
});
