import {packageGrantStore} from "../../../src/server/modules/commercial/public/package-grants";
import {withAssetPurchasedCommand} from "../../../src/server/modules/assets/public/purchases";
import {beforeAll,beforeEach,afterEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {createAsset,changeMaintenanceWork,maintenanceDetail} from "../../../src/server/modules/assets/public/asset-maintenance";
import {savePreventivePlan,changePreventivePlan,preventivePlanDetail,preventivePlanWork} from "../../../src/server/modules/maintenance/public/maintenance-preventive";
import {readSourceRecord} from "../../../src/server/modules/records/public/source-record-policy";
import {calendarData} from "../../../src/server/modules/work_projection/public/calendar/index";
import {collaborationActionDetail,createActionCard,createRecordRelation,changeActionCard,collaborationWorkspace} from "../../../src/server/modules/collaboration/public/collaboration-work";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
afterEach(()=>vi.unstubAllEnvs());
let f:Awaited<ReturnType<typeof expenseFixture>>,assetId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();for(const a of [f.owner,f.head,f.tie,f.claimant])a.modules=['maintenance','assets'];assetId=String((await createAsset(f.owner,{branchId:f.branch,departmentId:f.department,code:key(),name:'Synthetic zero-cost safety station',category:'Inspection equipment',currency:'THB',costMinor:0,acquiredDate:'2026-01-01'},key())).id);});
async function plan(overrides:Record<string,unknown>={}){return String((await savePreventivePlan(f.owner,null,{assetId,assetVersion:1,title:'Daily safety inspection',priority:'normal',ownerId:f.head.userId,intervalDays:1,firstDueAt:Date.now()-3*86400000,timeZone:'Asia/Bangkok',leadDays:0,reason:'Recurring preventive safety inspection',...overrides},key())).id);}
async function act(id:string,action:string,expectedVersion:number,who=f.tie){return changePreventivePlan(who,id,{action,expectedVersion,reason:'Reviewed current preventive definition and due work'},key());}

it('independently activates a recurring plan, creates each due work once and retains completed occurrence evidence across pause, revision and cancellation',async()=>{
 const id=await plan();await expect(act(id,'activate',1,f.owner)).rejects.toMatchObject({code:'INDEPENDENT_PLAN_REVIEW_REQUIRED'});
 await act(id,'activate',1);const first=await preventivePlanDetail(f.owner,id),source=await readSourceRecord(f.head,{kind:'maintenance_plan',id,version:2});expect(source).toMatchObject({kind:'maintenance_plan',version:2,status:'active'});
 const day=new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date(first.plan.next_due_at));
 const calendar=await calendarData(db,f.head,{date:day,view:'day',timezone:'Asia/Bangkok',layers:'maintenance',mode:'mine'});
 expect(calendar.sources).toEqual([{source:'maintenance',status:'ready'}]);expect(calendar.items.find(item=>item.sourceId===id)).toMatchObject({source:'maintenance',kind:'task',version:2,startsAt:first.plan.next_due_at,href:`/maintenance/plans/${id}`});
 expect((await preventivePlanWork(f.head)).find(p=>p.id===id)).toMatchObject({queue:'todo',canAct:true,type:'maintenance_plan'});
 const generation=key(),input={action:'generate',expectedVersion:2,reason:'Generate the first due safety inspection'},created=await changePreventivePlan(f.head,id,input,generation),replayed=await changePreventivePlan(f.head,id,input,generation);expect(replayed).toEqual(created);
 const firstWork=String(created.workOrderId);expect((await maintenanceDetail(f.head,firstWork)).work).toMatchObject({title:'Daily safety inspection',status:'scheduled',version:1,scheduled_at:first.plan.next_due_at});
 await expect(act(id,'generate',3,f.head)).rejects.toMatchObject({code:'CONFLICT'});
 expect(await db.prepare('SELECT COUNT(*) n FROM maintenance_preventive_occurrences WHERE plan_id=?').bind(id).first('n')).toBe(1);
 expect(await db.prepare('SELECT COUNT(*) n FROM command_receipts WHERE command_id=?').bind(`${generation}:work`).first('n')).toBe(1);
 await changeMaintenanceWork(f.head,firstWork,{action:'start',expectedVersion:1,reason:'Perform the scheduled safety inspection'},key());
 await changeMaintenanceWork(f.head,firstWork,{action:'complete',expectedVersion:2,resultingCondition:'good',evidence:'Safety station inspection and function check passed.',reason:'Completed the preventive checklist'},key());
 const second=await act(id,'generate',3,f.head);expect(second.workOrderId).not.toBe(firstWork);
 await act(id,'pause',4);await expect(act(id,'generate',5,f.head)).rejects.toMatchObject({code:'MAINTENANCE_PLAN_INACTIVE'});
 const current=await preventivePlanDetail(f.owner,id),asset=await db.prepare('SELECT version FROM operational_assets WHERE id=?').bind(assetId).first<{version:number}>();
 await savePreventivePlan(f.owner,id,{assetId,assetVersion:asset!.version,title:'Weekly safety inspection',priority:'high',ownerId:f.head.userId,intervalDays:7,firstDueAt:Date.now()+86400000,timeZone:'Asia/Bangkok',leadDays:7,expectedVersion:5,reason:'Reviewed weekly interval after daily inspection history'},key());
 await act(id,'activate',6);await act(id,'cancel',7);
 const final=await preventivePlanDetail(f.owner,id);expect(final.plan).toMatchObject({status:'cancelled',version:8,title:'Weekly safety inspection',interval_days:7});expect(final.occurrences).toHaveLength(2);
 expect(final.occurrences.map(o=>[o.work_order_id,o.status])).toEqual(expect.arrayContaining([[firstWork,'completed'],[second.workOrderId,'scheduled']]));
 expect(current.occurrences[0].definition_json).toContain('Daily safety inspection');
 expect(final.history.map(e=>e.action)).toEqual(['create','activate','generate','generate','pause','revise','activate','cancel']);
 await expect(db.prepare('DELETE FROM maintenance_preventive_occurrences WHERE plan_id=?').bind(id).run()).rejects.toThrow('MAINTENANCE_PREVENTIVE_HISTORY_IMMUTABLE');
});

