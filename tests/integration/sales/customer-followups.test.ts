import {afterEach,beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {saveMaster,saveOrder,orderSchema} from '@erpx/server-commerce/public/commerce';
import {customerFollowupDetail,customerFollowupWork,customerFollowupWorkspace,getCustomerFollowup} from "@erpx/server-sales/public/customer-followups";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
let followupDatabase:SqlDatabase|undefined;
afterEach(()=>{vi.unstubAllEnvs();followupDatabase=undefined;});
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:followupDatabase??env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,orderId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role='sales_operator';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'sales_operator');const party=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Follow-up customer'},key()),warehouse=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Follow-up warehouse'},key()),product=await saveMaster(f.owner,'products',null,{sku:key(),name:'Follow-up product',price:'10'},key());const order=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:party.id,warehouseId:warehouse.id,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product.id,quantity:'1',price:'10',taxBps:0}]}),key());orderId=String(order.id);});
/** Commands are native-only. Read oracles seed the exact owner rows the
 * `customer_followups_router` command persists, including its notification. */
async function seedFollowup(ownerId=f.head.userId,status:'open'|'completed'='open'){
 const id=`followup-${key()}`,now=Date.now();
 await db.batch([
  db.prepare('INSERT INTO customer_followups(id,company_id,order_id,owner_id,due_date,next_action,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,1,?,?,?)').bind(id,f.company,orderId,ownerId,'2026-09-22','Call customer to clarify installation requirements before quoting.',status,f.owner.userId,now,now),
  db.prepare("INSERT INTO customer_followup_events(id,followup_id,version,action,actor_id,owner_id,due_date,reason,created_at) VALUES(?,?,1,'create',?,?,?,?,?)").bind(`followup-event-${id}`,id,f.owner.userId,ownerId,'2026-09-22','Initial customer contact',now),
  db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'customer_followup',?,1,?,?)").bind(`followup-notice-${id}`,f.company,ownerId,id,status==='open'?'Customer follow-up assigned: review next action and due date':'Customer follow-up outcome recorded',now),
 ]);
 return id;
}

it('exposes the source-linked contact through Work detail workspace and the owner notification',async()=>{
 const id=await seedFollowup();
 expect((await customerFollowupWork(f.head)).find(x=>x.id===id)).toMatchObject({canAct:true,queue:'todo'});
 const detail=await customerFollowupDetail(f.owner,id);
 expect(detail.history).toHaveLength(1);
 expect(detail.canComplete).toBe(false);
 const own=await customerFollowupDetail(f.head,id);
 expect(own.canComplete).toBe(true);
 expect(await getCustomerFollowup(f.owner,id)).toMatchObject({id,status:'open',owner_id:f.head.userId});
 const {listStaffNotifications}=await import("@erpx/server-collaboration/public/staff-notifications");
 expect((await listStaffNotifications(db,f.head)).some(x=>x.record_id===id)).toBe(true);
 const workspace=await customerFollowupWorkspace(f.owner,orderId);
 expect(workspace.items.map(row=>row.id)).toContain(id);
});

it('hides details, counts, recipients and notifications outside source scope',async()=>{
 const id=await seedFollowup();
 for(const a of [{...f.owner,companyId:'other'},{...f.owner,branchId:'other'},{...f.owner,departmentId:'other'}]){
  await expect(getCustomerFollowup(a,id)).rejects.toMatchObject({status:404});
  expect((await customerFollowupWorkspace(a)).items).toEqual([]);
  expect(await customerFollowupWork(a)).toEqual([]);
 }
 await expect(getCustomerFollowup({...f.owner,role:'employee'},id)).rejects.toMatchObject({status:403});
 expect((await customerFollowupWorkspace(f.owner)).items).toHaveLength(1);
});

it('keeps revoked-owner work waiting and records immutable history',async()=>{
 const id=await seedFollowup();
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 expect((await customerFollowupWork(f.owner)).find(x=>x.id===id)).toMatchObject({canAct:false,queue:'waiting'});
 await expect(db.prepare('DELETE FROM customer_followup_events WHERE followup_id=?').bind(id).run()).rejects.toThrow('CUSTOMER_FOLLOWUP_HISTORY_IMMUTABLE');
});

async function purchasedFollowups(state:'active'|'draining'='active',endsAt:number|null=null){
 const {packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='sales');return packageGrantStore(db,{authorize:async()=>({userId:'isolated-followup-provider'}),obligations:async()=>{throw Error('FOLLOWUP_FIXTURE_CANNOT_INACTIVATE');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'sales',packageVersion:1,state,startsAt:0,endsAt,version:(prior?.version??0)+1,reason:'Isolated original quote Sales follow-up purchase',evidence:'test:followup-purchase'}});
}
it('applies the retained purchase history and new-work flags to reads',async()=>{
 const id=await seedFollowup();
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 expect((await customerFollowupWorkspace(f.owner)).items).toEqual([]);
 await purchasedFollowups();
 const active=await customerFollowupWorkspace(f.owner,orderId);
 expect(active.purchase).toMatchObject({newWork:true,settleExisting:true,history:true});
 expect(active.items.map(row=>row.id)).toContain(id);
 await purchasedFollowups('draining');
 const draining=await customerFollowupWorkspace(f.owner,orderId);
 expect(draining).toMatchObject({canCreate:false,purchase:{newWork:false,settleExisting:true,history:true}});
 expect(draining.items.map(row=>row.id)).toContain(id);
});
