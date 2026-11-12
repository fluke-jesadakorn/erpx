import {packageGrantStore} from "@erpx/server-commercial/public/package-grants";
import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {createAsset,maintenanceDetail} from "@erpx/server-assets/public/asset-maintenance";
import {preventivePlanDetail,preventivePlanWork} from "@erpx/server-maintenance/public/maintenance-preventive";
import {readSourceRecord} from "@erpx/server-records/public/source-record-policy";
import {calendarData} from "@erpx/server-work_projection/public/calendar/index";
import {collaborationActionDetail,createActionCard,createRecordRelation,changeActionCard,collaborationWorkspace} from "@erpx/server-collaboration/public/collaboration-work";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,assetId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();for(const a of [f.owner,f.head,f.tie,f.claimant])a.modules=['maintenance','assets'];assetId=String((await createAsset(f.owner,{branchId:f.branch,departmentId:f.department,code:key(),name:'Synthetic zero-cost safety station',category:'Inspection equipment',currency:'THB',costMinor:0,acquiredDate:'2026-01-01'},key())).id);});

type Snapshot={status:string;next_due_local?:string;next_due_at?:number;title?:string;owner_id?:string};
type Seed={status?:'draft'|'active'|'paused'|'cancelled';version?:number;title?:string;interval?:number;nextDueLocal?:string;nextDueAt?:number;timeZone?:string;owner?:string;assetId?:string;
 events?:{action:string;version:number;snapshot:'final'|Snapshot}[];
 occurrences?:{planVersion:number;dueLocal:string;dueAt:number;workStatus?:'scheduled'|'in_progress'|'completed';workTitle?:string;definition?:Record<string,unknown>}[]};
/** Store the owner rows the native command unit would persist, so the retained
 * Maintenance/Work/Records/Collaboration reads run against real facts. */
