import {expect,it} from 'vitest';
import {cp,mkdir,mkdtemp,readdir,rm} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {env} from './runtime-env';
import {migrateDatabase} from '../src/server/runtime/migrations';
it('preserves populated canonical receipts, referencing evidence, cache revisions, ACL and triggers through hash partition migration and replay',async()=>{
 const root=await mkdtemp(resolve('.runtime/tests/command-receipts-upgrade-')),db=env.H001_DB;
 try{
  await mkdir(join(root,'holding-write'));
  for(const name of(await readdir('migrations/postgres/holding-write')).filter(n=>n.endsWith('.sql')&&Number(n.slice(0,4))<=1039))await cp(join('migrations/postgres/holding-write',name),join(root,'holding-write',name));
  await migrateDatabase(db,'holding-write',root);
  await db.pool.query(`INSERT INTO command_receipts VALUES('receipt-original-one','original.operation','original.actor','original.digest','original.record','{"amount":107,"currency":"THB"}',1720000000000,'{"companyId":"original-company","roleId":"original-role"}'),('receipt-original-two','original.other','original.actor','another.digest','other.record',NULL,1720000000001,NULL)`);
  await db.pool.query('CREATE TABLE receipt_upgrade_evidence(id text PRIMARY KEY,command_id text REFERENCES command_receipts(command_id) ON UPDATE CASCADE ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED)');
  await db.pool.query("INSERT INTO receipt_upgrade_evidence VALUES('original-evidence','receipt-original-one')");
  await db.pool.query(`CREATE FUNCTION receipt_upgrade_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='denied.operation' THEN RAISE EXCEPTION 'receipt guard' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER receipt_upgrade_guard BEFORE INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION receipt_upgrade_guard(); GRANT SELECT ON command_receipts TO PUBLIC; GRANT UPDATE(result_json) ON command_receipts TO PUBLIC; SELECT ensure_work_cache_coverage()`);
  const rows=async(table:string)=>(await db.pool.query(`SELECT * FROM ${table} ORDER BY command_id`)).rows;
  const facts=await rows('command_receipts');
  const foreignKeys=async()=>(await db.pool.query("SELECT conrelid::regclass::text relation,conname,pg_get_constraintdef(oid) definition FROM pg_constraint WHERE confrelid='command_receipts'::regclass AND contype='f' ORDER BY relation,conname")).rows;
  const fks=await foreignKeys();expect(fks.length).toBeGreaterThanOrEqual(12);
  const metadata=async()=>(await db.pool.query("SELECT pg_get_userbyid(relowner) owner,relacl::text acl FROM pg_class WHERE oid='command_receipts'::regclass")).rows;
  const originalMetadata=await metadata();
  const revisions=async()=>(await db.pool.query('SELECT table_name,revision::text revision FROM work_cache_revisions ORDER BY table_name')).rows;
  const originalRevisions=await revisions();
  await migrateDatabase(db,'holding-write');
  expect(await rows('command_receipts')).toEqual(facts);expect(await rows('command_receipts_before_partition')).toEqual(facts);
  expect(await foreignKeys()).toEqual(fks);expect(await metadata()).toEqual(originalMetadata);expect(await revisions()).toEqual(originalRevisions);
  expect((await db.pool.query("SELECT attacl::text acl FROM pg_attribute WHERE attrelid='command_receipts'::regclass AND attname='result_json'")).rows[0].acl).toContain('=w/');
  expect((await db.pool.query('SELECT * FROM receipt_upgrade_evidence')).rows).toEqual([{id:'original-evidence',command_id:'receipt-original-one'}]);
  await expect(db.pool.query("INSERT INTO receipt_upgrade_evidence VALUES('missing-evidence','missing-receipt')")).rejects.toMatchObject({code:'23503'});
  await expect(db.pool.query("DELETE FROM command_receipts WHERE command_id='receipt-original-one'")).rejects.toMatchObject({code:'23001'});
  await expect(db.pool.query("INSERT INTO command_receipts VALUES('receipt-denied','denied.operation','actor','hash','record',NULL,1,NULL)")).rejects.toMatchObject({code:'23514'});
  expect((await db.pool.query("SELECT relkind FROM pg_class WHERE oid='command_receipts'::regclass")).rows[0].relkind).toBe('p');
  expect((await db.pool.query("SELECT count(*) n FROM pg_inherits WHERE inhparent='command_receipts'::regclass")).rows[0].n).toBe(32);
  expect((await db.pool.query("SELECT work_cache_tracks('command_receipts_before_partition') baseline,work_cache_tracks('command_receipts_partition_31') child")).rows[0]).toEqual({baseline:false,child:false});
  const receipts=(await db.pool.query('SELECT name,checksum FROM erpx_migrations ORDER BY name')).rows;
  await migrateDatabase(db,'holding-write');expect((await db.pool.query('SELECT name,checksum FROM erpx_migrations ORDER BY name')).rows).toEqual(receipts);
  expect(await rows('command_receipts')).toEqual(facts);expect(await revisions()).toEqual(originalRevisions);
 }finally{await rm(root,{recursive:true,force:true});}
},90_000);

it('fails closed and leaves original receipts intact when an unsupported row-security policy requires migration review',async()=>{
 const root=await mkdtemp(resolve('.runtime/tests/command-receipts-policy-')),db=env.H002_DB;
 try{
  await mkdir(join(root,'holding-write'));
  for(const name of(await readdir('migrations/postgres/holding-write')).filter(n=>n.endsWith('.sql')&&Number(n.slice(0,4))<=1039))await cp(join('migrations/postgres/holding-write',name),join(root,'holding-write',name));
  await migrateDatabase(db,'holding-write',root);
  await db.pool.query("INSERT INTO command_receipts VALUES('policy-original','test.operation','actor','hash','record',NULL,1,NULL); ALTER TABLE command_receipts ENABLE ROW LEVEL SECURITY; CREATE POLICY receipt_policy ON command_receipts USING(actor_id=current_user)");
  await expect(migrateDatabase(db,'holding-write')).rejects.toThrow('Unsupported command receipt security or dependency');
  expect((await db.pool.query("SELECT relkind,relrowsecurity FROM pg_class WHERE oid='command_receipts'::regclass")).rows[0]).toEqual({relkind:'r',relrowsecurity:true});
  expect((await db.pool.query("SELECT command_id FROM command_receipts WHERE command_id='policy-original'")).rows).toEqual([{command_id:'policy-original'}]);
  expect((await db.pool.query("SELECT to_regclass('command_receipts_before_partition') baseline")).rows[0].baseline).toBeNull();
  expect((await db.pool.query("SELECT count(*) n FROM erpx_migrations WHERE name LIKE '1040%'")).rows[0].n).toBe(0);
 }finally{await rm(root,{recursive:true,force:true});}
},90_000);
