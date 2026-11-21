import {notificationsPage,listStaffNotifications} from '@erpx/server-collaboration/public/staff-notifications';
import {beforeAll,beforeEach,expect,it,vi} from "vitest";
import {env,applyMigrations} from "../../support/runtime-env";
import {seedExpenseCompany} from "../../../fixtures/expenses/company";
import {calendarCommand} from "@erpx/server-work_projection/public/calendar/index";
import {publishInboxEvent,inboxPage,inboxSummary,changeInboxState,sendNotice,markStaffNotificationRead} from "@erpx/server-collaboration/public/inbox";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const nativeInbox=vi.hoisted(()=>({read:vi.fn(async()=>[] as import("@erpx/server-collaboration/domain/inbox").Candidate[]),state:vi.fn(async(...args:unknown[])=>{void args;return{id:"",readAt:null as number|null,archivedAt:null as number|null};})}));
vi.mock("../../../packages/server-collaboration/src/adapters/native-field-inbox",()=>({nativeFieldInboxCandidates:nativeInbox.read,nativeFieldInboxState:nativeInbox.state}));
const db=env.H001_DB,key=()=>crypto.randomUUID();
let f:Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS)});
beforeEach(async()=>{nativeInbox.read.mockReset();nativeInbox.state.mockReset();nativeInbox.read.mockResolvedValue([]);f=await seedExpenseCompany(db,env.H001_FILES)});
async function appointment(actor:WorkContext){
  const date=new Date(Date.now()+14*86400000).toISOString().slice(0,10);
  return calendarCommand(db,actor,{action:"appointment.save",organizerPersonId:`person-${actor.userId}`,participantIds:[],
    title:"Inbox source",description:"",startLocal:`${date}T09:00`,endLocal:`${date}T10:00`,timezone:"Asia/Bangkok",guests:[]},key());
}
it("pages past inaccessible deliveries and counts only visible unread items",async()=>{
  const source=await appointment(f.claimant);
  await publishInboxEvent(db,{key:`visible:${key()}`,companyId:f.claimant.companyId,sourceType:"calendar_removed",sourceId:source.id,
    sourceVersion:1,category:"calendar",preview:"Meeting changed",recipientIds:[f.claimant.userId]});
  for(let i=0;i<105;i++)await publishInboxEvent(db,{key:`inaccessible:${key()}`,companyId:f.claimant.companyId,sourceType:"unknown_source",sourceId:key(),
    sourceVersion:1,category:"work",preview:"Hidden update",recipientIds:[f.claimant.userId]});
  const page=await inboxPage(db,f.claimant,{view:"unread",limit:10});
  expect(page.items.some(item=>item.message==="Meeting changed")).toBe(true);
  expect(page.items.every(item=>item.message!=="Hidden update")).toBe(true);
  const summary=await inboxSummary(db,f.claimant);
  expect(summary.unread).toBeGreaterThanOrEqual(1);
  expect(summary.categories.work).toBe(0);
  const visible=page.items.find(item=>item.message==="Meeting changed")!;
  await changeInboxState(db,f.claimant,visible.id,{read:true});
  expect((await inboxPage(db,f.claimant,{view:"unread"})).items.some(item=>item.id===visible.id)).toBe(false);
});
it("sends a named notice without exposing recipient read state to the sender",async()=>{
  const colleague=await f.member("inbox-recipient","employee");
  const result=await sendNotice(db,f.claimant,{title:"Shift handoff",body:"Please review tomorrow's plan.",recipients:[colleague.userId]},key());
  expect(result.recipientCount).toBe(1);
  const page=await inboxPage(db,colleague,{view:"all",category:"notice"});
  expect(page.items.some(item=>item.message.includes("Shift handoff"))).toBe(true);
});
it("resolves the current appointment reference while retaining the delivered source identity",async()=>{
  const source=await appointment(f.claimant);
  await publishInboxEvent(db,{key:`appointment:${key()}`,companyId:f.claimant.companyId,sourceType:"calendar_appointment",sourceId:source.id,
    sourceVersion:1,category:"calendar",preview:"Appointment ready",recipientIds:[f.claimant.userId]});
  await db.prepare("UPDATE calendar_appointments SET version=version+1 WHERE id=?").bind(source.id).run();
  const page=await inboxPage(db,f.claimant,{view:"all",category:"calendar"});
  const item=page.items.find(item=>item.message==="Appointment ready")!;
  expect(item).toMatchObject({sourceType:"calendar_appointment",sourceId:source.id,
    source:{kind:"calendar_appointment",id:source.id,version:2,label:"Appointment",href:`/calendar?event=appointment:${encodeURIComponent(source.id)}`}});
  expect(item.href).toBe(item.source?.href);
});
it("keeps the Inbox page available when the selected role cannot read a delivered source",async()=>{
  const source=await appointment(f.claimant);
  await publishInboxEvent(db,{key:`visible:${key()}`,companyId:f.claimant.companyId,sourceType:"calendar_removed",sourceId:source.id,
    sourceVersion:1,category:"calendar",preview:"Visible meeting update",recipientIds:[f.claimant.userId]});
  await publishInboxEvent(db,{key:`restricted:${key()}`,companyId:f.claimant.companyId,sourceType:"quality_hold",sourceId:key(),
    sourceVersion:1,category:"work",preview:"Restricted quality evidence",recipientIds:[f.claimant.userId]});
  const page=await inboxPage(db,{...f.claimant,pageRequest:true},{view:"unread"});
  expect(page.items.some(item=>item.message==="Visible meeting update")).toBe(true);
  expect(page.items.some(item=>item.message==="Restricted quality evidence")).toBe(false);
  expect((await inboxSummary(db,{...f.claimant,pageRequest:true})).categories.work).toBe(0);
});
it("fans out a scoped manager notice once when the worker retries",async()=>{
  const manager=await f.member("notice-manager","people_manager");
  const colleague=await f.member("notice-department-colleague","employee");
  const result=await sendNotice(db,manager,{title:"Department update",body:"Review the schedule.",audience:{kind:"department",id:f.department}},key());
  expect(result.recipientCount).toBeGreaterThan(0);
  const {drainInboxNotices}=await import("@erpx/server-collaboration/public/inbox");
  await drainInboxNotices(db);await drainInboxNotices(db);
  const page=await inboxPage(db,colleague,{view:"all",category:"notice"});
  expect(page.items.filter(item=>item.sourceId===result.id)).toHaveLength(1);
});