it('serializes competing due generation and rejects stale, foreign-scope and revoked source access without orphan owner writes',async()=>{
 const id=await plan();await act(id,'activate',1);
 await expect(preventivePlanDetail({...f.owner,companyId:'foreign-company'},id)).rejects.toMatchObject({status:404});
 await expect(act(id,'generate',99,f.head)).rejects.toMatchObject({code:'CONFLICT'});expect(await db.prepare('SELECT COUNT(*) n FROM maintenance_preventive_occurrences WHERE plan_id=?').bind(id).first('n')).toBe(0);
 const outcomes=await Promise.allSettled([act(id,'generate',2,f.head),act(id,'generate',2,f.head)]);expect(outcomes.filter(x=>x.status==='fulfilled')).toHaveLength(1);
 expect(await db.prepare('SELECT COUNT(*) n FROM maintenance_work_orders WHERE asset_id=?').bind(assetId).first('n')).toBe(1);
 expect(await db.prepare('SELECT COUNT(*) n FROM maintenance_preventive_occurrences WHERE plan_id=?').bind(id).first('n')).toBe(1);
 await expect(readSourceRecord(f.head,{kind:'maintenance_plan',id,version:2})).rejects.toMatchObject({code:'SOURCE_RECORD_CHANGED'});
 const source=await readSourceRecord(f.head,{kind:'maintenance_plan',id,version:3});expect(source.href).toContain('/maintenance/plans/');
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 await expect(readSourceRecord(f.head,{kind:'maintenance_plan',id})).rejects.toMatchObject({status:404});
 expect((await preventivePlanWork(f.owner)).find(p=>p.id===id)?.blocker).toContain('no longer eligible');
});

it('keeps local preventive time through DST and records the spring gap resolution without changing the wall-clock recurrence',async()=>{
 const id=await plan({firstDueAt:Date.parse('2026-03-07T07:30:00Z'),timeZone:'America/New_York'});await act(id,'activate',1);const result=await act(id,'generate',2,f.head),detail=await preventivePlanDetail(f.owner,id);
 expect(detail.plan).toMatchObject({next_due_local:'2026-03-08T02:30',next_due_at:Date.parse('2026-03-08T07:00:00Z')});expect(result.nextDueAt).toBe(Date.parse('2026-03-08T07:00:00Z'));
 expect(JSON.parse(String(detail.occurrences[0].definition_json))).toMatchObject({timeZone:'America/New_York',dueLocal:'2026-03-07T02:30',nextResolvedLocal:'2026-03-08T03:00'});
});

