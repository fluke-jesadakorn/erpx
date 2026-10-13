import { afterAll,beforeAll,beforeEach,expect,it,vi } from "vitest";
import { createECDH } from "node:crypto";
import { hashPassword } from "better-auth/crypto";
import { env,applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { publishInboxEvent } from "../../../src/server/modules/collaboration/public/inbox";
import { drainWebPush,pushSettings,quietDelayMs,registerPush,removePushDevice,removePushSession,savePushPreferences,validatePushSubscription } from "../../../src/server/modules/collaboration/public/web-push";
import { leavePushForWorkspaceChange } from "../../../src/lib/shared/push-client";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "../../../src/server/modules/runtime/adapters/runtime/types";
import { createAuth } from "../../../src/server/modules/identity_access/public/auth";
import { DELETE as cleanPushSession } from "../../../src/app/api/v1/holdings/[holdingSlug]/push/session/route";

// Only runtime composition is isolated; normal Better Auth, registered Holding
// resolution, the route, owner cleanup and PostgreSQL effects remain real.
vi.mock("../../../src/server/modules/runtime/adapters/runtime/env",()=>({getRuntime:()=>env}));
const db=env.H001_DB,control=env.CONTROL_DB;
const previous=Object.fromEntries(["ERPX_PUSH_VAPID_PUBLIC_KEY","ERPX_PUSH_VAPID_PRIVATE_KEY","ERPX_PUSH_VAPID_SUBJECT","ERPX_PUSH_DATA_KEY"].map(key=>[key,process.env[key]]));
const subscription={endpoint:"https://fcm.googleapis.com/fcm/send/test-token",keys:{
  p256dh:Buffer.alloc(65,1).toString("base64url"),auth:Buffer.alloc(16,2).toString("base64url")}};
let actor:WorkContext,session:string,fixture:Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async()=>{
  const vapid=createECDH("prime256v1"),fixtureScalar=Buffer.alloc(32);fixtureScalar[31]=1;vapid.setPrivateKey(fixtureScalar);
  process.env.ERPX_PUSH_VAPID_PUBLIC_KEY=vapid.getPublicKey(undefined,"uncompressed").toString("base64url");
  // OpenSSL may omit leading zero bytes; the VAPID configuration requires a 32-byte scalar.
  process.env.ERPX_PUSH_VAPID_PRIVATE_KEY=Buffer.from(vapid.getPrivateKey().toString("hex").padStart(64,"0"),"hex").toString("base64url");
  process.env.ERPX_PUSH_VAPID_SUBJECT="mailto:push@example.test";
  process.env.ERPX_PUSH_DATA_KEY=Buffer.alloc(32,5).toString("base64url");
  await Promise.all([applyMigrations(db,env.STAFF_TEST_MIGRATIONS),applyMigrations(control,env.CONTROL_TEST_MIGRATIONS)]);
});
afterAll(()=>{for(const [key,value] of Object.entries(previous))if(value===undefined)delete process.env[key];else process.env[key]=value;});
beforeEach(async()=>{
  fixture=await seedExpenseCompany(db,env.H001_FILES);
  actor=fixture.claimant;session=crypto.randomUUID();
  actor.authority={sessionId:session,revision:1,assignmentVersion:1};
  const now=Date.now(),until=now+86_400_000,iso=new Date(now).toISOString();
  await control.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)')
    .bind(actor.userId,actor.name,`${actor.userId}@example.test`,now,now).run();
  await control.prepare('INSERT INTO session(id,token,"expiresAt","createdAt","updatedAt","userId") VALUES(?,?,?,?,?,?)')
    .bind(session,crypto.randomUUID(),new Date(until).toISOString(),iso,iso,actor.userId).run();
  await db.prepare("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
    .bind(session,actor.userId,actor.companyId,actor.assignmentId,1,1,until,now).run();
});

it("rejects arbitrary push endpoints and computes Bangkok quiet hours across midnight",()=>{
  expect(()=>validatePushSubscription({...subscription,endpoint:"https://127.0.0.1/secret"})).toThrow();
  const at=new Date("2026-09-26T16:30:00.000Z").getTime(); // 23:30 Bangkok
  expect(quietDelayMs(at,22*60,7*60)).toBe(7.5*60*60_000);
  expect(quietDelayMs(at,8*60,17*60)).toBe(0);
});

