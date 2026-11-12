import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {saveMaster,saveOrder,orderSchema} from '@erpx/server-commerce/public/commerce';
import {seedCustomerNote} from '../../support/helpers/customer-note-seed';
import {getSupportTicket,supportWork,supportWorkspace,supportDetail} from '@erpx/server-support/public/support';
import {seedSupportTicket} from "../../support/helpers/support-ticket-seed";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,order:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();for(const [a,role] of [[f.head,'support_agent'],[f.manager,'support_manager']] as const){a.role=role;a.branchId=null;a.departmentId=null;a.assignmentId=await f.grant(a,role);}const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Support customer'},key()),w=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Support warehouse'},key()),p=await saveMaster(f.owner,'products',null,{sku:key(),name:'Support item',price:'10'},key());order=String((await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:w.id,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:p.id,quantity:'1',price:'10',taxBps:0}]}),key())).id);});
async function orderTicket(overrides:Record<string,unknown>={}){return seedSupportTicket(db,f.owner,{orderId:order,title:'Installation assistance needed',issue:'Customer needs help confirming the setup procedure.',ownerId:f.head.userId,dueDate:'2026-09-24',...overrides});}

it('hands off to Support and keeps proposed resolution, independent confirmation and immutable history readable after a reload',async()=>{
 const created=await orderTicket(),id=String(created.id);
 expect((await supportWork(f.head)).find(x=>x.id===id)).toMatchObject({canAct:true,queue:'todo'});
 const {calendarData}=await import("@erpx/server-work_projection/public/calendar/index");
 expect((await calendarData(db,f.head,{date:'2026-09-24',view:'day',layers:'support',mode:'mine'})).items.some(x=>x.sourceId===id)).toBe(true);
 const noteParty=await db.prepare('SELECT party_id FROM commerce_orders WHERE id=?').bind(order).first<string>('party_id');if(!noteParty)throw Error('SUPPORT_NOTE_SOURCE_REQUIRED');
 const note=await seedCustomerNote(db,{companyId:f.company,customerId:noteParty,orderId:order,channel:'phone',occurredAt:Date.now(),body:'Customer confirmed the corrected installation',createdBy:f.owner.userId});
 const closed=await orderTicket({status:'closed',version:7,resolution:'Resolved by the assigned agent',resolvedBy:f.head.userId,confirmedBy:f.manager.userId,confirmationReason:'Independently recorded customer acceptance',confirmationNoteId:note.id,confirmationNoteVersion:1,events:[
  {action:'report',status:'triage',ownerId:null,dueDate:null},
  {action:'triage'},{action:'start'},{action:'wait-customer',status:'waiting_customer'},{action:'resume'},{action:'resolve',status:'resolved'},{action:'confirm',status:'closed'},
 ]}),closedId=String(closed.id);
 const detail=await supportDetail(f.manager,closedId);
 expect(detail.history.map(x=>x.action)).toEqual(['confirm','resolve','resume','wait-customer','start','triage','report']);
 expect(detail.row).toMatchObject({status:'closed',version:7,confirmation_reason:'Independently recorded customer acceptance'});
 expect((await supportWork(f.manager)).find(x=>x.id===closedId)).toMatchObject({queue:'completed'});
 expect(await db.prepare('SELECT version FROM commerce_orders WHERE id=?').bind(order).first('version')).toBe(1);
});

