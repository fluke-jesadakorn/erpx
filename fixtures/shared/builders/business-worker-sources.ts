import {seedCommerceFulfillment} from "../../../tests/support/helpers/commerce-fulfillment-seed";
/** Declared isolated inputs with native Business owner lifecycle sources for worker verification. */
import {readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
import type {FileStore} from "@erpx/server-runtime/adapters/runtime/types";
import {seedExpenseCompany} from "../../expenses/company";
import {mapSpendingOrganization} from "../../../tests/support/helpers/spending-organization";
import {deliveryEvidence} from "../../../tests/support/helpers/delivery-evidence";
import {moduleRegistry} from "@erpx/catalogue/module-registry";
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import {changeOrderReservation,orderSchema,saveMaster,saveOrder,transitionOrder} from '@erpx/server-commerce/public/commerce';
import {getOrder} from '@erpx/server-commerce/public/commerce-order-read';
import {seedNativeQualityHold} from "../../quality/hold";
import {changeManufacturingOrder,createPlanningScenario,reviewPlanningRecommendation} from "@erpx/server-manufacturing/public/production";
import {createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,drainAttachmentScans} from "@erpx/server-files/public/attachments/service";
import {saveCount,transitionCount,countSchema} from "@erpx/server-inventory/public/inventory-counts";
import {createAccount,createAccountSchema} from '@erpx/server-finance/public/finance';
import {markFinanceJournalPosted} from '../../../tests/support/helpers/finance-journal-seed';
const key=()=>crypto.randomUUID();
/** Commands are native-only. Seed the exact owner rows the native
 * settings/queue commands persist, including the delivery-worker source hash. */
async function seedQueuedNotice(db:PostgresDatabase,owner:{userId:string;assignmentId:string;role:string;companyId:string;branchId:string|null;departmentId:string|null},reviewer:{userId:string},fulfillmentId:string,adapter:'sandbox'|'resend',note:string,now:number){
 const facts=await db.prepare("SELECT f.number fulfillment_number,o.number order_number,f.fulfillment_date,o.id order_id,o.branch_id,o.department_id,o.party_id,p.name party_name,p.version party_version,p.notification_email,p.notification_consent FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id WHERE f.id=? AND o.company_id=?").bind(fulfillmentId,owner.companyId).first<{fulfillment_number:string;order_number:string;fulfillment_date:string;order_id:string;branch_id:string;department_id:string|null;party_id:string;party_name:string;party_version:number;notification_email:string|null;notification_consent:string|null}>();
 if(!facts?.notification_email||facts.notification_consent!=='granted')throw new Error('WORKER_NOTICE_CONSENT_REQUIRED');
 const lines=(await db.prepare('SELECT p.sku,fl.quantity_milli,p.unit FROM commerce_fulfillment_lines fl JOIN commerce_order_lines ol ON ol.id=fl.order_line_id JOIN commerce_products p ON p.id=ol.product_id WHERE fl.fulfillment_id=? ORDER BY ol.ordinal').bind(fulfillmentId).all<{sku:string;quantity_milli:number;unit:string}>()).results;
 const subject=`Delivery ${facts.fulfillment_number} for order ${facts.order_number}`,body=[`Hello ${facts.party_name},`,`Your delivery ${facts.fulfillment_number} for order ${facts.order_number} was recorded on ${facts.fulfillment_date}.`,...lines.map(line=>`${line.sku}: ${Number(line.quantity_milli)/1000} ${line.unit}`),`Update from our team: ${note}`].join('\n\n');
 const id=`customer-notification-${key()}`;
 await db.batch([
  db.prepare("INSERT INTO commerce_customer_notification_settings(company_id,adapter,version,updated_by,updated_at) VALUES(?,?,1,?,?) ON CONFLICT(company_id) DO UPDATE SET adapter=excluded.adapter,version=commerce_customer_notification_settings.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(owner.companyId,adapter,reviewer.userId,now),
  db.prepare('INSERT INTO commerce_customer_notifications(id,company_id,branch_id,order_id,fulfillment_id,party_id,party_version,recipient_email,subject,body_text,adapter,next_attempt_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(id,owner.companyId,facts.branch_id,facts.order_id,fulfillmentId,facts.party_id,facts.party_version,facts.notification_email.toLowerCase(),subject,body,adapter,now,owner.userId,now,now),
 ]);
 const hash=await db.prepare(`SELECT encode(sha256(convert_to(jsonb_build_array(n.id,n.company_id,n.branch_id,n.order_id,n.fulfillment_id,n.party_id,n.party_version,n.recipient_email,n.subject,n.body_text,n.adapter,n.created_by,n.created_at,delivery.fulfillment_id,delivery.order_id,delivery.company_id,delivery.branch_id,delivery.department_id,delivery.party_id,delivery.order_kind,customer.id,customer.company_id,customer.kind)::text,'UTF8')),'hex') hash FROM commerce_customer_notifications n LEFT JOIN (SELECT f.id fulfillment_id,o.id order_id,o.company_id,o.branch_id,o.department_id,o.kind order_kind,o.state,o.party_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.kind='sales') delivery ON delivery.fulfillment_id=n.fulfillment_id AND delivery.order_id=n.order_id AND delivery.company_id=n.company_id AND delivery.party_id=n.party_id LEFT JOIN (SELECT id,company_id,kind,status,notification_email,notification_consent FROM finance_parties WHERE kind='customer') customer ON customer.id=n.party_id AND customer.company_id=n.company_id WHERE n.id=?`).bind(id).first<string>('hash');
 if(!hash)throw new Error('WORKER_NOTICE_HASH_REQUIRED');
 const command=`queue-${id}`,authority=JSON.stringify({userId:owner.userId,assignmentId:owner.assignmentId,role:owner.role,companyId:owner.companyId,branchId:owner.branchId,departmentId:owner.departmentId,iam:{}});
 await db.batch([
  db.prepare("INSERT INTO sales_notification_execution_authorizations(command_id,notification_id,company_id,kind,actor_id,assignment_id,authority_json,source_hash,notification_version,created_at) VALUES(?,?,?,'queue',?,?,?,?,1,?)").bind(command,id,owner.companyId,owner.userId,owner.assignmentId,authority,hash,now),
  db.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,'commerce.customer-notification.queue',?,?,?,?,?,?)").bind(command,owner.userId,'0'.repeat(64),fulfillmentId,JSON.stringify({id}),now,authority),
 ]);
 return {id};
}
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
 // Field scheduled recipients, immutable IDs, retry/ack retention and rollback
 // now execute through Rust in scheduled_field_reservation_alerts_keep_original_phase_identity_and_rollback_sources.
 // This fixture deliberately seeds only the remaining legacy worker owners.
 const specId=`mfg-spec-${key()}`,specAt=now;
 await db.batch([
  db.prepare("INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,status,created_by,reviewed_by,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'approved',?,'native-reviewer',2,?,?)").bind(specId,f.company,'Declared worker production specification','bom',output,1000,10000,f.owner.userId,specAt,specAt),
  db.prepare("INSERT INTO manufacturing_specification_lines VALUES(?,?,?,?,?,?)").bind(`${specId}-line-0`,specId,product,1000,null,0),
 ]);
 const spec={id:specId};
 const plan=await createPlanningScenario(f.owner,{warehouseId:warehouse,specificationId:String(spec.id),specificationVersion:2,demandQuantity:'1',dueDate:new Date(now+86400000).toISOString().slice(0,10),capacityMinutes:10},key()),released=await reviewPlanningRecommendation(reviewer,String(plan.recommendationId),{action:'approve',expectedVersion:1,reason:'Independent native demand and capacity review'},key()),productionId=String(released.orderId);
 await changeManufacturingOrder(f.owner,productionId,{action:'reserve',expectedVersion:1,expiresAt:now+3600000,reason:'Native manufacturing reserves reviewed material requirements'},key());
 const due=await seedNativeQualityHold(db,f.owner,{warehouseId:warehouse,productId:heldProduct,expectedStockVersion:Number(await db.prepare('SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,heldProduct).first('version')),dueDate:new Date(now).toISOString().slice(0,10),reason:'Native due-soon inspection source',createdAt:now},key()),overdue=await seedNativeQualityHold(db,f.owner,{warehouseId:warehouse,productId:heldProduct,expectedStockVersion:Number(await db.prepare('SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,heldProduct).first('version')),dueDate:new Date(now-86400000).toISOString().slice(0,10),reason:'Native overdue inspection source',createdAt:now},key());
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
 const delivery=await seedCommerceFulfillment(db, f.owner,deliveryOrderId,{expectedVersion:3,date:new Date(now).toISOString().slice(0,10),lines:[{lineId:String(deliveryOrder.lines[0].id),quantity:'1',allocations:[]}],...proof,attachmentSetIds:proof.attachmentSetIds??[]},key());
 if(options.nativeOpening){
  const journal=await db.prepare("SELECT j.id,j.version FROM commerce_fulfillments d JOIN commerce_orders o ON o.id=d.order_id JOIN finance_journals j ON j.id=d.journal_id AND j.company_id=o.company_id WHERE d.id=? AND o.company_id=? AND j.source_type='delivery' AND j.source_id=d.id AND j.status='draft'").bind(delivery.fulfillmentId,f.company).first<{id:string;version:number}>();
  if(!journal)throw new Error('WORKER_NATIVE_DELIVERY_JOURNAL_REQUIRED');
  await markFinanceJournalPosted(db,journal.id,reviewer.userId);
 }
 const message=await seedQueuedNotice(db,f.owner,reviewer,String(delivery.fulfillmentId),adapter,'Declared native delivery notification; sandbox or injected no-network provider only',now);
 return {f,reviewer,qualityReviewer,party,salesId,productionId,openingCounts,evidence,warehouse,heldProduct,holdIds:[String(due.id),String(overdue.id)],messageId:String(message.id),fulfillmentId:String(delivery.fulfillmentId)};
}