it("keeps Push disabled without the encryption key without querying new tables",async()=>{
  const key=process.env.ERPX_PUSH_DATA_KEY;
  delete process.env.ERPX_PUSH_DATA_KEY;
  try {
    const missing={prepare:()=>{throw new Error("Push table should not be read");}} as unknown as SqlDatabase;
    expect((await pushSettings(missing,actor)).available).toBe(false);
  }finally{process.env.ERPX_PUSH_DATA_KEY=key;}
});

it("requires prior workspace Push cleanup before a company switch, and keeps same-company navigation",async()=>{
  const current={kind:"holding" as const,holdingSlug:"test",companyId:"company-a"};
  const cleanup=vi.fn(async()=>{});
  await leavePushForWorkspaceChange(current,{holdingSlug:"test",companyId:"company-a"},cleanup);
  expect(cleanup).not.toHaveBeenCalled();
  await leavePushForWorkspaceChange(current,{holdingSlug:"test",companyId:"company-b"},cleanup);
  expect(cleanup).toHaveBeenCalledExactlyOnceWith("test");
  const failed=vi.fn(async()=>{throw new Error("cleanup failed");});
  await expect(leavePushForWorkspaceChange(current,{holdingSlug:"test",companyId:"company-b"},failed))
    .rejects.toThrow("cleanup failed");
});

it("retires every company subscription for the current account session on workspace change",async()=>{
  await registerPush(db,actor,"http://localhost:3100","/holdings/test/inbox?tab=updates",subscription);
  await registerPush(db,{...actor,companyId:`second-company-${crypto.randomUUID()}`},"http://localhost:3100",
    "/holdings/test/inbox?tab=updates",subscription);
  expect((await db.prepare("SELECT count(*)::int n FROM push_subscriptions WHERE user_id=? AND session_id=?")
    .bind(actor.userId,session).first<{n:number}>())?.n).toBe(2);
  await removePushSession(db,actor.userId,session);
  expect((await db.prepare("SELECT count(*)::int n FROM push_subscriptions WHERE user_id=? AND session_id=?")
    .bind(actor.userId,session).first<{n:number}>())?.n).toBe(0);
  expect((await db.prepare("SELECT count(*)::int n FROM push_preferences WHERE user_id=? AND enabled=true")
    .bind(actor.userId).first<{n:number}>())?.n).toBe(0);
});

it("sends one generic push per existing unread Inbox delivery and stops on session removal",async()=>{
  await registerPush(db,actor,"http://localhost:3100","/holdings/test/inbox?tab=updates",subscription);
  const stored=await db.prepare("SELECT sealed_subscription FROM push_subscriptions WHERE company_id=? AND user_id=?")
    .bind(actor.companyId,actor.userId).first<{sealed_subscription:string}>();
  expect(stored?.sealed_subscription).not.toContain(subscription.endpoint);
  await publishInboxEvent(db,{key:`push:${crypto.randomUUID()}`,companyId:actor.companyId,
    sourceType:"task",sourceId:"test-task",sourceVersion:1,category:"work",
    preview:"Sensitive task text",recipientIds:[actor.userId]});
  const payloads:string[]=[];
  const send=vi.fn(async(_subscription:unknown,payload:string)=>{payloads.push(payload);return {statusCode:201};});
  const first=await drainWebPush(db,control,Date.now()+1000,send);
  expect(first.sent).toBe(1);
  expect(payloads).toHaveLength(1);
  expect(payloads[0]).not.toContain("Sensitive task text");
  expect(JSON.parse(payloads[0])).toMatchObject({path:"/holdings/test/inbox?tab=updates"});
  await drainWebPush(db,control,Date.now()+2000,send);
  expect(send).toHaveBeenCalledTimes(1);
  await removePushDevice(db,actor,subscription.endpoint);
  expect((await db.prepare("SELECT count(*)::int n FROM push_subscriptions WHERE company_id=? AND user_id=?")
    .bind(actor.companyId,actor.userId).first<{n:number}>())?.n).toBe(0);
});

