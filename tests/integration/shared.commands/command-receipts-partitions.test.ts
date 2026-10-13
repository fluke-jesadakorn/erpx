import {beforeAll,expect,it} from 'vitest';
import {env,applyMigrations} from "../../support/runtime-env";
const db=env.H001_DB;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
async function differentPartitions(){
 const keys=Array.from({length:80},()=>crypto.randomUUID());
 const rows=(await db.pool.query(`SELECT k,b FROM unnest($1::text[]) k CROSS JOIN generate_series(0,31) b WHERE satisfies_hash_partition('command_receipts'::regclass,32,b,k)`,[keys])).rows;
 const first=rows[0];return [first.k,rows.find(r=>r.b!==first.b)!.k] as string[];
}
const insert=`INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES($1,'test.operation','test.actor','test.hash','test.record','{"amount":107}',1,'{"companyId":"test-company"}')`;
it('commits simultaneous missing-key SERIALIZABLE receipt reads and writes on different hash partitions',async()=>{
 const [left,right]=await differentPartitions(),a=await db.pool.connect(),b=await db.pool.connect();
 try{
  for(const c of[a,b])await c.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
  expect((await a.query('SELECT * FROM command_receipts WHERE command_id=$1',[left])).rows).toEqual([]);
  expect((await b.query('SELECT * FROM command_receipts WHERE command_id=$1',[right])).rows).toEqual([]);
  await a.query(insert,[left]);await a.query('COMMIT');await b.query(insert,[right]);await b.query('COMMIT');
  expect((await db.pool.query('SELECT count(*) n FROM command_receipts WHERE command_id=ANY($1::text[])',[[left,right]])).rows[0].n).toBe(2);
 }finally{for(const c of[a,b]){await c.query('ROLLBACK').catch(()=>{});c.release();}}
});
it('retains one canonical same-key receipt under simultaneous SERIALIZABLE transactions',async()=>{
 const id=crypto.randomUUID(),a=await db.pool.connect(),b=await db.pool.connect();
 try{
  for(const c of[a,b]){await c.query('BEGIN ISOLATION LEVEL SERIALIZABLE');await c.query('SELECT command_id FROM command_receipts WHERE command_id=$1',[id]);}
  await a.query(insert,[id]);await a.query('COMMIT');
  await expect(b.query(insert,[id])).rejects.toMatchObject({code:'40001'});await b.query('ROLLBACK');
  expect((await db.pool.query('SELECT command_id,result_json,authority_json FROM command_receipts WHERE command_id=$1',[id])).rows).toEqual([{command_id:id,result_json:'{"amount":107}',authority_json:'{"companyId":"test-company"}'}]);
  await expect(db.pool.query(insert,[id])).rejects.toMatchObject({code:'23505'});
 }finally{for(const c of[a,b]){await c.query('ROLLBACK').catch(()=>{});c.release();}}
});
it('rolls back a receipt without leaving a canonical replay result',async()=>{
 const id=crypto.randomUUID(),c=await db.pool.connect();
 try{await c.query('BEGIN');await c.query(insert,[id]);await c.query('ROLLBACK');expect((await db.pool.query('SELECT * FROM command_receipts WHERE command_id=$1',[id])).rows).toEqual([]);}
 finally{await c.query('ROLLBACK').catch(()=>{});c.release();}
});