it("merges the native Field read model in PostgreSQL tuple order with exact paging and unread totals",async()=>{
  const {companyId,userId}=f.claimant,stamp=Date.now(),legacyId="middle-legacy"+key();
  const {id:sourceId}=await appointment(f.claimant);
  const eventId=await publishInboxEvent(db,{key:key(),companyId,sourceType:"calendar_removed",sourceId,sourceVersion:1,category:"calendar",preview:"Legacy middle",recipientIds:[userId]});
  await db.prepare("UPDATE inbox_deliveries SET id=?,delivered_at=? WHERE event_id=? AND user_id=?").bind(legacyId,stamp,eventId,userId).run();
  const make=(id:string,at:number):import("@erpx/server-collaboration/domain/inbox").Candidate=>({id,event_id:"native-"+id,company_id:companyId,user_id:userId,delivered_at:at,read_at:null,archived_at:null,source_type:"field_service_order",source_id:"native-order",source_version:1,category:"work",preview:"Native "+id,native_field_item:{id,eventId:"native-"+id,category:"work",message:"Native "+id,href:"/field-service/native-order",createdAt:at,readAt:null,archivedAt:null,sourceType:"field_service_order",sourceId:"native-order"}});
  const rows=[make("z".repeat(201),stamp),make("a-native",stamp)];
  nativeInbox.read.mockImplementation(async(...args:unknown[])=>{const query=args[1] as {category?:string;before?:{at:number;id:string};limit:number};return rows.filter(row=>(!query.category||row.category===query.category)&&(!query.before||row.delivered_at<query.before.at||(row.delivered_at===query.before.at&&row.id<query.before.id))).slice(0,query.limit);});
  // Generic Records must likewise never revive Field policy/reads, even when
  // old package enforcement is disabled. Evaluate its real partition on PG.
  const {serviceSourceReadScope,requireServiceWorkspaceRecordRead}=await import('@erpx/server-field_service/public/record-purchases');
  const partition=await serviceSourceReadScope(db,f.claimant,'records'),genericId='historical-generic-'+key();
  const generic=(await db.prepare(`WITH workspace_records AS(SELECT ?::text id,'field_service'::text module_id UNION ALL SELECT ?::text,'projects'::text) SELECT module_id FROM workspace_records WHERE ${partition.where}`).bind(genericId,genericId,...partition.args).all<{module_id:string}>()).results;
  expect(generic).toEqual([{module_id:'projects'}]);
  await expect(requireServiceWorkspaceRecordRead(db,f.claimant,'field_service',genericId)).rejects.toMatchObject({status:503,code:'FIELD_RECORD_SOURCE_NATIVE_UNIT_REQUIRED'});
  await expect(requireServiceWorkspaceRecordRead(db,f.claimant,'projects',genericId)).resolves.toBeUndefined();
  // A delivered physical Field row must never sneak back through the TS partition.
  await publishInboxEvent(db,{key:key(),companyId,sourceType:"field_service_order",sourceId:"unreadable-field",sourceVersion:1,category:"work",preview:"Do not expose legacy Field",recipientIds:[userId]});
  const first=await inboxPage(db,f.claimant,{view:"unread",limit:1});
  expect(first.items.map(item=>item.id)).toEqual([rows[0].id]);expect(first.hasMore).toBe(true);
  const second=await inboxPage(db,f.claimant,{view:"unread",limit:1,cursor:first.nextCursor!});
  expect(second.items.map(item=>item.id)).toEqual([legacyId]);
  const third=await inboxPage(db,f.claimant,{view:"unread",limit:1,cursor:second.nextCursor!});
  expect(third.items.map(item=>item.id)).toEqual([rows[1].id]);expect(third.hasMore).toBe(false);
  expect(await inboxSummary(db,f.claimant)).toMatchObject({unread:3,categories:{work:2,calendar:1}});
  expect((await inboxPage(db,f.claimant,{view:"all",category:"work"})).items.map(item=>item.id)).toEqual(rows.map(row=>row.id));
  const routed=await db.prepare("SELECT d.id,d.read_at FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id WHERE d.company_id=? AND d.user_id=? AND e.source_id='unreadable-field'").bind(companyId,userId).first<{id:string;read_at:number|null}>();expect(routed).not.toBeNull();
  nativeInbox.state.mockResolvedValue({id:routed!.id,readAt:stamp,archivedAt:null});
  expect(await changeInboxState(db,f.claimant,routed!.id,{read:true},'native-inbox-bridge')).toEqual({id:routed!.id,readAt:stamp,archivedAt:null});
  expect(nativeInbox.state).toHaveBeenCalledWith(f.claimant,routed!.id,'{"read":true}','native-inbox-bridge');
  expect((await db.prepare("SELECT read_at FROM inbox_deliveries WHERE id=?").bind(routed!.id).first<{read_at:number|null}>())!.read_at).toBeNull();
  await markStaffNotificationRead(db,f.claimant,routed!.id,'native-staff-bridge',stamp);expect(nativeInbox.state).toHaveBeenLastCalledWith(f.claimant,routed!.id,'{"read":true}','native-staff-bridge');
  expect((await db.prepare("SELECT read_at FROM staff_notifications WHERE id=?").bind(routed!.id).first<{read_at:number|null}>())?.read_at??null).toBeNull();
  nativeInbox.state.mockRejectedValue(Error('Native unavailable'));await expect(changeInboxState(db,f.claimant,routed!.id,{read:true},'native-inbox-unavailable')).rejects.toThrow('Native unavailable');
  expect((await db.prepare("SELECT read_at FROM inbox_deliveries WHERE id=?").bind(routed!.id).first<{read_at:number|null}>())!.read_at).toBeNull();
});

