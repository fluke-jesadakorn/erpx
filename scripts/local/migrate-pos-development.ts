/** Apply the pending holding schema with the installed development migration identities. */
import { readFile } from 'node:fs/promises';
import { dirname,join } from 'node:path';
import { assertHospitalityDevelopmentTarget } from '../../src/lib/hospitality-seed-target';
import { PostgresDatabase } from '../../src/server/runtime/postgres';
import { migrateDatabase } from '../../src/server/runtime/migrations';
const path=process.env.ERPX_DATABASE_MANIFEST;
if(!path)throw new Error('DEVELOPMENT_MANIFEST_REQUIRED');
const manifest=JSON.parse(await readFile(join(dirname(path),'databases-migrations.json'),'utf8'));
assertHospitalityDevelopmentTarget(manifest,process.env.ERPX_ENVIRONMENT);
const appManifest=JSON.parse(await readFile(path,'utf8'));
const jobsManifest=JSON.parse(await readFile(join(dirname(path),'databases-jobs.json'),'utf8'));
assertHospitalityDevelopmentTarget(appManifest,process.env.ERPX_ENVIRONMENT);
assertHospitalityDevelopmentTarget(jobsManifest,process.env.ERPX_ENVIRONMENT);
for(const key of ['H001_DB','H002_DB','H003_DB']){
 const url=new URL(manifest.databases[key]);
 if(!url.username.endsWith('_migration'))throw new Error('MIGRATION_IDENTITY_REQUIRED');
 const db=new PostgresDatabase({connectionString:url.toString(),max:1});
 try{
  await migrateDatabase(db,'holding-write');
  // The installed migration identity owns these new tables. Provisioning normally
  // refreshes these grants; use the same two development runtime identities here.
  const roles=[appManifest,jobsManifest].map(m=>new URL(m.databases[key]).username);
  if(roles.some(role=>!/^erpx_dev_[a-z0-9_]+_(app|jobs)$/.test(role)))throw new Error('DEVELOPMENT_RUNTIME_IDENTITY_REQUIRED');
  for(const role of new Set(roles))await db.exec(`GRANT SELECT,INSERT,UPDATE,DELETE ON pos_register_catalog,pos_product_images,pos_sale_documents,pos_day_closes TO "${role}"`);
  console.log(`${key}: development migrations and runtime grants applied`);
 }finally{await db.close();}
}
