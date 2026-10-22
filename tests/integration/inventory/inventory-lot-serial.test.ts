import { qualityInspectionSchema } from "@erpx/server-quality/public/quality";
import { fulfillmentSchema } from "@erpx/server-commerce/public/commerce";
import { transferActionSchema } from "@erpx/server-inventory/public/inventory-transfers";
import { returnRequestSchema } from "@erpx/server-commerce/public/commerce";
import { countActionSchema } from "@erpx/server-inventory/public/inventory-counts";
import { transitDispatchSchema } from "@erpx/server-inventory/public/inventory-transit";
import { transitReceiveSchema } from "@erpx/server-inventory/public/inventory-transit";
import { transitLossSchema } from "@erpx/server-inventory/public/inventory-transit";
import {afterEach,beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {deliveryEvidence} from "../../support/helpers/delivery-evidence";
import {decideCommerceReturn,fulfillOrder,getOrder,invoiceFulfillment,orderSchema,requestCommerceReturn,saveMaster,saveOrder,transitionOrder} from "@erpx/server-commerce/public/commerce";
import {inspectQualityHold,releaseQualityHold} from "@erpx/server-quality/public/quality";
import {assignFlowPrimary,resolveFlow} from "@erpx/server-workflow/public/flow-routing";
import {saveTransfer,transitionTransfer,transferDetail,transferSchema} from "@erpx/server-inventory/public/inventory-transfers";
import {decideTransitLoss,dispatchTransit,getTransit,receiveTransit,requestTransitLoss,transitDetail} from "@erpx/server-inventory/public/inventory-transit";
import {countDetail,countOptions,countsPage,getCount,saveCount,transitionCount} from "@erpx/server-inventory/public/inventory-counts";
import {getDomainWork} from "@erpx/server-work_projection/public/domain-work";
import {enrichFlowWork} from "@erpx/server-workflow/public/flow-work";
import {countSourceReadScope} from "@erpx/server-inventory/public/count-record-purchases";
import {listStaffNotifications} from "@erpx/server-collaboration/public/staff-notifications";
import {publishInboxEvent,inboxPage} from "@erpx/server-collaboration/public/inbox";
import {postInvoice,postJournal} from "@erpx/server-finance/public/finance";
import {journalDetail,ledgerWorkspace} from "@erpx/server-finance/public/ledger";
import {financeProductDocumentEntry,financeDocumentPostAccess,financeSourceDocumentReadLocks} from "@erpx/server-finance/public/document-purchases";
import {financeDocumentFlow,financeDocumentPresentationAccess} from "@erpx/server-finance/public/record-presentation";
import {bindRecordAttachmentSet,attachmentContent} from "@erpx/server-files/public/attachments/service";
import {attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet} from "../../support/helpers/attachment-v2";
import {packageGrantStore,packageGrantRevision,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import {withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations} from "@erpx/server-commercial/public/owner-package-obligations";

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>,warehouse:string,supplier:string,customer:string;
afterEach(()=>vi.unstubAllEnvs());
beforeAll(async()=>{enableAttachmentV2ForTest();await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
  f=await expenseFixture();
  // Inventory, receipt clearing and counterparty controls must remain distinct
  // so posted receipt/invoice/return journals prove actual stock and AP/AR.
  for(const [suffix,category] of [['inventory','asset'],['clearing','liability'],['receivable','asset']] as const)await db.prepare("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)").bind(`${f.company}-${suffix}`,f.company,f.chart,suffix,suffix,category).run();
  await db.prepare('UPDATE commerce_settings SET inventory_account_id=?,clearing_account_id=?,receivable_account_id=? WHERE company_id=?').bind(`${f.company}-inventory`,`${f.company}-clearing`,`${f.company}-receivable`,f.company).run();
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

async function supplyGrant(state:'active'|'draining'='active',endsAt:number|null=null){
 const previous=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='supply');
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-count-provider'}),obligations:async()=>{throw new Error('COUNT_FIXTURE_CANNOT_INACTIVATE');}});
 return store.apply({expectedVersion:previous?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'supply',packageVersion:1,state,startsAt:0,endsAt,version:(previous?.version??0)+1,reason:'Isolated Supply physical count owner workflow',evidence:'test:count-purchased-boundary'}});
}

it('guards physical Count creation, reads, Work and direct Flow while retaining source settlement and replay during draining',async()=>{
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Purchased count item',price:'10'},key())).id);
 const input={warehouseId:warehouse,countDate:'2026-09-02',reason:'Review the physical count owner boundary',lines:[{productId:product,countedQuantity:'0',allocations:[]}]},command=key();
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 await expect(saveCount(f.owner,null,input,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await expect(countOptions(f.owner)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 expect(await getDomainWork(f.owner,'stock_count')).toEqual([]);
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','shadow');const shadow=await saveCount(f.owner,null,input,key()),shadowId=String(shadow.id);
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 await expect(getCount(f.owner,shadowId,db)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await expect(resolveFlow(db,f.owner,'stock_count',shadowId)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await supplyGrant();const created=await saveCount(f.owner,null,input,command),id=String(created.id);
 const page=await countsPage(f.owner,{});expect(page.items.map(row=>row.id)).toContain(id);
 await transitionCount(f.owner,id,'submit',countActionSchema.parse({expectedVersion:1}),key());
 await supplyGrant('draining');
 expect(await saveCount(f.owner,null,input,command)).toEqual(created);
 await expect(saveCount(f.owner,null,input,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await expect(saveCount(f.owner,shadowId,{...input,expectedVersion:1},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 expect(await resolveFlow(db,f.owner,'stock_count',shadowId)).toMatchObject({canAct:false,canManage:false});
 const draftWork=(await getDomainWork(f.owner,'stock_count')).find(row=>row.id===shadowId);expect(draftWork).toMatchObject({canAct:false});
 expect((await enrichFlowWork(f.owner,[{...draftWork!,canAct:true}]))[0]).toMatchObject({canAct:false,flow:{canAct:false,canManage:false}});
 const flow=await resolveFlow(db,f.owner,'stock_count',id);
 await assignFlowPrimary(db,f.owner,'stock_count',id,{stepKey:'submitted',userId:f.head.userId,reason:'Select an independent current reviewer',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 await transitionCount(f.head,id,'reject',countActionSchema.parse({expectedVersion:2,reason:'Return the count for retained reconciliation'}),key());
 const cancel=key(),cancelInput=countActionSchema.parse({expectedVersion:3,reason:'Cancel the original retained count'}),closed=await transitionCount(f.owner,id,'cancel',cancelInput,cancel);
 expect(await transitionCount(f.owner,id,'cancel',cancelInput,cancel)).toEqual(closed);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();
 await expect(getCount(f.owner,id,db)).rejects.toMatchObject({code:'COUNT_ACCESS_CHANGED'});
 await expect(transitionCount(f.owner,id,'cancel',cancelInput,cancel)).rejects.toMatchObject({code:'COUNT_ACCESS_CHANGED'});
});

it('rolls back a physical Count and its receipt after late purchase expiry or current authority change',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await supplyGrant('active',Date.now()+3000);
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Atomic count item',price:'10'},key())).id),input={warehouseId:warehouse,countDate:'2026-09-02',reason:'Retain atomic owner purchase and authority checks',lines:[{productId:product,countedQuantity:'0',allocations:[]}]};
 const command=key(),name=`count_guard_${key().replaceAll('-','')}`;
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(3.15); RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${name} AFTER INSERT ON command_receipts FOR EACH ROW WHEN (NEW.command_id='${command}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(saveCount(f.owner,null,input,command)).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 for(const [table,column,value] of [['inventory_counts','id',`count-${command}`],['command_receipts','command_id',command],['audit_events','command_id',command],['outbox_events','command_id',command]])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE ${column}=?`).bind(value).first()).toBeNull();
 await supplyGrant();const revoke=key();
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}'; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${name} AFTER INSERT ON command_receipts FOR EACH ROW WHEN (NEW.command_id='${revoke}') EXECUTE FUNCTION ${name}()`).run();
 // The original command's final authority assertion can reject first, before
 // the enclosing purchase boundary returns; retain its existing409 contract.
 try{await expect(saveCount(f.owner,null,input,revoke)).rejects.toMatchObject({status:409,code:'CONFLICT'});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await db.prepare('SELECT 1 FROM inventory_counts WHERE id=?').bind(`count-${revoke}`).first()).toBeNull();
 expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.owner.assignmentId).first('revoked_at')).toBeNull();
 const result=await saveCount(f.owner,null,input,revoke);expect(await saveCount(f.owner,null,input,revoke)).toEqual(result);
});

it('filters physical Count source projections before limits and retains stored notifications through purchase and authority changes',async()=>{
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Projected count item',price:'10'},key())).id),now=Date.now(),ids:string[]=[],shared=key();
 const record=async(id:string,title:string,at:number)=>db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES('inventory',?,'counts',?,'','draft','{}',?,?,?,?,?)").bind(id,title,f.company,f.owner.userId,f.owner.userId,at,at).run();
 await record(shared,'Shared Stock reference',now);
 for(let index=0;index<16;index++){
  const created=await saveCount(f.owner,null,{warehouseId:warehouse,countDate:'2026-09-02',reason:'Actual private projected physical count',lines:[{productId:product,countedQuantity:'0',allocations:[]}]},key());ids.push(String(created.id));await record(String(created.id),'Private physical Count source',now+index+1);
 }
 for(let index=0;index<106;index++)await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'stock_count',?,?,?,?)").bind(key(),f.company,f.owner.userId,ids[0],index+1,'Review actual physical count',now+index).run();
 await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:'stock_count',sourceId:ids[0],sourceVersion:107,category:'work',preview:'Review actual physical count',recipientIds:[f.owner.userId]});
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 const scope=await countSourceReadScope(db,f.owner,'records'),rows=(await db.prepare(`SELECT id FROM workspace_records WHERE company_id=? AND ${scope.where} ORDER BY updated_at DESC LIMIT 1`).bind(f.company,...scope.args).all<{id:string}>()).results;
 expect(rows).toEqual([{id:shared}]);expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:'all'})).items).toEqual([]);
 await supplyGrant();await supplyGrant('draining');
 expect((await listStaffNotifications(db,f.owner)).length).toBeGreaterThan(0);expect((await inboxPage(db,f.owner,{view:'all'})).items.length).toBeGreaterThan(0);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();
 expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:'all'})).items).toEqual([]);
 expect(await db.prepare('SELECT count(*) n FROM staff_notifications WHERE company_id=?').bind(f.company).first('n')).toBe(106);expect(await db.prepare('SELECT count(*) n FROM inbox_events WHERE company_id=?').bind(f.company).first('n')).toBe(107);
});

