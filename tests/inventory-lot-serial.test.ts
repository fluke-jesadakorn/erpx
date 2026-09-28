import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {deliveryEvidence} from './helpers/delivery-evidence';
import {decideCommerceReturn,fulfillOrder,getOrder,invoiceFulfillment,orderSchema,requestCommerceReturn,saveMaster,saveOrder,transitionOrder} from '../src/server/commerce';
import {inspectQualityHold,releaseQualityHold} from '../src/server/quality';
import {assignFlowPrimary,resolveFlow} from '../src/server/flow-routing';
import {saveTransfer,transitionTransfer,transferDetail,transferSchema} from '../src/server/inventory-transfers';
import {decideTransitLoss,dispatchTransit,getTransit,receiveTransit,requestTransitLoss,transitDetail} from '../src/server/inventory-transit';
import {countDetail,saveCount,transitionCount} from '../src/server/inventory-counts';
import {postInvoice,postJournal} from '../src/server/finance';
import {attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet} from './helpers/attachment-v2';

vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>,warehouse:string,supplier:string,customer:string;
beforeAll(async()=>{enableAttachmentV2ForTest();await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
  f=await expenseFixture();
  for(const [actor,role] of [[f.head,'holding_owner'],[f.reviewer,'quality_inspector'],[f.manager,'quality_manager']] as const){actor.role=role;actor.branchId=null;actor.departmentId=null;actor.assignmentId=await f.grant(actor,role);}
  warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Traceable warehouse'},key())).id);
  supplier=String((await saveMaster(f.owner,'parties',null,{kind:'supplier',code:key(),name:'Traceable supplier'},key())).id);
  customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Traceable customer'},key())).id);
});

async function approved(kind:'purchase'|'sales',partyId:string,productId:string,quantity:string){
  const created=await saveOrder(f.owner,null,orderSchema.parse({kind,partyId,warehouseId:warehouse,orderDate:'2026-09-02',dueDate:'2026-09-05',lines:[{productId,quantity,price:'10',taxBps:0}]}),key()),id=String(created.id);
  await transitionOrder(f.owner,id,{action:'submit',expectedVersion:1},key());
  await transitionOrder(f.head,id,{action:'approve',expectedVersion:2},key());
  let flow=await resolveFlow(db,f.owner,'order',id);
  await assignFlowPrimary(db,f.owner,'order',id,{stepKey:'confirmed',userId:f.owner.userId,reason:'Assign warehouse receiving or delivery responsibility',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  flow=await resolveFlow(db,f.owner,'order',id);
  await assignFlowPrimary(db,f.owner,'order',id,{stepKey:'stock_gl',userId:f.manager.userId,reason:'Assign accounting review for the stock movement',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  return getOrder(f.owner,id);
}

it('reconciles lot balances to stock, quarantines one receipt lot, and releases it through independent Quality review',async()=>{
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Lot controlled item',unit:'each',trackingMode:'lot',price:'10'},key())).id);
  const purchase=await approved('purchase',supplier,product,'3'),purchaseId=String(purchase.order.id),purchaseLine=String(purchase.lines[0].id),receiptKey=key();
  await fulfillOrder(f.owner,purchaseId,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:purchaseLine,quantity:'3',allocations:[{lotCode:'LOT-A',quantity:'2',disposition:'available'},{lotCode:'LOT-B',quantity:'1',disposition:'quarantine',quarantineReason:'Packaging requires inspection',quarantineDueDate:'2026-09-04'}]}]},receiptKey);
  const stock=await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first();
  const total=await db.prepare('SELECT sum(b.quantity_milli) quantity_milli,sum(b.value_minor) value_minor FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=? AND l.product_id=?').bind(warehouse,product).first();
  expect(stock).toEqual({quantity_milli:3000,value_minor:3000});expect(total).toEqual(stock);
  const lotB=await db.prepare("SELECT l.id,b.version FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.product_id=? AND l.lot_code='LOT-B' AND b.warehouse_id=?").bind(product,warehouse).first<{id:string;version:number}>();
  const hold=await db.prepare("SELECT id,status,held_quantity_milli FROM inventory_quality_holds WHERE lot_id=?").bind(lotB!.id).first();
  expect(hold).toMatchObject({status:'held',held_quantity_milli:1000});
  const sale=await approved('sales',customer,product,'1'),saleId=String(sale.order.id),saleLine=String(sale.lines[0].id);
  const proof=await deliveryEvidence(db,f.owner,saleId);
  await expect(fulfillOrder(f.owner,saleId,{expectedVersion:3,date:'2026-09-03',lines:[{lineId:saleLine,quantity:'1',allocations:[{lotId:lotB!.id,quantity:'1'}]}],...proof},key())).rejects.toMatchObject({code:'LOT_UNAVAILABLE'});
  await inspectQualityHold(f.reviewer,String(hold!.id),{expectedVersion:1,result:'pass',evidence:'Package and contents inspected against supplier label'},key());
  await releaseQualityHold(f.manager,String(hold!.id),{expectedVersion:2,reason:'Independent review confirms this lot is usable'},key());
  const delivery=await fulfillOrder(f.owner,saleId,{expectedVersion:3,date:'2026-09-03',lines:[{lineId:saleLine,quantity:'1',allocations:[{lotId:lotB!.id,quantity:'1'}]}],...proof},key());
  expect((await getOrder(f.owner,saleId)).lotAllocations).toMatchObject([{lot_code:'LOT-B',quantity_milli:1000,disposition:'available'}]);
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:2000,value_minor:2000});
  await expect(db.prepare("UPDATE inventory_lot_moves SET source_id='changed' WHERE source_id=?").bind(`${delivery.fulfillmentId}-0`).run()).rejects.toThrow('LOT_EVIDENCE_IMMUTABLE');
});

