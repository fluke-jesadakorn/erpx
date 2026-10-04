import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {saveMaster,saveOrder,orderSchema} from '../src/server/commerce';
import {createSupportTicket,getSupportTicket,supportDetail} from '../src/server/support';
import {createSupportIncident,changeSupportIncident,getSupportIncident,incidentDetail,incidentWork,incidentWorkspace} from '../src/server/support-incidents';
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,tickets:string[];
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
 f=await expenseFixture();for(const [a,role] of [[f.head,'support_agent'],[f.manager,'support_manager']] as const){a.role=role;a.branchId=null;a.departmentId=null;a.assignmentId=await f.grant(a,role);}
 const warehouse=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Incident warehouse'},key()),product=await saveMaster(f.owner,'products',null,{sku:key(),name:'Incident product',price:'10'},key());tickets=[];
 for(const name of ['Customer one','Customer two']){const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name},key()),order=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:warehouse.id,orderDate:'2026-09-20',dueDate:'2026-09-24',lines:[{productId:product.id,quantity:'1',price:'10',taxBps:0}]}),key());tickets.push(String((await createSupportTicket(f.owner,{orderId:order.id,sourceVersion:1,title:`${name} installation issue`,issue:'Installation fails with the shared error code',ownerId:f.head.userId,dueDate:'2026-09-24'},key())).id));}
});
const input=()=>({ticketId:tickets[0],ticketVersion:1,title:'Shared installation outage',impact:'Two customer installations affected',severity:'high',ownerId:f.head.userId,dueDate:'2026-09-24'});
async function act(id:string,action:string,actor=f.head,extra:Record<string,unknown>={}){const r=await getSupportIncident(actor,id);return changeSupportIncident(actor,id,{action,expectedVersion:Number(r.version),reason:`Simulation evidence ${action}`,...extra},key());}
it('hands off one incident across owners and reviewers while preserving separate customer cases',async()=>{
 const k=key(),body=input(),receipt=await createSupportIncident(f.owner,body,k),id=String(receipt.id);expect(await createSupportIncident(f.owner,body,k)).toEqual(receipt);
 const originals=await Promise.all(tickets.map(t=>getSupportTicket(f.owner,t)));
 const {listStaffNotifications}=await import('../src/server/staff-notifications'),{calendarData}=await import('../src/server/calendar');
 expect((await incidentWork(f.head)).find(x=>x.id===id)).toMatchObject({canAct:true,queue:'todo'});
 expect((await listStaffNotifications(db,f.head)).some(x=>x.record_id===id)).toBe(true);
 expect((await calendarData(db,f.head,{date:'2026-09-24',view:'day',layers:'support',mode:'mine'})).items.some(x=>x.sourceId===id)).toBe(true);
 await act(id,'link',f.manager,{ticketId:tickets[1],ticketVersion:1});expect((await incidentDetail(f.manager,id)).links).toHaveLength(2);
 await act(id,'unlink',f.manager,{ticketId:tickets[1]});expect((await incidentDetail(f.manager,id)).links).toHaveLength(1);
 await expect(act(id,'unlink',f.manager,{ticketId:tickets[0]})).rejects.toMatchObject({code:'CONFLICT'});
 await act(id,'link',f.manager,{ticketId:tickets[1],ticketVersion:1});
 await act(id,'start');await act(id,'update');await act(id,'mitigate',f.head,{rootCause:'Shared installer config rejected a valid product code',mitigation:'Corrected installer config verified in the isolated environment'});
 expect((await incidentWork(f.manager)).find(x=>x.id===id)?.canAct).toBe(true);
 await act(id,'reject',f.manager);await act(id,'mitigate',f.head,{rootCause:'Confirmed config defect',mitigation:'Both simulated installation variants now pass'});await act(id,'close',f.manager);
 expect(await getSupportIncident(f.manager,id)).toMatchObject({status:'closed'});
 expect(await Promise.all(tickets.map(t=>getSupportTicket(f.owner,t)))).toEqual(originals);
 expect((await supportDetail(f.head,tickets[1])).incidents).toMatchObject([{id,status:'closed'}]);
 expect((await calendarData(db,f.head,{date:'2026-09-24',view:'day',layers:'support',mode:'mine'})).items.some(x=>x.sourceId===id)).toBe(false);
 await act(id,'reopen',f.manager);expect((await incidentWork(f.head)).find(x=>x.id===id)?.canAct).toBe(true);
 await expect(db.prepare('DELETE FROM support_incident_events WHERE incident_id=?').bind(id).run()).rejects.toThrow('SUPPORT_EVIDENCE_IMMUTABLE');
});
it('does not let the mitigating manager self-review through reassignment and requires current reviewer eligibility',async()=>{
 const id=String((await createSupportIncident(f.owner,{...input(),ownerId:f.manager.userId},key())).id);
 await act(id,'start',f.manager);await act(id,'mitigate',f.manager,{rootCause:'Simulation cause',mitigation:'Simulation mitigation evidence'});
 await act(id,'reassign',f.owner,{ownerId:f.head.userId,dueDate:'2026-09-25'});
 await expect(act(id,'close',f.manager)).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});
 expect((await incidentWork(f.manager)).find(x=>x.id===id)?.canAct).toBe(false);
 await db.prepare("UPDATE hr_people SET status='inactive' WHERE company_id=? AND user_id=?").bind(f.company,f.owner.userId).run();
 await expect(act(id,'close',f.owner)).rejects.toMatchObject({code:'CONFLICT'});
 expect((await incidentWork(f.head)).find(x=>x.id===id)?.blocker).toContain('coverage');
});
it('protects source scope, versions, duplicate links and recipient withdrawal',async()=>{
 const id=String((await createSupportIncident(f.owner,input(),key())).id);
 for(const a of [{...f.head,companyId:'other'},{...f.head,branchId:'other'},{...f.head,departmentId:'other'}]){expect((await incidentWorkspace(a)).items).toEqual([]);await expect(incidentDetail(a,id)).rejects.toMatchObject({status:404});}
 await expect(createSupportIncident(f.head,input(),key())).rejects.toMatchObject({status:403});
 await expect(act(id,'link',f.manager,{ticketId:tickets[0],ticketVersion:1})).rejects.toMatchObject({code:'CONFLICT'});
 const results=await Promise.allSettled([changeSupportIncident(f.head,id,{action:'start',expectedVersion:1,reason:'Concurrent investigation'},key()),changeSupportIncident(f.manager,id,{action:'cancel',expectedVersion:1,reason:'Concurrent cancellation'},key())]);expect(results.filter(r=>r.status==='fulfilled')).toHaveLength(1);
 const another=String((await createSupportIncident(f.owner,input(),key())).id);
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 expect((await incidentWork(f.manager)).find(x=>x.id===another)?.blocker).toContain('coverage');
 await expect(act(another,'start')).rejects.toMatchObject({code:'SUPPORT_INCIDENT_NOT_FOUND',status:404});
 expect(await db.prepare('SELECT version,status FROM support_incidents WHERE id=?').bind(another).first()).toEqual({version:1,status:'open'});
 await act(another,'reassign',f.manager,{ownerId:f.manager.userId,dueDate:'2026-09-25'});expect((await incidentWork(f.manager)).find(x=>x.id===another)?.canAct).toBe(true);
});
