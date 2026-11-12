import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {saveMaster,saveOrder,orderSchema} from '@erpx/server-commerce/public/commerce';
import {seedSupportTicket} from '../../support/helpers/support-ticket-seed';
import {seedSupportSession} from '../../support/helpers/support-session-seed';
import {supportSessionContext,supportSessionDetail,supportSessionWork} from "@erpx/server-support/public/support-sessions";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,ticketId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{vi.restoreAllMocks();f=await expenseFixture();for(const [a,role] of [[f.head,'support_agent'],[f.manager,'support_manager']] as const){a.role=role;a.branchId=null;a.departmentId=null;a.assignmentId=await f.grant(a,role);}const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Session customer'},key()),warehouse=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Session warehouse'},key()),product=await saveMaster(f.owner,'products',null,{sku:key(),name:'Session item',price:'42'},key()),order=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:warehouse.id,orderDate:'2026-09-20',dueDate:'2026-09-24',lines:[{productId:product.id,quantity:'1',price:'42',taxBps:0}]}),key());ticketId=String((await seedSupportTicket(db,f.owner,{orderId:String(order.id),title:'Customer setup context',issue:'Customer asks Support to review order setup.',ownerId:f.head.userId,dueDate:'2026-09-24'})).id);});
function seed(input:Parameters<typeof seedSupportSession>[2]){return seedSupportSession(db,f.manager,{ticketId,requestedFor:f.head.userId,createdBy:f.head.userId,reviewedBy:f.manager.userId,...input});}

it('reads pending and approved sessions and requires an explicit active capability',async()=>{
 const pending=await seed({status:'pending'});
 expect((await supportSessionWork(f.manager)).find(x=>x.id===pending.id)).toMatchObject({queue:'todo',canAct:true});
 const approved=await seed({status:'approved',capabilities:[{code:'customer_order_context'}]});
 expect(await supportSessionContext(f.head,approved.id)).toMatchObject({ticket_id:ticketId,order_kind:'quote',currency_code:'THB',evidence:[]});
 const detail=await supportSessionDetail(f.head,approved.id);
 expect(detail.context).toBeTruthy();expect(JSON.stringify(detail.context)).not.toContain('Session customer');
 expect(detail.history.map(x=>x.action)).toEqual(['request','approve']);
 expect(detail.capabilityHistory.map(x=>x.action)).toEqual(['issue']);
 const without=await seed({status:'approved'});
 await expect(supportSessionContext(f.head,without.id)).rejects.toMatchObject({code:'SUPPORT_CAPABILITY_REQUIRED'});
 expect((await supportSessionDetail(f.head,without.id)).context).toBeNull();
});

it('reads a whole-session revoke that removes all capability access while retaining immutable history',async()=>{
 const {id}=await seed({status:'revoked',version:6,capabilities:[{code:'customer_order_context',status:'revoked',version:4}]});
 await expect(supportSessionContext(f.head,id)).rejects.toMatchObject({status:403});
 const detail=await supportSessionDetail(f.manager,id);
 expect(detail.row).toMatchObject({status:'revoked',version:6});
 expect(detail.capabilities[0]).toMatchObject({status:'revoked',version:4});
 expect(detail.capabilityHistory.map(x=>x.action)).toEqual(['issue','revoke','reissue','session-revoke']);
});

it('projects expiry and an inactive requested target without exposing context',async()=>{
 const expired=await seed({status:'approved',expiresAt:Date.now()-1,createdAt:Date.now()-60_000,capabilities:[{code:'customer_order_context'}]});
 await expect(supportSessionContext(f.head,expired.id)).rejects.toMatchObject({status:403});
 expect((await supportSessionDetail(f.head,expired.id)).row.effective_status).toBe('expired');
 const next=await seed({status:'approved',capabilities:[{code:'customer_order_context'}]});
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 await expect(supportSessionContext(f.head,next.id)).rejects.toMatchObject({status:403});
 expect((await supportSessionWork(f.manager)).find(x=>x.id===next.id)?.blocker).toContain('no longer eligible');
});

it('keeps source and history immutable and rejects cross-scope reads',async()=>{
 const {id}=await seed({status:'pending'});
 for(const actor of [{...f.head,companyId:'other'},{...f.head,branchId:'other'},{...f.head,departmentId:'other'}])await expect(supportSessionDetail(actor,id)).rejects.toMatchObject({status:404});
 await expect(db.prepare('UPDATE support_sessions SET ticket_id=? WHERE id=?').bind('other',id).run()).rejects.toThrow('SUPPORT_SESSION_SOURCE_IMMUTABLE');
 await expect(db.prepare('DELETE FROM support_session_events WHERE session_id=?').bind(id).run()).rejects.toThrow('SUPPORT_SESSION_HISTORY_IMMUTABLE');
});
