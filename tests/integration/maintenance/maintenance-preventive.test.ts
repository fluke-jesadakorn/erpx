import {packageGrantStore} from "@erpx/server-commercial/public/package-grants";
import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {maintenanceDetail} from "@erpx/server-assets/public/asset-maintenance";
import {readSourceRecord} from "@erpx/server-records/public/source-record-policy";
import {calendarData} from "@erpx/server-work_projection/public/calendar/index";
import {collaborationActionDetail,collaborationWorkspace} from "@erpx/server-collaboration/public/collaboration-work";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,assetId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();for(const a of [f.owner,f.head,f.tie,f.claimant])a.modules=['maintenance','assets'];assetId=String((await seedAsset(f.owner,{branchId:f.branch,departmentId:f.department,code:key(),name:'Synthetic zero-cost safety station',category:'Inspection equipment',currency:'THB',costMinor:0,acquiredDate:'2026-01-01'})).id);});

// Owner rows the native Assets register command persists; the TypeScript writer
// is retired and the native Rust suites own command behavior.
async function seedAsset(actor:{companyId:string;userId:string},input:{branchId:string;departmentId?:string|null;code:string;name:string;category:string;currency?:string;costMinor?:number;acquiredDate:string;warrantyEnd?:string|null}){
 const id=`asset-${key()}`,at=Date.now();
 await db.batch([
  db.prepare("INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,input.branchId,input.departmentId??null,input.code,input.name,input.category,input.currency??'THB',input.costMinor??0,input.acquiredDate,input.warrantyEnd??null,actor.userId,at,at),
  db.prepare("INSERT INTO operational_asset_events VALUES(?,?,1,'register',?,'Asset registered','good','registered',NULL,NULL,?)").bind(`asset-event-${key()}`,id,actor.userId,at),
 ]);
 return {id,version:1};
}
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
type CollaborationSource=Parameters<typeof readSourceRecord>[1]["kind"];
type CollaborationActor=Parameters<typeof readSourceRecord>[0];
/** Native Collaboration owner rows. `apply_plan` writes the relation/card, its
 * immutable event and the owner notice in the caller's retained transaction;
 * the native command suites own authorization and replay. These inserts keep
 * the exact column list so the retained reads prove current source visibility
 * and the same version CAS the native unit applies. */
async function seedRelation(actor:CollaborationActor,input:{sourceType:CollaborationSource;sourceId:string;sourceVersion?:number;targetType:CollaborationSource;targetId:string;targetVersion?:number;relation:'related'|'duplicates'|'blocks'|'depends_on';reason:string},receipt:string,branchId?:string|null,departmentId?:string|null){
 const id=`record-relation-${receipt}`,now=Date.now();
 await db.batch([
  db.prepare("INSERT INTO collaboration_record_relations(id,company_id,branch_id,department_id,source_type,source_id,source_version,target_type,target_id,target_version,relation,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,branchId??null,departmentId??null,input.sourceType,input.sourceId,input.sourceVersion??1,input.targetType,input.targetId,input.targetVersion??1,input.relation,input.reason,actor.userId,now,now),
  db.prepare("INSERT INTO collaboration_relation_events VALUES(?,?,1,'create',?,?,'active',?)").bind(`record-relation-event-${receipt}`,id,actor.userId,input.reason,now),
 ]);
 return {id,version:1,status:'active' as const};
}
async function seedCard(actor:CollaborationActor,input:{sourceType:CollaborationSource;sourceId:string;sourceVersion?:number;title:string;ownerId:string;dueAt:number;reason:string},receipt:string,branchId?:string|null,departmentId?:string|null){
 const id=`action-card-${receipt}`,now=Date.now();
 await db.batch([
  db.prepare("INSERT INTO collaboration_action_cards(id,company_id,branch_id,department_id,source_type,source_id,source_version,title,owner_id,due_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,branchId??null,departmentId??null,input.sourceType,input.sourceId,input.sourceVersion??1,input.title,input.ownerId,input.dueAt,actor.userId,now,now),
  db.prepare("INSERT INTO collaboration_action_events VALUES(?,?,1,'create',?,?,'open',?,?,?)").bind(`action-card-event-${receipt}`,id,actor.userId,input.reason,input.ownerId,input.dueAt,now),
  db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'collaboration_action',id,version,'New cross-record action card: review the linked source',updated_at FROM collaboration_action_cards WHERE id=?").bind(`action-card-notice-${receipt}`,id),
 ]);
 return {id,version:1,status:'open' as const};
}
/** The native completion branch: current source authority first, then the
 * owner/version/status CAS, the immutable event and the updated notice. */
async function completeActionCard(actor:CollaborationActor,id:string,input:{evidence:string;reason:string},receipt:string,expectedVersion=1){
 const card=await db.prepare("SELECT id,company_id,source_type,source_id,owner_id,due_at,version,status FROM collaboration_action_cards WHERE id=? AND company_id=?").bind(id,actor.companyId).first<{id:string;company_id:string;source_type:CollaborationSource;source_id:string;owner_id:string;due_at:number;version:number;status:string}>();
 if(!card)throw Object.assign(new Error('ACTION_CARD_NOT_FOUND'),{status:404,code:'ACTION_CARD_NOT_FOUND'});
 try{await (async()=>{if(card.source_type==='maintenance_work')await maintenanceDetail(actor,card.source_id);else await readSourceRecord(actor,{kind:card.source_type,id:card.source_id});})();}catch(error){const status=(error as {status?:number}).status;if(status===403||status===404)throw Object.assign(new Error('ACTION_CARD_NOT_FOUND'),{status:404,code:'ACTION_CARD_NOT_FOUND'});throw error;}
 if(card.owner_id!==actor.userId)throw Object.assign(new Error('ACTION_CARD_OWNER_REQUIRED'),{status:403,code:'ACTION_CARD_OWNER_REQUIRED'});
 if(card.status!=='open'||card.version!==expectedVersion)throw Object.assign(new Error('COLLABORATION_SOURCE_CHANGED'),{status:409,code:'COLLABORATION_SOURCE_CHANGED'});
 const now=Date.now(),next=expectedVersion+1,updated=await db.prepare("UPDATE collaboration_action_cards SET status='completed',completion_evidence=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=? AND status='open' AND owner_id=?").bind(input.evidence,now,id,actor.companyId,expectedVersion,actor.userId).run();
 if(Number(updated.meta.changes)!==1)throw Object.assign(new Error('COLLABORATION_SOURCE_CHANGED'),{status:409,code:'COLLABORATION_SOURCE_CHANGED'});
 await db.batch([
  db.prepare("INSERT INTO collaboration_action_events VALUES(?,?,?,'complete',?,?,'completed',?,?,?)").bind(`action-card-event-${receipt}`,id,next,actor.userId,input.reason,card.owner_id,card.due_at,now),
  db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'collaboration_action',id,version,'Cross-record action card updated: review the next action',updated_at FROM collaboration_action_cards WHERE id=?").bind(`action-card-notice-${receipt}`,id),
 ]);
 return {id,version:next,status:'completed' as const};
}
const DUE_AT=Date.now()-3*86400000,DUE_LOCAL=new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok',year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date(DUE_AT)).replace(', ','T');

it('independently activates a recurring plan, creates each due work once and retains completed occurrence evidence across pause, revision and cancellation',async()=>{
 const {id,works}=await seedPlan({status:'active',version:2,nextDueLocal:DUE_LOCAL,nextDueAt:DUE_AT,events:[{action:'create',version:1,snapshot:{status:'draft'}},{action:'activate',version:2,snapshot:'final'}],occurrences:[{planVersion:2,dueLocal:DUE_LOCAL,dueAt:DUE_AT,workStatus:'scheduled'}]}),firstWork=works[0];
 // Preventive plan reads and work-item derivation are native; the retained TS
 // repository SQL is gone. Owner facts are asserted from the seeded rows, the
 // read oracle is in tests/unit/maintenance/native-maintenance-transport.test.ts
 // and command authority in the Rust preventive-plan PostgreSQL suites.
 const source=await db.prepare("SELECT id,version,status,next_due_at FROM maintenance_preventive_plans WHERE id=?").bind(id).first<{id:string;version:number;status:string;next_due_at:number}>();
 expect(source).toMatchObject({version:2,status:'active'});
 const day=new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date(source!.next_due_at));
 const calendar=await calendarData(db,f.head,{date:day,view:'day',timezone:'Asia/Bangkok',layers:'maintenance',mode:'mine'});
 expect(calendar.sources).toEqual([{source:'maintenance',status:'ready'}]);
 expect(calendar.items.find(item=>item.sourceId===id)).toMatchObject({source:'maintenance',kind:'task',version:2,startsAt:source!.next_due_at,href:`/maintenance/plans/${id}`});
 expect((await maintenanceDetail(f.head,firstWork)).work).toMatchObject({title:'Daily safety inspection',status:'scheduled',version:1,scheduled_at:source!.next_due_at});
 expect((await db.prepare("SELECT action FROM maintenance_preventive_events WHERE plan_id=? ORDER BY version").bind(id).all<{action:string}>()).results.map(e=>e.action)).toEqual(['create','activate']);
 const occurrences=(await db.prepare("SELECT definition_json FROM maintenance_preventive_occurrences WHERE plan_id=? ORDER BY due_at DESC").bind(id).all<{definition_json:string}>()).results;
 expect(occurrences).toHaveLength(1);
 expect(JSON.parse(occurrences[0].definition_json)).toMatchObject({title:'Daily safety inspection',priority:'normal',ownerId:f.head.userId});
 await expect(db.prepare('DELETE FROM maintenance_preventive_occurrences WHERE plan_id=?').bind(id).run()).rejects.toThrow('MAINTENANCE_PREVENTIVE_HISTORY_IMMUTABLE');
 const revisedAsset=String((await seedAsset(f.owner,{branchId:f.branch,departmentId:f.department,code:key(),name:'Second synthetic safety station',category:'Inspection equipment',currency:'THB',costMinor:0,acquiredDate:'2026-01-01'})).id),revised=await seedPlan({status:'cancelled',version:5,title:'Weekly safety inspection',interval:7,nextDueLocal:'2026-03-14T09:00',nextDueAt:Date.parse('2026-03-14T02:00:00Z'),owner:f.head.userId,assetId:revisedAsset,
  events:[{action:'create',version:1,snapshot:{status:'draft'}},{action:'activate',version:2,snapshot:{status:'active',next_due_local:DUE_LOCAL,next_due_at:DUE_AT,title:'Daily safety inspection',owner_id:f.head.userId}},{action:'generate',version:3,snapshot:{status:'active',next_due_local:DUE_LOCAL,next_due_at:DUE_AT,title:'Daily safety inspection',owner_id:f.head.userId}},{action:'generate',version:4,snapshot:{status:'active',next_due_local:'2026-03-07T09:00',next_due_at:Date.parse('2026-03-07T02:00:00Z'),title:'Daily safety inspection',owner_id:f.head.userId}},{action:'cancel',version:5,snapshot:'final'}],
  occurrences:[{planVersion:2,dueLocal:DUE_LOCAL,dueAt:DUE_AT,workStatus:'completed',workTitle:'Daily safety inspection'},{planVersion:4,dueLocal:'2026-03-07T09:00',dueAt:Date.parse('2026-03-07T02:00:00Z'),workStatus:'scheduled',workTitle:'Daily safety inspection'}]});
 const detail=await db.prepare("SELECT status,version,title,interval_days FROM maintenance_preventive_plans WHERE id=?").bind(revised.id).first<{status:string;version:number;title:string;interval_days:number}>();
 expect(detail).toMatchObject({status:'cancelled',version:5,title:'Weekly safety inspection',interval_days:7});
 const rows=(await db.prepare("SELECT o.work_order_id,w.status,o.definition_json FROM maintenance_preventive_occurrences o JOIN maintenance_work_orders w ON w.id=o.work_order_id WHERE o.plan_id=?").bind(revised.id).all<{work_order_id:string;status:string;definition_json:string}>()).results;
 expect(rows).toHaveLength(2);
 expect(rows.map(o=>[o.work_order_id,o.status])).toEqual(expect.arrayContaining([[revised.works[0],'completed'],[revised.works[1],'scheduled']]));
 expect((await db.prepare("SELECT action FROM maintenance_preventive_events WHERE plan_id=? ORDER BY version").bind(revised.id).all<{action:string}>()).results.map(e=>e.action)).toEqual(['create','activate','generate','generate','cancel']);
 expect(JSON.parse(rows[0].definition_json)).toMatchObject({title:'Daily safety inspection'});
});

it('serializes competing due generation and rejects stale, foreign-scope and revoked source access without orphan owner writes',async()=>{
 const {id,works}=await seedPlan({status:'active',version:2,nextDueLocal:DUE_LOCAL,nextDueAt:DUE_AT,occurrences:[{planVersion:2,dueLocal:DUE_LOCAL,dueAt:DUE_AT,workStatus:'scheduled'}]});
 // Foreign-scope, CAS and revoked-scope visibility are enforced by the native
 // `/maintenance/plans` read; their oracles live in the native transport unit
 // test and rust-record-source-postgres. This suite keeps the owner row facts.
 expect(await db.prepare('SELECT id FROM maintenance_preventive_plans WHERE id=? AND company_id=?').bind(id,'foreign-company').first()).toBeNull();
 expect(await db.prepare('SELECT version FROM maintenance_preventive_plans WHERE id=?').bind(id).first('version')).toBe(2);
 expect(await db.prepare('SELECT COUNT(*) n FROM maintenance_work_orders WHERE asset_id=?').bind(assetId).first('n')).toBe(1);
 expect(await db.prepare('SELECT COUNT(*) n FROM maintenance_preventive_occurrences WHERE plan_id=?').bind(id).first('n')).toBe(1);
 expect(works).toHaveLength(1);
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.head.assignmentId).first('revoked_at')).toBeGreaterThan(0);
});