it('leases physical Count source relations before the first Finance business snapshot even when no Company count exists',async()=>{
 const name=`count_lease_${key().replaceAll('-','')}`,marker=key();
 let ready!:()=>void,release!:()=>void;
 const entered=new Promise<void>(resolve=>{ready=resolve;}),released=new Promise<void>(resolve=>{release=resolve;});
 const holder=db.transaction(async tx=>{await tx.prepare('LOCK TABLE inventory_counts IN ROW EXCLUSIVE MODE').run();await tx.prepare("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,created_by,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,'2026-09-02','Declared concurrent physical count source',?,?,1,1)").bind(marker,f.company,f.branch,warehouse,marker,f.owner.userId,f.owner.userId).run();ready();await released;});
 await entered;
 const reader=db.transaction(async tx=>{await tx.prepare(`SET LOCAL application_name='${name}'`).run();await tx.batch(financeSourceDocumentReadLocks(tx,f.company));return tx.prepare('SELECT id FROM inventory_counts WHERE id=? AND company_id=?').bind(marker,f.company).first<{id:string}>();});
 let visible:{id:string}|null=null;
 try{
  let waiting:{backend_xmin:string|null;wait_event_type:string|null}|null=null;
  for(let attempt=0;attempt<40;attempt++){
   waiting=await db.prepare('SELECT backend_xmin::text,wait_event_type FROM pg_stat_activity WHERE application_name=?').bind(name).first();
   if(waiting?.wait_event_type==='Lock')break;
   await new Promise(resolve=>setTimeout(resolve,10));
  }
  expect(waiting).toMatchObject({wait_event_type:'Lock'});
 }finally{release();const completed=await Promise.allSettled([holder,reader]);for(const result of completed)if(result.status==='rejected')throw result.reason;if(completed[1].status==='fulfilled')visible=completed[1].value??null;}
 // A latched SERIALIZABLE snapshot from before the blocked lease would hide
 // this committed source. backend_xmin alone also covers catalog snapshots.
 expect(visible).toEqual({id:marker});
});

