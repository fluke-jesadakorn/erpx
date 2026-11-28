import {expect,it} from 'vitest';
import {cp,mkdir,mkdtemp,readdir,rm,writeFile} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {env} from '../../support/runtime-env';
import {inspectMigrations,migrateDatabase} from '@erpx/platform-runtime-server/public/runtime/migrations';

it.each([
 {track:'control' as const,db:env.CONTROL_DB,addition:'0037_migration_receipts.sql'},
 {track:'control-read' as const,db:env.CONTROL_READ_DB,addition:'0002_migration_receipts.sql'},
])('distinguishes a new $track database from missing receipt metadata and preserves populated upgrade/rollback history',async({track,db,addition})=>{
 const source=resolve('migrations/postgres'),old=await mkdtemp(resolve('.runtime/tests/migration-status-before-'));
 try{
  const files=(await readdir(join(source,track))).filter(name=>name.endsWith('.sql')).sort();
  expect(await inspectMigrations(db,track)).toEqual({track,applied:0,pending:files,latestApplied:null});
  await mkdir(join(old,track));
  for(const name of files.filter(name=>name!==addition))await cp(join(source,track,name),join(old,track,name));
  await migrateDatabase(db,track,old);
  await db.prepare("CREATE TABLE metadata_reader_retained(id text PRIMARY KEY,value text NOT NULL)").run();
  await db.prepare("INSERT INTO metadata_reader_retained VALUES('original','Retained original metadata Ω')").run();
  const receipts=(await db.prepare('SELECT * FROM erpx_migrations ORDER BY name').all()).results;
  await expect(inspectMigrations(db,track)).rejects.toMatchObject({code:'MIGRATION_RECEIPTS_UNAVAILABLE'});
  await migrateDatabase(db,track);
  expect(await inspectMigrations(db,track)).toEqual({track,applied:files.length,pending:[],latestApplied:files.at(-1)});
  expect((await db.prepare('SELECT * FROM erpx_migrations WHERE name<>? ORDER BY name').bind(addition).all()).results).toEqual(receipts);
  expect((await db.prepare('SELECT * FROM metadata_reader_retained').all()).results).toEqual([{id:'original',value:'Retained original metadata Ω'}]);
  const fn=await db.prepare("SELECT prosecdef,proconfig FROM pg_proc WHERE oid='public.erpx_migration_receipts()'::regprocedure").first();
  expect(fn).toEqual({prosecdef:true,proconfig:['search_path=pg_catalog']});
  expect(await db.prepare("SELECT EXISTS(SELECT 1 FROM pg_proc p CROSS JOIN LATERAL aclexplode(COALESCE(p.proacl,acldefault('f',p.proowner))) a WHERE p.oid='public.erpx_migration_receipts()'::regprocedure AND a.grantee=0 AND a.privilege_type='EXECUTE') allowed").first('allowed')).toBe(true);
  expect(await db.prepare("SELECT EXISTS(SELECT 1 FROM pg_class c CROSS JOIN LATERAL aclexplode(COALESCE(c.relacl,acldefault('r',c.relowner))) a WHERE c.oid='public.erpx_migrations'::regclass AND a.grantee=0 AND a.privilege_type='SELECT') allowed").first('allowed')).toBe(false);
  await migrateDatabase(db,track,old);
  expect(await inspectMigrations(db,track)).toEqual({track,applied:files.length,pending:[],latestApplied:files.at(-1)});
  expect((await db.prepare('SELECT * FROM metadata_reader_retained').all()).results).toEqual([{id:'original',value:'Retained original metadata Ω'}]);
 }finally{await rm(old,{recursive:true,force:true});}
},60000);