it("keeps pending work and its cursor when the same session registers its endpoint again",async()=>{
  await registerPush(db,actor,"http://localhost:3100","/holdings/test/inbox?tab=updates",subscription);
  await publishInboxEvent(db,{key:`push:${crypto.randomUUID()}`,companyId:actor.companyId,
    sourceType:"task",sourceId:"pending-task",sourceVersion:1,category:"work",
    preview:"Pending before registration retry",recipientIds:[actor.userId]});
  const delivery=await db.prepare("SELECT id FROM inbox_deliveries WHERE company_id=? AND user_id=? ORDER BY delivered_at DESC LIMIT 1")
    .bind(actor.companyId,actor.userId).first<{id:string}>();
  const stored=await db.prepare("SELECT id,cursor_at FROM push_subscriptions WHERE company_id=? AND user_id=?")
    .bind(actor.companyId,actor.userId).first<{id:string;cursor_at:number}>();
  const cursorAt=Number(stored!.cursor_at)-1,cursorId="previous-inbox-row";
  await db.prepare("UPDATE push_subscriptions SET cursor_at=?,cursor_id=? WHERE id=?")
    .bind(cursorAt,cursorId,stored!.id).run();
  await db.prepare("INSERT INTO push_delivery_jobs(id,company_id,inbox_delivery_id,subscription_id,next_attempt_at,created_at) VALUES(?,?,?,?,?,?)")
    .bind(crypto.randomUUID(),actor.companyId,delivery!.id,stored!.id,Date.now(),Date.now()).run();
  await registerPush(db,actor,"http://localhost:3100","/holdings/test/inbox?tab=updates",subscription);
  expect(await db.prepare("SELECT id,cursor_at,cursor_id FROM push_subscriptions WHERE id=?")
    .bind(stored!.id).first()).toMatchObject({id:stored!.id,cursor_at:cursorAt,cursor_id:cursorId});
  expect((await db.prepare("SELECT count(*)::int n FROM push_delivery_jobs WHERE subscription_id=? AND status='pending'")
    .bind(stored!.id).first<{n:number}>())?.n).toBe(1);
  const send=vi.fn(async()=>({statusCode:201}));
  expect((await drainWebPush(db,control,Date.now()+1000,send)).sent).toBe(1);
  expect(send).toHaveBeenCalledTimes(1);
});

it("drops a device when its active work context is revoked before delivery",async()=>{
  await registerPush(db,actor,"http://localhost:3100","/holdings/test/inbox?tab=updates",subscription);
  await publishInboxEvent(db,{key:`push:${crypto.randomUUID()}`,companyId:actor.companyId,
    sourceType:"task",sourceId:"test-task",sourceVersion:1,category:"work",
    preview:"Do not send",recipientIds:[actor.userId]});
  await db.prepare("DELETE FROM active_work_contexts WHERE session_id=? AND company_id=?")
    .bind(session,actor.companyId).run();
  const send=vi.fn(async()=>({statusCode:201}));
  await drainWebPush(db,control,Date.now()+1000,send);
  expect(send).not.toHaveBeenCalled();
  expect((await db.prepare("SELECT count(*)::int n FROM push_subscriptions WHERE session_id=?")
    .bind(session).first<{n:number}>())?.n).toBe(0);
});