it('keeps general reports scoped to their own reporter and preserves an employee withdrawal in readable history',async()=>{
 const own=await seedSupportTicket(db,f.claimant,{title:'Own workstation issue',issue:'My workstation keyboard stops responding after wake.'}),id=String(own.id);
 expect(await supportWorkspace(f.claimant)).toMatchObject({ownReportsOnly:true,canCreate:true,canCreateOrder:false,sources:[]});
 expect((await supportWorkspace(f.claimant)).items.map(row=>row.id)).toEqual([id]);
 expect((await supportWorkspace(f.tie)).items).toEqual([]);
 await expect(supportDetail(f.tie,id)).rejects.toMatchObject({status:403});
 expect(await supportDetail(f.claimant,id)).toMatchObject({canReport:true,canWork:false,canManage:false,incidents:[],notes:[],recipients:[]});
 const withdrawn=await seedSupportTicket(db,f.claimant,{title:'Replaced keyboard',issue:'The original keyboard was fixed before triage.',status:'cancelled',version:4,events:[
  {action:'report',status:'triage',ownerId:null,dueDate:null},
  {action:'edit-report'},{action:'add-information'},{action:'withdraw-report',status:'cancelled'},
 ]});
 expect(await getSupportTicket(f.claimant,String(withdrawn.id))).toMatchObject({status:'cancelled',version:4});
 expect((await supportDetail(f.claimant,String(withdrawn.id))).history.map(event=>event.action)).toEqual(['withdraw-report','add-information','edit-report','report']);
 expect((await supportWork(f.claimant)).find(row=>row.id===String(withdrawn.id))).toMatchObject({queue:'completed',relationships:['requested']});
});

it('pins a real asset source, follows its current authority and hides unreadable ticket bodies and links',async()=>{
 for(const actor of [f.owner,f.claimant,f.head,f.manager])actor.modules=['assets','support'];
 const at=Date.now(),assetId=`asset-${key()}`;
 await db.batch([
  db.prepare("INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(assetId,f.company,f.branch,f.department,key(),'Shared lobby display','Display','THB',0,'2026-09-20',null,f.owner.userId,at,at),
  db.prepare("INSERT INTO operational_asset_events VALUES(?,?,1,'register',?,'Asset registered','good','registered',NULL,NULL,?)").bind(`asset-event-${key()}`,assetId,f.owner.userId,at),
 ]);
 const sourceRecord={kind:'asset',id:assetId,version:1,companyId:f.company,href:`/assets/${assetId}`};
 const id=String((await seedSupportTicket(db,f.claimant,{sourceKind:'asset',sourceRecord,title:'Asset display fault',issue:'The source-linked lobby display has lost its schedule.'})).id),row=await getSupportTicket(f.claimant,id);
 expect(row).toMatchObject({source_kind:'asset',order_id:null,customer_id:null,status:'triage'});
 expect(JSON.parse(String(row.source_record_json))).toMatchObject(sourceRecord);
 await db.prepare('UPDATE operational_assets SET condition=?,version=version+1,updated_at=? WHERE id=?').bind('degraded',Date.now(),assetId).run();
 expect(JSON.parse(String((await getSupportTicket(f.claimant,id)).source_record_json)).version).toBe(1);
 const assigned=await seedSupportTicket(db,f.claimant,{sourceKind:'asset',sourceRecord,title:'Assigned asset issue',issue:'Assigned source-linked issue.',ownerId:f.head.userId,dueDate:'2026-10-05',status:'open',version:2,events:[{action:'report',status:'triage',ownerId:null,dueDate:null},{action:'triage'}]}),assignedId=String(assigned.id);
 const {calendarData}=await import("@erpx/server-work_projection/public/calendar/index"),calendarQuery={date:'2026-10-05',view:'day',layers:'support',mode:'mine'};
 expect((await calendarData(db,f.claimant,calendarQuery)).items.some(item=>item.sourceId===assignedId)).toBe(true);
 f.claimant.modules=['support'];
 expect((await calendarData(db,f.claimant,calendarQuery)).items.some(item=>item.sourceId===assignedId)).toBe(false);
 f.claimant.modules=['assets','support'];
 expect((await calendarData(db,f.claimant,calendarQuery)).items.some(item=>item.sourceId===assignedId)).toBe(true);
 f.claimant.modules=['support'];
 expect((await supportWorkspace(f.claimant)).items).toEqual([]);expect(await supportWork(f.claimant)).toEqual([]);
 await expect(supportDetail(f.claimant,assignedId)).rejects.toMatchObject({status:404});
 expect(JSON.stringify(await supportWorkspace(f.claimant))).not.toContain('Assigned asset issue');
});
