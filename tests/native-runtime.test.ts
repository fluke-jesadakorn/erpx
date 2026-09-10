import { describe,it,expect,beforeAll } from 'vitest';
import { randomUUID } from 'node:crypto';
import { env,applyMigrations } from './runtime-env';
import { claimJob,runOne } from '../src/server/runtime/jobs';
import { project } from '../src/server/job-projector';

describe('Native PostgreSQL and private file runtime',()=>{
 beforeAll(async()=>{
  await applyMigrations(env.H001_DB,env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.H001_READ_DB,[{name:'baseline',track:'holding-read'}]);
 });
 it('rejects unsafe integers instead of silently changing money or epoch values',async()=>{
  expect(await env.H001_DB.prepare('SELECT ?::bigint amount').bind(Number.MAX_SAFE_INTEGER).first('amount')).toBe(Number.MAX_SAFE_INTEGER);
  await expect(env.H001_DB.prepare("SELECT '9007199254740992'::bigint amount").first()).rejects.toThrow();
  await expect(env.H001_DB.prepare('SELECT ?::bigint amount').bind(Number.MAX_SAFE_INTEGER+1).first()).rejects.toThrow();
 });
 it('publishes exactly one immutable revision under concurrent upload and serves ranges',async()=>{
  const key='revisions/'+randomUUID();
  const results=await Promise.all(['abcdef','uvwxyz'].map(value=>env.H001_FILES.put(key,value,{onlyIf:{etagDoesNotMatch:'*'},httpMetadata:{contentType:'text/plain'},customMetadata:{revision:'1'}})));
  expect(results.filter(Boolean)).toHaveLength(1);
  const body=await env.H001_FILES.get(key);expect(body).toBeTruthy();
  const value=await body!.text();await body!.body.cancel();
  expect((await env.H001_FILES.head(key))?.customMetadata.revision).toBe('1');
  const range=await env.H001_FILES.get(key,{range:{offset:1,length:3}});
  expect(await range!.text()).toBe(value.slice(1,4));await range!.body.cancel();
  const suffix=await env.H001_FILES.get(key,{range:{suffix:2}});
  expect(await suffix!.text()).toBe(value.slice(-2));await suffix!.body.cancel();
  expect(await env.H002_FILES.get(key)).toBeNull();
  await expect(env.H001_FILES.put('../escape','x')).rejects.toThrow('INVALID_OBJECT_KEY');
 });
 it('does not expose an interrupted upload, and a retry can publish it',async()=>{
  const key='partial/'+randomUUID();
  const broken=new ReadableStream<Uint8Array>({start(c){c.enqueue(new Uint8Array([1,2,3]));c.error(new Error('disconnect'));}});
  await expect(env.H001_FILES.put(key,broken,{onlyIf:{etagDoesNotMatch:'*'}})).rejects.toThrow('disconnect');
  expect(await env.H001_FILES.head(key)).toBeNull();
  expect(await env.H001_FILES.put(key,'retry',{onlyIf:{etagDoesNotMatch:'*'}})).toBeTruthy();
 });
 async function event(id:string){
  await env.H001_DB.prepare(`INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)
   VALUES(?,?,'test.updated','test',?,1,?,?)`).bind(id,id,id,JSON.stringify({id,version:1,companyId:'company',branchId:null,departmentId:null}),Date.now()).run();
 }
 it('claims once concurrently and recovers a lease after a worker disappears',async()=>{
  const id=randomUUID();await event(id);const now=Date.now();
  const claims=await Promise.all([claimJob(env.H001_DB,now),claimJob(env.H001_DB,now)]);
  const winner=claims.find(Boolean)!;expect(claims.filter(Boolean)).toHaveLength(1);expect(winner.id).toBe(id);
  const recovered=await claimJob(env.H001_DB,now+90_001);
  expect(recovered?.id).toBe(id);expect(recovered?.lease_token).not.toBe(winner.lease_token);expect(recovered?.attempts).toBe(2);
  await env.H001_DB.prepare('DELETE FROM outbox_events WHERE id=?').bind(id).run();
 });
 it('retries five times then dead-letters while storing only an error code',async()=>{
  const id=randomUUID();await event(id);let executions=0;
  const failure:typeof project=async()=>{executions++;throw new Error('private document secret');};
  for(let i=0;i<6;i++){
   await env.H001_DB.prepare('UPDATE outbox_events SET next_attempt_at=0 WHERE id=?').bind(id).run();
   expect(await runOne(env,'h001',failure)).toBe(true);
  }
  const row=await env.H001_DB.prepare('SELECT attempts,last_error,dead_letter_at FROM outbox_events WHERE id=?').bind(id).first<{attempts:number,last_error:string,dead_letter_at:number}>();
  expect(row?.attempts).toBe(6);expect(row?.last_error).toBe('Error');expect(row?.dead_letter_at).toBeGreaterThan(0);
  expect(await runOne(env,'h001',failure)).toBe(false);expect(executions).toBe(6);
 });
 it('replays a projected event without duplicating its receipt',async()=>{
  const id=randomUUID();await event(id);
  const failAfterCommit:typeof project=async(e,message)=>{await project(e,message);throw new Error('process stopped after projection commit');};
  expect(await runOne(env,'h001',failAfterCommit)).toBe(true);
  await env.H001_DB.prepare('UPDATE outbox_events SET next_attempt_at=0 WHERE id=?').bind(id).run();
  expect(await runOne(env,'h001')).toBe(true);
  expect(await env.H001_READ_DB.prepare('SELECT count(*) n FROM projection_receipts WHERE event_id=?').bind(id).first('n')).toBe(1);
  expect(await env.H001_DB.prepare('SELECT processed_at FROM outbox_events WHERE id=?').bind(id).first('processed_at')).toBeGreaterThan(0);
 });
});