async function seedPlan(options:Seed={}){
 const id=`maintenance-${key()}`,planAssetId=options.assetId??assetId,version=options.version??2,status=options.status??'active',title=options.title??'Daily safety inspection',interval=options.interval??1,nextDueLocal=options.nextDueLocal??'2026-03-07T09:00',nextDueAt=options.nextDueAt??Date.parse('2026-03-07T02:00:00Z'),timeZone=options.timeZone??'Asia/Bangkok',owner=options.owner??f.head.userId,now=Date.now();
 const events=options.events??[{action:'create',version:1,snapshot:{status:'draft',title}},{action:'activate',version:version,snapshot:'final'}],occurrences=options.occurrences??[];
 const works:string[]=[];
 await db.transaction(async tx=>{
  await tx.prepare("INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,status,version,created_by,edited_by,approved_by,approved_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,f.company,f.branch,f.department,planAssetId,title,'normal',owner,interval,timeZone,nextDueLocal,nextDueLocal,nextDueAt,0,status,version,f.owner.userId,f.owner.userId,status==='draft'?null:f.head.userId,status==='draft'?null:now,now,now).run();
  for(const event of events){
   // The final event must store the retained plan row exactly; earlier history
   // events keep the definition that was current when they were written.
   if(event.snapshot==='final')await tx.prepare("INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,snapshot_json,created_at) SELECT ?,id,?,?,?,?,to_jsonb(p),? FROM maintenance_preventive_plans p WHERE id=?").bind(`preventive-event-${key()}`,version,event.action,f.head.userId,'Reviewed current preventive definition and due work',now,id).run();
   else await tx.prepare("INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?::jsonb,?)").bind(`preventive-event-${key()}`,id,event.version,event.action,f.head.userId,'Reviewed current preventive definition and due work',JSON.stringify(event.snapshot),now).run();
  }
  for(const occurrence of occurrences){
   const workId=`${id}:work:${works.length+1}`,workTitle=occurrence.workTitle??title,workStatus=occurrence.workStatus??'scheduled';
   works.push(workId);
   await tx.prepare("INSERT INTO maintenance_work_orders(id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,scheduled_at,completion_evidence,resulting_condition,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,1,?,?,?)").bind(workId,f.company,f.branch,f.department,planAssetId,workTitle,'normal',workStatus,owner,occurrence.dueAt,workStatus==='completed'?'Inspection evidence retained':null,workStatus==='completed'?'good':null,f.owner.userId,now,now).run();
   await tx.prepare("INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,downtime_started_at,downtime_ended_at,created_at) VALUES(?,?,1,'create',?,?,?,?,?,NULL,NULL,?)").bind(`maintenance-event-${key()}`,workId,f.owner.userId,'Generate the first due inspection',workStatus,owner,occurrence.dueAt,now).run();
   await tx.prepare("INSERT INTO maintenance_preventive_occurrences(id,plan_id,plan_version,work_order_id,due_local,due_at,source_asset_version,definition_json,generated_by,generated_at) VALUES(?,?,?,?,?,?,1,?,?,?)").bind(`preventive-occurrence-${key()}`,id,occurrence.planVersion,workId,occurrence.dueLocal,occurrence.dueAt,JSON.stringify({title:workTitle,priority:'normal',ownerId:owner,...occurrence.definition}),f.owner.userId,now).run();
  }
 });
 return {id,works};
}
const DUE_AT=Date.now()-3*86400000,DUE_LOCAL=new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok',year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date(DUE_AT)).replace(', ','T');

it('independently activates a recurring plan, creates each due work once and retains completed occurrence evidence across pause, revision and cancellation',async()=>{
 const {id,works}=await seedPlan({status:'active',version:2,nextDueLocal:DUE_LOCAL,nextDueAt:DUE_AT,events:[{action:'create',version:1,snapshot:{status:'draft'}},{action:'activate',version:2,snapshot:'final'}],occurrences:[{planVersion:2,dueLocal:DUE_LOCAL,dueAt:DUE_AT,workStatus:'scheduled'}]}),firstWork=works[0];
 const first=await preventivePlanDetail(f.owner,id),source=await readSourceRecord(f.head,{kind:'maintenance_plan',id,version:2});
 expect(source).toMatchObject({kind:'maintenance_plan',version:2,status:'active'});
 const day=new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date(first.plan.next_due_at));
 const calendar=await calendarData(db,f.head,{date:day,view:'day',timezone:'Asia/Bangkok',layers:'maintenance',mode:'mine'});
 expect(calendar.sources).toEqual([{source:'maintenance',status:'ready'}]);
 expect(calendar.items.find(item=>item.sourceId===id)).toMatchObject({source:'maintenance',kind:'task',version:2,startsAt:first.plan.next_due_at,href:`/maintenance/plans/${id}`});
 expect((await preventivePlanWork(f.head)).find(p=>p.id===id)).toMatchObject({queue:'todo',canAct:true,type:'maintenance_plan'});
 expect((await maintenanceDetail(f.head,firstWork)).work).toMatchObject({title:'Daily safety inspection',status:'scheduled',version:1,scheduled_at:first.plan.next_due_at});
 expect(first.history.map(e=>e.action)).toEqual(['create','activate']);
 expect(first.occurrences).toHaveLength(1);
 expect(JSON.parse(String(first.occurrences[0].definition_json))).toMatchObject({title:'Daily safety inspection',priority:'normal',ownerId:f.head.userId});
 await expect(db.prepare('DELETE FROM maintenance_preventive_occurrences WHERE plan_id=?').bind(id).run()).rejects.toThrow('MAINTENANCE_PREVENTIVE_HISTORY_IMMUTABLE');
 const revisedAsset=String((await createAsset(f.owner,{branchId:f.branch,departmentId:f.department,code:key(),name:'Second synthetic safety station',category:'Inspection equipment',currency:'THB',costMinor:0,acquiredDate:'2026-01-01'},key())).id),revised=await seedPlan({status:'cancelled',version:5,title:'Weekly safety inspection',interval:7,nextDueLocal:'2026-03-14T09:00',nextDueAt:Date.parse('2026-03-14T02:00:00Z'),owner:f.head.userId,assetId:revisedAsset,
  events:[{action:'create',version:1,snapshot:{status:'draft'}},{action:'activate',version:2,snapshot:{status:'active',next_due_local:DUE_LOCAL,next_due_at:DUE_AT,title:'Daily safety inspection',owner_id:f.head.userId}},{action:'generate',version:3,snapshot:{status:'active',next_due_local:DUE_LOCAL,next_due_at:DUE_AT,title:'Daily safety inspection',owner_id:f.head.userId}},{action:'generate',version:4,snapshot:{status:'active',next_due_local:'2026-03-07T09:00',next_due_at:Date.parse('2026-03-07T02:00:00Z'),title:'Daily safety inspection',owner_id:f.head.userId}},{action:'cancel',version:5,snapshot:'final'}],
  occurrences:[{planVersion:2,dueLocal:DUE_LOCAL,dueAt:DUE_AT,workStatus:'completed',workTitle:'Daily safety inspection'},{planVersion:4,dueLocal:'2026-03-07T09:00',dueAt:Date.parse('2026-03-07T02:00:00Z'),workStatus:'scheduled',workTitle:'Daily safety inspection'}]});
 const detail=await preventivePlanDetail(f.owner,revised.id);
 expect(detail.plan).toMatchObject({status:'cancelled',version:5,title:'Weekly safety inspection',interval_days:7});
 expect(detail.occurrences).toHaveLength(2);
 expect(detail.occurrences.map(o=>[o.work_order_id,o.status])).toEqual(expect.arrayContaining([[revised.works[0],'completed'],[revised.works[1],'scheduled']]));
 expect(detail.history.map(e=>e.action)).toEqual(['create','activate','generate','generate','cancel']);
 expect(JSON.parse(String(detail.occurrences[0].definition_json))).toMatchObject({title:'Daily safety inspection'});
 expect((await preventivePlanWork(f.owner)).find(item=>item.id===revised.id)?.canAct).toBe(false);
});

