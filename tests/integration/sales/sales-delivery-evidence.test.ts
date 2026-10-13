import { fulfillmentSchema } from "../../../src/server/modules/commerce/public/commerce";
import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {deliveryEvidence} from "../../support/helpers/delivery-evidence";
import {fulfillOrder,getOrder,orderSchema,saveMaster,saveOrder,transitionOrder} from "../../../src/server/modules/commerce/public/commerce";
import {attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet} from "../../support/helpers/attachment-v2";
import type {WorkContext} from "../../../src/server/modules/identity_access/public/authorization";
import {loadIamAuthority} from "../../../src/server/modules/identity_access/public/iam-store";
import {assignFlowPrimary,resolveFlow} from "../../../src/server/modules/workflow/public/flow-routing";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,warehouseActor:WorkContext;
beforeAll(async()=>{enableAttachmentV2ForTest();await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
  f=await expenseFixture();f.head.role='sales_manager';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'sales_manager');
  warehouseActor={...f.claimant,role:'warehouse_manager',assignmentId:await f.grant(f.claimant,'warehouse_manager'),modules:['inventory','operations']};
  await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(warehouseActor.modules),warehouseActor.assignmentId).run();
  warehouseActor.iam=await loadIamAuthority(db,warehouseActor.companyId,warehouseActor.userId,warehouseActor.role);
});

it('requires, preserves, exposes, and prevents reuse of delivery evidence',async()=>{
  const warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Evidence warehouse'},key())).id);
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Evidence product',price:'10'},key())).id);
  const customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Evidence customer'},key())).id);
  await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)').bind(warehouse,product).run();
  const created=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:customer,warehouseId:warehouse,orderDate:'2026-09-21',dueDate:'2026-09-22',lines:[{productId:product,quantity:'2',price:'10',taxBps:0}]}),key()),orderId=String(created.id);
  await transitionOrder(f.owner,orderId,{action:'submit',expectedVersion:1},key());await transitionOrder(f.head,orderId,{action:'approve',expectedVersion:2},key());
  const flow=await resolveFlow(db,f.owner,'order',orderId);
  await assignFlowPrimary(db,f.owner,'order',orderId,{stepKey:'confirmed',userId:warehouseActor.userId,reason:'Assign the approved order to its current warehouse delivery operator.',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  const lineId=String((await getOrder(f.owner,orderId)).lines[0].id);
  await expect(fulfillOrder(warehouseActor,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-22',lines:[{lineId,quantity:'1'}]}),key())).rejects.toMatchObject({code:'DELIVERY_EVIDENCE_REQUIRED'});
  const pending=await seedAttachmentSet(db,warehouseActor,{targetType:'commerce_order_delivery',targetId:orderId,purposeKey:'proof_of_delivery',componentRef:'delivery',expectedTargetVersion:3,requiredCount:1,sourceState:'quarantined'});
  await expect(fulfillOrder(warehouseActor,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-22',evidenceNote:'Customer receipt is still being scanned',attachmentSetIds:[pending.setId],lines:[{lineId,quantity:'1'}]}),key())).rejects.toMatchObject({code:'ATTACHMENT_SCAN_PENDING'});
  expect(await attachmentBindingState(db,pending.setId)).toMatchObject({set_state:'draft',link_state:'draft',binding_type:null,binding_id:null});
  expect((await getOrder(f.owner,orderId)).order).toMatchObject({version:3,state:'confirmed'});
  const proof=await deliveryEvidence(db,warehouseActor,orderId,'Customer signed carrier receipt CR-101');
  expect((await getOrder(f.owner,orderId)).pendingDeliveryEvidence).toHaveLength(1);
  const delivered=await fulfillOrder(warehouseActor,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-22',lines:[{lineId,quantity:'1'}],...proof}),key());
  const detail=await getOrder(f.owner,orderId);
  expect(detail.pendingDeliveryEvidence).toHaveLength(0);
  expect(detail.fulfillmentEvidence).toEqual([expect.objectContaining({fulfillment_id:delivered.fulfillmentId,attachment_id:proof.evidenceIds[0]})]);
  expect(detail.fulfillments[0]).toMatchObject({evidence_note:'Customer signed carrier receipt CR-101'});
  await expect(fulfillOrder(warehouseActor,orderId,fulfillmentSchema.parse({expectedVersion:4,date:'2026-09-22',lines:[{lineId,quantity:'1'}],...proof}),key())).rejects.toMatchObject({code:'DELIVERY_EVIDENCE_INVALID'});
  const clean=await seedAttachmentSet(db,warehouseActor,{targetType:'commerce_order_delivery',targetId:orderId,purposeKey:'proof_of_delivery',componentRef:'delivery',expectedTargetVersion:4,requiredCount:1,compatibilityAdapter:true}),previous=process.env.ERPX_ATTACHMENTS_V2;
  process.env.ERPX_ATTACHMENTS_V2='false';
  let final:Awaited<ReturnType<typeof fulfillOrder>>;
  try{final=await fulfillOrder(warehouseActor,orderId,fulfillmentSchema.parse({expectedVersion:4,date:'2026-09-22',evidenceNote:'Second customer delivery receipt checked',attachmentSetIds:[clean.setId],lines:[{lineId,quantity:'1'}]}),key());}
  finally{if(previous===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previous;}
  expect(await attachmentBindingState(db,clean.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'commerce_fulfillment',binding_id:String(final.fulfillmentId)});
  await expect(db.prepare('DELETE FROM commerce_fulfillment_evidence WHERE attachment_id=?').bind(proof.evidenceIds[0]).run()).rejects.toThrow('FULFILLMENT_EVIDENCE_IMMUTABLE');
});
