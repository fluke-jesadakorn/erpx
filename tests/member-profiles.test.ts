import { randomUUID } from "node:crypto";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { memberProfileView, saveMemberProfile } from "../src/server/member-profiles";

vi.mock("../src/server/runtime", () => ({
  runtimeEnv: async () => env,
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
}));

beforeAll(async () => {
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS);
});

let first: Awaited<ReturnType<typeof seedExpenseCompany>>;
let second: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeEach(async () => {
  first = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  second = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  for (const owner of [first.owner, second.owner]) {
    await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)')
      .bind(owner.userId, owner.name, `${owner.userId}@example.test`, Date.now(), Date.now()).run();
  }
});

it("shows a colleague in the same Holding without exposing private HR data or another Company's posts", async () => {
  await env.H001_DB.prepare("UPDATE hr_people SET email=?,phone=? WHERE user_id=?")
    .bind("private@example.test", "private phone", second.owner.userId).run();
  await env.H001_DB.prepare("INSERT INTO social_posts(id,author_id,author_name,company_id,body,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
    .bind(randomUUID(), second.owner.userId, second.owner.name, second.company, "Private company update", Date.now(), Date.now()).run();
  const profile = await memberProfileView(first.owner, second.owner.userId);
  expect(profile.name).toBe(second.owner.name);
  expect(profile.own).toBe(false);
  expect(profile.posts).toEqual([]);
  expect(JSON.stringify(profile)).not.toMatch(/private@example\.test|private phone|Private company update/);
});

it("hides a profile immediately after its last Holding assignment is revoked", async () => {
  await env.H001_DB.prepare("UPDATE assignments SET revoked_at=? WHERE user_id=?")
    .bind(Date.now(), second.owner.userId).run();
  await expect(memberProfileView(first.owner, second.owner.userId))
    .rejects.toMatchObject({ status: 404, code: "PROFILE_NOT_FOUND" });
});

it("shows only posts already readable to the viewer within the selected Company", async () => {
  const author = first.claimant.userId;
  await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)')
    .bind(author, "Claimant", `${author}@example.test`, Date.now(), Date.now()).run();
  const visible = randomUUID(), restricted = randomUUID(), now = Date.now();
  await env.H001_DB.prepare("INSERT INTO social_posts(id,author_id,author_name,company_id,body,audience_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
    .bind(visible,author,"Claimant",first.company,"A company update","company",now,now).run();
  await env.H001_DB.prepare("INSERT INTO social_posts(id,author_id,author_name,company_id,body,audience_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
    .bind(restricted,author,"Claimant",first.company,"A private update","participants",now+1,now+1).run();
  await env.H001_DB.prepare("INSERT INTO social_post_participants(post_id,user_id) VALUES(?,?)")
    .bind(restricted,author).run();
  const profile = await memberProfileView(first.owner,author);
  expect(profile.posts.map(post => post.id)).toEqual([visible]);
  expect(JSON.stringify(profile)).not.toContain("A private update");
});

it("paginates posts with equal timestamps without skipping them", async () => {
  const author = first.claimant.userId;
  await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)')
    .bind(author, "Claimant", `${author}@example.test`, Date.now(), Date.now()).run();
  const at = Date.now();
  const ids = Array.from({ length: 10 }, () => randomUUID());
  for (const id of ids) await env.H001_DB.prepare("INSERT INTO social_posts(id,author_id,author_name,company_id,body,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
    .bind(id,author,"Claimant",first.company,"A company update",at,at).run();
  const firstPage = await memberProfileView(first.owner,author);
  const secondPage = await memberProfileView(first.owner,author,firstPage.nextBefore!,firstPage.nextBeforeId!);
  expect(firstPage.posts).toHaveLength(8);
  expect(secondPage.posts).toHaveLength(2);
  expect(new Set([...firstPage.posts,...secondPage.posts].map(post => post.id))).toEqual(new Set(ids));
});

it("saves only the owner's profile with optimistic versioning and idempotency", async () => {
  const input = { bio: "Building our operations", links: [{ label: "Website", url: "https://example.test" }], version: 0 };
  const commandId = randomUUID();
  const saved = await saveMemberProfile(first.owner, input, commandId);
  expect(saved.version).toBe(1);
  expect(await saveMemberProfile(first.owner, input, commandId)).toEqual(saved);
  await expect(saveMemberProfile(first.owner, { ...input, bio: "Stale edit" }, randomUUID()))
    .rejects.toBeDefined();
  const profile = await memberProfileView(first.owner, first.owner.userId);
  expect(profile).toMatchObject({ bio: "Building our operations", links: input.links, version: 1, own: true });
  const colleague = await memberProfileView(first.owner, second.owner.userId);
  expect(colleague.bio).toBe("");
});

import {uploadMemberProfileMedia,removeMemberProfileMedia} from '../src/server/modules/organization/public/profile-media-command';
import {accountTimePreference,changeAccountTimePreference} from '../src/server/modules/identity_access/public/account-time-preference';
vi.mock('../src/server/modules/files/public/attachments/scanner',()=>({scanAttachmentBytes:async()=>({clean:true})}));
const profilePng=Uint8Array.from(Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64'));
it('rolls back profile media and its journal on late outbox failure, compensates storage and retries or replays each command once',async()=>{
 const actor=first.owner,db=env.H001_DB,key=randomUUID(),removeKey=randomUUID(),input={kind:'avatar' as const,contentType:'image/png',contentLength:String(profilePng.byteLength),version:'0',commandId:key},body=async()=>profilePng.buffer;
 const put=vi.spyOn(env.H001_FILES,'put'),del=vi.spyOn(env.H001_FILES,'delete');
 const rows=async(sql:string,bindings:string[])=>(await db.prepare(sql).bind(...bindings).all()).results;
 const snapshot=async()=>JSON.stringify({profile:await rows('SELECT * FROM member_profiles WHERE user_id=?',[actor.userId]),receipt:await rows('SELECT * FROM command_receipts WHERE command_id IN (?,?) ORDER BY command_id',[key,removeKey]),audit:await rows('SELECT * FROM audit_events WHERE command_id IN (?,?) ORDER BY id',[key,removeKey]),outbox:await rows('SELECT * FROM outbox_events WHERE aggregate_id=? ORDER BY id',[actor.userId]),assertions:await rows('SELECT * FROM command_assertions WHERE command_id IN (?,?) ORDER BY command_id,step',[key,removeKey])});
 const fail=async()=>{if(!/^[a-zA-Z0-9_-]+$/.test(actor.userId))throw new Error('Unsafe fixture identifier');await db.prepare(`ALTER TABLE outbox_events ADD CONSTRAINT test_profile_media_final_outbox CHECK(aggregate_id<>'${actor.userId}') NOT VALID`).run();};
 const clear=()=>db.prepare('ALTER TABLE outbox_events DROP CONSTRAINT test_profile_media_final_outbox').run();
 try{
  const before=await snapshot();await fail();try{await expect(uploadMemberProfileMedia(actor,input,body)).rejects.toMatchObject({code:'CONFLICT',cause:{code:'23514',constraint:'test_profile_media_final_outbox'}});expect(await snapshot()).toBe(before);const rejectedKey=put.mock.calls.at(-1)?.[0];if(!rejectedKey)throw new Error('Stored image compensation was not exercised');expect(await env.H001_FILES.head(rejectedKey)).toBeNull();expect(del).toHaveBeenCalledWith(rejectedKey);}finally{await clear();}
  const uploaded=await uploadMemberProfileMedia(actor,input,body);expect(uploaded).toEqual({version:1});const retained=await snapshot();expect(await uploadMemberProfileMedia(actor,input,body)).toEqual(uploaded);expect(await snapshot()).toBe(retained);
  const profile=await db.prepare('SELECT avatar_key,version FROM member_profiles WHERE user_id=?').bind(actor.userId).first<{avatar_key:string;version:number}>();if(!profile)throw new Error('Committed profile image is missing');expect(await env.H001_FILES.head(profile.avatar_key)).not.toBeNull();
  await expect(uploadMemberProfileMedia(actor,{...input,commandId:randomUUID()},body)).rejects.toBeDefined();
  const removal={kind:'avatar' as const,version:'1',commandId:removeKey},deletesBefore=del.mock.calls.length,beforeRemoval=await snapshot();await fail();try{await expect(removeMemberProfileMedia(actor,removal)).rejects.toMatchObject({code:'CONFLICT',cause:{code:'23514',constraint:'test_profile_media_final_outbox'}});expect(await snapshot()).toBe(beforeRemoval);expect(del.mock.calls.length).toBe(deletesBefore);expect(await env.H001_FILES.head(profile.avatar_key)).not.toBeNull();}finally{await clear();}
  const removed=await removeMemberProfileMedia(actor,removal);expect(removed).toEqual({version:2});const final=await snapshot();expect(await removeMemberProfileMedia(actor,removal)).toEqual(removed);expect(await snapshot()).toBe(final);expect(await env.H001_FILES.head(profile.avatar_key)).toBeNull();expect(del.mock.calls.filter(([objectKey])=>objectKey===profile.avatar_key)).toHaveLength(1);
 }finally{put.mockRestore();del.mockRestore();}
});
it('compares account time preference versions atomically on PostgreSQL across competing initial writes and stale changes',async()=>{
 const id=first.owner.userId;expect(await accountTimePreference(id)).toEqual({mode:'system',timeZone:null,hourCycle:'h23',version:0});const input={mode:'manual',timeZone:'Asia/Bangkok',hourCycle:'h12',version:0};
 const competing=await Promise.allSettled([changeAccountTimePreference(id,input),changeAccountTimePreference(id,{...input,timeZone:'UTC'})]);expect(competing.filter(x=>x.status==='fulfilled')).toHaveLength(1);const rejection=competing.find(x=>x.status==='rejected');if(rejection?.status!=='rejected')throw new Error('Competing create was not rejected');expect(rejection.reason).toMatchObject({status:409,code:'TIME_PREFERENCE_CHANGED'});
 expect(await accountTimePreference(id)).toMatchObject({version:1,mode:'manual'});await expect(changeAccountTimePreference(id,input)).rejects.toMatchObject({status:409,code:'TIME_PREFERENCE_CHANGED'});const changed=await changeAccountTimePreference(id,{mode:'system',timeZone:null,hourCycle:'h23',version:1});expect(changed).toEqual({mode:'system',timeZone:null,hourCycle:'h23',version:2});expect(await accountTimePreference(id)).toEqual(changed);
});