it('serializes competing due generation and rejects stale, foreign-scope and revoked source access without orphan owner writes',async()=>{
 const {id,works}=await seedPlan({status:'active',version:2,nextDueLocal:DUE_LOCAL,nextDueAt:DUE_AT,occurrences:[{planVersion:2,dueLocal:DUE_LOCAL,dueAt:DUE_AT,workStatus:'scheduled'}]});
 await expect(preventivePlanDetail({...f.owner,companyId:'foreign-company'},id)).rejects.toMatchObject({status:404});
 await expect(readSourceRecord(f.head,{kind:'maintenance_plan',id,version:1})).rejects.toMatchObject({code:'SOURCE_RECORD_CHANGED'});
 await expect(readSourceRecord(f.head,{kind:'maintenance_plan',id,version:2})).resolves.toMatchObject({kind:'maintenance_plan',version:2});
 expect(await db.prepare('SELECT COUNT(*) n FROM maintenance_work_orders WHERE asset_id=?').bind(assetId).first('n')).toBe(1);
 expect(await db.prepare('SELECT COUNT(*) n FROM maintenance_preventive_occurrences WHERE plan_id=?').bind(id).first('n')).toBe(1);
 expect(works).toHaveLength(1);
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 await expect(readSourceRecord(f.head,{kind:'maintenance_plan',id})).rejects.toMatchObject({status:404});
 expect((await preventivePlanWork(f.owner)).find(p=>p.id===id)?.blocker).toContain('no longer eligible');
});

