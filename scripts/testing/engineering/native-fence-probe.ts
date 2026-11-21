/** Disposable PG18 proof of the native engineering Runtime transaction fence. */
import {execFile} from 'node:child_process';
import assert from 'node:assert/strict';
import {randomBytes,createHash} from 'node:crypto';
import {mkdir,mkdtemp,readFile,rm,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {promisify} from 'node:util';
import {grantNativeEngineeringCatalogLocks} from './native-catalog-grants';
const exec=promisify(execFile),root=resolve('.'),dockerHost=process.env.ERPX_FENCE_DOCKER_HOST,clientImage=process.env.ERPX_FENCE_CLIENT_IMAGE;
if(!dockerHost?.startsWith('unix:///')||!clientImage||!/^sha256:[a-f0-9]{64}$/.test(clientImage))throw Error('OWNED_PROBE_DOCKER_AND_CLIENT_REQUIRED');
const postgres='sha256:979bccfde35445db618c65107d27d08c6c21339d8dc3ad4c830c6633947be0e4',id=randomBytes(16).toString('hex'),run=randomBytes(6).toString('hex'),label='com.erpx.native-fence-proof',env={PATH:process.env.PATH??'',DOCKER_HOST:dockerHost,NODE_ENV:'test' as const};
const temp=await mkdtemp(join(tmpdir(),'erpx-native-fence-'));const owned:string[]=[];let success=false;let evidenceRecord:Record<string,unknown>|undefined;
const sources=['scripts/testing/engineering/native-fence-probe.ts','scripts/testing/engineering/native-catalog-grants.ts','scripts/testing/engineering/native-cohort.ts','tests/support/runtime/native-engineering-fence-probe.ts','tests/support/runtime/native-engineering-cohort-holder.ts','packages/server-runtime/src/adapters/runtime/postgres.ts','packages/server-runtime/src/domain/native-engineering-roster.ts','tests/support/runtime/native-engineering-platform-probe.ts','packages/server-runtime/src/adapters/runtime/env.ts','packages/server-platform_administration/src/adapters/engineering-verification.ts','packages/server-platform_administration/src/application/engineering-verification.ts','packages/server-platform_administration/src/domain/engineering-verification.ts','migrations/postgres/holding-write/0965_engineering_verification_contexts.sql'];
async function sourceHashes(){return Object.fromEntries(await Promise.all(sources.map(async path=>[path,createHash('sha256').update(await readFile(join(root,path))).digest('hex')])));}
const sourceBefore=await sourceHashes();
async function docker(args:string[]){return exec('docker',args,{env,maxBuffer:1024*1024,timeout:30000});}
async function psql(container:string,database:string,sql:string){return docker(['exec',container,'psql','-XAt','-v','ON_ERROR_STOP=1','-U','postgres','-d',database,'-c',sql]);}
try{
 const bundled=join(temp,'probe.js'),holder=join(temp,'holder.js'),platform=join(temp,'platform.js');for(const [source,output] of [['native-engineering-fence-probe.ts',bundled],['native-engineering-cohort-holder.ts',holder]] as const)await exec(process.execPath,['build',join(root,'tests/support/runtime',source),'--target=bun','--outfile',output],{cwd:root,maxBuffer:1024*1024});
 const reloaded=join(temp,'native-fence-reloaded.js');await exec(process.execPath,['build',join(root,'packages/server-runtime/src/adapters/runtime/postgres.ts'),'--target=bun','--outfile',reloaded],{cwd:root,maxBuffer:1024*1024});
 await exec(process.execPath,['build',join(root,'tests/support/runtime/native-engineering-platform-probe.ts'),'--target=bun','--conditions=react-server','--external','@erpx/catalogue/release-artifact.json','--define','process.env.NODE_ENV=\"production\"','--outfile',platform],{cwd:root,maxBuffer:1024*1024});
 const server=(await docker(['run','-d','--name',`erpx-native-fence-${id}`,'--label',`${label}=${id}`,'--label',`com.erpx.native-engineering-run=${run}`,'--label',`com.erpx.native-engineering-nonce=${id}`,'--network','none','--tmpfs','/var/lib/postgresql:rw,nosuid,size=512m','-e','POSTGRES_HOST_AUTH_METHOD=trust',postgres])).stdout.trim();owned.push(server);
 if(!/^[a-f0-9]{64}$/.test(server))throw Error('PROBE_SERVER_ID_INVALID');
 let ready=false;for(let n=0;n<120;n++){const log=await docker(['logs',server]);const text=log.stdout+log.stderr,marker=text.indexOf('PostgreSQL init process complete');if(marker>=0&&text.slice(marker).includes('database system is ready to accept connections')){ready=true;break;}await Bun.sleep(250);}if(!ready)throw Error('PROBE_POSTGRES_NOT_READY');
 await psql(server,'postgres','CREATE ROLE erpx_test_admin LOGIN NOSUPERUSER NOCREATEROLE CREATEDB');
 const client=(await docker(['run','-d','--name',`erpx-native-fence-client-${id}`,'--label',`${label}=${id}`,'--network',`container:${server}`,'--entrypoint','sleep',clientImage,'infinity'])).stdout.trim();owned.push(client);
 if(!/^[a-f0-9]{64}$/.test(client))throw Error('PROBE_CLIENT_ID_INVALID');
 await docker(['cp',holder,`${client}:/tmp/holder.js`]);await docker(['exec','-d','-e',`ERPX_FENCE_RUN=${run}`,client,'sh','-c','bun /tmp/holder.js > /tmp/holder.log 2>&1']);
 let created=false;for(let n=0;n<120;n++){const log=await docker(['exec',client,'cat','/tmp/holder.log']);if(log.stdout.includes('EIGHT_READY')){created=true;break;}if(log.stdout.includes('error:'))throw Error('PROBE_COHORT_FAILED');await Bun.sleep(250);}if(!created)throw Error('PROBE_COHORT_TIMEOUT');
 const rosterPath=join(temp,'roster.json');await docker(['cp',`${client}:/tmp/.runtime/browser/${run}/native-engineering-leases.json`,rosterPath]);const roster=JSON.parse(await readFile(rosterPath,'utf8'));
 const grant={dockerHost,containerId:server,run,nonce:id,sourceFingerprint:'a'.repeat(64),fileManifestSha256:roster.fileManifestSha256,roster};
 await assert.rejects(()=>grantNativeEngineeringCatalogLocks({...grant,nonce:'0'.repeat(32)}),/NATIVE_CATALOG_GRANT_CONTAINER_INVALID/);
 const targetBefore=`erpx_dev_rust_${roster.entries[2].token}`;if((await psql(server,targetBefore,"SELECT has_table_privilege('erpx_test_admin','pg_catalog.pg_database','MAINTAIN')")).stdout.trim()!=='f')throw Error('PROBE_NEGATIVE_GRANT_CHANGED');
 await grantNativeEngineeringCatalogLocks(grant);
 const target=`erpx_dev_rust_${roster.entries[2].token}`;
 await psql(server,target,"SET ROLE erpx_test_admin;CREATE TABLE organization_nodes(id text PRIMARY KEY,kind text NOT NULL);INSERT INTO organization_nodes(id,kind) VALUES('synthetic-company','company');CREATE TABLE probe_effect(id int)");
 await docker(['cp',join(root,'migrations/postgres/holding-write/0965_engineering_verification_contexts.sql'),`${server}:/tmp/engineering-context.sql`]);
 await docker(['exec',server,'psql','-XAt','-v','ON_ERROR_STOP=1','-U','erpx_test_admin','-d',target,'-f','/tmp/engineering-context.sql']);
 const artifactDir=join(temp,'catalogue');await mkdir(artifactDir);await writeFile(join(artifactDir,'package.json'),JSON.stringify({name:'@erpx/catalogue',type:'module',exports:{'./release-artifact.json':'./release-artifact.json'}}));await writeFile(join(artifactDir,'release-artifact.json'),JSON.stringify({softwareVersion:`release-${'a'.repeat(64)}`}));await docker(['exec',client,'mkdir','-p','/tmp/node_modules/@erpx']);await docker(['cp',artifactDir,`${client}:/tmp/node_modules/@erpx/catalogue`]);
 await docker(['cp',platform,`${client}:/tmp/platform.js`]);const platformResult=await docker(['exec','-e',`ERPX_FENCE_RUN=${run}`,'-e',`ERPX_FENCE_TOKEN=${roster.entries[2].token}`,client,'bun','/tmp/platform.js']);if(platformResult.stdout.trim()!=='NATIVE_PLATFORM_FENCE_PASS')throw Error('PROBE_PLATFORM_ASSERTIONS_FAILED');
 await docker(['cp',reloaded,`${client}:/tmp/native-fence-reloaded.js`]);await docker(['cp',bundled,`${client}:/tmp/probe.js`]);const result=await docker(['exec','-e',`ERPX_FENCE_TOKEN=${roster.entries[2].token}`,client,'bun','/tmp/probe.js']);if(result.stdout.trim()!=='NATIVE_FENCE_ACTUAL_ADAPTER_PASS')throw Error('PROBE_ASSERTIONS_FAILED');
 await docker(['exec',client,'touch','/tmp/release']);let cleaned=false;for(let n=0;n<120;n++){const log=await docker(['exec',client,'cat','/tmp/holder.log']);if(log.stdout.includes('EIGHT_CLEANED')){cleaned=true;break;}if(log.stdout.includes('error:'))throw Error('PROBE_COHORT_CLEANUP_FAILED');await Bun.sleep(250);}if(!cleaned)throw Error('PROBE_COHORT_CLEANUP_TIMEOUT');
 if((await psql(server,'postgres',"SELECT count(*) FROM pg_database WHERE datname LIKE 'erpx_dev_rust_%'")).stdout.trim()!=='0')throw Error('PROBE_COHORT_LEAKED');
 const sourceAfter=await sourceHashes();if(JSON.stringify(sourceBefore)!==JSON.stringify(sourceAfter))throw Error('PROBE_SOURCE_CHANGED');
 evidenceRecord={postgresImage:postgres,clientImage,sourceHashes:sourceAfter,isolatedNetwork:true,eightLeasesVerifiedAndRemoved:true,platformEnrollmentAndLiveGuard:true,invalidGrantRejected:true,releaseArtifactTestStubbed:true,catalogMaintainWithoutUpdate:true,commentBlockedUntilCommit:true,unguardedQueryDenied:true,currentnessCallbackFailureRolledBack:true};
 success=true;
}finally{
 for(const container of owned.reverse()){
  const value=(await docker(['inspect','--format',`{{index .Config.Labels "${label}"}}`,container])).stdout.trim();if(value!==id)throw Error('PROBE_CONTAINER_IDENTITY_CHANGED');
  await docker(['stop','-t','5',container]);await docker(['rm',container]);
 }
 await rm(temp,{recursive:true,force:true});
}
if(success){if(process.env.ERPX_FENCE_PROOF)await writeFile(resolve(process.env.ERPX_FENCE_PROOF),JSON.stringify({...evidenceRecord,cleanupComplete:true},null,2),{mode:0o600,flag:'wx'});console.log('native engineering isolated fence proof passed');}
