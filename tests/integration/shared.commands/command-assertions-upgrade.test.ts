import {expect,it} from 'vitest';
import {cp,mkdir,mkdtemp,readdir,rm} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {env} from "../../support/runtime-env";
import {migrateDatabase} from "@erpx/server-runtime/adapters/runtime/migrations";
it('preserves populated original assertions and cache facts across the additive transaction-local guard upgrade and replay',async()=>{
 const root=await mkdtemp(resolve('.runtime/tests/command-assertions-upgrade-'));const db=env.H001_DB;
 try{
  await mkdir(join(root,'holding-write'));
  for(const name of(await readdir('migrations/postgres/holding-write')).filter(n=>n.endsWith('.sql')&&Number(n.slice(0,4))<=1038))await cp(join('migrations/postgres/holding-write',name),join(root,'holding-write',name));
  await migrateDatabase(db,'holding-write',root);
  await db.pool.query("INSERT INTO command_assertions VALUES('original-one','reviewed',1),('original-two','original',1)");
  const before=(await db.pool.query('SELECT * FROM command_assertions ORDER BY command_id,step')).rows;
  const revisions=async()=>(await db.pool.query('SELECT table_name,revision::text revision FROM work_cache_revisions ORDER BY table_name')).rows;
  const originalRevisions=await revisions();
  // Isolate 1039; later IAM migrations intentionally update cache facts.
  await cp('migrations/postgres/holding-write/1039_transaction_local_command_assertions.sql',join(root,'holding-write','1039_transaction_local_command_assertions.sql'));
  await migrateDatabase(db,'holding-write',root);
  expect((await db.pool.query('SELECT * FROM command_assertions ORDER BY command_id,step')).rows).toEqual(before);
  expect((await db.pool.query('SELECT * FROM command_assertion_baselines ORDER BY command_id,step')).rows).toEqual(before);
  expect(await revisions()).toEqual(originalRevisions);
  expect((await db.pool.query("SELECT work_cache_tracks('command_assertion_baselines') tracked")).rows[0].tracked).toBe(false);
  expect((await db.pool.query("SELECT reloptions FROM pg_class WHERE oid='command_assertions'::regclass")).rows[0].reloptions).toContain('security_invoker=true');
  const receipts=(await db.pool.query('SELECT name,checksum FROM erpx_migrations ORDER BY name')).rows;await migrateDatabase(db,'holding-write',root);
  expect((await db.pool.query('SELECT name,checksum FROM erpx_migrations ORDER BY name')).rows).toEqual(receipts);
  expect(await revisions()).toEqual(originalRevisions);expect((await db.pool.query('SELECT * FROM command_assertions ORDER BY command_id,step')).rows).toEqual(before);
 }finally{await rm(root,{recursive:true,force:true});}
},90_000);
