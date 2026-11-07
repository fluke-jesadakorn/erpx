import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { Pool } from 'pg';
import { prepareNativeLeases } from '../../local/dev-container-native';
import { auditDevContainerForeignKeys } from '../../local/dev-container-verify-fk';
const secret=readFileSync('/run/erpx-secrets/test-admin','utf8').trim();
const adminPassword=readFileSync('/run/erpx-secrets/postgres','utf8').trim();
const u=new URL('postgresql://postgres@127.0.0.1:5432/postgres'); u.password=adminPassword;
const admin=new Pool({connectionString:u.href,max:1});
for(let i=0;;i++){try{await admin.query('SELECT 1');break;}catch(e){if(i===120)throw e;await Bun.sleep(500);}}
const key='lease-qualification',sourceId='a'.repeat(64),config='/case/private'; mkdirSync(config,{recursive:true,mode:0o700});
const bindings=['CONTROL_DB','CONTROL_READ_DB',...['H001','H002','H003'].flatMap(x=>[`${x}_DB`,`${x}_READ_DB`])];
const old=(b:string)=>`erpx_dev_${b.toLowerCase().replace(/_db$/,'').replace(/^(h00[1-3])$/,'$1_write')}`;
await admin.query(`CREATE ROLE erpx_test_admin LOGIN CREATEDB NOSUPERUSER NOCREATEROLE PASSWORD '${secret}'`);
const roles:Record<string,string>={},manifests:Record<string,{version:number;databases:Record<string,string>}>={};
for(const kind of ['app','jobs','migration']) manifests[kind]={version:1,databases:{}};
for(const binding of bindings){const db=old(binding);for(const kind of ['app','jobs','migration']){const role=`${db}_${kind}`,password='b'.repeat(64);roles[role]=password;await admin.query(`CREATE ROLE "${role}" LOGIN PASSWORD '${password}'`);const url=new URL(`postgresql://${role}@127.0.0.1:5432/${db}`);url.password=password;manifests[kind]!.databases[binding]=url.href;}await admin.query(`CREATE DATABASE "${db}" OWNER "${db}_migration"`);await admin.query(`REVOKE ALL ON DATABASE "${db}" FROM PUBLIC`);await admin.query(`GRANT CONNECT ON DATABASE "${db}" TO "${db}_app", "${db}_jobs", "${db}_migration"`);const c=new Pool({connectionString:new URL(`/${db}`,u).href,max:1});try{await c.query("CREATE TABLE retained_fixture(value text NOT NULL)");await c.query("INSERT INTO retained_fixture VALUES('preserve')");}finally{await c.end();}}
function put(name:string,data:unknown){writeFileSync(`${config}/${name}`,JSON.stringify(data),{mode:0o600});}
put('provisioning.json',roles);put('databases.json',manifests.app);put('databases-jobs.json',manifests.jobs);put('databases-migrations.json',manifests.migration);
writeFileSync(`${config}/native-leases.json.next`,'orphan',{mode:0o600});writeFileSync(`${config}/bootstrap-phase.json.tmp`,'orphan',{mode:0o600});
const blocker=new Pool({connectionString:new URL(`/${old(bindings[7]!)}`,u).href,max:1});await blocker.query('SELECT 1');
let failed=false;try{await prepareNativeLeases(config,key,sourceId,u.href);}catch{failed=true;}if(!failed)throw Error('ACTIVE_CLIENT_NOT_REJECTED');
let counts=await admin.query("SELECT count(*)::int n FROM pg_database WHERE datname LIKE 'erpx_dev_rust_%'");if(counts.rows[0].n!==0)throw Error('ACTIVE_CLIENT_PARTIAL_RENAME');
await blocker.end();
const initial=JSON.parse(readFileSync(`${config}/native-leases.json`,'utf8'));const tokens=initial.entries.map((e:{token:string})=>e.token).join(',');
const original=Pool.prototype.query;let injected=false;
// Test-only wrapper throws after the real PostgreSQL rename has committed.
(Pool.prototype as unknown as {query:typeof original}).query=async function(this: Pool,...args:Parameters<typeof original>){const result=await Reflect.apply(original,this,args);if(!injected&&typeof args[0]==='string'&&args[0].startsWith('ALTER DATABASE ')&&args[0].includes(' RENAME TO ')){injected=true;throw Error('TEST_AFTER_COMMITTED_RENAME');}return result;} as typeof original;
try{await prepareNativeLeases(config,key,sourceId,u.href);throw Error('FAULT_NOT_INJECTED');}catch(e){if(!(e instanceof Error)||e.message!=='TEST_AFTER_COMMITTED_RENAME')throw e;}finally{Pool.prototype.query=original;}
counts=await admin.query("SELECT count(*)::int n FROM pg_database WHERE datname LIKE 'erpx_dev_rust_%'");if(counts.rows[0].n!==1)throw Error('FIRST_RENAME_NOT_COMMITTED');
const done=await prepareNativeLeases(config,key,sourceId,u.href);if(!done.complete||done.entries.map(e=>e.token).join(',')!==tokens)throw Error('LEASE_IDENTITY_CHANGED');
const repeat=await prepareNativeLeases(config,key,sourceId,u.href);if(JSON.stringify(repeat)!==JSON.stringify(done))throw Error('REPEAT_CHANGED_LEASES');
for(const e of done.entries){const state=await admin.query("SELECT pg_get_userbyid(datdba) owner,shobj_description(oid,'pg_database') marker FROM pg_database WHERE datname=$1",[e.newName]);if(state.rows[0]?.owner!=='erpx_test_admin'||state.rows[0]?.marker!==`erpx-rust-test:${e.token}`)throw Error('LEASE_DATABASE_PROOF_FAILED');const c=new Pool({connectionString:new URL(`/${e.newName}`,u).href,max:1});try{if((await c.query('SELECT value FROM retained_fixture')).rows[0]?.value!=='preserve')throw Error('DATA_NOT_RETAINED');}finally{await c.end();}}
for(const name of ['databases.json','databases-jobs.json','databases-migrations.json']){const m=JSON.parse(readFileSync(`${config}/${name}`,'utf8'));for(const e of done.entries){const url=m.databases[e.binding];if(new URL(url).pathname!==`/${e.newName}`)throw Error('MANIFEST_NOT_REWRITTEN');const client=new Pool({connectionString:url,max:1});try{await client.query('SELECT 1');}finally{await client.end();}}}
if(readFileSync(`${config}/native-leases.json.next`,'utf8')!=='orphan'||readFileSync(`${config}/bootstrap-phase.json.tmp`,'utf8')!=='orphan')throw Error('ORPHAN_CHANGED');
// Actual referenced-partition regression: a valid receipt exists in exactly
// one partition; inherited partition FKs must not produce false orphans.
const fkUrl=new URL(`/${done.entries[0]!.newName}`,u).href, fkPool=new Pool({connectionString:fkUrl,max:1});
const fk=await fkPool.connect();try{
 await fk.query('CREATE TABLE fk_receipts(id int PRIMARY KEY) PARTITION BY HASH(id)');
 for(let i=0;i<4;i++)await fk.query(`CREATE TABLE fk_receipts_${i} PARTITION OF fk_receipts FOR VALUES WITH (MODULUS 4,REMAINDER ${i})`);
 await fk.query('CREATE TABLE fk_source(id int PRIMARY KEY,receipt_id int REFERENCES fk_receipts(id))');
 await fk.query('INSERT INTO fk_receipts VALUES(7)');await fk.query('INSERT INTO fk_source VALUES(1,7)');
 await fk.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');if((await auditDevContainerForeignKeys(fk)).length!==0)throw Error('PARTITION_VALID_FK_REJECTED');await fk.query('COMMIT');
 await fk.query('ALTER TABLE fk_source DISABLE TRIGGER ALL');
 await fk.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');let denied=false;try{await auditDevContainerForeignKeys(fk);}catch{denied=true;}await fk.query('ROLLBACK');if(!denied)throw Error('DISABLED_FK_NOT_REJECTED');
 await fk.query('INSERT INTO fk_source VALUES(2,999)');await fk.query('ALTER TABLE fk_source ENABLE TRIGGER ALL');
 await fk.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');if((await auditDevContainerForeignKeys(fk)).length!==1)throw Error('ACTUAL_FK_ORPHAN_NOT_REPORTED');await fk.query('ROLLBACK');
}finally{fk.release();await fkPool.end();}
console.log('FK_PARTITION_QUALIFICATION_PASS valid-root disabled-trigger-denied real-orphan-reported');
await admin.end();console.log('LEASE_QUALIFICATION_PASS active-client/no-rename committed-rename/recovery same-tokens retained-data owner/comment manifests repeat orphan');
