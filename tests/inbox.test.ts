import {beforeAll,beforeEach,expect,it,vi} from "vitest";
import {env,applyMigrations} from "./runtime-env";
import {seedExpenseCompany} from "../fixtures/expenses/company";
import {calendarCommand} from "../src/server/calendar";
import {publishInboxEvent,inboxPage,inboxSummary,changeInboxState,sendNotice} from "../src/server/inbox";
import type {WorkContext} from "../src/server/authorization";
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const db=env.H001_DB,key=()=>crypto.randomUUID();
let f:Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS)});
beforeEach(async()=>{f=await seedExpenseCompany(db,env.H001_FILES)});
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
it("fans out a scoped manager notice once when the worker retries",async()=>{
  const manager=await f.member("notice-manager","people_manager");
  const colleague=await f.member("notice-department-colleague","employee");
  const result=await sendNotice(db,manager,{title:"Department update",body:"Review the schedule.",audience:{kind:"department",id:f.department}},key());
  expect(result.recipientCount).toBeGreaterThan(0);
  const {drainInboxNotices}=await import("../src/server/inbox");
  await drainInboxNotices(db);await drainInboxNotices(db);
  const page=await inboxPage(db,colleague,{view:"all",category:"notice"});
  expect(page.items.filter(item=>item.sourceId===result.id)).toHaveLength(1);
});
