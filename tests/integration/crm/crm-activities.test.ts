import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {createCRMLead} from "@erpx/server-crm/public/crm";
import {crmActivityWorkspace,crmActivityWork} from "@erpx/server-crm/public/crm-activities";
import {seedNativeActivity,changeNativeActivity} from "./crm-activities-oracle";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,leadId:string;
const seed=(input:{key:string;ownerId:string;actorId?:string;activityType?:string;subject?:string;details?:string;dueDate?:string;expectedVersion?:number})=>seedNativeActivity(db,{key:input.key,leadId,companyId:f.owner.companyId,branchId:f.owner.branchId,departmentId:f.owner.departmentId,actorId:input.actorId??f.owner.userId,ownerId:input.ownerId,activityType:input.activityType??'email',subject:input.subject??'Send installation schedule',details:input.details??'Email the customer with the approved delivery window.',dueDate:input.dueDate??'2026-09-24',expectedVersion:input.expectedVersion??1});
const change=(input:{key:string;id:string;actorId:string;action:'complete'|'cancel'|'reassign';expectedVersion:number;reason:string;manager?:boolean;ownerId?:string;dueDate?:string})=>changeNativeActivity(db,{...input,companyId:f.owner.companyId,branchId:f.owner.branchId,departmentId:f.owner.departmentId});
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role='sales_manager';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'sales_manager');leadId=String((await createCRMLead(f.owner,{branchId:f.branch,title:'Activity test lead',contactName:'Activity customer',contactDetail:'A real follow-up channel',source:'Inbound email',brief:'Needs a confirmed installation date.',dueDate:'2026-09-24'},key())).id);});
// Native-seeding oracle: the deleted TypeScript activity writers are replaced by
// rows matching packages/server-crm/src/adapters/activities_native.rs. Command
// authorization, purchase and Workflow assertions moved to the Rust owner tests.
it('schedules, completes and reloads a lead activity with Work and immutable history',async()=>{const seedInput={key:key(),ownerId:f.owner.userId},created=await seed(seedInput);expect(await seed(seedInput)).toEqual(created);let view=await crmActivityWorkspace(f.owner,leadId);expect(view.items[0]).toMatchObject({id:created.id,status:'open',version:1,canComplete:true});expect((await crmActivityWork(f.owner)).find(x=>x.id===created.id)).toMatchObject({queue:'todo',type:'crm_activity'});await change({key:key(),id:created.id,actorId:f.owner.userId,action:'complete',expectedVersion:1,reason:'Customer confirmed the proposed installation date.'});view=await crmActivityWorkspace(f.owner,leadId);expect(view.items[0]).toMatchObject({status:'completed',version:2});expect(view.history.filter(x=>x.activity_id===created.id)).toHaveLength(2);await expect(db.prepare('DELETE FROM crm_lead_activity_events WHERE activity_id=?').bind(created.id).run()).rejects.toThrow('CRM_ACTIVITY_EVIDENCE_IMMUTABLE');});
it('protects source versions, scopes reads and lets a manager reassign',async()=>{await expect(seed({key:key(),ownerId:f.owner.userId,expectedVersion:99})).rejects.toMatchObject({code:'CONFLICT'});const created=await seed({key:key(),ownerId:f.owner.userId,activityType:'call',subject:'Manager handoff',details:'Call back after technical review.'});await expect(crmActivityWorkspace({...f.owner,branchId:'other'},leadId)).rejects.toMatchObject({status:404});await change({key:key(),id:created.id,actorId:f.head.userId,action:'reassign',expectedVersion:1,reason:'Manager assigns the next customer call',manager:true,ownerId:f.head.userId,dueDate:'2026-09-25'});expect((await crmActivityWorkspace(f.head,leadId)).items[0]).toMatchObject({owner_id:f.head.userId,version:2});});
it('lets an independent sales manager cancel a follow-up with replay-safe history',async()=>{const created=await seed({key:key(),ownerId:f.owner.userId,subject:'Confirm new delivery time',details:'Ask whether the customer wants a revised installation slot.'}),changeInput={key:key(),id:created.id,actorId:f.head.userId,action:'cancel' as const,expectedVersion:1,reason:'Customer withdrew the installation request.',manager:true},cancelled=await change(changeInput);expect(await change(changeInput)).toEqual(cancelled);expect(await crmActivityWorkspace(f.owner,leadId)).toMatchObject({items:[expect.objectContaining({id:created.id,status:'cancelled',version:2,outcome:changeInput.reason})]});expect((await crmActivityWork(f.owner)).find(item=>item.id===created.id)).toMatchObject({queue:'completed',status:'cancelled'});expect((await crmActivityWorkspace(f.owner,leadId)).history.filter(item=>item.activity_id===created.id)).toHaveLength(2);});
it('rejects PostgreSQL-incompatible reassign reasons without partial effects and checks replay first',async()=>{
 const created=await seed({key:key(),ownerId:f.owner.userId,activityType:'call',subject:'Unicode handoff',details:'Review the next customer action.'});
 const id=created.id;
 const base={action:'reassign' as const,expectedVersion:1,ownerId:f.head.userId,dueDate:'2026-09-25'};
 for(const reason of ['Bad\u0000reason','Bad\ud800reason']){
  const command=key(),input={...base,reason};
  // The JSON actually produced by the adapter cannot be stored in this JSONB
  // column: neither NUL nor an isolated surrogate is a PostgreSQL character.
  await expect(db.prepare('SELECT ?::jsonb').bind(JSON.stringify(input)).first()).rejects.toThrow();
  await expect(change({key:command,id,actorId:f.head.userId,reason,manager:true,...base})).rejects.toThrow();
  const state=await db.prepare('SELECT owner_id,version FROM crm_lead_activities WHERE id=?').bind(id).first<{owner_id:string;version:number}>();
  expect(state).toMatchObject({owner_id:f.owner.userId,version:1});
  const effects=await db.prepare('SELECT (SELECT count(*) FROM crm_lead_activity_events WHERE activity_id=?) events,(SELECT count(*) FROM command_receipts WHERE command_id=?) receipts,(SELECT count(*) FROM audit_events WHERE command_id=?) audits,(SELECT count(*) FROM outbox_events WHERE command_id=?) outbox,(SELECT count(*) FROM staff_notifications WHERE id=?) notices').bind(id,command,command,command,`crm-activity-notice-${command}`).first<Record<string,unknown>>();
  expect(Object.fromEntries(Object.entries(effects!).map(([k,v])=>[k,Number(v)]))).toEqual({events:1,receipts:0,audits:0,outbox:0,notices:0});
 }
 const command=key(),valid={...base,reason:'Reviewed handoff'},result=await change({key:command,id,actorId:f.head.userId,manager:true,...valid});
 expect(await change({key:command,id,actorId:f.head.userId,manager:true,...valid})).toEqual(result);
 await expect(change({key:command,id,actorId:f.head.userId,manager:true,...valid,reason:'Bad\ud800reason'})).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
 const history=await crmActivityWorkspace(f.head,leadId);expect(history.items[0]).toMatchObject({id,owner_id:f.head.userId,version:2});expect(history.history.filter(x=>x.activity_id===id)).toHaveLength(2);
});
