import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {createAsset,changeMaintenanceWork,maintenanceDetail} from '../src/server/asset-maintenance';
import {savePreventivePlan,changePreventivePlan,preventivePlanDetail,preventivePlanWork} from '../src/server/maintenance-preventive';
import {readSourceRecord} from '../src/server/source-record-policy';
import {calendarData} from '../src/server/calendar';
import {collaborationActionDetail,createActionCard,createRecordRelation,changeActionCard,collaborationWorkspace} from '../src/server/collaboration-work';
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
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