it('requires one unit per serial and rejects a serial number already received',async()=>{
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Serialized item',unit:'each',trackingMode:'serial',price:'20'},key())).id);
  const purchase=await approved('purchase',supplier,product,'2'),id=String(purchase.order.id),line=String(purchase.lines[0].id);
  await expect(fulfillOrder(f.owner,id,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:line,quantity:'2',allocations:[{serialNumber:'SN-001',quantity:'2'}]}]},key())).rejects.toMatchObject({code:'SERIAL_QUANTITY_INVALID'});
  await fulfillOrder(f.owner,id,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:line,quantity:'1',allocations:[{serialNumber:'SN-001',quantity:'1'}]}]},key());
  await expect(fulfillOrder(f.owner,id,{expectedVersion:4,date:'2026-09-03',lines:[{lineId:line,quantity:'1',allocations:[{serialNumber:'SN-001',quantity:'1'}]}]},key())).rejects.toMatchObject({code:'SERIAL_ALREADY_RECEIVED'});
  await expect(db.prepare("UPDATE commerce_products SET tracking_mode='lot' WHERE id=?").bind(product).run()).rejects.toThrow('TRACKING_MODE_LOCKED');
});

it('moves selected lot quantities and values between warehouses as one direct transfer',async()=>{
  const target=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Traceable destination'},key())).id);
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Transferred lot item',unit:'box',trackingMode:'lot',price:'10'},key())).id);
  const purchase=await approved('purchase',supplier,product,'3'),purchaseId=String(purchase.order.id),purchaseLine=String(purchase.lines[0].id);
  await fulfillOrder(f.owner,purchaseId,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:purchaseLine,quantity:'3',allocations:[{lotCode:'MOVE-A',quantity:'2'},{lotCode:'MOVE-B',quantity:'1'}]}]},key());
  const lots=(await db.prepare("SELECT l.id,l.lot_code FROM inventory_lots l WHERE l.product_id=? ORDER BY l.lot_code").bind(product).all<{id:string;lot_code:string}>()).results;
  const transfer=await saveTransfer(f.owner,null,transferSchema.parse({sourceWarehouseId:warehouse,targetWarehouseId:target,transferDate:'2026-09-03',reason:'Replenish the destination with selected physical lots',lines:[{productId:product,quantity:'2',allocations:lots.map(lot=>({lotId:lot.id,quantity:'1'}))}]}),key()),transferId=String(transfer.id);
  const detail=await transferDetail(f.owner,transferId);expect(detail.allocations.map(a=>String(a.lot_code)).sort()).toEqual(['MOVE-A','MOVE-B']);
  await transitionTransfer(f.owner,transferId,'submit',{expectedVersion:1},key());const flow=await resolveFlow(db,f.owner,'transfer',transferId);await assignFlowPrimary(db,f.owner,'transfer',transferId,{stepKey:'submitted',userId:f.head.userId,reason:'Assign an independent receiving reviewer',saveDefault:false,expectedVersion:flow.version,recordVersion:2},key());
  const wrongFiles=await seedAttachmentSet(db,f.head,{targetType:'inventory_transfer',targetId:transferId,purposeKey:'receipt_evidence',componentRef:'direct-receipt',expectedTargetVersion:2});
  await expect(transitionTransfer(f.head,transferId,'post',{expectedVersion:2,reason:'Wrong receipt context must not post',attachmentSetIds:[wrongFiles.setId]},key())).rejects.toMatchObject({code:'TRANSFER_ATTACHMENT_PURPOSE'});
  expect(await attachmentBindingState(db,wrongFiles.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
  const receiptFiles=await seedAttachmentSet(db,f.head,{targetType:'inventory_transfer',targetId:transferId,purposeKey:'direct_receipt_evidence',componentRef:'direct-receipt',expectedTargetVersion:2});
  await transitionTransfer(f.head,transferId,'post',{expectedVersion:2,reason:'Both selected lots counted at the receiving warehouse',attachmentSetIds:[receiptFiles.setId]},key());
  expect(await attachmentBindingState(db,receiptFiles.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'direct_transfer_receipt',binding_id:`${transferId}:3`});
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:1000,value_minor:1000});
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(target,product).first()).toEqual({quantity_milli:2000,value_minor:2000});
  expect((await db.prepare('SELECT l.lot_code,b.quantity_milli,b.value_minor FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=? AND l.product_id=? ORDER BY l.lot_code').bind(target,product).all()).results).toEqual([{lot_code:'MOVE-A',quantity_milli:1000,value_minor:1000},{lot_code:'MOVE-B',quantity_milli:1000,value_minor:1000}]);
  await expect(db.prepare('DELETE FROM inventory_transfer_lot_allocations WHERE transfer_line_id=?').bind(detail.lines[0].id).run()).rejects.toThrow('TRANSFER_LOT_EVIDENCE_IMMUTABLE');
});