it.each([{label:'POS',packageId:'pos'},{label:'Hotel',packageId:'pms'}] as const)('retains a pending Stock count in $label foundation withdrawal until the normal owner cancels it',async({packageId})=>{
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Private foundation count item',price:'10'},key())).id);
 const count=await saveCount(f.owner,null,{warehouseId:warehouse,countDate:'2026-09-02',reason:'Review pre-existing Stock foundation work',lines:[{productId:product,countedQuantity:'0',allocations:[]}]},key());
 const scope={companyId:f.company,packageId,packageVersion:1},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-stock-settlement-provider'}),obligations:(tx,company,packageId)=>ownerPackageObligations(db,tx,company,packageId)});
 const grant=(state:'active'|'draining'|'inactive',version:number)=>({companyId:f.company,packageId,packageVersion:1,state,version,startsAt:0,endsAt:null,reason:'Review Stock foundation before package withdrawal',evidence:'test:stock-foundation-withdrawal'});
 await store.apply({expectedVersion:0,idempotencyKey:key(),grant:grant('active',1)});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:grant('draining',2)});
 const observe=()=>withOwnerPackageObligations(db,scope,tx=>readOwnerPackageObligations(db,tx,{...scope,now:Date.now()}));
 const before=await observe();expect(before.observations.find(row=>row.moduleId==='inventory')?.outstanding).toBe(1);expect(JSON.stringify(before)).not.toContain('Private foundation count item');
 const request={companyId:f.company,expectedRevision:packageGrantRevision(f.company,await readCompanyPackageGrants(db,f.company)),idempotencyKey:key(),changes:[{expectedVersion:2,grant:grant('inactive',3)}]};
 await expect(withOwnerPackageObligations(db,scope,tx=>store.batch.previewIn(tx,request))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await expect(withOwnerPackageObligations(db,scope,tx=>store.batch.applyIn(tx,request,async()=>{}))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await transitionCount(f.owner,String(count.id),'cancel',countActionSchema.parse({expectedVersion:1,reason:'Owner cancels the unsubmitted count'}),key());
 expect((await observe()).total).toBe(0);await withOwnerPackageObligations(db,scope,tx=>store.batch.applyIn(tx,request,async()=>{}));
 expect(await db.prepare('SELECT status FROM inventory_counts WHERE id=?').bind(count.id).first()).toEqual({status:'cancelled'});expect((await readCompanyPackageGrants(db,f.company))[0]).toMatchObject({state:'inactive',version:3});
 expect(await db.prepare('SELECT count(*) n FROM company_package_change_sets WHERE id=?').bind(request.idempotencyKey).first()).toEqual({n:1});
});
it('reconciles lot balances to stock, quarantines one receipt lot, and releases it through independent Quality review',async()=>{
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Lot controlled item',unit:'each',trackingMode:'lot',price:'10'},key())).id);
  const purchase=await approved('purchase',supplier,product,'3'),purchaseId=String(purchase.order.id),purchaseLine=String(purchase.lines[0].id),receiptKey=key();
  await fulfillOrder(f.owner,purchaseId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-02',lines:[{lineId:purchaseLine,quantity:'3',allocations:[{lotCode:'LOT-A',quantity:'2',disposition:'available'},{lotCode:'LOT-B',quantity:'1',disposition:'quarantine',quarantineReason:'Packaging requires inspection',quarantineDueDate:'2026-09-04'}]}]}),receiptKey);
  const stock=await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first();
  const total=await db.prepare('SELECT sum(b.quantity_milli) quantity_milli,sum(b.value_minor) value_minor FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=? AND l.product_id=?').bind(warehouse,product).first();
  expect(stock).toEqual({quantity_milli:3000,value_minor:3000});expect(total).toEqual(stock);
  const lotB=await db.prepare("SELECT l.id,b.version FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.product_id=? AND l.lot_code='LOT-B' AND b.warehouse_id=?").bind(product,warehouse).first<{id:string;version:number}>();
  const hold=await db.prepare("SELECT id,status,held_quantity_milli FROM inventory_quality_holds WHERE lot_id=?").bind(lotB!.id).first();
  expect(hold).toMatchObject({status:'held',held_quantity_milli:1000});
  const sale=await approved('sales',customer,product,'1'),saleId=String(sale.order.id),saleLine=String(sale.lines[0].id);
  const proof=await deliveryEvidence(db,f.owner,saleId);
  await expect(fulfillOrder(f.owner,saleId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-03',lines:[{lineId:saleLine,quantity:'1',allocations:[{lotId:lotB!.id,quantity:'1'}]}],...proof}),key())).rejects.toMatchObject({code:'LOT_UNAVAILABLE'});
  await inspectQualityHold(f.reviewer,String(hold!.id),qualityInspectionSchema.parse({expectedVersion:1,result:'pass',evidence:'Package and contents inspected against supplier label'}),key());
  await releaseQualityHold(f.manager,String(hold!.id),{expectedVersion:2,reason:'Independent review confirms this lot is usable'},key());
  const delivery=await fulfillOrder(f.owner,saleId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-03',lines:[{lineId:saleLine,quantity:'1',allocations:[{lotId:lotB!.id,quantity:'1'}]}],...proof}),key());
  expect((await getOrder(f.owner,saleId)).lotAllocations).toMatchObject([{lot_code:'LOT-B',quantity_milli:1000,disposition:'available'}]);
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:2000,value_minor:2000});
  await expect(db.prepare("UPDATE inventory_lot_moves SET source_id='changed' WHERE source_id=?").bind(`${delivery.fulfillmentId}-0`).run()).rejects.toThrow('LOT_EVIDENCE_IMMUTABLE');
});

it('requires one unit per serial and rejects a serial number already received',async()=>{
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Serialized item',unit:'each',trackingMode:'serial',price:'20'},key())).id);
  const purchase=await approved('purchase',supplier,product,'2'),id=String(purchase.order.id),line=String(purchase.lines[0].id);
  await expect(fulfillOrder(f.owner,id,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-02',lines:[{lineId:line,quantity:'2',allocations:[{serialNumber:'SN-001',quantity:'2'}]}]}),key())).rejects.toMatchObject({code:'SERIAL_QUANTITY_INVALID'});
  await fulfillOrder(f.owner,id,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-02',lines:[{lineId:line,quantity:'1',allocations:[{serialNumber:'SN-001',quantity:'1'}]}]}),key());
  await expect(fulfillOrder(f.owner,id,fulfillmentSchema.parse({expectedVersion:4,date:'2026-09-03',lines:[{lineId:line,quantity:'1',allocations:[{serialNumber:'SN-001',quantity:'1'}]}]}),key())).rejects.toMatchObject({code:'SERIAL_ALREADY_RECEIVED'});
  await expect(db.prepare("UPDATE commerce_products SET tracking_mode='lot' WHERE id=?").bind(product).run()).rejects.toThrow('TRACKING_MODE_LOCKED');
});

it('moves selected lot quantities and values between warehouses as one direct transfer',async()=>{
  const target=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Traceable destination'},key())).id);
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Transferred lot item',unit:'box',trackingMode:'lot',price:'10'},key())).id);
  const purchase=await approved('purchase',supplier,product,'3'),purchaseId=String(purchase.order.id),purchaseLine=String(purchase.lines[0].id);
  await fulfillOrder(f.owner,purchaseId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-02',lines:[{lineId:purchaseLine,quantity:'3',allocations:[{lotCode:'MOVE-A',quantity:'2'},{lotCode:'MOVE-B',quantity:'1'}]}]}),key());
  const lots=(await db.prepare("SELECT l.id,l.lot_code FROM inventory_lots l WHERE l.product_id=? ORDER BY l.lot_code").bind(product).all<{id:string;lot_code:string}>()).results;
  const transfer=await saveTransfer(f.owner,null,transferSchema.parse({sourceWarehouseId:warehouse,targetWarehouseId:target,transferDate:'2026-09-03',reason:'Replenish the destination with selected physical lots',lines:[{productId:product,quantity:'2',allocations:lots.map(lot=>({lotId:lot.id,quantity:'1'}))}]}),key()),transferId=String(transfer.id);
  const detail=await transferDetail(f.owner,transferId);expect(detail.allocations.map(a=>String(a.lot_code)).sort()).toEqual(['MOVE-A','MOVE-B']);
  await transitionTransfer(f.owner,transferId,'submit',transferActionSchema.parse({expectedVersion:1}),key());const flow=await resolveFlow(db,f.owner,'transfer',transferId);await assignFlowPrimary(db,f.owner,'transfer',transferId,{stepKey:'submitted',userId:f.head.userId,reason:'Assign an independent receiving reviewer',saveDefault:false,expectedVersion:flow.version,recordVersion:2},key());
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
  const receipt=await fulfillOrder(f.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-02',lines:[{lineId:orderLine,quantity:'2',allocations:[{lotCode:'RETURN-A',quantity:'1'},{lotCode:'RETURN-B',quantity:'1'}]}]}),key()),fulfillmentId=String(receipt.fulfillmentId),goodsJournal=String(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(fulfillmentId).first('journal_id'));
  let flow=await resolveFlow(db,f.owner,'journal',goodsJournal);await assignFlowPrimary(db,f.owner,'journal',goodsJournal,{stepKey:'draft',userId:f.manager.userId,reason:'Assign goods receipt journal review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postJournal(f.manager,goodsJournal,key(),{expectedVersion:1});
  flow=await resolveFlow(db,f.owner,'order',orderId);await assignFlowPrimary(db,f.owner,'order',orderId,{stepKey:'invoicing',userId:f.owner.userId,reason:'Assign invoice preparation after goods posting',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());const invoice=await invoiceFulfillment(f.owner,fulfillmentId,{expectedVersion:1,date:'2026-09-02',dueDate:'2026-09-03'},key());
  flow=await resolveFlow(db,f.owner,'invoice',String(invoice.invoiceId));await assignFlowPrimary(db,f.owner,'invoice',String(invoice.invoiceId),{stepKey:'draft',userId:f.manager.userId,reason:'Assign invoice posting review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,String(invoice.invoiceId),{expectedVersion:1,periodId:f.period,entryDate:'2026-09-02'},key());
  const detail=await getOrder(f.owner,orderId),line=detail.fulfillmentLines[0],lotB=detail.lotAllocations.find(a=>a.lot_code==='RETURN-B')!;
  const requested=await requestCommerceReturn(f.owner,fulfillmentId,returnRequestSchema.parse({expectedVersion:2,date:'2026-09-03',reason:'Supplier authorized return of the labelled damaged lot',lines:[{lineId:String(line.id),quantity:'1',allocations:[{lotId:String(lotB.lot_id),quantity:'1'}]}]}),key());
  await decideCommerceReturn(f.head,String(requested.id),'approve',{expectedVersion:1,reason:'Physical RETURN-B label and supplier authorization verified'},key());
  expect((await db.prepare('SELECT l.lot_code,l.status,b.quantity_milli,b.value_minor FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.product_id=? AND b.warehouse_id=? ORDER BY l.lot_code').bind(product,warehouse).all()).results).toEqual([{lot_code:'RETURN-A',status:'active',quantity_milli:1000,value_minor:1000},{lot_code:'RETURN-B',status:'returned',quantity_milli:0,value_minor:0}]);
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM inventory_lot_moves WHERE source_type='supplier_return' AND lot_id=?").bind(lotB.lot_id).first()).toEqual({quantity_milli:-1000,value_minor:-1000});
  const sale=await approved('sales',customer,product,'1'),saleId=String(sale.order.id),saleLine=String(sale.lines[0].id),lotA=detail.lotAllocations.find(a=>a.lot_code==='RETURN-A')!,delivery=await fulfillOrder(f.owner,saleId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-03',lines:[{lineId:saleLine,quantity:'1',allocations:[{lotId:String(lotA.lot_id),quantity:'1'}]}],...(await deliveryEvidence(db,f.owner,saleId))}),key()),deliveryId=String(delivery.fulfillmentId),deliveryJournal=String(await db.prepare('SELECT journal_id FROM commerce_fulfillments WHERE id=?').bind(deliveryId).first('journal_id'));
  flow=await resolveFlow(db,f.owner,'journal',deliveryJournal);await assignFlowPrimary(db,f.owner,'journal',deliveryJournal,{stepKey:'draft',userId:f.manager.userId,reason:'Assign delivery journal review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postJournal(f.manager,deliveryJournal,key(),{expectedVersion:1});
  flow=await resolveFlow(db,f.owner,'order',saleId);await assignFlowPrimary(db,f.owner,'order',saleId,{stepKey:'invoicing',userId:f.owner.userId,reason:'Assign customer invoice preparation',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());const customerInvoice=await invoiceFulfillment(f.owner,deliveryId,{expectedVersion:1,date:'2026-09-03',dueDate:'2026-09-04'},key());
  flow=await resolveFlow(db,f.owner,'invoice',String(customerInvoice.invoiceId));await assignFlowPrimary(db,f.owner,'invoice',String(customerInvoice.invoiceId),{stepKey:'draft',userId:f.manager.userId,reason:'Assign customer invoice posting review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,String(customerInvoice.invoiceId),{expectedVersion:1,periodId:f.period,entryDate:'2026-09-03'},key());
  const saleDetail=await getOrder(f.owner,saleId),customerReturn=await requestCommerceReturn(f.owner,deliveryId,returnRequestSchema.parse({expectedVersion:2,date:'2026-09-04',reason:'Customer returned the labelled unused lot',lines:[{lineId:String(saleDetail.fulfillmentLines[0].id),quantity:'1',allocations:[{lotId:String(lotA.lot_id),quantity:'1'}],condition:'unopened',conditionNote:'Label intact but Quality must verify packaging',qualityDueDate:'2026-09-05'}]}),key());await decideCommerceReturn(f.head,String(customerReturn.id),'approve',{expectedVersion:1,reason:'RETURN-A label and customer return evidence verified'},key());
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?').bind(lotA.lot_id,warehouse).first()).toEqual({quantity_milli:1000,value_minor:1000});
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM inventory_lot_moves WHERE source_type='customer_return' AND lot_id=?").bind(lotA.lot_id).first()).toEqual({quantity_milli:1000,value_minor:1000});
  expect(await db.prepare("SELECT status,lot_id,held_quantity_milli,quantity_scope FROM inventory_quality_holds WHERE source_type='customer_return'").first()).toEqual({status:'held',lot_id:lotA.lot_id,held_quantity_milli:1000,quantity_scope:'partial'});
  expect((await getOrder(f.owner,saleId)).returnQualityHolds).toHaveLength(1);
});

it('posts a lot-level physical count with a missing lot and a newly found lot',async()=>{
  await supplyGrant();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
  f.manager.role='finance_approver';f.manager.assignmentId=await f.grant(f.manager,'finance_approver');
  await db.prepare("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,status,created_at,updated_at) VALUES(?,?,?,'5400','Stock variance','expense','THB','active',1,1)").bind(`variance-${key()}`,f.company,f.chart).run();
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Counted lot item',unit:'box',trackingMode:'lot',price:'10'},key())).id);
  const purchase=await approved('purchase',supplier,product,'2'),purchaseId=String(purchase.order.id),purchaseLine=String(purchase.lines[0].id);
  await fulfillOrder(f.owner,purchaseId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-02',lines:[{lineId:purchaseLine,quantity:'2',allocations:[{lotCode:'COUNT-A',quantity:'1'},{lotCode:'COUNT-B',quantity:'1'}]}]}),key());
  const lots=(await db.prepare("SELECT l.id,l.lot_code FROM inventory_lots l WHERE l.product_id=? ORDER BY l.lot_code").bind(product).all<{id:string;lot_code:string}>()).results;
  const created=await saveCount(f.owner,null,{warehouseId:warehouse,countDate:'2026-09-02',reason:'Reconcile labelled stock found during cycle count',lines:[{productId:product,countedQuantity:'3',allocations:[{lotId:lots[0].id,countedQuantity:'0',manufacturedDate:'',expiryDate:''},{lotId:lots[1].id,countedQuantity:'1',manufacturedDate:'',expiryDate:''},{lotCode:'COUNT-C',countedQuantity:'2',unitCost:'15',manufacturedDate:'2026-08-01',expiryDate:'2027-08-01'}]}]},key()),countId=String(created.id);
  let detail=await countDetail(f.owner,countId);expect(detail.lotLines).toHaveLength(3);expect(detail.count.net_value_minor).toBe(2000);
  const countFiles=await seedAttachmentSet(db,f.owner,{targetType:'stock_count',targetId:countId,purposeKey:'count_sheet',componentRef:'submit',expectedTargetVersion:1});
  const submitKey=key();await transitionCount(f.owner,countId,'submit',{expectedVersion:1,attachmentSetIds:[countFiles.setId]},submitKey);expect(await attachmentBindingState(db,countFiles.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'stock_count_submission',binding_id:`count-submit-${submitKey}`});const flow=await resolveFlow(db,f.owner,'stock_count',countId);await assignFlowPrimary(db,f.owner,'stock_count',countId,{stepKey:'submitted',userId:f.manager.userId,reason:'Assign independent count review',saveDefault:false,expectedVersion:flow.version,recordVersion:2},key());
  await supplyGrant('draining');
  await transitionCount(f.manager,countId,'approve',countActionSchema.parse({expectedVersion:2,reason:'Labels and valuation evidence checked'}),key());await transitionCount(f.manager,countId,'post',countActionSchema.parse({expectedVersion:3,reason:'Approved count reconciles to the inventory control total'}),key());
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:3000,value_minor:4000});
  expect((await db.prepare('SELECT l.lot_code,l.status,b.quantity_milli,b.value_minor FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.product_id=? AND b.warehouse_id=? ORDER BY l.lot_code').bind(product,warehouse).all()).results).toEqual([{lot_code:'COUNT-A',status:'depleted',quantity_milli:0,value_minor:0},{lot_code:'COUNT-B',status:'active',quantity_milli:1000,value_minor:1000},{lot_code:'COUNT-C',status:'active',quantity_milli:2000,value_minor:3000}]);
  detail=await countDetail(f.owner,countId);expect(detail.lotLines.some(l=>l.source_lot_id==null&&l.lot_code==='COUNT-C')).toBe(true);
  const journalId=String(detail.count.journal_id),assignment=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first<{version:number}>();
  expect((await journalDetail(f.owner,journalId,db)).journal).toMatchObject({id:journalId,status:'posted',source_type:'stock_count',source_id:countId});
  expect(await financeProductDocumentEntry(db,{companyId:f.company,branchId:f.owner.branchId,departmentId:f.owner.departmentId,userId:f.owner.userId,assignmentId:f.owner.assignmentId,assignmentVersion:assignment!.version,role:f.owner.role},{kind:'journal',id:journalId})).toEqual({capability:'finance.inventory-valuation',purchasePurpose:'history'});
  expect(await financeDocumentPresentationAccess(f.owner,'journal',journalId)).toEqual({sourceOnly:true,historyOnly:true});expect(await financeDocumentPostAccess(f.owner,'journal',journalId)).toEqual({canPost:false});expect(await financeDocumentFlow(f.owner,'journal',journalId)).toMatchObject({canAct:false,canManage:false});
  const journalVersion=Number(await db.prepare('SELECT version FROM finance_journals WHERE id=?').bind(journalId).first('version'));
  await expect(postJournal(f.owner,journalId,key(),{expectedVersion:journalVersion})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(ledgerWorkspace(f.owner,f.chart)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  const support=await seedAttachmentSet(db,f.owner,{targetType:'finance_journal',targetId:journalId,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:journalVersion}),binding=key();
  await bindRecordAttachmentSet(db,f.owner,support.setId,journalVersion,binding);expect(await attachmentBindingState(db,support.setId)).toMatchObject({set_state:'bound',link_state:'bound'});
  const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(support.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Count source retained',{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(support.sourceId).run();expect(await (await attachmentContent(db,env.H001_FILES,f.owner,support.linkId,'original')).object.text()).toBe('Count source retained');
  await expect(db.prepare("UPDATE inventory_count_lot_lines SET counted_quantity_milli=1000 WHERE count_line_id=? AND lot_code='COUNT-C'").bind(detail.lines[0].id).run()).rejects.toThrow('COUNT_LOT_EVIDENCE_IMMUTABLE');
});

it('preserves lot identity through dispatch, partial receipt, and an independently approved transit loss',async()=>{
  f.manager.role='finance_approver';f.manager.assignmentId=await f.grant(f.manager,'finance_approver');
  const target=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Lot transit destination'},key())).id);
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Lot transport item',unit:'box',trackingMode:'lot',price:'10'},key())).id);
  const purchase=await approved('purchase',supplier,product,'2'),purchaseId=String(purchase.order.id),purchaseLine=String(purchase.lines[0].id);
  await fulfillOrder(f.owner,purchaseId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-02',lines:[{lineId:purchaseLine,quantity:'2',allocations:[{lotCode:'ROAD-A',quantity:'1'},{lotCode:'ROAD-B',quantity:'1'}]}]}),key());
  const lots=(await db.prepare('SELECT id FROM inventory_lots WHERE product_id=? ORDER BY lot_code').bind(product).all<{id:string}>()).results;
  const transfer=await saveTransfer(f.owner,null,transferSchema.parse({sourceWarehouseId:warehouse,targetWarehouseId:target,transferDate:'2026-09-03',deliveryMode:'transit',arrivalDueDate:'2026-09-05',reason:'Track both supplier lots while held by the carrier',lines:[{productId:product,quantity:'2',allocations:lots.map(lot=>({lotId:lot.id,quantity:'1'}))}]}),key()),transferId=String(transfer.id);
  await transitionTransfer(f.owner,transferId,'submit',transferActionSchema.parse({expectedVersion:1}),key());const flow=await resolveFlow(db,f.owner,'transfer',transferId);await assignFlowPrimary(db,f.owner,'transfer',transferId,{stepKey:'submitted',userId:f.head.userId,reason:'Assign independent transport approval',saveDefault:false,expectedVersion:flow.version,recordVersion:2},key());await transitionTransfer(f.head,transferId,'post',transferActionSchema.parse({expectedVersion:2,reason:'Both labelled lots approved for carrier dispatch'}),key());
  const transitId=`transit-${transferId}`;await dispatchTransit(f.owner,transitId,transitDispatchSchema.parse({expectedVersion:1,date:'2026-09-03',trackingReference:'LOT-ROAD-001',evidence:'Carrier accepted both sealed and labelled lots'}),key());
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:0,value_minor:0});
  const transit=await transitDetail(f.head,transitId);expect(transit.lots).toHaveLength(2);const lineId=String(transit.lines[0].id),roadA=transit.lots.find(l=>l.lot_code==='ROAD-A')!,roadB=transit.lots.find(l=>l.lot_code==='ROAD-B')!;
  await receiveTransit(f.head,transitId,transitReceiveSchema.parse({expectedVersion:2,date:'2026-09-04',evidence:'ROAD-B physically counted at destination before ROAD-A',lines:[{lineId,quantity:'1',allocations:[{transitLotId:String(roadB.id),quantity:'1'}]}]}),key());
  const loss=await requestTransitLoss(f.head,transitId,transitLossSchema.parse({expectedVersion:3,date:'2026-09-04',lineId,quantity:'1',allocations:[{transitLotId:String(roadA.id),quantity:'1'}],evidence:'ROAD-A missing under carrier incident LOT-ROAD-002'}),key());
  await decideTransitLoss(f.manager,transitId,{expectedVersion:4,date:'2026-09-04',proposalId:String(loss.proposalId),decision:'approve',periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Independent Finance reviewed carrier evidence and lot cost'},key());
  expect((await getTransit(f.owner,transitId)).status).toBe('completed');
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(target,product).first()).toEqual({quantity_milli:1000,value_minor:1000});
  expect(await db.prepare('SELECT l.lot_code,b.quantity_milli FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=? AND l.product_id=?').bind(target,product).first()).toEqual({lot_code:'ROAD-B',quantity_milli:1000});
  expect(await db.prepare('SELECT sum(received_milli) received,sum(lost_milli) lost,sum(settled_value_minor) value FROM inventory_transit_lots WHERE transit_line_id=?').bind(lineId).first()).toEqual({received:1000,lost:1000,value:2000});
});

it.each(['stock_count','inventory_stock_count'] as const)('fresh private Inventory bytes fence the original %s source, actual Stock facts and deferred current IAM under Supply-only draining',async(targetType)=>{
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Private counted stock',price:'10'},key())).id),created=await saveCount(f.owner,null,{warehouseId:warehouse,countDate:'2026-09-02',reason:'Original private physical count source',lines:[{productId:product,countedQuantity:'0',allocations:[]}]},key()),id=String(created.id),file=await seedAttachmentSet(db,f.owner,{targetType,targetId:id,purposeKey:'count_evidence',componentRef:'submit',expectedTargetVersion:1});
 await transitionCount(f.owner,id,'submit',{expectedVersion:1,attachmentSetIds:[file.setId]},key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes=`Retained ${targetType} exact private bytes`;await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await supplyGrant();await supplyGrant('draining');
 let consumed=0;const store:import("@erpx/server-runtime/public/runtime/types").FileStore={get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;},put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
 expect(await(await attachmentContent(db,store,f.owner,file.linkId,'original')).object.text()).toBe(bytes);await expect(db.transaction(tx=>attachmentContent(tx,store,f.owner,file.linkId,'original'))).rejects.toMatchObject({code:'INVENTORY_FILE_READ_TRANSACTION_REQUIRED'});
 for(const fault of ['count','warehouse','product','iam'] as const){const trigger=`inventory_bytes_${key().replaceAll('-','')}`,effect=fault==='count'?`UPDATE inventory_counts SET reason=reason||' unexpected edit' WHERE id='${id}'`:fault==='warehouse'?`UPDATE commerce_warehouses SET name=name||' unexpected edit' WHERE id='${warehouse}'`:fault==='product'?`UPDATE commerce_products SET name=name||' unexpected edit' WHERE id='${product}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}'`,before=await db.prepare('SELECT reason,version FROM inventory_counts WHERE id=?').bind(id).first();await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{await expect(attachmentContent(db,store,f.owner,file.linkId,'original')).rejects.toMatchObject({code:fault==='iam'?'ATTACHMENT_FILE_READ_CHANGED':'INVENTORY_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(await db.prepare('SELECT reason,version FROM inventory_counts WHERE id=?').bind(id).first()).toEqual(before);expect(await db.prepare('SELECT name FROM commerce_products WHERE id=?').bind(product).first('name')).toBe('Private counted stock');expect(await db.prepare('SELECT name FROM commerce_warehouses WHERE id=?').bind(warehouse).first('name')).toBe('Traceable warehouse');expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first()).toEqual({revoked_at:null,version:1});}
 expect(consumed).toBe(5);expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(file.linkId).first('n'))).toBe(1);expect(await(await attachmentContent(db,store,f.owner,file.linkId,'original')).object.text()).toBe(bytes);await env.H001_FILES.delete(objectKey);await expect(attachmentContent(db,store,f.owner,file.linkId,'original')).rejects.toMatchObject({status:503,code:'FILE_UNAVAILABLE'});expect(consumed).toBe(6);expect(await db.prepare("SELECT reason FROM attachment_access_events WHERE link_id=? AND reason='FILE_UNAVAILABLE'").bind(file.linkId).first('reason')).toBe('FILE_UNAVAILABLE');expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['supply']);
});
it('fresh neutral lot private evidence requires real retained shared Stock history and preserves a zero-balance Sales-only COA without opening Count management',async()=>{
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Zero balance retained lot product',trackingMode:'lot',price:'10'},key())).id),lot=key();await db.prepare("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES(?,?,?,'lot',?,?,?)").bind(lot,f.company,product,`COA-${key()}`,f.owner.userId,Date.now()).run();const file=await seedAttachmentSet(db,f.owner,{targetType:'inventory_lot',targetId:lot,purposeKey:'coa',componentKind:'record',expectedTargetVersion:1});await bindRecordAttachmentSet(db,f.owner,file.setId,1,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes='Original retained neutral lot certificate';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-neutral-lot-provider'}),obligations:async()=>{throw Error('LOT_FIXTURE_CANNOT_INACTIVATE');}});await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'sales',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Original included shared Stock lot evidence',evidence:'test:lot-included-private-history'}});expect(await(await attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')).object.text()).toBe(bytes);await expect(countOptions(f.owner)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const previous=(await readCompanyPackageGrants(db,f.company))[0];await store.apply({expectedVersion:previous.version,idempotencyKey:key(),grant:{...previous,state:'draining',version:previous.version+1,reason:'Retain only original lot evidence during Sales draining',evidence:'test:lot-included-private-history'}});expect(await(await attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')).object.text()).toBe(bytes);
 const trigger=`lot_byte_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE inventory_lots SET expiry_date='2027-01-01' WHERE id='${lot}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{await expect(attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')).rejects.toMatchObject({code:'INVENTORY_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(await db.prepare('SELECT expiry_date FROM inventory_lots WHERE id=?').bind(lot).first('expiry_date')).toBeNull();expect(await db.prepare('SELECT count(*) n FROM inventory_lot_balances WHERE lot_id=?').bind(lot).first('n')).toBe(0);expect(await db.prepare('SELECT count(*) n FROM inventory_lot_moves WHERE lot_id=?').bind(lot).first('n')).toBe(0);expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['sales']);
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.owner.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(await db.prepare("SELECT reason FROM attachment_access_events WHERE link_id=? AND outcome='denied' ORDER BY created_at DESC LIMIT 1").bind(file.linkId).first('reason')).toBe('ATTACHMENT_TARGET_NOT_FOUND');expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});
});
it('fresh lot private evidence waits for an actual normal disabled File bind before its first snapshot with zero database retries',async()=>{
 const {committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Concurrent native lot product',trackingMode:'lot',price:'10'},key())).id),lot=key();await db.prepare("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,created_by,created_at) VALUES(?,?,?,'lot',?,?,?)").bind(lot,f.company,product,`COA-${key()}`,f.owner.userId,Date.now()).run();const file=await seedAttachmentSet(db,f.owner,{targetType:'inventory_lot',targetId:lot,purposeKey:'certificate',componentKind:'record',expectedTargetVersion:1}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Actually concurrently bound Inventory certificate',{httpMetadata:{contentType:'application/pdf'}});const trigger=`lot_snapshot_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF(SELECT version FROM inventory_lots WHERE id='${lot}')<>2 THEN RAISE EXCEPTION 'INVENTORY_FILE_STALE_FIRST_SNAPSHOT';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{const measured=await measureReads(()=>committedSourceVisibility(db,tx=>bindRecordAttachmentSet(tx,f.owner,file.setId,1,key()),()=>attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')));expect(await measured.result.object.text()).toBe('Actually concurrently bound Inventory certificate');expect(measured.metrics.deadlockFailures??0).toBe(0);expect(measured.metrics.serializationFailures??0).toBe(0);expect(measured.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(await db.prepare('SELECT version FROM inventory_lots WHERE id=?').bind(lot).first('version')).toBe(2);
});