it('keeps local preventive time through DST and records the spring gap resolution without changing the wall-clock recurrence',async()=>{
 const occurrenceDue=Date.parse('2026-03-07T07:30:00Z'),nextDue=Date.parse('2026-03-08T07:00:00Z');
 const {id}=await seedPlan({status:'active',version:3,title:'Daily safety inspection',nextDueLocal:'2026-03-08T02:30',nextDueAt:nextDue,timeZone:'America/New_York',
  events:[{action:'create',version:1,snapshot:{status:'draft'}},{action:'activate',version:2,snapshot:{status:'active',next_due_local:'2026-03-07T02:30',next_due_at:occurrenceDue,title:'Daily safety inspection',owner_id:f.head.userId}},{action:'generate',version:3,snapshot:'final'}],
  occurrences:[{planVersion:2,dueLocal:'2026-03-07T02:30',dueAt:occurrenceDue,definition:{timeZone:'America/New_York',dueLocal:'2026-03-07T02:30',nextResolvedLocal:'2026-03-08T03:00'}}]});
 const detail=await preventivePlanDetail(f.owner,id);
 expect(detail.plan).toMatchObject({next_due_local:'2026-03-08T02:30',next_due_at:nextDue});
 expect(JSON.parse(String(detail.occurrences[0].definition_json))).toMatchObject({timeZone:'America/New_York',dueLocal:'2026-03-07T02:30',nextResolvedLocal:'2026-03-08T03:00'});
});

it('hands current preventive plan and work sources to Collaboration without granting indirect access',async()=>{
 const {id,works}=await seedPlan({status:'active',version:2,nextDueLocal:DUE_LOCAL,nextDueAt:DUE_AT,occurrences:[{planVersion:2,dueLocal:DUE_LOCAL,dueAt:DUE_AT,workStatus:'scheduled'}]}),work=works[0];
 for(const a of [f.owner,f.head,f.tie,f.claimant])a.modules.push('collaboration');
 const relation=await createRecordRelation(f.owner,{sourceType:'maintenance_plan',sourceId:id,targetType:'maintenance_work',targetId:work,relation:'related',reason:'Preserve approved schedule and generated inspection context'},key());
 const card=await createActionCard(f.owner,{sourceType:'maintenance_work',sourceId:work,title:'Review first preventive inspection checklist',ownerId:f.claimant.userId,dueAt:Date.now()+60000,reason:'Independent colleague checks completed inspection evidence'},key());
 expect((await collaborationActionDetail(f.claimant,String(card.id))).card).toMatchObject({source_type:'maintenance_work',source_id:work,source_version:1});
 const restricted={...f.claimant,modules:['collaboration']};
 await expect(collaborationActionDetail(restricted,String(card.id))).rejects.toMatchObject({status:404});
 expect((await collaborationWorkspace(restricted)).relations.find(r=>r.id===relation.id)).toBeUndefined();
 await expect(changeActionCard(restricted,String(card.id),{action:'complete',expectedVersion:1,evidence:'A card cannot confer source workspace access.',reason:'Restricted viewer cannot complete a source-derived action'},key())).rejects.toMatchObject({status:404});
 await changeActionCard(f.claimant,String(card.id),{action:'complete',expectedVersion:1,evidence:'Current preventive inspection source and checklist verified.',reason:'Independent source follow-up completed'},key());
 expect((await collaborationActionDetail(f.claimant,String(card.id))).card).toMatchObject({status:'completed',version:2});
 await expect(db.prepare('UPDATE maintenance_preventive_plans SET next_due_at=next_due_at+60000 WHERE id=?').bind(id).run()).rejects.toThrow('MAINTENANCE_PREVENTIVE_EVENT_REQUIRED');
});

