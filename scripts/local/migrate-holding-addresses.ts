/** Install only the authorized route migration, never unrelated pending work. */
import {readFile,mkdir,copyFile} from 'node:fs/promises';
import {dirname,join,resolve} from 'node:path';
import {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {migrateDatabase} from '@erpx/server-runtime/adapters/runtime/migrations';
import {reservedWorkspaceSlugs} from '@erpx/contracts/holding-hosts';
import {assertHospitalityDevelopmentTarget} from '@erpx/feature-shared/lib/hospitality-seed-target';
import type {DatabaseManifest} from '@erpx/server-runtime/adapters/runtime/env';

const manifestPath=process.env.ERPX_DATABASE_MANIFEST;
if(!manifestPath)throw new Error('DEVELOPMENT_MANIFEST_REQUIRED');
const manifest=JSON.parse(await readFile(join(dirname(manifestPath),'databases-migrations.json'),'utf8')) as DatabaseManifest;
assertHospitalityDevelopmentTarget(manifest,process.env.ERPX_ENVIRONMENT);
const apply=process.argv.includes('--apply'),root=resolve('.scratch/dev/holding-addresses/migrations');
const file='1073_holding_workspace_route_slugs.sql';
await mkdir(join(root,'holding-write'),{recursive:true,mode:0o700});
await copyFile(resolve('migrations/postgres/holding-write',file),join(root,'holding-write',file));
const reports=[];
for(const [binding,connectionString]of Object.entries(manifest.databases)){
 if(!/^H\d+_DB$/.test(binding))continue;
 const identity=new URL(connectionString);
 if(!/^erpx_dev_[a-z0-9_]+_migration$/.test(identity.username))throw new Error('DEVELOPMENT_MIGRATION_IDENTITY_REQUIRED');
 const db=new PostgresDatabase({connectionString,max:1});
 try{
  const current=await db.prepare('SELECT current_database() name,current_user identity').first<{name:string;identity:string}>();
  if(!current||!/^erpx_dev_/.test(current.name)||current.identity!==identity.username)throw new Error('DEVELOPMENT_DATABASE_IDENTITY_MISMATCH');
  const rows=await db.prepare(`SELECT id,kind,slug,parent_id,company_id,branch_id FROM organization_nodes WHERE kind IN ('branch','department') AND slug IN (${reservedWorkspaceSlugs.map(()=>'?').join(',')}) ORDER BY id`).bind(...reservedWorkspaceSlugs).all();
  if(apply)await migrateDatabase(db,'holding-write',root);
  const remaining=apply?(await db.prepare(`SELECT count(*)::int total FROM organization_nodes WHERE kind IN ('branch','department') AND slug IN (${reservedWorkspaceSlugs.map(()=>'?').join(',')})`).bind(...reservedWorkspaceSlugs).first<{total:number}>())?.total:undefined;
  reports.push({binding,database:current.name,changed:rows.results.length,remaining,applied:apply});
 }finally{await db.close();}
}
console.log(JSON.stringify({environment:'development',reports},null,2));
