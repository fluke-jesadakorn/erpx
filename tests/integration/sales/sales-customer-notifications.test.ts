import { fulfillmentSchema } from "../../../src/server/modules/commerce/public/commerce";
import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {deliveryEvidence} from "../../support/helpers/delivery-evidence";
import {fulfillOrder,getOrder,orderSchema,saveMaster,saveOrder,transitionOrder} from "../../../src/server/modules/commerce/public/commerce";
import {deliverCustomerNotification,drainCustomerNotifications,queueDeliveryNotification,retryCustomerNotification,updateCustomerNotificationSettings} from "../../../src/server/modules/sales/public/sales-customer-notifications";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role='sales_manager';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'sales_manager');});

async function delivered(){
  const warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Notification warehouse'},key())).id);
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Notification product',price:'10'},key())).id);
  const customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Notification customer',notificationEmail:'receiving@example.invalid',notificationConsent:'granted'},key())).id);
  await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)').bind(warehouse,product).run();
  const order=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:customer,warehouseId:warehouse,orderDate:'2026-09-21',dueDate:'2026-09-22',lines:[{productId:product,quantity:'1',price:'10',taxBps:0}]}),key()),orderId=String(order.id);
  await transitionOrder(f.owner,orderId,{action:'submit',expectedVersion:1},key());await transitionOrder(f.head,orderId,{action:'approve',expectedVersion:2},key());
  const lineId=String((await getOrder(f.owner,orderId)).lines[0].id),proof=await deliveryEvidence(db,f.owner,orderId);
  const delivery=await fulfillOrder(f.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-22',lines:[{lineId,quantity:'1'}],...proof}),key());
  return {orderId,customer,fulfillmentId:String(delivery.fulfillmentId)};
}

it('queues a consent-aware immutable snapshot and delivers only through the sandbox adapter',async()=>{
  const source=await delivered();
  await expect(queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Carrier CR-101 confirmed the delivery window'},key())).rejects.toMatchObject({code:'CUSTOMER_NOTIFICATION_PROVIDER_REQUIRED'});
  await updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'sandbox'},key());
  const command=key(),queued=await queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Carrier CR-101 confirmed the delivery window'},command);
  expect(await queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Carrier CR-101 confirmed the delivery window'},command)).toEqual(queued);
  const before=await db.prepare('SELECT * FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first<Record<string,unknown>>();
  expect(before).toMatchObject({recipient_email:'receiving@example.invalid',adapter:'sandbox',status:'pending',attempts:0});
  await drainCustomerNotifications(db,{},Date.now());
  expect(await db.prepare('SELECT status,attempts,provider_message_id FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first()).toEqual({status:'sent',attempts:1,provider_message_id:`sandbox-${queued.id}`});
  await expect(queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Duplicate customer notice'},key())).rejects.toMatchObject({code:'CUSTOMER_ALREADY_NOTIFIED'});
  await expect(db.prepare('DELETE FROM commerce_customer_notifications WHERE id=?').bind(queued.id).run()).rejects.toThrow('CUSTOMER_NOTIFICATION_HISTORY_IMMUTABLE');
});

it('cancels a queued notice when consent or the address changes before provider delivery',async()=>{
  const source=await delivered();await updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'sandbox'},key());
  const queued=await queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Prepared before dispatch confirmation'},key());
  await saveMaster(f.owner,'parties',source.customer,{kind:'customer',code:key(),name:'Notification customer',paymentTermsDays:30,creditLimit:'0',status:'active',notificationEmail:'receiving@example.invalid',notificationConsent:'revoked',expectedVersion:1},key());
  const send=vi.fn();await deliverCustomerNotification(db,{},String(queued.id),send,Date.now());expect(send).not.toHaveBeenCalled();
  expect(await db.prepare('SELECT status,last_error FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first()).toEqual({status:'cancelled',last_error:'CUSTOMER_CONSENT_OR_EMAIL_CHANGED'});
});

it('records provider failure and retries the same immutable message without duplicate creation',async()=>{
  const source=await delivered();await updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'resend'},key());
  const queued=await queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Live adapter retry evidence'},key()),failed=vi.fn<(input:RequestInfo | URL,init?:RequestInit)=>Promise<Response>>(async()=>new Response('{}',{status:503})),now=Date.now();
  await expect(deliverCustomerNotification(db,{RESEND_API_KEY:'test-key',RESEND_FROM:'dispatch@example.invalid'},String(queued.id),failed,now)).rejects.toThrow('CUSTOMER_EMAIL_DELIVERY_FAILED');
  expect(await db.prepare('SELECT status,version,attempts,last_error FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first()).toEqual({status:'failed',version:2,attempts:1,last_error:'CUSTOMER_EMAIL_DELIVERY_FAILED'});
  await retryCustomerNotification(f.owner,String(queued.id),{expectedVersion:2},key());
  const sent=vi.fn<(input:RequestInfo | URL,init?:RequestInit)=>Promise<Response>>(async()=>Response.json({id:'provider-message-1'}));await deliverCustomerNotification(db,{RESEND_API_KEY:'test-key',RESEND_FROM:'dispatch@example.invalid'},String(queued.id),sent,now+2000);
  expect(sent).toHaveBeenCalledTimes(1);const request=sent.mock.calls[0]![1] as RequestInit;expect(request.headers).toMatchObject({'Idempotency-Key':queued.id});
  expect(await db.prepare('SELECT status,version,attempts,provider_message_id FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first()).toEqual({status:'sent',version:4,attempts:1,provider_message_id:'provider-message-1'});
});

it('holds future customer delivery jobs without sending or consuming them in production',async()=>{
  const source=await delivered();await updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'resend'},key());
  const queued=await queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Deferred pilot notice'},key());
  const send=vi.fn<typeof fetch>();vi.stubEnv('NODE_ENV','production');
  try {
    await expect(queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Another notice'},key())).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
    await deliverCustomerNotification(db,{RESEND_API_KEY:'unused',RESEND_FROM:'unused@example.invalid'},String(queued.id),send,Date.now());
    await drainCustomerNotifications(db,{RESEND_API_KEY:'unused',RESEND_FROM:'unused@example.invalid'},Date.now());
    expect(send).not.toHaveBeenCalled();
    expect(await db.prepare('SELECT status,attempts FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first()).toEqual({status:'pending',attempts:0});
  } finally {vi.unstubAllEnvs();}
});