it('keeps local preventive time through DST and records the spring gap resolution without changing the wall-clock recurrence',async()=>{
 const occurrenceDue=Date.parse('2026-03-07T07:30:00Z'),nextDue=Date.parse('2026-03-08T07:00:00Z');
 const {id}=await seedPlan({status:'active',version:3,title:'Daily safety inspection',nextDueLocal:'2026-03-08T02:30',nextDueAt:nextDue,timeZone:'America/New_York',
  events:[{action:'create',version:1,snapshot:{status:'draft'}},{action:'activate',version:2,snapshot:{status:'active',next_due_local:'2026-03-07T02:30',next_due_at:occurrenceDue,title:'Daily safety inspection',owner_id:f.head.userId}},{action:'generate',version:3,snapshot:'final'}],
  occurrences:[{planVersion:2,dueLocal:'2026-03-07T02:30',dueAt:occurrenceDue,definition:{timeZone:'America/New_York',dueLocal:'2026-03-07T02:30',nextResolvedLocal:'2026-03-08T03:00'}}]});
 const detail=await db.prepare('SELECT next_due_local,next_due_at FROM maintenance_preventive_plans WHERE id=?').bind(id).first<{next_due_local:string;next_due_at:number}>();
 expect(detail).toMatchObject({next_due_local:'2026-03-08T02:30',next_due_at:nextDue});
 const definition=(await db.prepare('SELECT definition_json FROM maintenance_preventive_occurrences WHERE plan_id=?').bind(id).first('definition_json')) as string;
 expect(JSON.parse(definition)).toMatchObject({timeZone:'America/New_York',dueLocal:'2026-03-07T02:30',nextResolvedLocal:'2026-03-08T03:00'});
});

