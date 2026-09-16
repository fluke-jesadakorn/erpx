import { it,expect } from 'vitest';
import { env } from './runtime-env';
import { mkdtemp,mkdir,writeFile,symlink,readFile,rm } from 'node:fs/promises';
import { resolve,join } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
const execute=promisify(execFile);
it('restores the same database and file snapshot and refuses corrupted backups',async()=>{
 const home=await mkdtemp(resolve('.runtime/tests/recovery-'));
 try{
  for(const directory of ['config','data/config','data/files','runtime','releases/2026-09-08T00-00-00-000Z'])await mkdir(join(home,directory),{recursive:true});
  const admin=new URL(process.env.ERPX_TEST_ADMIN_URL??'postgresql://erpx_test_admin@127.0.0.1:5432/postgres');
  await writeFile(join(home,'config/admin.json'),JSON.stringify({host:admin.hostname,port:Number(admin.port),user:admin.username,password:admin.password}));
  await writeFile(join(home,'config/production.env'),'BETTER_AUTH_SECRET=snapshot-auth-secret\n');
  const release=join(home,'releases/2026-09-08T00-00-00-000Z');
  await writeFile(join(release,'release.json'),JSON.stringify({id:'2026-09-08T00-00-00-000Z',schemaSha256:'test'}));
  await writeFile(join(release,'server.js'),'');await symlink(release,join(home,'current'));
  await symlink(resolve(process.env.ERPX_TEST_PG_BIN??'/opt/homebrew/opt/postgresql@18/bin','..'),join(home,'runtime/postgres'));
  const databases=Object.fromEntries(Object.entries(env).filter(([,db])=>db instanceof Object&&'pool' in db).map(([key,db])=>[key,(db as typeof env.H001_DB).pool.options.connectionString]));
  await writeFile(join(home,'data/config/databases.json'),JSON.stringify({databases}));
  await env.H001_DB.exec('CREATE TABLE backup_fixture(id text PRIMARY KEY,version bigint); INSERT INTO backup_fixture VALUES(\'record\',1)');
  await writeFile(join(home,'data/files/revision'),'version-one');
  const args=['ops/macos/erpxctl.py','--home',home,'--offline-test'];
  const first=await execute('python3',[...args,'backup']);const snapshot=first.stdout.trim().replace('Backup: ','');
  await env.H001_DB.exec("UPDATE backup_fixture SET version=2 WHERE id='record'");
  await writeFile(join(home,'data/files/revision'),'version-two');
  await execute('python3',[...args,'restore',snapshot]);
  expect(await env.H001_DB.prepare('SELECT version FROM backup_fixture').first('version')).toBe(1);
  expect(await readFile(join(home,'data/files/revision'),'utf8')).toBe('version-one');
  await writeFile(join(snapshot,'files/revision'),'tampered');
  await expect(execute('python3',[...args,'restore',snapshot])).rejects.toThrow('BACKUP_CHECKSUM_MISMATCH');
 }finally{await rm(home,{recursive:true,force:true});}
},30_000);
