import {beforeAll,expect,it} from 'vitest';
import {env,applyMigrations} from './runtime-env';
const db=env.H001_DB;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
const key=()=>crypto.randomUUID();

it('commits independent SERIALIZABLE command guards after both missing-key clears without a shared assertion write',async()=>{
 const a=await db.pool.connect(),b=await db.pool.connect(),left=key(),right=key();
 try{
  await a.query('BEGIN ISOLATION LEVEL SERIALIZABLE');await b.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
  await a.query('DELETE FROM command_assertions WHERE command_id=$1',[left]);await b.query('DELETE FROM command_assertions WHERE command_id=$1',[right]);
  await a.query("INSERT INTO command_assertions VALUES($1,'authorized',1)",[left]);await a.query('DELETE FROM command_assertions WHERE command_id=$1',[left]);await a.query('COMMIT');
  await b.query("INSERT INTO command_assertions VALUES($1,'authorized',1)",[right]);await b.query('DELETE FROM command_assertions WHERE command_id=$1',[right]);await b.query('COMMIT');
  expect((await db.pool.query('SELECT count(*) n FROM command_assertions WHERE command_id=ANY($1::text[])',[[left,right]])).rows[0].n).toBe(0);
 }finally{for(const c of[a,b]){await c.query('ROLLBACK').catch(()=>{});c.release();}}
});

it('rejects real false predicates, duplicate steps and null fields with the original SQL constraints',async()=>{
 const c=await db.pool.connect(),id=key();
 try{
  await c.query('BEGIN');await c.query("INSERT INTO command_assertions VALUES($1,'valid',1)",[id]);
  for(const [sql,args,code,constraint] of[
   ["INSERT INTO command_assertions VALUES($1,'denied',CASE WHEN false THEN 1 ELSE 0 END)",[id],'23514','command_assertions_ok_check'],
   ["INSERT INTO command_assertions VALUES($1,'valid',1)",[id],'23505','command_assertions_pkey'],
   ["INSERT INTO command_assertions VALUES(NULL,'null',1)",[],'23502',null],
   ["UPDATE command_assertions SET ok=0 WHERE command_id=$1",[id],'23514','command_assertions_ok_check'],
  ] as const){
   await c.query('SAVEPOINT guard_probe');let error:unknown;
   try{await c.query(sql,[...args]);}catch(caught){error=caught;}
   expect(error).toMatchObject({code,...constraint?{constraint}:{}});
   await c.query('ROLLBACK TO SAVEPOINT guard_probe');await c.query('RELEASE SAVEPOINT guard_probe');
   expect((await c.query('SELECT step,ok FROM command_assertions WHERE command_id=$1',[id])).rows).toEqual([{step:'valid',ok:1}]);
  }
  await c.query('ROLLBACK');
 }finally{await c.query('ROLLBACK').catch(()=>{});c.release();}
});

it('keeps multi-row guards visible only to their transaction and clears pooled state on commit and rollback',async()=>{
 const a=await db.pool.connect(),b=await db.pool.connect(),id=key();
 try{
  for(const end of['COMMIT','ROLLBACK']){
   await a.query('BEGIN');await a.query("INSERT INTO command_assertions VALUES($1,'left',1),($1,'right',1)",[id]);
   expect((await a.query('SELECT count(*) n FROM command_assertions WHERE command_id=$1',[id])).rows[0].n).toBe(2);
   expect((await b.query('SELECT count(*) n FROM command_assertions WHERE command_id=$1',[id])).rows[0].n).toBe(0);
   await a.query(end);
   expect((await a.query('SELECT count(*) n FROM command_assertions WHERE command_id=$1',[id])).rows[0].n).toBe(0);
   expect((await a.query("SELECT NULLIF(current_setting('erpx.command_assertions',true),'') value")).rows[0].value).toBeNull();
  }
 }finally{for(const c of[a,b]){await c.query('ROLLBACK').catch(()=>{});c.release();}}
});

it('restores guards after a savepoint rollback and preserves update uniqueness',async()=>{
 const c=await db.pool.connect(),id=key();
 try{
  await c.query('BEGIN');await c.query("INSERT INTO command_assertions VALUES($1,'first',1),($1,'second',1)",[id]);
  await c.query('SAVEPOINT preserved_guard');await c.query("DELETE FROM command_assertions WHERE command_id=$1 AND step='first'",[id]);await c.query('ROLLBACK TO SAVEPOINT preserved_guard');
  expect((await c.query('SELECT step FROM command_assertions WHERE command_id=$1 ORDER BY step',[id])).rows).toEqual([{step:'first'},{step:'second'}]);
  await expect(c.query("UPDATE command_assertions SET step='second' WHERE command_id=$1 AND step='first'",[id])).rejects.toMatchObject({code:'23505',constraint:'command_assertions_pkey'});
  await c.query('ROLLBACK');
 }finally{await c.query('ROLLBACK').catch(()=>{});c.release();}
});

it('clears only the selected command while retaining other pending guards in the same transaction',async()=>{
 const c=await db.pool.connect(),left=key(),right=key();
 try{
  await c.query('BEGIN');await c.query("INSERT INTO command_assertions VALUES($1,'first',1),($1,'second',1),($2,'other',1)",[left,right]);
  await c.query('DELETE FROM command_assertions WHERE command_id=$1',[left]);
  expect((await c.query('SELECT command_id,step FROM command_assertions')).rows).toEqual([{command_id:right,step:'other'}]);await c.query('ROLLBACK');
 }finally{await c.query('ROLLBACK').catch(()=>{});c.release();}
});

it('retains legacy rows and their administrative update and deletion while preventing collisions with local guards',async()=>{
 const c=await db.pool.connect(),legacy=key(),local=key();
 try{
  await c.query("INSERT INTO command_assertion_baselines VALUES($1,'legacy',1)",[legacy]);
  expect((await db.pool.query('SELECT step FROM command_assertions WHERE command_id=$1',[legacy])).rows).toEqual([{step:'legacy'}]);
  await c.query('BEGIN');await c.query("INSERT INTO command_assertions VALUES($1,'local',1)",[local]);
  await expect(c.query("UPDATE command_assertions SET command_id=$1,step='local' WHERE command_id=$2",[local,legacy])).rejects.toMatchObject({code:'23505',constraint:'command_assertions_pkey'});await c.query('ROLLBACK');
  await c.query("UPDATE command_assertions SET step='updated' WHERE command_id=$1",[legacy]);
  expect((await db.pool.query('SELECT step FROM command_assertions WHERE command_id=$1',[legacy])).rows).toEqual([{step:'updated'}]);
  await c.query('DELETE FROM command_assertions WHERE command_id=$1',[legacy]);expect((await db.pool.query('SELECT command_id FROM command_assertions WHERE command_id=$1',[legacy])).rows).toEqual([]);
 }finally{await c.query('ROLLBACK').catch(()=>{});await c.query('DELETE FROM command_assertion_baselines WHERE command_id=$1',[legacy]);c.release();}
});

it('fails closed on malformed transaction-local state and restores a reusable connection after rollback',async()=>{
 const c=await db.pool.connect();
 try{
  await c.query('BEGIN');await c.query("SELECT set_config('erpx.command_assertions','{broken',true)");
  await expect(c.query('SELECT * FROM command_assertions')).rejects.toMatchObject({code:'22P02'});await c.query('ROLLBACK');
  expect((await c.query('SELECT count(*) n FROM command_assertions')).rows[0].n).toBe(0);
 }finally{await c.query('ROLLBACK').catch(()=>{});c.release();}
});