it("transfers an endpoint to the new account without carrying over queued jobs",async()=>{
  await registerPush(db,actor,"http://localhost:3100","/holdings/test/inbox?tab=updates",subscription);
  await publishInboxEvent(db,{key:`push:${crypto.randomUUID()}`,companyId:actor.companyId,
    sourceType:"task",sourceId:"old-task",sourceVersion:1,category:"work",
    preview:"Old account",recipientIds:[actor.userId]});
  const firstDelivery=await db.prepare("SELECT id FROM inbox_deliveries WHERE company_id=? AND user_id=? ORDER BY delivered_at DESC LIMIT 1")
    .bind(actor.companyId,actor.userId).first<{id:string}>();
  const firstSubscription=await db.prepare("SELECT id FROM push_subscriptions WHERE company_id=? AND user_id=?")
    .bind(actor.companyId,actor.userId).first<{id:string}>();
  await db.prepare("INSERT INTO push_delivery_jobs(id,company_id,inbox_delivery_id,subscription_id,next_attempt_at,created_at) VALUES(?,?,?,?,?,?)")
    .bind(crypto.randomUUID(),actor.companyId,firstDelivery!.id,firstSubscription!.id,Date.now(),Date.now()).run();
  const next=await fixture.member("push-second-account"),nextSession=crypto.randomUUID();
  next.authority={sessionId:nextSession,revision:1,assignmentVersion:1};
  const now=Date.now(),iso=new Date(now).toISOString();
  await control.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)')
    .bind(next.userId,next.name,`${next.userId}@example.test`,now,now).run();
  await control.prepare('INSERT INTO session(id,token,"expiresAt","createdAt","updatedAt","userId") VALUES(?,?,?,?,?,?)')
    .bind(nextSession,crypto.randomUUID(),new Date(now+86_400_000).toISOString(),iso,iso,next.userId).run();
  await db.prepare("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
    .bind(nextSession,next.userId,next.companyId,next.assignmentId,1,1,now+86_400_000,now).run();
  await registerPush(db,next,"http://localhost:3100","/holdings/test/inbox?tab=updates",subscription);
  expect((await db.prepare("SELECT count(*)::int n FROM push_delivery_jobs WHERE company_id=?")
    .bind(actor.companyId).first<{n:number}>())?.n).toBe(0);
  expect((await db.prepare("SELECT user_id FROM push_subscriptions WHERE company_id=?")
    .bind(actor.companyId).first<{user_id:string}>())?.user_id).toBe(next.userId);
  await publishInboxEvent(db,{key:`push:${crypto.randomUUID()}`,companyId:next.companyId,
    sourceType:"task",sourceId:"new-task",sourceVersion:1,category:"work",
    preview:"New account",recipientIds:[next.userId]});
  const send=vi.fn(async()=>({statusCode:201}));
  expect((await drainWebPush(db,control,Date.now()+1000,send)).sent).toBe(1);
  expect(send).toHaveBeenCalledTimes(1);
});

it("honors a category turned off before an already-queued push is sent",async()=>{
  await registerPush(db,actor,"http://localhost:3100","/holdings/test/inbox?tab=updates",subscription);
  await publishInboxEvent(db,{key:`push:${crypto.randomUUID()}`,companyId:actor.companyId,
    sourceType:"task",sourceId:"queued-task",sourceVersion:1,category:"work",
    preview:"Queued",recipientIds:[actor.userId]});
  const delivery=await db.prepare("SELECT id FROM inbox_deliveries WHERE company_id=? AND user_id=? ORDER BY delivered_at DESC LIMIT 1")
    .bind(actor.companyId,actor.userId).first<{id:string}>();
  const stored=await db.prepare("SELECT id FROM push_subscriptions WHERE company_id=? AND user_id=?")
    .bind(actor.companyId,actor.userId).first<{id:string}>();
  await db.prepare("INSERT INTO push_delivery_jobs(id,company_id,inbox_delivery_id,subscription_id,next_attempt_at,created_at) VALUES(?,?,?,?,?,?)")
    .bind(crypto.randomUUID(),actor.companyId,delivery!.id,stored!.id,Date.now(),Date.now()).run();
  await savePushPreferences(db,actor,{work:false,mention:true,system:true,calendar:false,notice:false,
    quietStartMinute:null,quietEndMinute:null});
  const send=vi.fn(async()=>({statusCode:201}));
  expect((await drainWebPush(db,control,Date.now()+1000,send)).sent).toBe(0);
  expect(send).not.toHaveBeenCalled();
});