it('hands current preventive plan and work sources to Collaboration without granting indirect access',async()=>{
 for(const a of [f.owner,f.head,f.tie,f.claimant])a.modules.push('collaboration');
 const id=await plan();await act(id,'activate',1);const generated=await act(id,'generate',2,f.head),work=String(generated.workOrderId);
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

it('composes the purchased preventive addon into one real Work and Asset transaction with final source rollback and draining history',async()=>{
 const store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-preventive-provider"}),obligations:async()=>{throw Error("No preventive inactive witness is invented");}}),grant={companyId:f.company,packageId:"assets",packageVersion:1,state:"active" as const,startsAt:0,endsAt:null,version:1,reason:"Actual recurring asset work",evidence:"test:preventive-purchases"};await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv("ERPX_ASSETS_PACKAGE_ENFORCEMENT","enforced");await expect(plan()).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{...grant,packageId:"assets-preventive"}});const id=await plan();await act(id,"activate",1);
 const command=key(),input={action:"generate",expectedVersion:2,reason:"Generate current purchased due work"},fault=`preventive_final_${key().replaceAll('-','')}`,tables=["operational_assets","maintenance_work_orders","maintenance_work_events","maintenance_preventive_plans","maintenance_preventive_occurrences","maintenance_preventive_events","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE operational_assets SET condition='unsafe' WHERE id='${assetId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}:work') EXECUTE FUNCTION ${fault}()`);try{await expect(changePreventivePlan(f.head,id,input,command)).rejects.toMatchObject({code:"ASSETS_SOURCE_CHANGED"});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
 const saved=await changePreventivePlan(f.head,id,input,command);expect(await changePreventivePlan(f.head,id,input,command)).toEqual(saved);expect(await db.prepare("SELECT count(*) n FROM maintenance_preventive_occurrences WHERE plan_id=?").bind(id).first("n")).toBe(1);
 await expect(db.transaction(tx=>withAssetPurchasedCommand(f.owner,{key:key(),operation:"asset.condition",capability:"assets.register",purpose:"settle-existing",target:{kind:"asset",id:assetId},permissions:["records.write"]},async()=>true,tx))).rejects.toMatchObject({code:"ASSETS_TRANSACTION_OWNERSHIP_REQUIRED"});
 await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});await expect(act(id,"generate",3,f.head)).rejects.toMatchObject({code:"DEPENDENCY_REQUIRED"});await act(id,"pause",3);await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,packageId:"assets-preventive",state:"draining",version:2}});await expect(act(id,"activate",4)).rejects.toMatchObject({code:"PACKAGE_DRAINING"});await act(id,"cancel",4);
 const work=String(saved.workOrderId);await changeMaintenanceWork(f.head,work,{action:"start",expectedVersion:1,reason:"Finish the already-generated work"},key());await changeMaintenanceWork(f.head,work,{action:"complete",expectedVersion:2,resultingCondition:"good",evidence:"Existing inspection evidence retained",reason:"Close the old generated work"},key());expect((await preventivePlanDetail(f.owner,id)).plan.status).toBe("cancelled");expect((await maintenanceDetail(f.owner,work)).work.status).toBe("completed");expect((await preventivePlanWork(f.owner)).every(item=>!item.canAct)).toBe(true);
});

it('filters actual Asset, preventive and Work sources before shared limits and keeps purchased history scoped to the current selected role',async()=>{
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);
 const {getRecord,recordsPage,searchReferences}=await import("../../../src/server/modules/records/public/records"),{listStaffNotifications,notificationsPage}=await import("../../../src/server/modules/collaboration/public/staff-notifications"),{publishInboxEvent,inboxPage}=await import("../../../src/server/modules/collaboration/public/inbox");
 const id=await plan();await act(id,"activate",1);const generated=await act(id,"generate",2,f.head),work=String(generated.workOrderId),now=Date.now(),shared=key(),recordActor={...f.owner,modules:["assets","maintenance"]};
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
});