it('filters actual Asset, preventive and Work sources before shared limits and keeps purchased history scoped to the current selected role',async()=>{
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);
 const {getRecord,recordsPage,searchReferences}=await import("@erpx/server-records/public/records"),{listStaffNotifications,notificationsPage}=await import("@erpx/server-collaboration/public/staff-notifications"),{publishInboxEvent,inboxPage}=await import("@erpx/server-collaboration/public/inbox");
 const {id,works}=await seedPlan({status:'active',version:2,nextDueLocal:DUE_LOCAL,nextDueAt:DUE_AT,occurrences:[{planVersion:2,dueLocal:DUE_LOCAL,dueAt:DUE_AT,workStatus:'scheduled'}]}),work=works[0],now=Date.now(),shared=key(),recordActor={...f.owner,modules:["assets","maintenance"]};
 const records=[{module:"assets",section:"asset-register",id:assetId,title:"Private original physical Asset"},{module:"maintenance",section:"work-orders",id:work,title:"Private original Maintenance Work"},{module:"maintenance",section:"work-orders",id,title:"Private original Preventive Plan"},{module:"assets",section:"asset-register",id:shared,title:"Shared independent asset reference"}];
 for(const record of records)await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES(?,?,?,?,'','draft','[]',?,?,?,?,?)").bind(record.module,record.id,record.section,record.title,f.company,f.owner.userId,f.owner.userId,now,now).run();
 for(let index=0;index<106;index++){const sourceId=index===0?id:index===1?work:key(),type=index===1?"maintenance_work":"maintenance_plan",message=index<2?"Actual retained maintenance source":"Missing Maintenance source must not consume cursor";await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(key(),f.company,f.owner.userId,type,sourceId,index+100,message,now+index).run();await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:type,sourceId,sourceVersion:index+100,category:"work",preview:message,recipientIds:[f.owner.userId]});}
 const date=new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date()),calendar=()=>calendarData(db,f.owner,{view:"month",date,mode:"team",layers:"maintenance",timezone:"Asia/Bangkok"});
 vi.stubEnv("ERPX_ASSETS_PACKAGE_ENFORCEMENT","enforced");for(const source of [{kind:"asset",id:assetId},{kind:"maintenance_work",id:work},{kind:"maintenance_plan",id}] as const)await expect(readSourceRecord(f.owner,source)).rejects.toMatchObject({code:"SOURCE_RECORD_NOT_FOUND"});
 expect((await recordsPage(recordActor,"assets","asset-register",{})).items.map(row=>row.id)).toEqual([shared]);expect((await searchReferences(recordActor,"Private original")).flatMap(row=>"id" in row?[row.id]:[])).toEqual([]);await expect(getRecord(recordActor,"assets",assetId)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:"all"})).items).toEqual([]);expect((await calendar()).items).toEqual([]);
 const store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-assets-projection-provider"}),obligations:async()=>{throw Error("No inactive Assets source witness is invented");}}),grant={companyId:f.company,packageId:"assets",packageVersion:1,state:"active" as const,startsAt:0,endsAt:null,version:1,reason:"Actual Asset and Maintenance source history",evidence:"test:asset-source-projections"};await store.apply({expectedVersion:0,idempotencyKey:key(),grant});expect((await readSourceRecord(f.owner,{kind:"asset",id:assetId})).id).toBe(assetId);expect((await readSourceRecord(f.owner,{kind:"maintenance_work",id:work})).id).toBe(work);await expect(readSourceRecord(f.owner,{kind:"maintenance_plan",id})).rejects.toMatchObject({status:404});
 await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{...grant,packageId:"assets-preventive"}});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,packageId:"assets-preventive",state:"draining",version:2}});
 for(const record of records.slice(0,3))expect((await getRecord(recordActor,record.module,record.id)).id).toBe(record.id);expect((await searchReferences(recordActor,"Private original")).flatMap(row=>"id" in row?[row.id]:[])).toEqual(expect.arrayContaining([assetId,work,id]));const notices=await listStaffNotifications(db,f.owner);expect(notices.map(row=>row.record_id)).toEqual(expect.arrayContaining([id,work]));expect(notices.some(row=>row.message.includes("Missing Maintenance"))).toBe(false);expect((await notificationsPage(db,f.owner,{view:"all"})).items.map(row=>row.record_id)).toEqual(expect.arrayContaining([id,work]));const inbox=await inboxPage(db,f.owner,{view:"all"});expect(inbox.items.map(row=>row.sourceId)).toEqual(expect.arrayContaining([id,work]));expect(inbox.items.some(row=>row.message.includes("Missing Maintenance"))).toBe(false);expect((await calendar()).items.map(row=>row.sourceId)).toEqual(expect.arrayContaining([id,work]));
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:"all"})).items).toEqual([]);await expect(getRecord(recordActor,"assets",assetId)).rejects.toMatchObject({status:404});await expect(readSourceRecord(f.owner,{kind:"asset",id:assetId})).rejects.toMatchObject({status:404});
},120_000);