it("retries a temporary provider failure without another Inbox event",async()=>{
  await registerPush(db,actor,"http://localhost:3100","/holdings/test/inbox?tab=updates",subscription);
  await publishInboxEvent(db,{key:`push:${crypto.randomUUID()}`,companyId:actor.companyId,
    sourceType:"task",sourceId:"retry-task",sourceVersion:1,category:"work",
    preview:"Retry",recipientIds:[actor.userId]});
  const failure=vi.fn(async()=>{throw Object.assign(new Error("provider unavailable"),{statusCode:503});});
  expect((await drainWebPush(db,control,Date.now()+1000,failure)).sent).toBe(0);
  const job=await db.prepare("SELECT status,attempts,next_attempt_at FROM push_delivery_jobs WHERE company_id=?")
    .bind(actor.companyId).first<{status:string;attempts:number;next_attempt_at:number}>();
  expect(job).toMatchObject({status:"pending",attempts:1});
  const success=vi.fn(async()=>({statusCode:201}));
  expect((await drainWebPush(db,control,job!.next_attempt_at+1,success)).sent).toBe(1);
  expect(success).toHaveBeenCalledTimes(1);
  expect((await db.prepare("SELECT count(*)::int n FROM push_delivery_jobs WHERE company_id=?")
    .bind(actor.companyId).first<{n:number}>())?.n).toBe(1);
});

const pushOrigin="http://localhost:3100";
async function signedInPushSession() {
  const now=Date.now(),email=`${actor.userId}@example.test`,password="Fictional-push-password-2026!";
  await db.prepare("UPDATE assignments SET scope_kind='department',scope_node_id=?,branch_id=? WHERE user_id=?")
    .bind(fixture.department,fixture.branch,actor.userId).run();
  expect((await db.prepare("SELECT count(*)::int n FROM assignments WHERE user_id=? AND scope_kind='holding' AND revoked_at IS NULL")
    .bind(actor.userId).first<{n:number}>())?.n).toBe(0);
  expect((await db.prepare("SELECT count(*)::int n FROM assignments WHERE user_id=? AND scope_kind='department' AND scope_node_id=? AND revoked_at IS NULL")
    .bind(actor.userId,fixture.department).first<{n:number}>())?.n).toBe(1);
  await control.prepare('INSERT INTO account(id,"accountId","providerId","userId",password,"createdAt","updatedAt") VALUES(?,?,\'credential\',?,?,?,?)')
    .bind(crypto.randomUUID(),actor.userId,actor.userId,await hashPassword(password),now,now).run();
  const auth=await createAuth(pushOrigin);
  const login=await auth.handler(new Request(`${pushOrigin}/api/auth/sign-in/email`,{
    method:"POST",headers:{"Content-Type":"application/json",Origin:pushOrigin,Host:"localhost:3100"},
    body:JSON.stringify({email,password}),
  }));
  expect(login.status).toBe(200);
  const cookie=login.headers.getSetCookie().map(value=>value.split(";")[0]).join("; ");
  expect(cookie).toContain("better-auth.session_token=");
  expect(cookie).not.toContain("erpx_company_");
  const authenticated=await auth.api.getSession({headers:new Headers({Cookie:cookie})});
  expect(authenticated?.user.id).toBe(actor.userId);
  if(!authenticated)throw Error("Normal sign-in did not establish its session");
  return {cookie,id:authenticated.session.id};
}
async function registeredPushHolding() {
  const slug=`push-cleanup-${crypto.randomUUID()}`;
  await control.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h001',?,'Fictional Push holding','H001_DB','H001_READ_DB','H001_FILES','active') ON CONFLICT(holding_id) DO UPDATE SET slug=excluded.slug,status='active'")
    .bind(slug).run();
  return slug;
}
function pushCleanupRequest(slug:string,cookie?:string,origin:string|null=pushOrigin,body?:unknown) {
  const headers=new Headers({Host:"localhost:3100"});
  if(cookie)headers.set("Cookie",cookie);
  if(origin!==null)headers.set("Origin",origin);
  if(body!==undefined)headers.set("Content-Type","application/json");
  return cleanPushSession(new Request(`${pushOrigin}/api/v1/holdings/${slug}/push/session`,{
    method:"DELETE",headers,...(body!==undefined?{body:JSON.stringify(body)}:{}),
  }),{params:Promise.resolve({holdingSlug:slug})});
}

