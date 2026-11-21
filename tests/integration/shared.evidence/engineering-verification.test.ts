import {beforeAll,afterAll,beforeEach,afterEach,expect,it,vi} from 'vitest';
import {chmod,mkdir,writeFile,rm,readFile} from 'node:fs/promises';
import {createHash,randomBytes} from 'node:crypto';
import {join,resolve} from 'node:path';
import {Pool} from 'pg';
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
import {applyMigrations,env} from "../../support/runtime-env";
import {runWithRequestMemo} from "@erpx/server-runtime/public/runtime/request-memo";
import {engineeringVerificationContext,enrollEngineeringVerification,isolatedEngineeringVerification} from "@erpx/server-platform_administration/public/engineering-verification";
import { assertProductionModuleEnabled, assertProductionFeatureRoute, productionFeatureGuard, scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {legalCompanyCapabilityReady} from "@erpx/server-legal/public/legal/runtime-gate";
import {companyReleaseStatus} from "@erpx/server-platform_administration/public/release-acceptance";
import {getRuntime,closeRuntime} from "@erpx/server-runtime/adapters/runtime/env";
import {NextRequest} from 'next/server';
import {proxy} from "@erpx/workplace/proxy";
import {frontOrigin} from "@erpx/feature-shared/lib/front-host";
import {consoleOrigin,marketingOrigin} from "@erpx/feature-runtime/lib/erpx-hosts";
const source='a'.repeat(64),version=`release-${source}`,run=randomBytes(6).toString('hex'),directory=resolve('.runtime/browser',run),databaseName=`erpx_browser_${run}_h003_db`;
vi.mock("@erpx/catalogue/release-artifact.json",()=>({default:{softwareVersion:`release-${'a'.repeat(64)}`}}));
let db:PostgresDatabase,admin:Pool,manifestSha:string;
const key=()=>crypto.randomUUID();
beforeAll(async()=>{
 const url=new URL(process.env.ERPX_TEST_ADMIN_URL!);if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!=='/postgres')throw new Error('LOCAL_TEST_ADMIN_REQUIRED');
 admin=new Pool({connectionString:url.href,max:1});await admin.query(`CREATE DATABASE "${databaseName}" TEMPLATE template0`);url.pathname=databaseName;db=new PostgresDatabase({connectionString:url.href,max:2});
 await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await mkdir(join(directory,'release'),{recursive:true});
 const manifest=JSON.stringify({'server.js':'b'.repeat(64)});manifestSha=createHash('sha256').update(manifest).digest('hex');
 await writeFile(join(directory,'release/release-files.json'),manifest);await writeFile(join(directory,'release/release.json'),JSON.stringify({sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha}));
 await writeFile(join(directory,'engineering-attestation.json'),JSON.stringify({scope:'isolated-engineering',run,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha}));
 for(const id of ['enabled','disabled'])await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,?,?)").bind(id,id,`Synthetic ${id}`,id).run();
 for(const id of ['enabled','disabled'])await db.prepare('INSERT INTO legal_settings(company_id,enabled,ai_enabled) VALUES(?,true,true)').bind(id).run();
});
beforeEach(()=>{vi.stubEnv('NODE_ENV','production');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('PORT','3104');vi.stubEnv('ERPX_BROWSER_SIMULATION_PORT','3104');vi.stubEnv('ERPX_ENGINEERING_VERIFICATION_RUN',run);vi.stubEnv('ERPX_DATA_DIR',directory);});
afterEach(async()=>{await closeRuntime();vi.unstubAllEnvs();});
afterAll(async()=>{await db?.close();if(admin){const row=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[databaseName])).rows[0];if(row?.owner!=='erpx_test_admin')throw new Error('ISOLATED_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${databaseName}"`);await admin.end();}await rm(directory,{recursive:true,force:true});});
const enrollment=()=>({run,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha,companyIds:['enabled'],disabledCompanyIds:['disabled']});
it('authorizes only explicitly enrolled disposable Companies on the attested build while certification stays pending',async()=>{
 await enrollEngineeringVerification(db,enrollment());expect(engineeringVerificationContext()).toMatchObject({scope:'isolated-engineering',softwareVersion:version});
 expect(await isolatedEngineeringVerification(db,'enabled')).toBe(true);expect(await isolatedEngineeringVerification(db,'disabled')).toBe(false);
 await expect(assertProductionModuleEnabled(db,'enabled','legal')).resolves.toBeUndefined();await expect(assertProductionModuleEnabled(db,'enabled','manufacturing')).resolves.toBeUndefined();
 await expect(assertProductionFeatureRoute(db,'enabled','people/payroll','POST')).resolves.toBeUndefined();
 expect(await legalCompanyCapabilityReady(db,'enabled','operations')).toBe(true);expect(await legalCompanyCapabilityReady(db,'disabled','operations')).toBe(false);
 expect(await db.prepare(`SELECT ${scheduledFeaturePredicate('?', 'operations')} enabled`).bind('enabled').first('enabled')).toBe(true);
 await db.batch(productionFeatureGuard(db,key(),'enabled','legal'));await db.batch(productionFeatureGuard(db,key(),'enabled','agent'));
 expect((await companyReleaseStatus(db,'enabled',undefined,'legal-th-operations-v1')).state).toBe('pending');expect(await db.prepare('SELECT count(*) n FROM company_release_acceptances').first('n')).toBe(0);
});
it('requires exact physical databases even with an otherwise matching local attestation',async()=>{
 await applyMigrations(env.H001_DB,env.STAFF_TEST_MIGRATIONS);
 expect(await isolatedEngineeringVerification(env.H001_DB,'enabled')).toBe(false);
 await expect(assertProductionModuleEnabled(env.H001_DB,'enabled','legal')).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
 await expect(enrollEngineeringVerification(env.H001_DB,enrollment())).rejects.toThrow('EXACT_ENGINEERING_ADMIN_DATABASE_REQUIRED');
});
it('refuses production, staging, wrong ports, changed run IDs and redirected data directories',async()=>{
 for(const override of [{ERPX_ENVIRONMENT:'production'},{ERPX_ENVIRONMENT:'staging'},{PORT:'3101'},{ERPX_BROWSER_SIMULATION_PORT:'3103'},{ERPX_ENGINEERING_VERIFICATION_RUN:'other'},{ERPX_DATA_DIR:resolve('.runtime/browser',`${run}-other`)}])expect(engineeringVerificationContext({...process.env,...override})).toBeNull();
 vi.stubEnv('ERPX_ENVIRONMENT','production');expect(await isolatedEngineeringVerification(db,'enabled')).toBe(false);await expect(assertProductionModuleEnabled(db,'enabled','legal')).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
});
it('rejects a changed frozen manifest or mismatched source/build attestation',async()=>{
 const path=join(directory,'release/release-files.json'),original=await readFile(path);try{await writeFile(path,'{}');expect(engineeringVerificationContext()).toBeNull();expect(await isolatedEngineeringVerification(db,'enabled')).toBe(false);}finally{await writeFile(path,original);}
 await expect(enrollEngineeringVerification(db,{...enrollment(),sourceFingerprint:'c'.repeat(64),softwareVersion:`release-${'c'.repeat(64)}`})).rejects.toThrow('ENGINEERING_RUN_BUILD_CHANGED');
 await expect(enrollEngineeringVerification(db,{...enrollment(),softwareVersion:'unbuilt'})).rejects.toThrow('ENGINEERING_SOURCE_BUILD_MISMATCH');
});
it('withdraws and expires engineering execution without widening another Company or creating acceptance',async()=>{
 await db.prepare('UPDATE engineering_verification_contexts SET expires_at=? WHERE company_id=? AND run_id=?').bind(Date.now()-1,'enabled',run).run();expect(await isolatedEngineeringVerification(db,'enabled')).toBe(false);
 await expect(db.batch(productionFeatureGuard(db,key(),'enabled','legal'))).rejects.toThrow();
 await enrollEngineeringVerification(db,enrollment());await db.prepare('UPDATE engineering_verification_contexts SET enabled=false WHERE company_id=? AND run_id=?').bind('enabled',run).run();
 expect(await legalCompanyCapabilityReady(db,'enabled','operations')).toBe(false);expect(await db.prepare(`SELECT ${scheduledFeaturePredicate('?', 'operations')} enabled`).bind('disabled').first('enabled')).toBe(false);
 await expect(enrollEngineeringVerification(db,{...enrollment(),companyIds:['missing']})).rejects.toThrow('ENGINEERING_COMPANY_NOT_FOUND');
 expect(await db.prepare('SELECT count(*) n FROM company_release_acceptances').first('n')).toBe(0);
});

async function bootstrap(){
 const bindings=['CONTROL_DB','CONTROL_READ_DB','H001_DB','H001_READ_DB','H002_DB','H002_READ_DB','H003_DB','H003_READ_DB'];
 const databaseUrl=new URL(process.env.ERPX_TEST_ADMIN_URL!),databases=Object.fromEntries(bindings.map(binding=>{const url=new URL(databaseUrl);url.pathname=`erpx_browser_${run}_${binding.toLowerCase()}`;return [binding,url.href];}));
 await writeFile(join(directory,'databases.json'),JSON.stringify({version:1,databases,slots:[1,2,3].map(number=>({id:`h00${number}`,write:`H00${number}_DB`,read:`H00${number}_READ_DB`,files:`H00${number}_FILES`}))}));
 vi.stubEnv('ERPX_DATABASE_MANIFEST',join(directory,'databases.json'));vi.stubEnv('BETTER_AUTH_SECRET','isolated-engineering-bootstrap-secret-32-characters');vi.stubEnv('ERPX_PUBLIC_ORIGIN','http://127.0.0.1:3104');vi.stubEnv('ERPX_MARKETING_ORIGIN','http://dev-erpx.localhost:3104');vi.stubEnv('ERPX_FRONT_PMS_ORIGIN','http://dev-pms.localhost:3104');vi.stubEnv('ERPX_FRONT_POS_ORIGIN','http://dev-pos.localhost:3104');
 for(const name of ['ERPX_MINIO_ENDPOINT','ERPX_MINIO_BUCKET','ERPX_MINIO_ACCESS_KEY','ERPX_MINIO_SECRET_KEY','ERPX_MIGRATION_DATABASE_MANIFEST'])vi.stubEnv(name,'');
 // The optional migration path is absent, exactly as in a disposable run.
 delete process.env.ERPX_MIGRATION_DATABASE_MANIFEST;
 return databases;
}
it('boots a compiled local sandbox only with attested exact databases and loopback 3104 public hosts',async()=>{
 await bootstrap();const runtime=getRuntime();expect(runtime.ERPX_ENVIRONMENT).toBe('local');expect(runtime.ERPX_PUBLIC_ORIGIN).toBe('http://127.0.0.1:3104');expect(runtime.ERPX_MARKETING_ORIGIN).toBe('http://dev-erpx.localhost:3104');expect(runtime.ERPX_FRONT_POS_ORIGIN).toBe('http://dev-pos.localhost:3104');
 expect((runtime.H002_DB as PostgresDatabase).pool.options.connectionString).toContain(`/erpx_browser_${run}_h002_db`);expect((runtime.H003_DB as PostgresDatabase).pool.options.connectionString).toContain(`/erpx_browser_${run}_h003_db`);expect((runtime.H003_READ_DB as PostgresDatabase).pool.options.connectionString).toContain(`/erpx_browser_${run}_h003_read_db`);
});
it('routes attested compiled console marketing and Front requests without relaxing ordinary HTTPS or redirecting HTTP to itself',async()=>{
 await bootstrap();
 expect(frontOrigin('pms')?.origin).toBe('http://dev-pms.localhost:3104');expect(frontOrigin('pos')?.origin).toBe('http://dev-pos.localhost:3104');
 expect(consoleOrigin()?.origin).toBe('http://127.0.0.1:3104');expect(marketingOrigin()?.origin).toBe('http://dev-erpx.localhost:3104');
 for(const host of ['127.0.0.1:3104','dev-pms.localhost:3104','dev-pos.localhost:3104','dev-erpx.localhost:3104']) {
  const response=proxy(new NextRequest(`http://${host}/api/health`,{headers:{host,'x-forwarded-proto':'http'}}));expect(response.status).toBe(200);expect(response.headers.get('location')).toBeNull();
 }
 expect(proxy(new NextRequest('http://127.0.0.1:3104/login',{headers:{host:'127.0.0.1:3104','x-forwarded-proto':'http'}})).status).toBe(200);
 for(const value of ['http://external.invalid:3104','http://dev-pos.localhost:3101']) {vi.stubEnv('ERPX_FRONT_POS_ORIGIN',value);expect(()=>frontOrigin('pos')).toThrow('INVALID_FRONT_ORIGIN');}
 vi.stubEnv('ERPX_FRONT_POS_ORIGIN','http://dev-pos.localhost:3104');vi.stubEnv('ERPX_ENGINEERING_VERIFICATION_RUN','');
 expect(()=>frontOrigin('pos')).toThrow('INVALID_FRONT_ORIGIN');expect(()=>consoleOrigin()).toThrow('INVALID_ERPX_ORIGIN');expect(()=>marketingOrigin()).toThrow('INVALID_ERPX_ORIGIN');
});
it('refuses compiled sandbox bootstrap before opening pools when manifest targets an installed database or external host',async()=>{
 const databases=await bootstrap(),path=join(directory,'databases.json'),original=await readFile(path);
 const changed={...databases,H002_DB:databases.H002_DB.replace(`erpx_browser_${run}_h002_db`,'erpx_dev_h002_db')};
 await writeFile(path,JSON.stringify({version:1,databases:changed,slots:[]}));expect(()=>getRuntime()).toThrow('ENGINEERING_DATABASE_MANIFEST_MISMATCH');await writeFile(path,original);
 const parsed=JSON.parse(original.toString());delete parsed.databases.H003_READ_DB;await writeFile(path,JSON.stringify(parsed));expect(()=>getRuntime()).toThrow('ENGINEERING_DATABASE_MANIFEST_MISMATCH');await writeFile(path,original);
 await writeFile(join(directory,'databases-migrations.json'),JSON.stringify({...parsed,databases:{...databases,H003_DB:databases.H003_DB.replace(`erpx_browser_${run}_h003_db`,'erpx_dev_h003_db')}}));expect(()=>getRuntime()).toThrow('ENGINEERING_MIGRATION_MANIFEST_MISMATCH');await rm(join(directory,'databases-migrations.json'));
 vi.stubEnv('ERPX_FRONT_POS_ORIGIN','http://external.invalid:3104');expect(()=>getRuntime()).toThrow('INVALID_FRONT_ORIGIN');vi.stubEnv('ERPX_FRONT_POS_ORIGIN','http://dev-pos.localhost:3101');expect(()=>getRuntime()).toThrow('INVALID_FRONT_ORIGIN');
 vi.stubEnv('ERPX_FRONT_POS_ORIGIN','http://dev-pos.localhost:3104');vi.stubEnv('ERPX_MINIO_ENDPOINT','https://external-storage.invalid');expect(()=>getRuntime()).toThrow('ENGINEERING_LOCAL_FILES_REQUIRED');
});
it('preserves ordinary compiled production and staging HTTPS requirements when engineering attestation is absent',async()=>{
 await bootstrap();vi.stubEnv('ERPX_ENGINEERING_VERIFICATION_RUN','');expect(()=>getRuntime()).toThrow('INVALID_PUBLIC_ORIGIN');
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://production.invalid');vi.stubEnv('ERPX_MARKETING_ORIGIN','');vi.stubEnv('ERPX_FRONT_PMS_ORIGIN','');vi.stubEnv('ERPX_FRONT_POS_ORIGIN','');expect(getRuntime().ERPX_ENVIRONMENT).toBe('production');await closeRuntime();
 vi.stubEnv('ERPX_ENVIRONMENT','staging');vi.stubEnv('ERPX_PUBLIC_ORIGIN','http://127.0.0.1:3104');expect(()=>getRuntime()).toThrow('INVALID_PUBLIC_ORIGIN');vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://staging.invalid');expect(getRuntime().ERPX_ENVIRONMENT).toBe('staging');
});

it('reads changed manifest bytes and attestation fields afresh even within one HTTP invocation',async()=>{
 const manifestPath=join(directory,'release/release-files.json'),attestationPath=join(directory,'engineering-attestation.json');
 const manifest=await readFile(manifestPath),attestation=await readFile(attestationPath);
 try{await runWithRequestMemo(async()=>{
  expect(engineeringVerificationContext()).not.toBeNull();
  const changed=Buffer.from(manifest); changed[changed.indexOf('b'.repeat(64))]='c'.charCodeAt(0);
  expect(changed.length).toBe(manifest.length); await writeFile(manifestPath,changed);
  expect(engineeringVerificationContext()).toBeNull();
  await writeFile(manifestPath,manifest); expect(engineeringVerificationContext()).not.toBeNull();
  await writeFile(attestationPath,JSON.stringify({...JSON.parse(attestation.toString()),softwareVersion:'unbuilt'}));
  expect(engineeringVerificationContext()).toBeNull();
  await writeFile(attestationPath,attestation); expect(engineeringVerificationContext()).not.toBeNull();
 });}finally{await writeFile(manifestPath,manifest);await writeFile(attestationPath,attestation);}
});
it('does not memoize Company enablement or expiry inside a request that reused the artifact digest',async()=>{
 await enrollEngineeringVerification(db,enrollment());
 try{await runWithRequestMemo(async()=>{
  expect(await isolatedEngineeringVerification(db,'enabled')).toBe(true);
  await db.prepare('UPDATE engineering_verification_contexts SET enabled=false WHERE company_id=? AND run_id=?').bind('enabled',run).run();
  expect(await isolatedEngineeringVerification(db,'enabled')).toBe(false);
  await enrollEngineeringVerification(db,enrollment()); expect(await isolatedEngineeringVerification(db,'enabled')).toBe(true);
  await db.prepare('UPDATE engineering_verification_contexts SET expires_at=? WHERE company_id=? AND run_id=?').bind(Date.now()-1,'enabled',run).run();
  expect(await isolatedEngineeringVerification(db,'enabled')).toBe(false);
 });}finally{await enrollEngineeringVerification(db,enrollment());}
});

it('rejects unfenced native leases and requires the exact private eight-database roster',async()=>{
 const nativeRun=randomBytes(6).toString('hex'),nativeDirectory=resolve('.runtime/browser',nativeRun);
 await mkdir(join(nativeDirectory,'release'),{recursive:true,mode:0o700});
 const releaseBytes=JSON.stringify({'server.js':'b'.repeat(64)}),digest=createHash('sha256').update(releaseBytes).digest('hex');
 await writeFile(join(nativeDirectory,'release/release-files.json'),releaseBytes);
 await writeFile(join(nativeDirectory,'release/release.json'),JSON.stringify({sourceFingerprint:source,softwareVersion:version,fileManifestSha256:digest}));
 const {createNativeEngineeringCohort}=await import('../../../scripts/testing/engineering/native-cohort');
 const cohort=await createNativeEngineeringCohort({directory:nativeDirectory,run:nativeRun,sourceFingerprint:source,fileManifestSha256:digest,adminUrl:process.env.ERPX_TEST_ADMIN_URL!});
 const path=join(nativeDirectory,'engineering-attestation.json'),rosterPath=join(nativeDirectory,'native-engineering-leases.json');
 const attestation={scope:'isolated-engineering',run:nativeRun,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:digest,nativeLeaseSha256:cohort.rosterSha256};
 await writeFile(path,JSON.stringify(attestation),{mode:0o600});
 const nativeDb=new PostgresDatabase({connectionString:cohort.databases.H001_DB!,max:2});
 try{
  await applyMigrations(nativeDb,env.STAFF_TEST_MIGRATIONS);await nativeDb.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('native-company','company','native-company','Native synthetic','native-company')").run();
  vi.stubEnv('ERPX_ENGINEERING_VERIFICATION_RUN',nativeRun);vi.stubEnv('ERPX_DATA_DIR',nativeDirectory);vi.stubEnv('ERPX_DATABASE_MANIFEST',join(nativeDirectory,'databases.json'));vi.stubEnv('BETTER_AUTH_SECRET','isolated-engineering-bootstrap-secret-32-characters');vi.stubEnv('ERPX_PUBLIC_ORIGIN','http://127.0.0.1:3104');vi.stubEnv('ERPX_MARKETING_ORIGIN','');vi.stubEnv('ERPX_FRONT_PMS_ORIGIN','');vi.stubEnv('ERPX_FRONT_POS_ORIGIN','');
  vi.stubEnv('ERPX_MIGRATION_DATABASE_MANIFEST','');delete process.env.ERPX_MIGRATION_DATABASE_MANIFEST;
  expect(getRuntime().ERPX_ENVIRONMENT).toBe('local');await closeRuntime();
  const request={run:nativeRun,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:digest,companyIds:['native-company']};
  // A bare native lease on the shared test cluster receives no catalog grants
  // and must never acquire engineering authorization. The same-unit COMMENT
  // contention, expiry and file-drift checks run in the dedicated network-none
  // 8-DB driver tests/support/runtime/native-engineering-platform-probe.ts.
  expect(()=>enrollEngineeringVerification(nativeDb,request)).toThrow('NATIVE_ENGINEERING_FENCE_REQUIRED');
  await expect(isolatedEngineeringVerification(nativeDb,'native-company')).rejects.toThrow('NATIVE_ENGINEERING_FENCE_REQUIRED');
  const roster=await readFile(rosterPath),changed=JSON.parse(roster.toString());changed.entries[2].oid+=1000000;
  await writeFile(rosterPath,JSON.stringify(changed),{mode:0o600});expect(engineeringVerificationContext()).toBeNull();expect(await isolatedEngineeringVerification(nativeDb,'native-company')).toBe(false);
  await writeFile(rosterPath,roster);await chmod(rosterPath,0o644);expect(engineeringVerificationContext()).toBeNull();await chmod(rosterPath,0o600);expect(engineeringVerificationContext()).not.toBeNull();
  const originalManifest=await readFile(join(nativeDirectory,'databases.json'));try{const swapped=JSON.parse(originalManifest.toString());[swapped.databases.H001_DB,swapped.databases.H002_DB]=[swapped.databases.H002_DB,swapped.databases.H001_DB];await writeFile(join(nativeDirectory,'databases.json'),JSON.stringify(swapped));expect(()=>getRuntime()).toThrow('ENGINEERING_DATABASE_MANIFEST_MISMATCH');}finally{await writeFile(join(nativeDirectory,'databases.json'),originalManifest);}

 }finally{await closeRuntime();await nativeDb.close();await cohort.cleanup();await rm(nativeDirectory,{recursive:true,force:true});}
},120000);