it('preserves selected lot identity through supplier and customer returns',async()=>{
  f.manager.role='finance_approver';f.manager.assignmentId=await f.grant(f.manager,'finance_approver');
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Returned lot item',unit:'box',trackingMode:'lot',price:'10'},key())).id),purchase=await approved('purchase',supplier,product,'2'),orderId=String(purchase.order.id),orderLine=String(purchase.lines[0].id);
  const receipt=await fulfillOrder(f.owner,orderId,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:orderLine,quantity:'2',allocations:[{lotCode:'RETURN-A',quantity:'1'},{lotCode:'RETURN-B',quantity:'1'}]}]},key()),fulfillmentId=String(receipt.fulfillmentId),goodsJournal=String(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(fulfillmentId).first('journal_id'));
  let flow=await resolveFlow(db,f.owner,'journal',goodsJournal);await assignFlowPrimary(db,f.owner,'journal',goodsJournal,{stepKey:'draft',userId:f.manager.userId,reason:'Assign goods receipt journal review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postJournal(f.manager,goodsJournal,key(),{expectedVersion:1});
  flow=await resolveFlow(db,f.owner,'order',orderId);await assignFlowPrimary(db,f.owner,'order',orderId,{stepKey:'invoicing',userId:f.owner.userId,reason:'Assign invoice preparation after goods posting',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());const invoice=await invoiceFulfillment(f.owner,fulfillmentId,{expectedVersion:1,date:'2026-09-02',dueDate:'2026-09-03'},key());
  flow=await resolveFlow(db,f.owner,'invoice',String(invoice.invoiceId));await assignFlowPrimary(db,f.owner,'invoice',String(invoice.invoiceId),{stepKey:'draft',userId:f.manager.userId,reason:'Assign invoice posting review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,String(invoice.invoiceId),{expectedVersion:1,periodId:f.period,entryDate:'2026-09-02'},key());
  const detail=await getOrder(f.owner,orderId),line=detail.fulfillmentLines[0],lotB=detail.lotAllocations.find(a=>a.lot_code==='RETURN-B')!;
  const requested=await requestCommerceReturn(f.owner,fulfillmentId,{expectedVersion:2,date:'2026-09-03',reason:'Supplier authorized return of the labelled damaged lot',lines:[{lineId:String(line.id),quantity:'1',allocations:[{lotId:String(lotB.lot_id),quantity:'1'}]}]},key());
  await decideCommerceReturn(f.head,String(requested.id),'approve',{expectedVersion:1,reason:'Physical RETURN-B label and supplier authorization verified'},key());
  expect((await db.prepare('SELECT l.lot_code,l.status,b.quantity_milli,b.value_minor FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.product_id=? AND b.warehouse_id=? ORDER BY l.lot_code').bind(product,warehouse).all()).results).toEqual([{lot_code:'RETURN-A',status:'active',quantity_milli:1000,value_minor:1000},{lot_code:'RETURN-B',status:'returned',quantity_milli:0,value_minor:0}]);
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM inventory_lot_moves WHERE source_type='supplier_return' AND lot_id=?").bind(lotB.lot_id).first()).toEqual({quantity_milli:-1000,value_minor:-1000});
  const sale=await approved('sales',customer,product,'1'),saleId=String(sale.order.id),saleLine=String(sale.lines[0].id),lotA=detail.lotAllocations.find(a=>a.lot_code==='RETURN-A')!,delivery=await fulfillOrder(f.owner,saleId,{expectedVersion:3,date:'2026-09-03',lines:[{lineId:saleLine,quantity:'1',allocations:[{lotId:String(lotA.lot_id),quantity:'1'}]}],...(await deliveryEvidence(db,f.owner,saleId))},key()),deliveryId=String(delivery.fulfillmentId),deliveryJournal=String(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(deliveryId).first('journal_id'));
  flow=await resolveFlow(db,f.owner,'journal',deliveryJournal);await assignFlowPrimary(db,f.owner,'journal',deliveryJournal,{stepKey:'draft',userId:f.manager.userId,reason:'Assign delivery journal review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postJournal(f.manager,deliveryJournal,key(),{expectedVersion:1});
  flow=await resolveFlow(db,f.owner,'order',saleId);await assignFlowPrimary(db,f.owner,'order',saleId,{stepKey:'invoicing',userId:f.owner.userId,reason:'Assign customer invoice preparation',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());const customerInvoice=await invoiceFulfillment(f.owner,deliveryId,{expectedVersion:1,date:'2026-09-03',dueDate:'2026-09-04'},key());
  flow=await resolveFlow(db,f.owner,'invoice',String(customerInvoice.invoiceId));await assignFlowPrimary(db,f.owner,'invoice',String(customerInvoice.invoiceId),{stepKey:'draft',userId:f.manager.userId,reason:'Assign customer invoice posting review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,String(customerInvoice.invoiceId),{expectedVersion:1,periodId:f.period,entryDate:'2026-09-03'},key());
  const saleDetail=await getOrder(f.owner,saleId),customerReturn=await requestCommerceReturn(f.owner,deliveryId,{expectedVersion:2,date:'2026-09-04',reason:'Customer returned the labelled unused lot',lines:[{lineId:String(saleDetail.fulfillmentLines[0].id),quantity:'1',allocations:[{lotId:String(lotA.lot_id),quantity:'1'}],condition:'unopened',conditionNote:'Label intact but Quality must verify packaging',qualityDueDate:'2026-09-05'}]},key());await decideCommerceReturn(f.head,String(customerReturn.id),'approve',{expectedVersion:1,reason:'RETURN-A label and customer return evidence verified'},key());
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?').bind(lotA.lot_id,warehouse).first()).toEqual({quantity_milli:1000,value_minor:1000});
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM inventory_lot_moves WHERE source_type='customer_return' AND lot_id=?").bind(lotA.lot_id).first()).toEqual({quantity_milli:1000,value_minor:1000});
  expect(await db.prepare("SELECT status,lot_id,held_quantity_milli,quantity_scope FROM inventory_quality_holds WHERE source_type='customer_return'").first()).toEqual({status:'held',lot_id:lotA.lot_id,held_quantity_milli:1000,quantity_scope:'partial'});
  expect((await getOrder(f.owner,saleId)).returnQualityHolds).toHaveLength(1);
});

it('posts a lot-level physical count with a missing lot and a newly found lot',async()=>{
  f.manager.role='finance_approver';f.manager.assignmentId=await f.grant(f.manager,'finance_approver');
  await db.prepare("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,status,created_at,updated_at) VALUES(?,?,?,'5400','Stock variance','expense','THB','active',1,1)").bind(`variance-${key()}`,f.company,f.chart).run();
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Counted lot item',unit:'box',trackingMode:'lot',price:'10'},key())).id);
  const purchase=await approved('purchase',supplier,product,'2'),purchaseId=String(purchase.order.id),purchaseLine=String(purchase.lines[0].id);
  await fulfillOrder(f.owner,purchaseId,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:purchaseLine,quantity:'2',allocations:[{lotCode:'COUNT-A',quantity:'1'},{lotCode:'COUNT-B',quantity:'1'}]}]},key());
  const lots=(await db.prepare("SELECT l.id,l.lot_code FROM inventory_lots l WHERE l.product_id=? ORDER BY l.lot_code").bind(product).all<{id:string;lot_code:string}>()).results;
  const created=await saveCount(f.owner,null,{warehouseId:warehouse,countDate:'2026-09-02',reason:'Reconcile labelled stock found during cycle count',lines:[{productId:product,countedQuantity:'3',allocations:[{lotId:lots[0].id,countedQuantity:'0',manufacturedDate:'',expiryDate:''},{lotId:lots[1].id,countedQuantity:'1',manufacturedDate:'',expiryDate:''},{lotCode:'COUNT-C',countedQuantity:'2',unitCost:'15',manufacturedDate:'2026-08-01',expiryDate:'2027-08-01'}]}]},key()),countId=String(created.id);
  let detail=await countDetail(f.owner,countId);expect(detail.lotLines).toHaveLength(3);expect(detail.count.net_value_minor).toBe(2000);
  const countFiles=await seedAttachmentSet(db,f.owner,{targetType:'stock_count',targetId:countId,purposeKey:'count_sheet',componentRef:'submit',expectedTargetVersion:1});
  const submitKey=key();await transitionCount(f.owner,countId,'submit',{expectedVersion:1,attachmentSetIds:[countFiles.setId]},submitKey);expect(await attachmentBindingState(db,countFiles.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'stock_count_submission',binding_id:`count-submit-${submitKey}`});const flow=await resolveFlow(db,f.owner,'stock_count',countId);await assignFlowPrimary(db,f.owner,'stock_count',countId,{stepKey:'submitted',userId:f.manager.userId,reason:'Assign independent count review',saveDefault:false,expectedVersion:flow.version,recordVersion:2},key());
  await transitionCount(f.manager,countId,'approve',{expectedVersion:2,reason:'Labels and valuation evidence checked'},key());await transitionCount(f.manager,countId,'post',{expectedVersion:3,reason:'Approved count reconciles to the inventory control total'},key());
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:3000,value_minor:4000});
  expect((await db.prepare('SELECT l.lot_code,l.status,b.quantity_milli,b.value_minor FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.product_id=? AND b.warehouse_id=? ORDER BY l.lot_code').bind(product,warehouse).all()).results).toEqual([{lot_code:'COUNT-A',status:'depleted',quantity_milli:0,value_minor:0},{lot_code:'COUNT-B',status:'active',quantity_milli:1000,value_minor:1000},{lot_code:'COUNT-C',status:'active',quantity_milli:2000,value_minor:3000}]);
  detail=await countDetail(f.owner,countId);expect(detail.lotLines.some(l=>l.source_lot_id==null&&l.lot_code==='COUNT-C')).toBe(true);
  await expect(db.prepare("UPDATE inventory_count_lot_lines SET counted_quantity_milli=1000 WHERE count_line_id=? AND lot_code='COUNT-C'").bind(detail.lines[0].id).run()).rejects.toThrow('COUNT_LOT_EVIDENCE_IMMUTABLE');
});

it('preserves lot identity through dispatch, partial receipt, and an independently approved transit loss',async()=>{
  f.manager.role='finance_approver';f.manager.assignmentId=await f.grant(f.manager,'finance_approver');
  const target=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Lot transit destination'},key())).id);
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Lot transport item',unit:'box',trackingMode:'lot',price:'10'},key())).id);
  const purchase=await approved('purchase',supplier,product,'2'),purchaseId=String(purchase.order.id),purchaseLine=String(purchase.lines[0].id);
  await fulfillOrder(f.owner,purchaseId,{expectedVersion:3,date:'2026-09-02',lines:[{lineId:purchaseLine,quantity:'2',allocations:[{lotCode:'ROAD-A',quantity:'1'},{lotCode:'ROAD-B',quantity:'1'}]}]},key());
  const lots=(await db.prepare('SELECT id FROM inventory_lots WHERE product_id=? ORDER BY lot_code').bind(product).all<{id:string}>()).results;
  const transfer=await saveTransfer(f.owner,null,transferSchema.parse({sourceWarehouseId:warehouse,targetWarehouseId:target,transferDate:'2026-09-03',deliveryMode:'transit',arrivalDueDate:'2026-09-05',reason:'Track both supplier lots while held by the carrier',lines:[{productId:product,quantity:'2',allocations:lots.map(lot=>({lotId:lot.id,quantity:'1'}))}]}),key()),transferId=String(transfer.id);
  await transitionTransfer(f.owner,transferId,'submit',{expectedVersion:1},key());const flow=await resolveFlow(db,f.owner,'transfer',transferId);await assignFlowPrimary(db,f.owner,'transfer',transferId,{stepKey:'submitted',userId:f.head.userId,reason:'Assign independent transport approval',saveDefault:false,expectedVersion:flow.version,recordVersion:2},key());await transitionTransfer(f.head,transferId,'post',{expectedVersion:2,reason:'Both labelled lots approved for carrier dispatch'},key());
  const transitId=`transit-${transferId}`;await dispatchTransit(f.owner,transitId,{expectedVersion:1,date:'2026-09-03',trackingReference:'LOT-ROAD-001',evidence:'Carrier accepted both sealed and labelled lots'},key());
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:0,value_minor:0});
  const transit=await transitDetail(f.head,transitId);expect(transit.lots).toHaveLength(2);const lineId=String(transit.lines[0].id),roadA=transit.lots.find(l=>l.lot_code==='ROAD-A')!,roadB=transit.lots.find(l=>l.lot_code==='ROAD-B')!;
  await receiveTransit(f.head,transitId,{expectedVersion:2,date:'2026-09-04',evidence:'ROAD-B physically counted at destination before ROAD-A',lines:[{lineId,quantity:'1',allocations:[{transitLotId:String(roadB.id),quantity:'1'}]}]},key());
  const loss=await requestTransitLoss(f.head,transitId,{expectedVersion:3,date:'2026-09-04',lineId,quantity:'1',allocations:[{transitLotId:String(roadA.id),quantity:'1'}],evidence:'ROAD-A missing under carrier incident LOT-ROAD-002'},key());
  await decideTransitLoss(f.manager,transitId,{expectedVersion:4,date:'2026-09-04',proposalId:String(loss.proposalId),decision:'approve',periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Independent Finance reviewed carrier evidence and lot cost'},key());
  expect((await getTransit(f.owner,transitId)).status).toBe('completed');
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(target,product).first()).toEqual({quantity_milli:1000,value_minor:1000});
  expect(await db.prepare('SELECT l.lot_code,b.quantity_milli FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=? AND l.product_id=?').bind(target,product).first()).toEqual({lot_code:'ROAD-B',quantity_milli:1000});
  expect(await db.prepare('SELECT sum(received_milli) received,sum(lost_milli) lost,sum(settled_value_minor) value FROM inventory_transit_lots WHERE transit_line_id=?').bind(lineId).first()).toEqual({received:1000,lost:1000,value:2000});
});