it("uses same-origin and normal session authentication for Push cleanup before workspace selection",async()=>{
  const slug=await registeredPushHolding();
  for(const origin of [null,"https://unrelated.example"]) {
    const denied=await pushCleanupRequest(slug,undefined,origin);
    expect(denied.status).toBe(403);
    await expect(denied.json()).resolves.toEqual({error:{code:"ORIGIN_FORBIDDEN",message:"Use this workspace's own origin."}});
  }
  const signedOut=await pushCleanupRequest(slug);
  expect(signedOut.status).toBe(401);
  await expect(signedOut.json()).resolves.toEqual({error:{code:"UNAUTHENTICATED",message:"Sign in to continue."}});
  const authenticated=await signedInPushSession();
  const cleaned=await pushCleanupRequest(slug,authenticated.cookie);
  expect(cleaned.status).toBe(200);
  expect(cleaned.headers.get("cache-control")).toBe("private, no-store");
  await expect(cleaned.json()).resolves.toEqual({data:{removed:true}});
});

it("refuses unknown, inactive and unregistered Holdings without changing authenticated Push subscriptions",async()=>{
  const slug=await registeredPushHolding(),authenticated=await signedInPushSession();
  const current={...actor,authority:{sessionId:authenticated.id,revision:1,assignmentVersion:1}};
  await registerPush(db,current,pushOrigin,"/holdings/test/inbox?tab=updates",subscription);
  const stored=await db.prepare("SELECT * FROM push_subscriptions WHERE user_id=? AND session_id=? ORDER BY id")
    .bind(actor.userId,authenticated.id).all();
  await control.prepare("UPDATE holding_bindings SET status='suspended' WHERE holding_id='h001'").run();
  await control.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h999',?,'Unregistered fictional holding','H999_DB','H999_READ_DB','H999_FILES','active')")
    .bind(`unregistered-${slug}`).run();
  for(const requested of [`missing-${slug}`,slug,`unregistered-${slug}`]) {
    const denied=await pushCleanupRequest(requested,authenticated.cookie);
    expect(denied.status).toBe(404);
    await expect(denied.json()).resolves.toEqual({error:{code:"NOT_FOUND",message:"Workspace not found."}});
  }
  expect((await db.prepare("SELECT * FROM push_subscriptions WHERE user_id=? AND session_id=? ORDER BY id")
    .bind(actor.userId,authenticated.id).all()).results).toEqual(stored.results);
});

