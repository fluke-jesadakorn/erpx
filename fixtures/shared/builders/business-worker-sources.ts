/** Declared isolated inputs with native Business owner lifecycle sources for worker verification. */
import {readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {PostgresDatabase} from "../../../src/server/modules/runtime/adapters/runtime/postgres";
import type {FileStore} from "../../../src/server/modules/runtime/adapters/runtime/types";
import {seedExpenseCompany} from "../../expenses/company";
import {mapSpendingOrganization} from "../../../tests/support/helpers/spending-organization";
import {deliveryEvidence} from "../../../tests/support/helpers/delivery-evidence";
import {moduleRegistry} from "../../../src/data/module-registry";
import {assignFlowPrimary,resolveFlow} from "../../../src/server/modules/workflow/public/flow-routing";
import {changeOrderReservation,fulfillOrder,getOrder,orderSchema,saveMaster,saveOrder,transitionOrder} from "../../../src/server/modules/commerce/public/commerce";
import {createQualityHold} from "../../../src/server/modules/quality/public/quality";
import {changeManufacturingOrder,createPlanningScenario,createSpecification,reviewPlanningRecommendation,reviewSpecification} from "../../../src/server/modules/manufacturing/public/production";
import {createSupportTicket} from "../../../src/server/modules/support/public/support";
import {changeAgreement,changeServiceOrder,createAgreement,createServiceOrder} from "../../../src/server/modules/field_service/public/field-service";
import {queueDeliveryNotification,updateCustomerNotificationSettings} from "../../../src/server/modules/sales/public/sales-customer-notifications";
import {createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,drainAttachmentScans} from "../../../src/server/modules/files/public/attachments/service";
import {saveCount,transitionCount,countSchema} from "../../../src/server/modules/inventory/public/inventory-counts";
import {createAccount,createAccountSchema,postJournal} from "../../../src/server/modules/finance/public/finance";
const key=()=>crypto.randomUUID();
export async function seedBusinessWorkerSources(db:PostgresDatabase,files:FileStore,options:{now:number;label:string;proof:'declared-source-fixture'|'clamav';nativeOpening?:boolean;adapter?:'sandbox'|'resend'}){
 const {now}=options,adapter=options.adapter??'sandbox';
 const url=new URL(String(db.pool.options.connectionString));
 if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||!/^\/erpx_browser_[a-f0-9]{12}_h001_db$/.test(url.pathname))throw new Error('EXACT_ISOLATED_BUSINESS_WORKER_DATABASE_REQUIRED');
 const f=await seedExpenseCompany(db,files);await mapSpendingOrganization(f);
 const reviewer={...f.tie,role:'holding_owner',assignmentId:await f.grant(f.tie,'holding_owner')},qualityReviewer={...f.manager,role:'quality_manager',assignmentId:await f.grant(f.manager,'quality_manager')};
 const modules=moduleRegistry.map(module=>module.id);for(const actor of [f.owner,reviewer,qualityReviewer,f.claimant,f.manager]){actor.modules=modules;await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(modules),actor.assignmentId).run();}
 const inventory=await createAccount(f.owner,createAccountSchema.parse({chartId:f.chart,code:'WORKER-INVENTORY',name:'Fictional worker source stock asset',category:'asset',currency:'THB'}),key());
 await db.prepare('UPDATE commerce_settings SET inventory_account_id=? WHERE company_id=?').bind(inventory.id,f.company).run();
 const warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Declared worker stock fixture'},key())).id);
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Declared reserved replacement',price:'10'},key())).id),heldProduct=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Declared inspection sample',price:'10'},key())).id),output=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Declared finished sample',price:'10'},key())).id);
 // Opening stock is a declared fixture. Every reservation, approval, hold, delivery and message below uses its native owner command.
 const openingCounts:string[]=[];
 if(options.nativeOpening){
  await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'5400','Declared worker opening stock variance','expense','THB',?,?)").bind(`${f.company}-worker-stock-variance`,f.chart,f.company,now,now).run();
  for(const productId of [product,heldProduct]){
   const count=await saveCount(f.owner,null,countSchema.parse({warehouseId:warehouse,countDate:new Date(now).toISOString().slice(0,10),reason:'Declared fictional quantity and cost, independently counted and posted through the native owner.',lines:[{productId,countedQuantity:'5',unitCost:'6'}]}),key()),id=String(count.id);
   await transitionCount(f.owner,id,'submit',{expectedVersion:1,attachmentSetIds:[]},key());
   for(const [version,step,action] of [[2,'submitted','approve'],[3,'approved','post']] as const){
    const flow=await resolveFlow(db,f.owner,'stock_count',id);await assignFlowPrimary(db,f.owner,'stock_count',id,{stepKey:step,userId:f.manager.userId,expectedVersion:flow.version,recordVersion:version,reason:'Independent Finance owner reviews the declared opening valuation.',saveDefault:false},key());
    await transitionCount(f.manager,id,action,{expectedVersion:version,attachmentSetIds:[],reason:'Independent Finance posts the exact fictional opening stock value.'},key());
   }
   openingCounts.push(id);
  }
 }else await db.batch([product,heldProduct].map(id=>db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,5000,3000,1)').bind(warehouse,id)));
 const party=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Declared notified customer',notificationEmail:'receiver@engineering.invalid',notificationConsent:'granted'},key())).id);
 async function order(kind:'sales'|'quote'){
  const result=await saveOrder(f.owner,null,orderSchema.parse({kind,partyId:party,warehouseId:warehouse,orderDate:new Date(now).toISOString().slice(0,10),dueDate:new Date(now+86400000).toISOString().slice(0,10),lines:[{productId:product,quantity:'1',price:'10',taxBps:0}]}),key()),id=String(result.id);
  if(kind==='sales'){await transitionOrder(f.owner,id,{action:'submit',expectedVersion:1},key());await transitionOrder(reviewer,id,{action:'approve',expectedVersion:2},key());}return getOrder(f.owner,id);
 }
 const sales=await order('sales'),salesId=String(sales.order.id);await changeOrderReservation(f.owner,salesId,{action:'reserve',expectedVersion:3,lineId:String(sales.lines[0].id),quantity:'1',expiresAt:now+3600000,reason:'Native worker fixture dispatch commitment'},key());
 const quote=await order('quote'),ticket=await createSupportTicket(f.owner,{orderId:String(quote.order.id),sourceVersion:1,title:'Declared worker field dispatch',issue:'Native reserved spare is needed for this fictional customer visit.',ownerId:reviewer.userId,dueDate:new Date(now+86400000).toISOString().slice(0,10)},key());
 const agreement=await createAgreement(f.owner,{branchId:f.branch,departmentId:null,customerId:party,assetId:null,title:'Declared worker field coverage',startDate:'2026-01-01',endDate:'2027-12-31',currency:'THB',visitRateMinor:1000,warrantyDiscountBps:0},key());
 await changeAgreement(reviewer,String(agreement.id),{action:'activate',expectedVersion:1,reason:'Different native reviewer confirms fictional coverage'},key());
 const field=await createServiceOrder(f.owner,{ticketId:String(ticket.id),ticketVersion:1,agreementId:String(agreement.id),agreementVersion:2,assetId:null,issue:'Inspect the fictional replacement unit',site:'Declared engineering site',technicianId:f.claimant.userId,scheduledAt:now+60000},key()),fieldId=String(field.id);
 await changeServiceOrder(f.claimant,fieldId,{action:'reserve-part',expectedVersion:1,warehouseId:warehouse,productId:product,quantityMilli:1000,expiresAt:now+3600000,reason:'Native technician reserves the actual spare for the appointment'},key());
 const spec=await createSpecification(f.owner,{name:'Declared worker production specification',mode:'bom',outputProductId:output,batchQuantity:'1',expectedYieldBps:10000,lines:[{productId:product,quantity:'1'}]},key());
 await reviewSpecification(reviewer,String(spec.id),{action:'approve',expectedVersion:1,reason:'Independent native material specification review'},key());
 const plan=await createPlanningScenario(f.owner,{warehouseId:warehouse,specificationId:String(spec.id),specificationVersion:2,demandQuantity:'1',dueDate:new Date(now+86400000).toISOString().slice(0,10),capacityMinutes:10},key()),released=await reviewPlanningRecommendation(reviewer,String(plan.recommendationId),{action:'approve',expectedVersion:1,reason:'Independent native demand and capacity review'},key()),productionId=String(released.orderId);
 await changeManufacturingOrder(f.owner,productionId,{action:'reserve',expectedVersion:1,expiresAt:now+3600000,reason:'Native manufacturing reserves reviewed material requirements'},key());
 const due=await createQualityHold(f.owner,{warehouseId:warehouse,productId:heldProduct,expectedStockVersion:Number(await db.prepare('SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,heldProduct).first('version')),dueDate:new Date(now).toISOString().slice(0,10),reason:'Native due-soon inspection source'},key()),overdue=await createQualityHold(f.owner,{warehouseId:warehouse,productId:heldProduct,expectedStockVersion:Number(await db.prepare('SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,heldProduct).first('version')),dueDate:new Date(now-86400000).toISOString().slice(0,10),reason:'Native overdue inspection source'},key());
 const deliveryOrder=await order('sales'),deliveryOrderId=String(deliveryOrder.order.id);
 for(const [stepKey,userId] of [['confirmed',f.owner.userId],['stock_gl',reviewer.userId]] as const){const flow=await resolveFlow(db,f.owner,'order',deliveryOrderId);await assignFlowPrimary(db,f.owner,'order',deliveryOrderId,{stepKey,userId,reason:'Native owner assigns the responsible warehouse and independent Finance reviewer',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());}
 let evidence: {sourceId:string;sha256:string;engine:string}|null=null;
 let proof:{evidenceIds:string[];evidenceNote:string;attachmentSetIds?:string[]};
 if(options.proof==='clamav'){
  const bytes=await readFile(join(process.cwd(),'public/icon-192.png')),set=await createAttachmentSet(db,f.owner,{targetType:'commerce_order_delivery',targetId:deliveryOrderId,componentKind:'action',componentRef:'delivery',purposeKey:'proof_of_delivery',expectedTargetVersion:3},key());
  const reserved=await reserveAttachmentFile(db,f.owner,set.id,{filename:'declared-worker-delivery-proof.png',mediaType:'image/png',bytes:bytes.byteLength,caption:`Declared repository icon fixture for ${options.label}; not a real customer signature`},key());
  const uploaded=await completeAttachmentUpload(db,files,f.owner,reserved.uploadIntentId,Uint8Array.from(bytes).buffer);
  if(!await drainAttachmentScans(db,files,uploaded.sourceId))throw new Error('WORKER_NATIVE_DELIVERY_SCAN_REQUIRED');
  const scanned=await db.prepare("SELECT a.source_state,a.checksum_sha256,s.scanner_engine FROM attachments a JOIN attachment_scan_attempts s ON s.attachment_id=a.id AND s.state='clean' WHERE a.id=? ORDER BY s.attempt DESC LIMIT 1").bind(uploaded.sourceId).first<{source_state:string;checksum_sha256:string;scanner_engine:string}>();
  const sha256=createHash('sha256').update(bytes).digest('hex');if(scanned?.source_state!=='available'||scanned.checksum_sha256!==sha256||!scanned.scanner_engine)throw new Error('WORKER_NATIVE_DELIVERY_SCAN_MISMATCH');
  evidence={sourceId:uploaded.sourceId,sha256,engine:scanned.scanner_engine};proof={evidenceIds:[],evidenceNote:'Declared repository icon stands in for the fictional receiver evidence; actual clean File Source is consumed by native fulfillment.',attachmentSetIds:[set.id]};
 }else proof=await deliveryEvidence(db,f.owner,deliveryOrderId);
 const delivery=await fulfillOrder(f.owner,deliveryOrderId,{expectedVersion:3,date:new Date(now).toISOString().slice(0,10),lines:[{lineId:String(deliveryOrder.lines[0].id),quantity:'1',allocations:[]}],...proof,attachmentSetIds:proof.attachmentSetIds??[]},key());
 if(options.nativeOpening){
  const journal=await db.prepare("SELECT j.id,j.version FROM commerce_fulfillments d JOIN commerce_orders o ON o.id=d.order_id JOIN finance_journals j ON j.id=d.journal_id AND j.company_id=o.company_id WHERE d.id=? AND o.company_id=? AND j.source_type='delivery' AND j.source_id=d.id AND j.status='draft'").bind(delivery.fulfillmentId,f.company).first<{id:string;version:number}>();
  if(!journal)throw new Error('WORKER_NATIVE_DELIVERY_JOURNAL_REQUIRED');
  await postJournal(reviewer,journal.id,key(),{expectedVersion:Number(journal.version)});
 }
 await updateCustomerNotificationSettings(reviewer,{expectedVersion:0,adapter},key());
 const message=await queueDeliveryNotification(f.owner,String(delivery.fulfillmentId),{expectedVersion:1,note:'Declared native delivery notification; sandbox or injected no-network provider only'},key());
 return {f,reviewer,qualityReviewer,party,salesId,fieldId,productionId,openingCounts,evidence,warehouse,heldProduct,holdIds:[String(due.id),String(overdue.id)],messageId:String(message.id),fulfillmentId:String(delivery.fulfillmentId)};
}
