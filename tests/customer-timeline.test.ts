import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {saveMaster,saveOrder,orderSchema} from '../src/server/commerce';
import {createCRMLead,changeCRMLead,getCRMLead} from '../src/server/crm';
import {changeCRMIdentity} from '../src/server/crm-identities';
import {createCustomerNote,changeCustomerNote} from '../src/server/customer-notes';
import {createCustomerFollowup,changeCustomerFollowup} from '../src/server/customer-followups';
import {customerDirectory,customerTimeline,customerTimelineEvents} from '../src/server/customer-timeline';
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,customer:string,order:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Timeline customer'},key())).id);const w=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Timeline warehouse'},key()),p=await saveMaster(f.owner,'products',null,{sku:key(),name:'Timeline product',price:'10'},key());order=String((await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer,warehouseId:w.id,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:p.id,quantity:'1',price:'10',taxBps:0}]}),key())).id);});
it('federates actual CRM, notes and follow-up source events without copying private payloads or mutating their owners',async()=>{
 const lead=String((await createCRMLead(f.owner,{branchId:f.branch,title:'Timeline inquiry',contactName:'Private simulated contact',contactDetail:'PRIVATE_CONTACT',source:'Phone',brief:'PRIVATE_BRIEF',dueDate:'2026-09-23'},key())).id);
 f.head.role='sales_manager';f.head.assignmentId=await f.grant(f.head,'sales_manager');const leadRow=await getCRMLead(f.owner,lead);await changeCRMIdentity(f.head,{action:'link-finance',accountId:leadRow.account_id,customerId:customer,expectedVersion:1,evidence:'Reviewed customer identity in Finance'},key());
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
it('federates scoped Field Service, Finance and Legal state with module and permission gates',async()=>{
 const serviceId=key(),serviceEventId=key(),invoiceId=key(),legalId=key(),legalEventId=key();
 await db.prepare(`INSERT INTO field_service_agreements(id,company_id,branch_id,department_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,created_at,updated_at)
   VALUES(?,?,?,?,?,?,?,?,?,?,'active',?,?,?)`).bind(serviceId,f.company,f.branch,f.department,customer,'Timeline service','2026-09-01','2027-09-01','THB',2500,f.owner.userId,Date.now(),Date.now()).run();
 await db.prepare(`INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES(?,?,?,?,?,?,?,?)`).bind(serviceEventId,serviceId,1,'activate',f.owner.userId,'Service agreement accepted','active',Date.now()).run();
 await db.prepare(`INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,status,version,created_by,created_at,updated_at)
   VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(invoiceId,f.company,customer,'receivable','TIMELINE-INV','2026-09-20','2026-10-20','THB',2500,0,2500,2500,f.accounts.revenue,f.accounts.cash,'standalone','posted',1,f.owner.userId,Date.now(),Date.now()).run();
 await db.prepare(`INSERT INTO legal_items(id,company_id,branch_id,department_id,kind,title,description,classification,country,jurisdiction,language,currency,timezone,priority,due_date,owner_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(legalId,f.company,f.branch,f.department,'contract','Timeline legal contract','Scoped legal source','internal','TH','TH','th','THB','Asia/Bangkok','normal','2026-10-01',f.owner.userId,f.owner.userId,Date.now(),Date.now()).run();
 await db.prepare('INSERT INTO legal_native_references(item_id,target_module,target_id,ordinal,created_by,created_at) VALUES(?,?,?,?,?,?)').bind(legalId,'commerce_order',order,0,f.owner.userId,Date.now()).run();
 await db.prepare('INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)').bind(legalEventId,legalId,'created',f.owner.userId,'Private legal evidence',1,Date.now()).run();
 const serviceViewer={...f.owner,modules:['field_service']};
 const visible=await customerTimeline(serviceViewer,customer);
 expect(visible.items.map(item=>item.source_type)).toContain('field_service');
 expect(visible.items.map(item=>item.source_type)).toContain('finance');
 expect(visible.items.map(item=>item.source_type)).toContain('legal');
 expect(visible.coverage).toContain('Field Service agreements and visits');
 expect(visible.coverage).toContain('Finance invoice status');
 expect(visible.coverage).toContain('Legal records linked to orders');
 expect(visible.items.find(item=>item.source_type==='field_service')).toMatchObject({href:'/field-service',status:'activate'});
 expect(visible.items.find(item=>item.source_type==='finance')).toMatchObject({href:`/finance/invoices/${invoiceId}`,status:'posted'});
 expect(visible.items.find(item=>item.source_type==='legal')).toMatchObject({href:'/legal',status:'created'});
 const commerceOnly=await customerTimeline({...f.owner,role:'finance_preparer',modules:[]},customer);
 expect(commerceOnly.items.some(item=>item.source_type==='field_service')).toBe(false);
 expect(commerceOnly.items.some(item=>item.source_type==='finance')).toBe(true);
 expect(commerceOnly.items.some(item=>item.source_type==='legal')).toBe(false);
 expect(commerceOnly.coverage).not.toContain('Field Service agreements and visits');
});
it('applies requester Legal visibility to the shared Customer 360 event projection',async()=>{
 const requester=await f.member('customer-legal-requester','legal_requester');
 const ownId=key(),otherId=key(),contractId=key();
 for(const [id,kind,creator,classification] of [[ownId,'request',requester.userId,'confidential'],[otherId,'request',f.owner.userId,'internal'],[contractId,'contract',requester.userId,'internal']]){
  await db.prepare("INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'TH','Thailand','th','THB','Asia/Bangkok','normal',?,?,?,?)").bind(id,f.company,kind,`Customer Legal ${id}`,classification,creator,creator,Date.now(),Date.now()).run();
  await db.prepare('INSERT INTO legal_native_references(item_id,target_module,target_id,ordinal,created_by,created_at) VALUES(?,?,?,?,?,?)').bind(id,'commerce_order',order,0,creator,Date.now()).run();
  await db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,'created',?,'Fixture event',1,?)").bind(key(),id,creator,Date.now()).run();
 }
 await db.prepare("INSERT INTO legal_members VALUES(?,?,'read')").bind(ownId,requester.userId).run();
 const projection=customerTimelineEvents(requester);
 expect((await db.prepare(`${projection.sql} WHERE e.source_type='legal'`).bind(...projection.values).all<{source_id:string}>()).results.map(r=>r.source_id)).toEqual([ownId]);
 await db.prepare('DELETE FROM legal_members WHERE item_id=? AND user_id=?').bind(ownId,requester.userId).run();
 expect((await db.prepare(`${projection.sql} WHERE e.source_type='legal'`).bind(...projection.values).all()).results).toEqual([]);
});