it("cleans only the normally authenticated user and session across Companies while preserving other sessions, users and jobs",async()=>{
  const slug=await registeredPushHolding(),authenticated=await signedInPushSession();
  const current={...actor,authority:{sessionId:authenticated.id,revision:1,assignmentVersion:1}},secondCompany=`second-cleanup-company-${crypto.randomUUID()}`;
  const foreign=fixture.manager,foreignSession=crypto.randomUUID(),now=Date.now(),iso=new Date(now).toISOString();
  await control.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)')
    .bind(foreign.userId,foreign.name,`${foreign.userId}@example.test`,now,now).run();
  await control.prepare('INSERT INTO session(id,token,"expiresAt","createdAt","updatedAt","userId") VALUES(?,?,?,?,?,?)')
    .bind(foreignSession,crypto.randomUUID(),new Date(now+86400000).toISOString(),iso,iso,foreign.userId).run();
  const otherUser={...foreign,authority:{sessionId:foreignSession,revision:1,assignmentVersion:1}};
  await registerPush(db,current,pushOrigin,"/holdings/test/inbox?tab=updates",subscription);
  await registerPush(db,{...current,companyId:secondCompany},pushOrigin,"/holdings/test/inbox?tab=updates",subscription);
  await registerPush(db,actor,pushOrigin,"/holdings/test/inbox?tab=updates",{...subscription,endpoint:`${subscription.endpoint}-other-session`});
  await registerPush(db,otherUser,pushOrigin,"/holdings/test/inbox?tab=updates",{...subscription,endpoint:`${subscription.endpoint}-other-user`});
  await publishInboxEvent(db,{key:`session-cleanup:${crypto.randomUUID()}`,companyId:actor.companyId,sourceType:"task",sourceId:"fictional-session-cleanup",sourceVersion:1,category:"work",preview:"Fictional Push cleanup proof",recipientIds:[actor.userId,foreign.userId]});
  const subscriptions=(await db.prepare("SELECT id,user_id,session_id FROM push_subscriptions WHERE company_id=? AND user_id IN (?,?) ORDER BY id")
    .bind(actor.companyId,actor.userId,foreign.userId).all<{id:string;user_id:string;session_id:string}>()).results;
  expect(subscriptions).toHaveLength(3);
  for(const stored of subscriptions) {
    const delivery=await db.prepare("SELECT id FROM inbox_deliveries WHERE company_id=? AND user_id=? ORDER BY delivered_at DESC,id DESC LIMIT 1")
      .bind(actor.companyId,stored.user_id).first<{id:string}>();
    expect(delivery?.id).toBeTruthy();
    await db.prepare("INSERT INTO push_delivery_jobs(id,company_id,inbox_delivery_id,subscription_id,next_attempt_at,created_at) VALUES(?,?,?,?,?,?)")
      .bind(crypto.randomUUID(),actor.companyId,delivery!.id,stored.id,now,now).run();
  }
  const preserved=(await db.prepare("SELECT * FROM push_subscriptions WHERE company_id=? AND (user_id=? AND session_id=? OR user_id=?) ORDER BY id")
    .bind(actor.companyId,actor.userId,session,foreign.userId).all()).results;
  const preservedJobs=(await db.prepare("SELECT j.* FROM push_delivery_jobs j JOIN push_subscriptions s ON s.id=j.subscription_id WHERE s.company_id=? AND (s.user_id=? AND s.session_id=? OR s.user_id=?) ORDER BY j.id")
    .bind(actor.companyId,actor.userId,session,foreign.userId).all()).results;
  const preferences=(await db.prepare("SELECT * FROM push_preferences WHERE company_id=? AND user_id IN (?,?) ORDER BY user_id")
    .bind(actor.companyId,actor.userId,foreign.userId).all()).results;
  expect(preserved).toHaveLength(2);expect(preservedJobs).toHaveLength(2);
  // Request content is untrusted: even explicit foreign IDs cannot select its owner.
  const cleaned=await pushCleanupRequest(slug,authenticated.cookie,pushOrigin,{userId:foreign.userId,sessionId:foreignSession});
  expect(cleaned.status).toBe(200);
  await expect(cleaned.json()).resolves.toEqual({data:{removed:true}});
  expect((await db.prepare("SELECT count(*)::int n FROM push_subscriptions WHERE user_id=? AND session_id=?")
    .bind(actor.userId,authenticated.id).first<{n:number}>())?.n).toBe(0);
  const currentSubscription=subscriptions.find(s=>s.session_id===authenticated.id)!;
  expect((await db.prepare("SELECT count(*)::int n FROM push_delivery_jobs WHERE subscription_id=?")
    .bind(currentSubscription.id).first<{n:number}>())?.n).toBe(0);
  expect((await db.prepare("SELECT * FROM push_subscriptions WHERE company_id=? AND (user_id=? AND session_id=? OR user_id=?) ORDER BY id")
    .bind(actor.companyId,actor.userId,session,foreign.userId).all()).results).toEqual(preserved);
  expect((await db.prepare("SELECT j.* FROM push_delivery_jobs j JOIN push_subscriptions s ON s.id=j.subscription_id WHERE s.company_id=? AND (s.user_id=? AND s.session_id=? OR s.user_id=?) ORDER BY j.id")
    .bind(actor.companyId,actor.userId,session,foreign.userId).all()).results).toEqual(preservedJobs);
  expect((await db.prepare("SELECT * FROM push_preferences WHERE company_id=? AND user_id IN (?,?) ORDER BY user_id")
    .bind(actor.companyId,actor.userId,foreign.userId).all()).results).toEqual(preferences);
  expect((await db.prepare("SELECT enabled FROM push_preferences WHERE company_id=? AND user_id=?")
    .bind(secondCompany,actor.userId).first<{enabled:boolean}>())?.enabled).toBe(false);
  const repeated=await pushCleanupRequest(slug,authenticated.cookie);
  expect(repeated.status).toBe(200);
  await expect(repeated.json()).resolves.toEqual({data:{removed:true}});
  expect((await db.prepare("SELECT * FROM push_subscriptions WHERE company_id=? AND (user_id=? AND session_id=? OR user_id=?) ORDER BY id")
    .bind(actor.companyId,actor.userId,session,foreign.userId).all()).results).toEqual(preserved);
});
