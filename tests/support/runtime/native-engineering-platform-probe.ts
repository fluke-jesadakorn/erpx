/** Real Platform enrollment and live guard, with only the release artifact test-stubbed. */
import assert from 'node:assert/strict';
import {chmod,readFile,writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {Pool} from 'pg';
const run=process.env.ERPX_FENCE_RUN,token=process.env.ERPX_FENCE_TOKEN;
if(!run||!/^[a-f0-9]{12}$/.test(run)||!token||!/^[a-f0-9]{32}$/.test(token))throw Error('OWNED_PLATFORM_PROBE_REQUIRED');
const directory=`/tmp/.runtime/browser/${run}`,source='a'.repeat(64),name=`erpx_dev_rust_${token}`,lease=`erpx-rust-test:${token}`;
Object.assign(process.env,{NODE_ENV:'production',ERPX_ENVIRONMENT:'local',PORT:'3104',ERPX_BROWSER_SIMULATION_PORT:'3104',ERPX_ENGINEERING_VERIFICATION_RUN:run,ERPX_DATA_DIR:directory,ERPX_DATABASE_MANIFEST:directory+'/databases.json',ERPX_PUBLIC_ORIGIN:'http://127.0.0.1:3104',BETTER_AUTH_SECRET:'isolated-test-secret-at-least-thirty-two-characters'});
const {getRuntime,closeRuntime}=await import('@erpx/server-runtime/adapters/runtime/env');
const {engineeringVerificationContext,enrollEngineeringVerification,isolatedEngineeringVerification}=await import('@erpx/server-platform_administration/public/engineering-verification');
const {PostgresDatabase}=await import('@erpx/server-runtime/adapters/runtime/postgres');
const roster=JSON.parse(await readFile(directory+'/native-engineering-leases.json','utf8'));
const db=getRuntime().H001_DB as InstanceType<typeof PostgresDatabase>,writer=new Pool({connectionString:`postgresql://erpx_test_admin:synthetic@127.0.0.1:5432/${name}`,max:1});
try{
 assert(engineeringVerificationContext()?.nativeRoster);const request={run,sourceFingerprint:source,softwareVersion:`release-${source}`,fileManifestSha256:roster.fileManifestSha256,companyIds:['synthetic-company']};
 await enrollEngineeringVerification(db,request);assert.equal(await isolatedEngineeringVerification(db,'synthetic-company'),true);
 const manifestPath=directory+'/databases.json',manifest=await readFile(manifestPath);await closeRuntime();
 try{const swapped=JSON.parse(manifest.toString());[swapped.databases.H001_DB,swapped.databases.H002_DB]=[swapped.databases.H002_DB,swapped.databases.H001_DB];await writeFile(manifestPath,JSON.stringify(swapped));assert.throws(()=>getRuntime(),/ENGINEERING_DATABASE_MANIFEST_MISMATCH/);}finally{await writeFile(manifestPath,manifest);}
 // The original db remains closed; subsequent assertions use the reloaded pool.
 const live=getRuntime().H001_DB as InstanceType<typeof PostgresDatabase>;

 const bare=new PostgresDatabase({connectionString:`postgresql://erpx_test_admin:synthetic@127.0.0.1:5432/${name}`,max:1});try{await assert.rejects(()=>isolatedEngineeringVerification(bare,'synthetic-company'),/NATIVE_ENGINEERING_FENCE_REQUIRED/);}finally{await bare.close();}
 await live.transaction(async tx=>{assert.equal(await isolatedEngineeringVerification(tx,'synthetic-company'),true);const c=await writer.connect();try{await c.query("SET lock_timeout='400ms'");await assert.rejects(()=>c.query(`COMMENT ON DATABASE "${name}" IS 'changed'`),e=>(e as {code?:string}).code==='55P03');}finally{c.release();}assert.equal(await isolatedEngineeringVerification(tx,'synthetic-company'),true);});
 await writer.query(`COMMENT ON DATABASE "${name}" IS 'changed'`);assert.equal(await isolatedEngineeringVerification(live,'synthetic-company'),false);await writer.query(`COMMENT ON DATABASE "${name}" IS '${lease}'`);assert.equal(await isolatedEngineeringVerification(live,'synthetic-company'),true);
 const path=directory+'/native-engineering-leases.json',original=await readFile(path);try{await assert.rejects(()=>live.transaction(async tx=>{await tx.prepare('INSERT INTO probe_effect(id) VALUES(?)').bind(1).run();await writeFile(path,'{}');}),/NATIVE_ENGINEERING_ATTESTATION_CHANGED/);}finally{await writeFile(path,original);}
 assert.equal(Number((await writer.query('SELECT count(*) n FROM probe_effect')).rows[0].n),0);
 await writer.query('UPDATE engineering_verification_contexts SET enabled=false WHERE company_id=$1',['synthetic-company']);assert.equal(await isolatedEngineeringVerification(live,'synthetic-company'),false);
 await writer.query('UPDATE engineering_verification_contexts SET enabled=true,created_at=0,expires_at=1 WHERE company_id=$1',['synthetic-company']);assert.equal(await isolatedEngineeringVerification(live,'synthetic-company'),false);
 await enrollEngineeringVerification(live,request);assert.equal(await isolatedEngineeringVerification(live,'synthetic-company'),true);
 await chmod(path,0o644);try{assert.equal(await isolatedEngineeringVerification(live,'synthetic-company'),false);await assert.rejects(()=>live.prepare('SELECT 1').first(),/NATIVE_ENGINEERING_ATTESTATION_CHANGED/);}finally{await chmod(path,0o600);}
 await closeRuntime();const attestationPath=directory+'/engineering-attestation.json',attestationBytes=await readFile(attestationPath),rosterBytes=await readFile(path);
 try{for(const change of ['oid','boot'] as const){const edited=JSON.parse(rosterBytes.toString());if(change==='oid')edited.entries[2].oid+=1000000;else edited.postmasterStartedMs+=1;const changed=JSON.stringify(edited),attestation=JSON.parse(attestationBytes.toString());attestation.nativeLeaseSha256=createHash('sha256').update(changed).digest('hex');await writeFile(path,changed);await writeFile(attestationPath,JSON.stringify(attestation));const currentDb=getRuntime().H001_DB as InstanceType<typeof PostgresDatabase>;assert.equal(await isolatedEngineeringVerification(currentDb,'synthetic-company'),false);await assert.rejects(()=>enrollEngineeringVerification(currentDb,request),/EXACT_ENGINEERING_ADMIN_DATABASE_REQUIRED/);await closeRuntime();}}finally{await writeFile(path,rosterBytes);await writeFile(attestationPath,attestationBytes);}


 console.log('NATIVE_PLATFORM_FENCE_PASS');
}finally{await writer.end();await closeRuntime();}