it("retains non-Field Staff acknowledgement time and account/Company isolation",async()=>{
 const own='legacy-staff-'+key(),other='legacy-staff-'+key();
 for(const [id,user] of [[own,f.claimant.userId],[other,f.owner.userId]])await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES(?,?,?,'post','historical-missing-source',1,'Legacy notice',1)").bind(id,f.claimant.companyId,user).run();
 await markStaffNotificationRead(db,f.claimant,own,'legacy-staff-key',123);await markStaffNotificationRead(db,f.claimant,other,'legacy-other-key',123);
 expect((await db.prepare("SELECT read_at FROM staff_notifications WHERE id=?").bind(own).first<{read_at:number|null}>())?.read_at).toBe(123);expect((await db.prepare("SELECT read_at FROM staff_notifications WHERE id=?").bind(other).first<{read_at:number|null}>())?.read_at).toBeNull();expect(nativeInbox.state).not.toHaveBeenCalled();
});

it('preserves Staff native row ordering, bidirectional scoped cursors and the separate top-list tie order',async()=>{
 const stamp=1700000000000,{companyId,userId}=f.claimant;
 const rows:Array<import('@erpx/server-collaboration/domain/inbox').Candidate>=Array.from({length:65},(_,i)=>{const id='native-staff-'+String(i).padStart(3,'0');return{id,event_id:'legacy:'+id,company_id:companyId,user_id:userId,delivered_at:stamp,read_at:null,archived_at:stamp,source_type:'field_service_order',source_id:'native-order',source_version:1,category:'work',preview:'Native Staff '+id,native_field_item:{id,eventId:'legacy:'+id,category:'work',message:'Native Staff '+id,href:'/field-service/native-order',createdAt:stamp,readAt:null,archivedAt:stamp,sourceType:'field_service_order',sourceId:'native-order'}};});
 const queries:import('@erpx/native-sdk/field-service-parts').NativeFieldInboxQuery[]=[];
 nativeInbox.read.mockImplementation(async(...args:unknown[])=>{const q=args[1] as import('@erpx/native-sdk/field-service-parts').NativeFieldInboxQuery;queries.push(q);const order=(a:typeof rows[number],b:typeof rows[number])=>a.id===b.id?0:a.id<b.id?-1:1;let result=[...rows].sort(order);if(q.projection==='staff-list')return result.slice(0,q.limit);if(q.before)result=result.filter(row=>q.reverse?row.id>q.before!.id:row.id<q.before!.id);if(!q.reverse)result.reverse();return result.slice(0,q.limit);});
 const first=await notificationsPage(db,f.claimant,{view:'all'});expect(first.items).toHaveLength(30);expect(first.items[0].id).toBe('native-staff-064');expect(first.items[29].id).toBe('native-staff-035');expect(first.previousCursor).toBeNull();expect(first.hasMore).toBe(true);expect(first.items.every(item=>item.href==='/field-service/native-order'&&!Object.hasOwn(item,'native_target'))).toBe(true);
 const second=await notificationsPage(db,f.claimant,{view:'all',cursor:first.nextCursor!});expect(second.items[0].id).toBe('native-staff-034');expect(second.items[29].id).toBe('native-staff-005');
 const previous=await notificationsPage(db,f.claimant,{view:'all',cursor:second.previousCursor!});expect(previous.items.map(row=>row.id)).toEqual(first.items.map(row=>row.id));expect(queries.some(q=>q.projection==='staff-page'&&q.reverse&&q.before?.key===String(stamp))).toBe(true);
 const third=await notificationsPage(db,f.claimant,{view:'all',cursor:second.nextCursor!});expect(third.items).toHaveLength(5);expect(third.items[0].id).toBe('native-staff-004');expect(third.nextCursor).toBeNull();
 const list=await listStaffNotifications(db,f.claimant);expect(list).toHaveLength(65);expect(list[0].id).toBe('native-staff-000');expect(list[64].id).toBe('native-staff-064');
 const count=queries.length;await expect(notificationsPage(db,f.claimant,{view:'unread',cursor:first.nextCursor!})).rejects.toMatchObject({status:422,code:'INVALID_CURSOR'});expect(queries).toHaveLength(count);
});
