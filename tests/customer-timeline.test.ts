import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {saveMaster,saveOrder,orderSchema} from '../src/server/commerce';
import {createCRMLead,changeCRMLead} from '../src/server/crm';
import {createCustomerNote,changeCustomerNote} from '../src/server/customer-notes';
import {createCustomerFollowup,changeCustomerFollowup} from '../src/server/customer-followups';
import {customerDirectory,customerTimeline} from '../src/server/customer-timeline';
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,customer:string,order:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Timeline customer'},key())).id);const w=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Timeline warehouse'},key()),p=await saveMaster(f.owner,'products',null,{sku:key(),name:'Timeline product',price:'10'},key());order=String((await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer,warehouseId:w.id,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:p.id,quantity:'1',price:'10',taxBps:0}]}),key())).id);});
it('federates actual CRM, notes and follow-up source events without copying private payloads or mutating their owners',async()=>{
 const lead=String((await createCRMLead(f.owner,{branchId:f.branch,title:'Timeline inquiry',contactName:'Private simulated contact',contactDetail:'PRIVATE_CONTACT',source:'Phone',brief:'PRIVATE_BRIEF',dueDate:'2026-09-23'},key())).id);
 await changeCRMLead(f.owner,lead,{action:'qualify',expectedVersion:1,customerId:customer,expectedValue:'10',closeDate:'2026-09-25',reason:'PRIVATE_CRM_REASON'},key());
 const note=await createCustomerNote(f.owner,{orderId:order,expectedVersion:1,channel:'phone',occurredAt:Date.now(),body:'PRIVATE_NOTE_CONTENT'},key());
 await changeCustomerNote(f.owner,String(note.id),{action:'correct',expectedVersion:1,body:'CORRECTED_PRIVATE_NOTE',reason:'PRIVATE_CORRECTION'},key());
 await changeCustomerNote(f.owner,String(note.id),{action:'archive',expectedVersion:2,reason:'PRIVATE_ARCHIVE_REASON'},key());
 const follow=await createCustomerFollowup(f.owner,{orderId:order,expectedVersion:1,ownerId:f.owner.userId,dueDate:'2026-09-23',nextAction:'PRIVATE_FOLLOWUP'},key());
 await changeCustomerFollowup(f.owner,String(follow.id),{action:'complete',expectedVersion:1,reason:'PRIVATE_OUTCOME'},key());
 const data=await customerTimeline(f.owner,customer);
 expect(data.items).toHaveLength(8);
 expect(new Set(data.items.map(x=>x.source_type))).toEqual(new Set(['order','contact','followup','crm']));
 expect(data.items.filter(x=>x.source_type==='contact').map(x=>x.status)).toEqual(['archive','correct','create']);
 expect(JSON.stringify(data)).not.toContain('PRIVATE_');
 expect(Object.keys(data.items[0]).sort()).toEqual(['at','href','id','source_id','source_type','status','title','version']);
 expect(await db.prepare('SELECT version FROM commerce_orders WHERE id=?').bind(order).first('version')).toBe(1);
 expect(await db.prepare('SELECT version FROM crm_leads WHERE id=?').bind(lead).first('version')).toBe(2);
});
it('hides customer identity, entries and counts outside company/branch/department scope and from non-commerce roles',async()=>{
 const unlinked=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Unlinked private customer'},key());
 expect((await customerDirectory(f.owner)).map(x=>x.id)).toEqual([customer]);
 await expect(customerTimeline(f.owner,String(unlinked.id))).rejects.toMatchObject({status:404});
 for(const a of [{...f.owner,companyId:'other'},{...f.owner,branchId:'other'},{...f.owner,departmentId:'other'}]){expect(await customerDirectory(a)).toEqual([]);await expect(customerTimeline(a,customer)).rejects.toMatchObject({status:404});}
 await expect(customerTimeline({...f.owner,role:'employee'},customer)).rejects.toMatchObject({status:403});
});
it('paginates source events deterministically without duplicates and rejects malformed cursors',async()=>{
 for(let i=0;i<4;i++)await createCustomerNote(f.owner,{orderId:order,expectedVersion:1,channel:'phone',occurredAt:Date.now(),body:`Contact ${i}`},key());
 const all=await customerTimeline(f.owner,customer),seen:string[]=[];let cursor:string|undefined;
 do{const page=await customerTimeline(f.owner,customer,{limit:2,cursor});seen.push(...page.items.map(x=>x.id));cursor=page.nextCursor??undefined;}while(cursor);
 expect(seen).toEqual(all.items.map(x=>x.id));expect(new Set(seen).size).toBe(5);
 await expect(customerTimeline(f.owner,customer,{cursor:'bad-cursor'})).rejects.toMatchObject({code:'TIMELINE_CURSOR_INVALID'});
});
it('includes Support lifecycle only when the viewer also has the Support owner permission',async()=>{
 const {createSupportTicket,changeSupportTicket}=await import('../src/server/support');
 const ticket=await createSupportTicket(f.owner,{orderId:order,sourceVersion:1,title:'Help installing the item',issue:'PRIVATE_SUPPORT_ISSUE',ownerId:f.owner.userId,dueDate:'2026-09-24'},key());
 await changeSupportTicket(f.owner,String(ticket.id),{action:'start',expectedVersion:1,reason:'PRIVATE_SUPPORT_REASON'},key());
 const visible=await customerTimeline(f.owner,customer);
 expect(visible.items.filter(e=>e.source_type==='support')).toHaveLength(2);
 expect(JSON.stringify(visible)).not.toContain('PRIVATE_SUPPORT');
 const buyer=await customerTimeline({...f.owner,role:'purchasing_operator'},customer);
 expect(buyer.items.some(e=>e.source_type==='support')).toBe(false);
 expect(buyer.coverage).not.toContain('Support case history');
});
