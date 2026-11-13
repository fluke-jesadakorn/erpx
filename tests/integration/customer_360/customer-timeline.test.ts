import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {saveMaster,saveOrder,orderSchema} from '@erpx/server-commerce/public/commerce';
import {createCRMLead,getCRMLead} from "@erpx/server-crm/public/crm";
import {changeCRMIdentity} from "@erpx/server-crm/public/crm-identities";
import {seedCustomerNote,seedCustomerNoteChange} from "../../support/helpers/customer-note-seed";
import {seedCrmQualifiedLead} from "../../support/helpers/crm-qualify-seed";
import {customerDirectory,customerTimeline,customerTimelineEvents} from "@erpx/server-customer_360/public/customer-timeline";
import {withNativeFieldFixture} from '../../support/files/field-files-native-parity';
import {seedExpenseCompany} from '../../../fixtures/expenses/company';
import type {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
let activeDatabase:PostgresDatabase|undefined;
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:activeDatabase??env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,customer:string,order:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Timeline customer'},key())).id);const w=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Timeline warehouse'},key()),p=await saveMaster(f.owner,'products',null,{sku:key(),name:'Timeline product',price:'10'},key());order=String((await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer,warehouseId:w.id,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:p.id,quantity:'1',price:'10',taxBps:0}]}),key())).id);});
it('federates actual CRM, notes and follow-up source events without copying private payloads or mutating their owners',async()=>{
 const lead=String((await createCRMLead(f.owner,{branchId:f.branch,title:'Timeline inquiry',contactName:'Private simulated contact',contactDetail:'PRIVATE_CONTACT',source:'Phone',brief:'PRIVATE_BRIEF',dueDate:'2026-09-23'},key())).id);
 f.head.role='sales_manager';f.head.assignmentId=await f.grant(f.head,'sales_manager');const leadRow=await getCRMLead(f.owner,lead);await changeCRMIdentity(f.head,{action:'link-finance',accountId:leadRow.account_id,customerId:customer,expectedVersion:1,evidence:'Reviewed customer identity in Finance'},key());
 await seedCrmQualifiedLead(db,{companyId:f.company,ownerId:f.owner.userId,leadId:lead,customerId:customer,expectedValue:'10',closeDate:'2026-09-25',reason:'PRIVATE_CRM_REASON'});
 // Note commands are native-owned; seed the exact immutable owner rows they persist.
 const note=await seedCustomerNote(db,{companyId:f.company,customerId:customer,orderId:order,channel:'phone',occurredAt:Date.now(),body:'PRIVATE_NOTE_CONTENT',createdBy:f.owner.userId});
 await seedCustomerNoteChange(db,{noteId:note.id,action:'correct',expectedVersion:1,body:'CORRECTED_PRIVATE_NOTE',reason:'PRIVATE_CORRECTION',actorId:f.owner.userId});
 await seedCustomerNoteChange(db,{noteId:note.id,action:'archive',expectedVersion:2,reason:'PRIVATE_ARCHIVE_REASON',actorId:f.owner.userId});
 // Follow-up commands are native-only; the timeline oracle seeds the exact
 // owner rows (follow-up, immutable events and notification) those commands persist.
 const follow={id:`followup-${key()}`},followNow=Date.now();
 await db.batch([
  db.prepare('INSERT INTO customer_followups(id,company_id,order_id,owner_id,due_date,next_action,status,outcome,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,2,?,?,?)').bind(follow.id,f.company,order,f.owner.userId,'2026-09-23','PRIVATE_FOLLOWUP','completed','PRIVATE_OUTCOME',f.owner.userId,followNow,followNow),
  db.prepare("INSERT INTO customer_followup_events(id,followup_id,version,action,actor_id,owner_id,due_date,reason,created_at) VALUES(?,?,1,'create',?,?,?,?,?)").bind(`followup-event-${follow.id}-1`,follow.id,f.owner.userId,f.owner.userId,'2026-09-23','PRIVATE_FOLLOWUP',followNow),
  db.prepare("INSERT INTO customer_followup_events(id,followup_id,version,action,actor_id,owner_id,due_date,reason,created_at) VALUES(?,?,2,'complete',?,?,?,?,?)").bind(`followup-event-${follow.id}-2`,follow.id,f.owner.userId,f.owner.userId,'2026-09-23','PRIVATE_OUTCOME',followNow+1),
  db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'customer_followup',?,2,'Customer follow-up outcome recorded',?)").bind(`followup-notice-${follow.id}`,f.company,f.owner.userId,follow.id,followNow+1),
 ]);
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
 for(let i=0;i<4;i++)await seedCustomerNote(db,{companyId:f.company,customerId:customer,orderId:order,channel:'phone',occurredAt:Date.now(),body:`Contact ${i}`,createdBy:f.owner.userId});
 const all=await customerTimeline(f.owner,customer),seen:string[]=[];let cursor:string|undefined;
 do{const page=await customerTimeline(f.owner,customer,{limit:2,cursor});seen.push(...page.items.map(x=>x.id));cursor=page.nextCursor??undefined;}while(cursor);
 expect(seen).toEqual(all.items.map(x=>x.id));expect(new Set(seen).size).toBe(5);
 await expect(customerTimeline(f.owner,customer,{cursor:'bad-cursor'})).rejects.toMatchObject({code:'TIMELINE_CURSOR_INVALID'});
});
it('includes Support lifecycle only when the viewer also has the Support owner permission',async()=>{
 const {seedSupportTicket}=await import("../../support/helpers/support-ticket-seed");
 const ticket=await seedSupportTicket(db,f.owner,{orderId: order,title:'Help installing the item',issue:'PRIVATE_SUPPORT_ISSUE',ownerId:f.owner.userId,dueDate:'2026-09-24',status:'in_progress',version:2,events:[{action:'report',reason:'PRIVATE_SUPPORT_REASON',status:'triage',ownerId:null,dueDate:null},{action:'start',reason:'PRIVATE_SUPPORT_REASON'}]});
 const visible=await customerTimeline(f.owner,customer);
 expect(visible.items.filter(e=>e.source_type==='support')).toHaveLength(2);
 expect(JSON.stringify(visible)).not.toContain('PRIVATE_SUPPORT');
 const buyer=await customerTimeline({...f.owner,role:'purchasing_operator'},customer);
 expect(buyer.items.some(e=>e.source_type==='support')).toBe(false);
 expect(buyer.coverage).not.toContain('Support case history');
});
it('federates scoped Field Service, Finance and Legal state with module and permission gates',async()=>{
 let prepared:{db:PostgresDatabase;f:Awaited<ReturnType<typeof seedExpenseCompany>>;customer:string;order:string}|undefined;
 try{await withNativeFieldFixture(async native=>{
 if(!prepared)throw new Error('NATIVE_TIMELINE_FIXTURE_REQUIRED');
 const {db,f,customer,order}=prepared;
 const invoiceId=key(),legalId=key(),legalEventId=key();
 const created=await native.requestAs<{id:string}>('owner','POST','/api/v1/holdings/acme/field-service',{entity:'agreement',branchId:f.branch,departmentId:f.department,customerId:customer,title:'Timeline service',startDate:'2026-09-01',endDate:'2027-09-01',currency:'THB',visitRateMinor:2500,warrantyDiscountBps:0},key());
 expect(created.status,JSON.stringify(created.json)).toBe(200);
 await native.advanceClock();
 const reviewed=await native.requestAs('reviewer','POST','/api/v1/holdings/acme/field-service',{entity:'agreement',id:created.json.data.id,action:'activate',expectedVersion:1,reason:'Independent service manager accepted the timeline fixture agreement'},key());
 expect(reviewed.status,JSON.stringify(reviewed.json)).toBe(200);
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
 },async db=>{
 activeDatabase=db;
 const f=await seedExpenseCompany(db,env.H001_FILES);
 const customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Timeline customer'},key())).id);
 const w=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Timeline warehouse'},key()),p=await saveMaster(f.owner,'products',null,{sku:key(),name:'Timeline product',price:'10'},key());
 const order=String((await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer,warehouseId:w.id,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:p.id,quantity:'1',price:'10',taxBps:0}]}),key())).id);
 // Publish explicit fixture permissions; production IAM and independent-person
 // rules are still evaluated by native commands after normal Account/SSO login.
 await db.exec(`INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_field_parts',2,'["records.read","records.write","records.review","module.field_service.read"]','[]','["holding","branch","department"]','synthetic-timeline',0);UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_field_parts'`);
 const actors=[];for(const [label,actor]of [['owner',f.owner],['reviewer',f.tie]]as const){const assignmentId=await f.grant(actor,'custom_field_parts');await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(['field_service']),assignmentId).run();const row=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(assignmentId).first<number>('version');if(row===null)throw new Error('NATIVE_TIMELINE_ASSIGNMENT_REQUIRED');actors.push({label,userId:actor.userId,assignmentId,assignmentVersion:Number(row),role:'custom_field_parts',modules:['field_service'],branchId:null,departmentId:null});}
 prepared={db,f,customer,order};return {companyId:f.company,actors};
 },'agreement');}finally{activeDatabase=undefined;}
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
 const projection=await customerTimelineEvents(requester,db);
 expect((await db.prepare(`${projection.sql} WHERE e.source_type='legal'`).bind(...projection.values).all<{source_id:string}>()).results.map(r=>r.source_id)).toEqual([ownId]);
 await db.prepare('DELETE FROM legal_members WHERE item_id=? AND user_id=?').bind(ownId,requester.userId).run();
 expect((await db.prepare(`${projection.sql} WHERE e.source_type='legal'`).bind(...projection.values).all()).results).toEqual([]);
});
