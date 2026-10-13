import {beforeAll,expect,it} from 'vitest';
import {env} from "../../support/runtime-env";
const db=env.H001_DB,other=env.H002_DB;
beforeAll(async()=>{await db.exec('CREATE TABLE owned_batch_transport(id integer PRIMARY KEY,value integer NOT NULL)');});
it('rejects every cross-database statement before dispatch even when the owner catches the error and commits',async()=>{
 await db.transaction(async tx=>{
  await expect(tx.batch([tx.prepare('INSERT INTO owned_batch_transport VALUES(1,1)'),other.prepare('SELECT 1')])).rejects.toThrow('CROSS_DATABASE_TRANSACTION');
 });
 expect(await db.prepare('SELECT count(*)::integer n FROM owned_batch_transport WHERE id=1').first('n')).toBe(0);
});
it('validates all batch parameters before dispatch without partial writes when conversion fails',async()=>{
 await db.transaction(async tx=>{
  await expect(tx.batch([tx.prepare('INSERT INTO owned_batch_transport VALUES(2,1)'),tx.prepare('SELECT ?::bigint').bind(Number.MAX_SAFE_INTEGER+1)])).rejects.toThrow('DATABASE_NUMBER_OUT_OF_RANGE');
 });
 expect(await db.prepare('SELECT count(*)::integer n FROM owned_batch_transport WHERE id=2').first('n')).toBe(0);
});
it('runs dependent pipelined statements in order and returns each result under SERIALIZABLE',async()=>{
 await db.transaction(async tx=>{
  expect((tx as unknown as {pool:{pipeline:boolean}}).pool.pipeline).toBe(true);
  expect(await tx.prepare("SELECT current_setting('transaction_isolation') value").first('value')).toBe('serializable');
  const result=await tx.batch([
   tx.prepare('INSERT INTO owned_batch_transport VALUES(3,10) RETURNING value'),
   tx.prepare('UPDATE owned_batch_transport SET value=value+5 WHERE id=3 RETURNING value'),
   tx.prepare('SELECT value FROM owned_batch_transport WHERE id=3'),
  ]);
  expect(result.map(row=>row.results)).toEqual([[{value:10}],[{value:15}],[{value:15}]]);
 },{sessionLock:{namespace:'test:pipelined',key:'dependent'},pipelineBatches:true});
});
it('settles a failed middle statement before rollback, preserves its original error and permits clean connection reuse',async()=>{
 await expect(db.transaction(async tx=>{
  await tx.batch([
   tx.prepare('INSERT INTO owned_batch_transport VALUES(4,10)'),
   tx.prepare('INSERT INTO owned_batch_transport VALUES(4,20)'),
   tx.prepare('INSERT INTO owned_batch_transport VALUES(5,30)'),
  ]);
 },{sessionLock:{namespace:'test:pipelined',key:'failure'},pipelineBatches:true})).rejects.toMatchObject({code:'23505'});
 expect(await db.prepare('SELECT count(*)::integer n FROM owned_batch_transport WHERE id IN (4,5)').first('n')).toBe(0);
 await db.transaction(async tx=>{await tx.prepare('INSERT INTO owned_batch_transport VALUES(6,60)').run();});
 expect(await db.prepare('SELECT value FROM owned_batch_transport WHERE id=6').first('value')).toBe(60);
});
