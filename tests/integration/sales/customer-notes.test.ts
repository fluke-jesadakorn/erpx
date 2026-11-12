import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {saveMaster,saveOrder,orderSchema} from '@erpx/server-commerce/public/commerce';
import {customerNotesForOrder,customerNotesForCustomer} from "@erpx/server-sales/public/customer-notes";
import {seedCustomerNote,seedCustomerNoteChange} from "../../support/helpers/customer-note-seed";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,orderId:string,customerId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();const party=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Simulated contact customer'},key());customerId=String(party.id);const warehouse=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Contact warehouse'},key()),product=await saveMaster(f.owner,'products',null,{sku:key(),name:'Contact product',price:'10'},key());const order=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:party.id,warehouseId:warehouse.id,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product.id,quantity:'1',price:'10',taxBps:0}]}),key());orderId=String(order.id);});
/** Note commands are native-owned. These read oracles seed the exact immutable
 * owner rows the native command persists (create/correct/archive versions). */
async function note(body='Customer requested a revised delivery proposal; no new commitment approved.',order?:string|null){return seedCustomerNote(db,{companyId:f.company,customerId,orderId:order===undefined?orderId:order,channel:'phone',occurredAt:Date.now()-1000,body,createdBy:f.owner.userId});}

it('records standalone Customer 360 contact evidence with correction, archive and reload history',async()=>{
 const created=await note('Customer asked for a copy of the updated service terms.',null),id=created.id;
 let view=await customerNotesForCustomer(f.owner,customerId);
 expect(view.notes[0]).toMatchObject({id,order_id:null,status:'active',version:1});
 expect(view.customer.id).toBe(customerId);
 await seedCustomerNoteChange(db,{noteId:id,action:'correct',expectedVersion:1,body:'Customer asked for the updated service terms by email.',reason:'Confirmed the wording in the mailbox',actorId:f.owner.userId});
 await seedCustomerNoteChange(db,{noteId:id,action:'archive',expectedVersion:2,reason:'Replaced by the signed customer correspondence',actorId:f.owner.userId});
 view=await customerNotesForCustomer(f.owner,customerId);
 expect(view.notes[0]).toMatchObject({id,status:'archived',version:3,canChange:false});
 expect(view.history.filter(entry=>entry.note_id===id)).toHaveLength(3);
});

it('reads order contact evidence without rewriting source or ordered history',async()=>{
 const created=await note(),id=created.id;
 await seedCustomerNoteChange(db,{noteId:id,action:'correct',expectedVersion:1,body:'Customer requested Tuesday, subject to order approval.',reason:'Correct transcription after checking the call',actorId:f.owner.userId});
 await seedCustomerNoteChange(db,{noteId:id,action:'archive',expectedVersion:2,reason:'Superseded by signed order evidence',actorId:f.owner.userId});
 const view=await customerNotesForOrder(f.owner,orderId);
 expect(view.notes[0]).toMatchObject({status:'archived',version:3,canChange:false});
 expect(view.history).toHaveLength(3);
 expect(view.history.find(entry=>entry.version===1)?.body).toBe('Customer requested a revised delivery proposal; no new commitment approved.');
 expect(view.order.version).toBe(1);
 await expect(db.prepare('DELETE FROM customer_contact_note_history WHERE note_id=?').bind(id).run()).rejects.toThrow('CUSTOMER_NOTE_EVIDENCE_IMMUTABLE');
});

it('hides notes outside company, branch, department and commerce roles',async()=>{
 await note();
 await expect(customerNotesForOrder({...f.owner,companyId:'other'},orderId)).rejects.toMatchObject({status:404});
 await expect(customerNotesForCustomer({...f.owner,companyId:'other'},customerId)).rejects.toMatchObject({status:404});
 for(const actor of [{...f.owner,branchId:'other'},{...f.owner,departmentId:'other'}]){
  await expect(customerNotesForOrder(actor,orderId)).rejects.toMatchObject({status:404});
  expect((await customerNotesForCustomer(actor,customerId)).notes).toEqual([]);
 }
 await expect(customerNotesForOrder({...f.owner,role:'employee'},orderId)).rejects.toMatchObject({status:403});
});

it('keeps customer evidence attached to its original customer source',async()=>{
 await note();
 const other=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Different customer'},key());
 await expect(db.prepare('UPDATE commerce_orders SET party_id=? WHERE id=?').bind(other.id,orderId).run()).rejects.toThrow('CUSTOMER_CONTACT_SOURCE_HAS_EVIDENCE');
});
