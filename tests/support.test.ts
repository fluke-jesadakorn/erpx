import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {saveMaster,saveOrder,orderSchema} from '../src/server/commerce';
import {createCustomerNote} from '../src/server/customer-notes';
import {createSupportTicket,changeSupportTicket,getSupportTicket,supportWork,supportWorkspace,supportDetail} from '../src/server/support';
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,order:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();for(const [a,role] of [[f.head,'support_agent'],[f.manager,'support_manager']] as const){a.role=role;a.branchId=null;a.departmentId=null;a.assignmentId=await f.grant(a,role);}const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Support customer'},key()),w=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Support warehouse'},key()),p=await saveMaster(f.owner,'products',null,{sku:key(),name:'Support item',price:'10'},key());order=String((await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:w.id,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:p.id,quantity:'1',price:'10',taxBps:0}]}),key())).id);});
const input=()=>({orderId:order,sourceVersion:1,title:'Installation assistance needed',issue:'Customer needs help confirming the setup procedure.',ownerId:f.head.userId,dueDate:'2026-09-24'});
async function ticket(){return String((await createSupportTicket(f.owner,input(),key())).id);}
async function act(id:string,action:string,actor=f.head){const r=await getSupportTicket(actor,id);return changeSupportTicket(actor,id,{action,expectedVersion:Number(r.version),reason:`Simulated evidence: ${action}`},key());}
it('hands off to Support and separates proposed resolution from independently reviewed customer confirmation',async()=>{
 const k=key(),body=input(),created=await createSupportTicket(f.owner,body,k),id=String(created.id);expect(await createSupportTicket(f.owner,body,k)).toEqual(created);
 expect((await supportWork(f.head)).find(x=>x.id===id)).toMatchObject({canAct:true,queue:'todo'});
 const {listStaffNotifications}=await import('../src/server/staff-notifications'),{calendarData}=await import('../src/server/calendar');
 expect((await listStaffNotifications(db,f.head)).some(x=>x.record_id===id)).toBe(true);
 expect((await calendarData(db,f.head,{date:body.dueDate,view:'day',layers:'support',mode:'mine'})).items.some(x=>x.sourceId===id)).toBe(true);
 const oldNote=await createCustomerNote(f.owner,{orderId:order,expectedVersion:1,channel:'phone',occurredAt:Date.now(),body:'Old customer contact before any resolution'},key());
 await act(id,'start');await act(id,'wait-customer');await expect(act(id,'resolve')).rejects.toMatchObject({code:'SUPPORT_STATE_CHANGED'});await act(id,'resume');await act(id,'resolve');
 expect((await listStaffNotifications(db,f.manager)).some(x=>x.record_id===id)).toBe(true);
 expect((await supportWork(f.manager)).find(x=>x.id===id)).toMatchObject({canAct:true,queue:'todo'});
 await expect(changeSupportTicket(f.manager,id,{action:'confirm',expectedVersion:5,reason:'Cannot use an old contact as customer acceptance',noteId:oldNote.id,noteVersion:1},key())).rejects.toMatchObject({code:'CONFLICT'});
 const note=await createCustomerNote(f.owner,{orderId:order,expectedVersion:1,channel:'phone',occurredAt:Date.now(),body:'Simulated customer confirmed that the instructions solved the issue'},key());
 await changeSupportTicket(f.manager,id,{action:'confirm',expectedVersion:5,reason:'Manager reviewed the recorded customer confirmation',noteId:note.id,noteVersion:1},key());
 expect(await getSupportTicket(f.head,id)).toMatchObject({status:'closed',confirmation_note_id:note.id,version:6});
 expect((await calendarData(db,f.head,{date:body.dueDate,view:'day',layers:'support',mode:'mine'})).items.some(x=>x.sourceId===id)).toBe(false);
 await act(id,'reopen',f.manager);expect(await getSupportTicket(f.head,id)).toMatchObject({status:'in_progress',resolution:null});
 expect((await supportDetail(f.manager,id)).history).toHaveLength(7);
 expect(await db.prepare('SELECT version FROM commerce_orders WHERE id=?').bind(order).first('version')).toBe(1);
});
it('blocks inactive recipients and protects scope, event history and stale concurrent actions',async()=>{
 const id=await ticket();for(const a of [{...f.head,companyId:'other'},{...f.head,branchId:'other'},{...f.head,departmentId:'other'}]){expect((await supportWorkspace(a)).items).toEqual([]);await expect(getSupportTicket(a,id)).rejects.toMatchObject({status:404});}
 await expect(getSupportTicket({...f.head,role:'employee'},id)).rejects.toMatchObject({status:403});
 const results=await Promise.allSettled([changeSupportTicket(f.head,id,{action:'start',expectedVersion:1,reason:'Start customer assistance'},key()),changeSupportTicket(f.manager,id,{action:'cancel',expectedVersion:1,reason:'Customer withdrew the request'},key())]);expect(results.filter(x=>x.status==='fulfilled')).toHaveLength(1);
 const another=await ticket();await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 await expect(act(another,'start')).rejects.toMatchObject({code:'CONFLICT'});
 expect((await supportWork(f.manager)).find(x=>x.id===another)?.blocker).toContain('no longer eligible');
 await changeSupportTicket(f.manager,another,{action:'reassign',expectedVersion:1,ownerId:f.manager.userId,dueDate:'2026-09-25',reason:'Manager takes over after access withdrawal'},key());
 expect((await supportWork(f.manager)).find(x=>x.id===another)?.canAct).toBe(true);
 await expect(db.prepare('DELETE FROM support_ticket_events WHERE ticket_id=?').bind(another).run()).rejects.toThrow('SUPPORT_EVIDENCE_IMMUTABLE');
});
it('does not allow the resolving manager to approve their own resolution after reassignment',async()=>{
 const id=String((await createSupportTicket(f.owner,{...input(),ownerId:f.manager.userId},key())).id);
 await act(id,'start',f.manager);await act(id,'resolve',f.manager);
 await changeSupportTicket(f.manager,id,{action:'reassign',expectedVersion:3,ownerId:f.head.userId,dueDate:'2026-09-25',reason:'Support agent takes next customer contact'},key());
 const note=await createCustomerNote(f.owner,{orderId:order,expectedVersion:1,channel:'phone',occurredAt:Date.now(),body:'Simulated customer confirmation'},key());
 await expect(changeSupportTicket(f.manager,id,{action:'confirm',expectedVersion:4,reason:'Cannot approve own resolution after changing the owner',noteId:note.id,noteVersion:1},key())).rejects.toMatchObject({code:'INDEPENDENT_CONFIRMATION_REQUIRED'});
 expect((await supportWork(f.manager)).find(x=>x.id===id)?.canAct).toBe(false);
});