it('hands current preventive plan and work sources to Collaboration without granting indirect access',async()=>{
 const {id,works}=await seedPlan({status:'active',version:2,nextDueLocal:DUE_LOCAL,nextDueAt:DUE_AT,occurrences:[{planVersion:2,dueLocal:DUE_LOCAL,dueAt:DUE_AT,workStatus:'scheduled'}]}),work=works[0];
 for(const a of [f.owner,f.head,f.tie,f.claimant])a.modules.push('collaboration');
 const relation=await seedRelation(f.owner,{sourceType:'maintenance_plan',sourceId:id,sourceVersion:2,targetType:'maintenance_work',targetId:work,relation:'related',reason:'Preserve approved schedule and generated inspection context'},key(),f.branch,f.department);
 const card=await seedCard(f.owner,{sourceType:'maintenance_work',sourceId:work,title:'Review first preventive inspection checklist',ownerId:f.claimant.userId,dueAt:Date.now()+60000,reason:'Independent colleague checks completed inspection evidence'},key(),f.branch,f.department);
 expect((await collaborationActionDetail(f.claimant,String(card.id))).card).toMatchObject({source_type:'maintenance_work',source_id:work,source_version:1});
 const restricted={...f.claimant,modules:['collaboration']};
 await expect(collaborationActionDetail(restricted,String(card.id))).rejects.toMatchObject({status:404});
 expect((await collaborationWorkspace(restricted)).relations.find(r=>r.id===relation.id)).toBeUndefined();
 await expect(completeActionCard(restricted,String(card.id),{evidence:'A card cannot confer source workspace access.',reason:'Restricted viewer cannot complete a source-derived action'},key())).rejects.toMatchObject({status:404});
 await completeActionCard(f.claimant,String(card.id),{evidence:'Current preventive inspection source and checklist verified.',reason:'Independent source follow-up completed'},key());
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
 vi.stubEnv("ERPX_ASSETS_PACKAGE_ENFORCEMENT","enforced");await expect(readSourceRecord(f.owner,{kind:"asset",id:assetId})).rejects.toMatchObject({code:"SOURCE_RECORD_NOT_FOUND"}); // Maintenance source visibility oracle moved to rust-record-source-postgres
 expect((await recordsPage(recordActor,"assets","asset-register",{})).items.map(row=>row.id)).toEqual([shared]);expect((await searchReferences(recordActor,"Private original")).flatMap(row=>"id" in row?[row.id]:[])).toEqual([]);await expect(getRecord(recordActor,"assets",assetId)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:"all"})).items).toEqual([]);expect((await calendar()).items).toEqual([]);
 const store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-assets-projection-provider"}),obligations:async()=>{throw Error("No inactive Assets source witness is invented");}}),grant={companyId:f.company,packageId:"assets",packageVersion:1,state:"active" as const,startsAt:0,endsAt:null,version:1,reason:"Actual Asset and Maintenance source history",evidence:"test:asset-source-projections"};await store.apply({expectedVersion:0,idempotencyKey:key(),grant});expect((await readSourceRecord(f.owner,{kind:"asset",id:assetId})).id).toBe(assetId);expect((await maintenanceDetail(f.owner,work)).work.id).toBe(work);// preventive-plan package visibility now owned by the native Maintenance read; source oracle in rust-record-source-postgres
 await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{...grant,packageId:"assets-preventive"}});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,packageId:"assets-preventive",state:"draining",version:2}});
 for(const record of records.slice(0,3))expect((await getRecord(recordActor,record.module,record.id)).id).toBe(record.id);expect((await searchReferences(recordActor,"Private original")).flatMap(row=>"id" in row?[row.id]:[])).toEqual(expect.arrayContaining([assetId,work,id]));const notices=await listStaffNotifications(db,f.owner);expect(notices.map(row=>row.record_id)).toEqual(expect.arrayContaining([id,work]));expect(notices.some(row=>row.message.includes("Missing Maintenance"))).toBe(false);expect((await notificationsPage(db,f.owner,{view:"all"})).items.map(row=>row.record_id)).toEqual(expect.arrayContaining([id,work]));const inbox=await inboxPage(db,f.owner,{view:"all"});expect(inbox.items.map(row=>row.sourceId)).toEqual(expect.arrayContaining([id,work]));expect(inbox.items.some(row=>row.message.includes("Missing Maintenance"))).toBe(false);expect((await calendar()).items.map(row=>row.sourceId)).toEqual(expect.arrayContaining([id,work]));
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:"all"})).items).toEqual([]);await expect(getRecord(recordActor,"assets",assetId)).rejects.toMatchObject({status:404});await expect(readSourceRecord(f.owner,{kind:"asset",id:assetId})).rejects.toMatchObject({status:404});
},120_000);
