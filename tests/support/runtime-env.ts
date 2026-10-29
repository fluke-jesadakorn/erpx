import { createHash, randomUUID } from 'node:crypto';
import { mkdir, rm, writeFile, readdir, readFile } from 'node:fs/promises';
import { join, relative } from 'node:path';
import { Pool } from 'pg';
import { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";
import { LocalFileStore } from "@erpx/server-runtime/adapters/runtime/filesystem";
import { migrateDatabase, refreshMigrationCoverage } from "@erpx/server-runtime/public/runtime/migrations";
import type { SqlDatabase } from "@erpx/server-runtime/adapters/runtime/types";
import { assertSuiteDatabaseIdentity, createSuiteDatabases, cleanupSuiteDatabases, databaseTracks, type DatabaseSlot, type NativeFixturePlan } from './postgres-templates';

const adminUrl=process.env.ERPX_TEST_ADMIN_URL ?? 'postgresql://erpx_test_admin@127.0.0.1:5432/postgres';
const parsedAdminUrl=new URL(adminUrl);
if(parsedAdminUrl.hostname!=='127.0.0.1'||parsedAdminUrl.port!=='5432'||parsedAdminUrl.username!=='erpx_test_admin'||parsedAdminUrl.pathname!=='/postgres')throw new Error('LOCAL_TEST_DATABASE_REQUIRED');
const prefix=`erpx_test_${randomUUID().replaceAll('-','').slice(0,10)}`;
const directory=join(process.cwd(),'.runtime/tests',prefix);
const names=['CONTROL_DB','CONTROL_READ_DB','H001_DB','H001_READ_DB','H002_DB','H002_READ_DB','H003_DB','H003_READ_DB'] as const;
const databases=Object.fromEntries(names.map(name=>{const url=new URL(adminUrl);url.pathname=`${prefix}_${name.toLowerCase()}`;return [name,new PostgresDatabase({connectionString:url.href,max:4})];})) as Record<typeof names[number],PostgresDatabase>;
const runtimeEnvironmentNames=['ERPX_DATABASE_MANIFEST','ERPX_MIGRATION_DATABASE_MANIFEST','ERPX_ENVIRONMENT','ERPX_PUBLIC_ORIGIN','ERPX_MARKETING_ORIGIN','ERPX_FRONT_PMS_ORIGIN','ERPX_FRONT_POS_ORIGIN','ERPX_DATA_DIR','ERPX_PROCESS_ROLE','ERPX_PUBLIC_BETA','ERPX_MINIO_ENDPOINT','ERPX_MINIO_BUCKET','ERPX_MINIO_ACCESS_KEY','ERPX_MINIO_SECRET_KEY','BETTER_AUTH_URL','BETTER_AUTH_SECRET'];
const originalRuntimeEnvironment=new Map<string,string|undefined>();
let fixturePlan:NativeFixturePlan|undefined,fixtureFile:string|undefined;
let selectedNames:readonly typeof names[number][]=names;
const preparedTracks=new WeakMap<SqlDatabase,string>();
let brokerDatabasesCreated=false;
const legacyCreatedNames:string[]=[];
const fixtureTimings={setupMs:0,migrationMs:0,migrationCalls:0};
export function configureDatabaseFixture(plan:NativeFixturePlan,file:string){
 if(plan.version!==1||plan.root!==process.cwd())throw new Error('NATIVE_FIXTURE_ROOT_MISMATCH');
 const key=relative(plan.root,file).replaceAll('\\','/');
 if(!plan.files[key])throw new Error(`NATIVE_FIXTURE_FILE_UNMAPPED:${key}`);
 fixturePlan=plan;fixtureFile=key;selectedNames=plan.files[key].slots;
}
export type TestMigration={ name:string; track:string };
export async function assertCurrentFixtureDatabase(database: PostgresDatabase, slot: DatabaseSlot) {
  if (!fixturePlan || !fixtureFile || database !== databases[slot] || !selectedNames.includes(slot))
    throw new Error('EXACT_NATIVE_SOURCE_DATABASE_REQUIRED');
  return assertSuiteDatabaseIdentity(fixturePlan.root, prefix, slot, database);
}
export const env={...databases,H001_FILES:new LocalFileStore(join(directory,'h001')),H002_FILES:new LocalFileStore(join(directory,'h002')),H003_FILES:new LocalFileStore(join(directory,'h003')),
  ERPX_ENVIRONMENT:'local',BETTER_AUTH_URL:'http://localhost:3100',BETTER_AUTH_SECRET:'test-secret-private-local-fixtures-only-2026',
  STAFF_TEST_MIGRATIONS:[{name:'0001_baseline',track:'holding-write'}] as TestMigration[],
  CONTROL_TEST_MIGRATIONS:[{name:'0001_baseline',track:'control'}] as TestMigration[],
};
export async function applyMigrations(database:SqlDatabase,migrations:TestMigration[]) {
  if(!(database instanceof PostgresDatabase))throw new Error('POSTGRES_TEST_DATABASE_REQUIRED');
  const started=performance.now();
  try {for(const track of new Set(migrations.map(m=>m.track))){
   if(preparedTracks.get(database)===track){
    // A clone avoids replay, but it must still prove every migration checksum.
    const receipts=await database.prepare('SELECT name,checksum FROM erpx_migrations').all<{name:string;checksum:string}>();
    const applied=new Map(receipts.results.map(row=>[row.name,row.checksum]));
    const migrationRoot=join(process.cwd(),'migrations/postgres',track);
    const files=(await readdir(migrationRoot)).filter(name=>name.endsWith('.sql')).sort();
    if(applied.size!==files.length)throw new Error('TEMPLATE_MIGRATION_INVENTORY_CHANGED');
    for(const name of files){const checksum=createHash('sha256').update(await readFile(join(migrationRoot,name))).digest('hex');
     if(applied.get(name)!==checksum)throw new Error(`MIGRATION_CHANGED:${track}:${name}`);
    }
    await refreshMigrationCoverage(database, track);
    continue;
   }
   await migrateDatabase(database,track);
  }} finally {fixtureTimings.migrationMs+=performance.now()-started;fixtureTimings.migrationCalls++;}
}
export async function setupDatabases(){
  const started=performance.now();
  const admin=new Pool({connectionString:adminUrl,max:1});
  try {
    const role=await admin.query('SELECT current_user AS name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user');
    const guard=role.rows[0];
    if(guard?.name!=='erpx_test_admin'||guard.rolsuper||!guard.rolcreatedb||guard.rolcreaterole)throw new Error('UNSAFE_TEST_DATABASE_ROLE');
    const forbidden=await admin.query("SELECT datname FROM pg_database WHERE (datname LIKE 'epsx\\_%' ESCAPE '\\' OR datname IN ('n8n','plf_webapp_dev')) AND has_database_privilege(current_user,datname,'CONNECT')");
    if(forbidden.rowCount)throw new Error('ERPX_TEST_ROLE_CAN_CONNECT_TO_PROJECT_DATABASE');
    await mkdir(directory,{recursive:true,mode:0o700});
    if(fixturePlan&&fixtureFile){
      const profile=fixturePlan.files[fixtureFile];
      await createSuiteDatabases(fixturePlan.root,prefix,profile.slots,fixturePlan.templates,profile.mode,profile.migratedSlots);
      brokerDatabasesCreated=true;
      if(profile.mode==='template')for(const name of profile.migratedSlots)preparedTracks.set(databases[name],databaseTracks[name]);
    }else for(const name of names){const database=`${prefix}_${name.toLowerCase()}`;await admin.query(`CREATE DATABASE "${database}" TEMPLATE template0`);legacyCreatedNames.push(database);}
  }
  finally {await admin.end();}
  const manifestPath=join(directory,'databases.json'),manifestDatabases=Object.fromEntries(selectedNames.map(name=>{const url=new URL(adminUrl);url.pathname=`${prefix}_${name.toLowerCase()}`;return [name,url.href];}));
  const slots=['h001','h002','h003'].filter(id=>selectedNames.includes(`${id.toUpperCase()}_DB` as DatabaseSlot)&&selectedNames.includes(`${id.toUpperCase()}_READ_DB` as DatabaseSlot)).map(id=>({id,write:`${id.toUpperCase()}_DB`,read:`${id.toUpperCase()}_READ_DB`,files:`${id.toUpperCase()}_FILES`}));
  await writeFile(manifestPath,JSON.stringify({version:1,databases:manifestDatabases,slots},null,2)+'\n',{mode:0o600});
  for(const name of runtimeEnvironmentNames)originalRuntimeEnvironment.set(name,process.env[name]);
  for(const name of ['ERPX_MARKETING_ORIGIN','ERPX_FRONT_PMS_ORIGIN','ERPX_FRONT_POS_ORIGIN','ERPX_MINIO_ENDPOINT','ERPX_MINIO_BUCKET','ERPX_MINIO_ACCESS_KEY','ERPX_MINIO_SECRET_KEY'])delete process.env[name];
  Object.assign(process.env,{ERPX_DATABASE_MANIFEST:manifestPath,ERPX_MIGRATION_DATABASE_MANIFEST:join(directory,'databases-migrations.json'),ERPX_ENVIRONMENT:'local',ERPX_PUBLIC_ORIGIN:'https://console-dev-erpx.jesadakorn.com',ERPX_DATA_DIR:directory,ERPX_PROCESS_ROLE:'app',ERPX_PUBLIC_BETA:'false',BETTER_AUTH_URL:'http://localhost:3100',BETTER_AUTH_SECRET:'test-runtime-secret-never-used-outside-integration'});
  fixtureTimings.setupMs=performance.now()-started;
}
export async function cleanupDatabases(){
  if(fixturePlan?.benchmark)await writeFile(join(fixturePlan.benchmark.directory,prefix+'.json'),JSON.stringify({file:fixtureFile,mode:fixturePlan.benchmark.mode,...fixtureTimings})+'\n',{mode:0o600});
  try{
    await Promise.all(Object.values(databases).map(db=>db.close()));
    if(fixturePlan){if(brokerDatabasesCreated)await cleanupSuiteDatabases(fixturePlan.root,prefix);}
    else {
      const admin=new Pool({connectionString:adminUrl,max:1});
      try{for(const database of legacyCreatedNames){
        if(!/^erpx_test_[a-f0-9]{10}_(?:control(?:_read)?|h00[1-3](?:_read)?)_db$/.test(database))throw new Error('UNSAFE_TEST_DATABASE_PREFIX');
        const ownership=await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[database]);
        if(ownership.rowCount&&ownership.rows[0].owner!=='erpx_test_admin')throw new Error('UNSAFE_TEST_DATABASE_OWNER');
        await admin.query(`DROP DATABASE IF EXISTS "${database}"`);
      }}finally{await admin.end();}
    }
  }finally{
    await rm(directory,{recursive:true,force:true});
    for(const [name,value] of originalRuntimeEnvironment){if(value===undefined)delete process.env[name];else process.env[name]=value;}
    originalRuntimeEnvironment.clear();
  }
}
