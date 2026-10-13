import {beforeAll,expect,it} from 'vitest';
import {env,applyMigrations} from "../../support/runtime-env";
import {seedExpenseCompany} from "../../../fixtures/expenses/company";
import {defaultConfig} from "../../../src/server/modules/pms/public/rules/pms";
const db=env.H001_DB,ids=[crypto.randomUUID(),crypto.randomUUID()];
let company:string;
beforeAll(async()=>{
 await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);
 const f=await seedExpenseCompany(db,env.H001_FILES);company=f.company;
 for(const [i,id] of ids.entries())await db.pool.query("INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES($1,$2,$3,$4,'2026-10-09','setup',$5,1,$6,1)",[id,company,crypto.randomUUID(),`Indexed mock hotel ${i}`,JSON.stringify(defaultConfig),f.owner.userId]);
 await db.pool.query('ANALYZE pms_properties');
});
it('keeps indexed-read preference local to commit and rollback while preserving SERIALIZABLE and ordinary transaction defaults',async()=>{
 const options={sessionLock:{namespace:'test:indexed-read',key:crypto.randomUUID()},preferIndexedReads:true};
 expect(await db.prepare("SELECT current_setting('enable_seqscan') setting").first('setting')).toBe('on');
 await db.transaction(async tx=>{
  expect(await tx.prepare("SELECT current_setting('enable_seqscan') setting").first('setting')).toBe('off');
  expect(await tx.prepare("SELECT current_setting('transaction_isolation') setting").first('setting')).toBe('serializable');
 },options);
 await expect(db.transaction(async tx=>{
  expect(await tx.prepare("SELECT current_setting('enable_seqscan') setting").first('setting')).toBe('off');throw Error('indexed rollback fixture');
 },options)).rejects.toThrow('indexed rollback fixture');
 await db.transaction(async tx=>{expect(await tx.prepare("SELECT current_setting('enable_seqscan') setting").first('setting')).toBe('on');});
});
it('commits both analyzed small-table Property headers once without false SERIALIZABLE retries or shared predicate locks',async()=>{
 let entered=0,release!:()=>void;const barrier=new Promise<void>(resolve=>{release=resolve;}),attempts=[0,0];
 const before=(await db.pool.query('SELECT id,version FROM pms_properties ORDER BY id')).rows;
 await Promise.all(ids.map((id,i)=>db.transaction(async tx=>{
  attempts[i]++;
  await tx.prepare(`WITH command_context AS MATERIALIZED (SELECT set_config('erpx.pms_write','on',true),set_config('erpx.pms_property',?::text,true))
    SELECT p.* FROM pms_properties p CROSS JOIN command_context WHERE p.id=? AND p.company_id=? FOR UPDATE OF p`).bind(id,id,company).first();
  if(attempts[i]===1){if(++entered===2)release();await barrier;}
  await tx.prepare('UPDATE pms_properties SET version=version+1 WHERE id=?').bind(id).run();
 },{sessionLock:{namespace:'erpx:pms:property',key:id},preferIndexedReads:true})));
 expect(attempts).toEqual([1,1]);
 const after=(await db.pool.query('SELECT id,version FROM pms_properties ORDER BY id')).rows;
 expect(after).toEqual(before.map(row=>({...row,version:Number(row.version)+1})));
},15_000);
