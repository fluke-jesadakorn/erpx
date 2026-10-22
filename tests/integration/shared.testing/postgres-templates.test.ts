import { afterAll, expect, it } from 'vitest';
import { randomUUID } from 'node:crypto';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawn } from 'node:child_process';
import { Pool } from 'pg';
import { env, applyMigrations } from '../../support/runtime-env';
import { ensureSchemaTemplate,createSuiteDatabases,completeSuiteLease,recoverAbandonedSuiteLeases,cleanupSuiteDatabases,schemaFingerprint,testAdministratorUrl } from '../../support/postgres-templates';

const root=process.cwd(),admin=new Pool({connectionString:testAdministratorUrl(),max:1});
const prefixes:string[]=[];
const prefix=()=>{const value='erpx_test_'+randomUUID().replaceAll('-','').slice(0,10);prefixes.push(value);return value;};
const lease=(value:string)=>join(root,'.scratch/dev/postgres-templates',value+'.lease.json');
afterAll(async()=>{
 for(const value of prefixes){await admin.query(`DROP DATABASE IF EXISTS "${value}_control_db"`);await rm(lease(value),{force:true});}
 await admin.end();
});
it('clones immutable schemas into isolated suite databases without replaying or sharing rows',async()=>{
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);
 const template=await ensureSchemaTemplate(root,'control','broker-isolation-probe/v1');
 const metadata=(await admin.query("SELECT datallowconn,pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1",[template.database])).rows[0];
 expect(metadata).toEqual({datallowconn:false,owner:'erpx_test_admin'});
 const left=prefix(),right=prefix();
 for(const value of [left,right])await createSuiteDatabases(root,value,['CONTROL_DB'],{control:template},'template',['CONTROL_DB']);
 const connection=(value:string)=>{const url=new URL(testAdministratorUrl());url.pathname=`/${value}_control_db`;return new Pool({connectionString:url.href,max:1});};
 const a=connection(left),b=connection(right);
 try{
  expect(Number((await a.query('SELECT COUNT(*) n FROM erpx_migrations')).rows[0].n)).toBeGreaterThan(0);
  await a.query('CREATE TABLE fixture_isolation_probe(id text PRIMARY KEY)');await a.query("INSERT INTO fixture_isolation_probe VALUES('left-only')");
  expect((await b.query("SELECT to_regclass('public.fixture_isolation_probe') present")).rows[0].present).toBeNull();
  expect((await a.query('SELECT current_database() name')).rows[0].name).not.toBe((await b.query('SELECT current_database() name')).rows[0].name);
 }finally{await a.end();await b.end();}
 for(const value of [left,right]){await admin.query(`DROP DATABASE "${value}_control_db"`);await completeSuiteLease(root,value);}
 const again=await ensureSchemaTemplate(root,'control','broker-isolation-probe/v1');expect(again).toEqual(template);
});
it('invalidates templates for migration bytes, fixture revision, and PostgreSQL settings',async()=>{
 const temporary=await mkdtemp(join(tmpdir(),'erpx-template-key-'));
 try{const directory=join(temporary,'migrations/postgres/control');await mkdir(directory,{recursive:true});const file=join(directory,'0001.sql');await writeFile(file,'CREATE TABLE example(id integer);');
  const key=await schemaFingerprint(temporary,'control','18.6:UTC','profile-a');
  expect(await schemaFingerprint(temporary,'control','18.6:UTC','profile-b')).not.toBe(key);
  expect(await schemaFingerprint(temporary,'control','18.6:Asia/Bangkok','profile-a')).not.toBe(key);
  await writeFile(file,'CREATE TABLE example(id bigint);');expect(await schemaFingerprint(temporary,'control','18.6:UTC','profile-a')).not.toBe(key);
 }finally{await rm(temporary,{recursive:true,force:true});}
});
it('recovers only dead owned leases and leaves a live suite alone',async()=>{
 const dead=prefix(),live=prefix();await createSuiteDatabases(root,live,['CONTROL_DB'],{},'raw',[]);
 const code=`import {createSuiteDatabases} from ${JSON.stringify(join(root,'tests/support/postgres-templates.ts'))};await createSuiteDatabases(${JSON.stringify(root)},${JSON.stringify(dead)},['CONTROL_DB'],{},'raw',[]);`;
 const bun=process.env.ERPX_BUN_BIN;if(!bun)throw Error('PINNED_BUN_FOR_PROCESS_PROOF_REQUIRED');
 const child=spawn(bun,['--conditions=react-server','-e',code],{stdio:['ignore','pipe','pipe'],env:{...process.env,ERPX_TEST_ADMIN_URL:testAdministratorUrl()}});
 let diagnostic='';child.stderr.on('data',chunk=>{diagnostic+=chunk.toString();});
 const exit=await new Promise<number|null>((done,reject)=>{child.once('error',reject);child.once('exit',done);});if(exit!==0)throw Error('OWNED_LEASE_CHILD_FAILED:'+diagnostic);
 await recoverAbandonedSuiteLeases(root);
 expect((await admin.query('SELECT datname FROM pg_database WHERE datname=$1',[dead+'_control_db'])).rowCount).toBe(0);
 expect((await admin.query('SELECT datname FROM pg_database WHERE datname=$1',[live+'_control_db'])).rowCount).toBe(1);
 const recovered=JSON.parse(await readFile(lease(dead),'utf8'));expect(recovered.completed).toBe(true);expect(recovered.recoveredAt).toBeTypeOf('number');
});

it('refuses a colliding prefix from another root without adopting or dropping its database',async()=>{
 const value=prefix();await createSuiteDatabases(root,value,['CONTROL_DB'],{},'raw',[]);
 const temporary=await mkdtemp(join(tmpdir(),'erpx-lease-collision-'));
 try{await expect(createSuiteDatabases(temporary,value,['CONTROL_DB'],{},'raw',[])).rejects.toThrow('TEST_DATABASE_PREFIX_ALREADY_IN_USE');
  expect((await admin.query('SELECT 1 FROM pg_database WHERE datname=$1',[value+'_control_db'])).rowCount).toBe(1);
 }finally{await rm(temporary,{recursive:true,force:true});}
 await cleanupSuiteDatabases(root,value);
});
it('refuses a changed generation marker and leaves the original database untouched',async()=>{
 const value=prefix();await createSuiteDatabases(root,value,['CONTROL_DB'],{},'raw',[]);
 const original=await readFile(lease(value),'utf8'),changed={...JSON.parse(original),token:randomUUID()};await writeFile(lease(value),JSON.stringify(changed)+'\n',{mode:0o600});
 try{await expect(cleanupSuiteDatabases(root,value)).rejects.toThrow('TEST_DATABASE_OWNERSHIP_CHANGED');expect((await admin.query('SELECT 1 FROM pg_database WHERE datname=$1',[value+'_control_db'])).rowCount).toBe(1);}
 finally{await writeFile(lease(value),original,{mode:0o600});await cleanupSuiteDatabases(root,value);}
});