// Exercise the actual existing migration-owned metadata reader; do not grant
// app access to erpx_migrations or rewrite a migration receipt to fake absence.
// This suite's restricted erpx_test_admin is the migration owner, not a runtime
// app-role login. Native ACL facts below preserve that explicit proof limit.
import {payrollOtherSchemaAvailable} from '@erpx/people-payroll-server/adapters/payroll-other-remittance-sources';
it('reads actual Payroll other-liability schema availability through the migration-owned function before and after1066 without opening the receipt table',async()=>{
 const db=env.H001_DB,source=resolve('migrations/postgres'),old=await mkdtemp(resolve('.runtime/tests/payroll-schema-before-'));
 try{
  const files=(await readdir(join(source,'holding-write'))).filter(name=>name.endsWith('.sql')).sort();
  await mkdir(join(old,'holding-write'));
  for(const name of files.filter(name=>name<'1066_payroll_other_liability_remittances.sql'))await cp(join(source,'holding-write',name),join(old,'holding-write',name));
  await migrateDatabase(db,'holding-write',old);
  const before=(await db.prepare('SELECT * FROM erpx_migrations ORDER BY name').all()).results;
  expect(before.some(receipt=>receipt.name==='1066_payroll_other_liability_remittances.sql')).toBe(false);
  expect(await payrollOtherSchemaAvailable(db)).toBe(false);
  const privileges=async()=>db.prepare(`SELECT p.prosecdef,
   EXISTS(SELECT 1 FROM aclexplode(COALESCE(p.proacl,acldefault('f',p.proowner))) a WHERE a.grantee=0 AND a.privilege_type='EXECUTE') public_execute,
   EXISTS(SELECT 1 FROM pg_class c CROSS JOIN LATERAL aclexplode(COALESCE(c.relacl,acldefault('r',c.relowner))) a WHERE c.oid='public.erpx_migrations'::regclass AND a.grantee=0 AND a.privilege_type='SELECT') public_receipt_read
   FROM pg_proc p WHERE p.oid='public.erpx_migration_receipts()'::regprocedure`).first();
  const originalAccess=await privileges();expect(originalAccess).toEqual({prosecdef:true,public_execute:true,public_receipt_read:false});
  await migrateDatabase(db,'holding-write');
  expect(await payrollOtherSchemaAvailable(db)).toBe(true);
  expect((await db.prepare('SELECT * FROM erpx_migrations WHERE name<? ORDER BY name').bind('1066_payroll_other_liability_remittances.sql').all()).results).toEqual(before);
  expect(await privileges()).toEqual(originalAccess);
  const available=(await inspectMigrations(db,'holding-write'));expect(available.pending).toEqual([]);expect(available.latestApplied).toBe(files.at(-1));
  // Unchanged older-source migration is a no-op and does not retract the new
  // native receipt; the detector must keep its honest current schema result.
  await migrateDatabase(db,'holding-write',old);expect(await payrollOtherSchemaAvailable(db)).toBe(true);
 }finally{await rm(old,{recursive:true,force:true});}
});

// Regression: the receipt insert used to travel over the native HTTP command
// ingress, so it either failed with 42P01 on a target database without ledger
// tables or committed the receipt outside the DDL transaction. The receipt is
// now bound from the Rust-owned contract and runs on the migrating connection.
it('records the receipt on the migrating connection without the native command ingress',async()=>{
 const root=await mkdtemp(resolve('.runtime/tests/migration-receipt-'));
 const previous=process.env.ERPX_NATIVE_RUNTIME_CONFIG;
 try{
  await mkdir(join(root,'control'));
  await writeFile(join(root,'control','0001_receipt_probe.sql'),'CREATE TABLE migration_receipt_probe(id integer PRIMARY KEY)');
  // A loopback discard port: any HTTP command transport would fail closed.
  process.env.ERPX_NATIVE_RUNTIME_CONFIG=JSON.stringify({privateBaseUrl:'http://127.0.0.1:9',timeoutMs:250,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h',companyId:'c'}]});
  await migrateDatabase(env.CONTROL_READ_DB,'control',root);
  const receipt=await env.CONTROL_READ_DB.prepare("SELECT checksum FROM erpx_migrations WHERE name='0001_receipt_probe.sql'").first<{checksum:string}>();
  expect(receipt?.checksum).toMatch(/^[a-f0-9]{64}$/);
  expect(await env.CONTROL_READ_DB.prepare("SELECT to_regclass('public.migration_receipt_probe') IS NOT NULL present").first<boolean>('present')).toBe(true);
  await writeFile(join(root,'control','0002_broken_probe.sql'),'CREATE TABLE broken_probe(');
  await expect(migrateDatabase(env.CONTROL_READ_DB,'control',root)).rejects.toBeTruthy();
  expect(await env.CONTROL_READ_DB.prepare("SELECT count(*)::int n FROM erpx_migrations WHERE name='0002_broken_probe.sql'").first<number>('n')).toBe(0);
 }finally{
  if(previous===undefined)delete process.env.ERPX_NATIVE_RUNTIME_CONFIG;else process.env.ERPX_NATIVE_RUNTIME_CONFIG=previous;
  await env.CONTROL_READ_DB.prepare('DROP TABLE IF EXISTS migration_receipt_probe').run();
  await env.CONTROL_READ_DB.prepare("DELETE FROM erpx_migrations WHERE name='0001_receipt_probe.sql'").run();
  await rm(root,{recursive:true,force:true});
 }
},60000);
