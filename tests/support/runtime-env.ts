import { randomUUID } from 'node:crypto';
import { mkdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { Pool } from 'pg';
import { PostgresDatabase } from "../../src/server/modules/runtime/adapters/runtime/postgres";
import { LocalFileStore } from "../../src/server/modules/runtime/adapters/runtime/filesystem";
import { migrateDatabase } from "../../src/server/modules/runtime/adapters/runtime/migrations";
import type { SqlDatabase } from "../../src/server/modules/runtime/adapters/runtime/types";

const adminUrl=process.env.ERPX_TEST_ADMIN_URL ?? 'postgresql://erpx_test_admin@127.0.0.1:5432/postgres';
const parsedAdminUrl=new URL(adminUrl);
if(parsedAdminUrl.hostname!=='127.0.0.1'||parsedAdminUrl.port!=='5432'||parsedAdminUrl.username!=='erpx_test_admin'||parsedAdminUrl.pathname!=='/postgres')throw new Error('LOCAL_TEST_DATABASE_REQUIRED');
const prefix=`erpx_test_${randomUUID().replaceAll('-','').slice(0,10)}`;
const directory=join(process.cwd(),'.runtime/tests',prefix);
const names=['CONTROL_DB','CONTROL_READ_DB','H001_DB','H001_READ_DB','H002_DB','H002_READ_DB','H003_DB','H003_READ_DB'] as const;
const databases=Object.fromEntries(names.map(name=>{const url=new URL(adminUrl);url.pathname=`${prefix}_${name.toLowerCase()}`;return [name,new PostgresDatabase({connectionString:url.href,max:4})];})) as Record<typeof names[number],PostgresDatabase>;
export type TestMigration={ name:string; track:string };
export const env={...databases,H001_FILES:new LocalFileStore(join(directory,'h001')),H002_FILES:new LocalFileStore(join(directory,'h002')),H003_FILES:new LocalFileStore(join(directory,'h003')),
  ERPX_ENVIRONMENT:'local',BETTER_AUTH_URL:'http://localhost:3100',BETTER_AUTH_SECRET:'test-secret-private-local-fixtures-only-2026',
  STAFF_TEST_MIGRATIONS:[{name:'0001_baseline',track:'holding-write'}] as TestMigration[],
  CONTROL_TEST_MIGRATIONS:[{name:'0001_baseline',track:'control'}] as TestMigration[],
};
export async function applyMigrations(database:SqlDatabase,migrations:TestMigration[]) {
  if(!(database instanceof PostgresDatabase))throw new Error('POSTGRES_TEST_DATABASE_REQUIRED');
  for(const track of new Set(migrations.map(m=>m.track)))await migrateDatabase(database,track);
}
export async function setupDatabases(){
  const admin=new Pool({connectionString:adminUrl,max:1});
  try {
    const role=await admin.query('SELECT current_user AS name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user');
    const guard=role.rows[0];
    if(guard?.name!=='erpx_test_admin'||guard.rolsuper||!guard.rolcreatedb||guard.rolcreaterole)throw new Error('UNSAFE_TEST_DATABASE_ROLE');
    const forbidden=await admin.query("SELECT datname FROM pg_database WHERE (datname LIKE 'epsx\\_%' ESCAPE '\\' OR datname IN ('n8n','plf_webapp_dev')) AND has_database_privilege(current_user,datname,'CONNECT')");
    if(forbidden.rowCount)throw new Error('ERPX_TEST_ROLE_CAN_CONNECT_TO_PROJECT_DATABASE');
    await mkdir(directory,{recursive:true});for(const name of names)await admin.query(`CREATE DATABASE "${prefix}_${name.toLowerCase()}" TEMPLATE template0`);
  }
  finally {await admin.end();}
}
export async function cleanupDatabases(){
  await Promise.all(Object.values(databases).map(db=>db.close()));
  const admin=new Pool({connectionString:adminUrl,max:1});
  try {for(const name of names){
    const database=`${prefix}_${name.toLowerCase()}`;
    if(!database.startsWith('erpx_test_'))throw new Error('UNSAFE_TEST_DATABASE_PREFIX');
    const ownership=await admin.query('SELECT pg_get_userbyid(datdba) AS owner FROM pg_database WHERE datname=$1',[database]);
    if(ownership.rowCount&&ownership.rows[0].owner!=='erpx_test_admin')throw new Error('UNSAFE_TEST_DATABASE_OWNER');
    await admin.query(`DROP DATABASE IF EXISTS "${database}"`);
  }}
  finally {await admin.end();await rm(directory,{recursive:true,force:true});}
}
