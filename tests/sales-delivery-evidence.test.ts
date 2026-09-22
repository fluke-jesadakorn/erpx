import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {deliveryEvidence} from './helpers/delivery-evidence';
import {fulfillOrder,getOrder,orderSchema,saveMaster,saveOrder,transitionOrder} from '../src/server/commerce';

vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role='sales_manager';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'sales_manager');});

it('requires, preserves, exposes, and prevents reuse of delivery evidence',async()=>{
  const warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Evidence warehouse'},key())).id);
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Evidence product',price:'10'},key())).id);
  const customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Evidence customer'},key())).id);
  await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)').bind(warehouse,product).run();
  const created=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:customer,warehouseId:warehouse,orderDate:'2026-09-21',dueDate:'2026-09-22',lines:[{productId:product,quantity:'2',price:'10',taxBps:0}]}),key()),orderId=String(created.id);
  await transitionOrder(f.owner,orderId,{action:'submit',expectedVersion:1},key());await transitionOrder(f.head,orderId,{action:'approve',expectedVersion:2},key());
  const lineId=String((await getOrder(f.owner,orderId)).lines[0].id);
  await expect(fulfillOrder(f.owner,orderId,{expectedVersion:3,date:'2026-09-22',lines:[{lineId,quantity:'1'}]},key())).rejects.toMatchObject({code:'DELIVERY_EVIDENCE_REQUIRED'});
  const proof=await deliveryEvidence(db,f.owner,orderId,'Customer signed carrier receipt CR-101');
  expect((await getOrder(f.owner,orderId)).pendingDeliveryEvidence).toHaveLength(1);
  const delivered=await fulfillOrder(f.owner,orderId,{expectedVersion:3,date:'2026-09-22',lines:[{lineId,quantity:'1'}],...proof},key());
  const detail=await getOrder(f.owner,orderId);
  expect(detail.pendingDeliveryEvidence).toHaveLength(0);
  expect(detail.fulfillmentEvidence).toEqual([expect.objectContaining({fulfillment_id:delivered.fulfillmentId,attachment_id:proof.evidenceIds[0]})]);
  expect(detail.fulfillments[0]).toMatchObject({evidence_note:'Customer signed carrier receipt CR-101'});
  await expect(fulfillOrder(f.owner,orderId,{expectedVersion:4,date:'2026-09-22',lines:[{lineId,quantity:'1'}],...proof},key())).rejects.toMatchObject({code:'DELIVERY_EVIDENCE_INVALID'});
  await expect(db.prepare('DELETE FROM commerce_fulfillment_evidence WHERE attachment_id=?').bind(proof.evidenceIds[0]).run()).rejects.toThrow('FULFILLMENT_EVIDENCE_IMMUTABLE');
});
