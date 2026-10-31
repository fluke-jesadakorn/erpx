/** Own a single compiled-engineering native cohort; never connects to host Postgres. */
import {execFile} from 'node:child_process';
import {randomBytes} from 'node:crypto';
import {chmod,mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {promisify} from 'node:util';
import {grantNativeEngineeringCatalogLocks} from './native-catalog-grants';
const exec=promisify(execFile),root=resolve('.'),dockerHost=process.env.ERPX_COHORT_DOCKER_HOST,clientImage=process.env.ERPX_COHORT_CLIENT_IMAGE,caseVolume=process.env.ERPX_COHORT_CASE_VOLUME,output=process.env.ERPX_COHORT_OUTPUT;
if(!dockerHost?.startsWith('unix:///')||!clientImage||!/^sha256:[a-f0-9]{64}$/.test(clientImage)||caseVolume!=='erpx-native-ssr-case-20261031'||!output||!resolve(output).startsWith(join(root,'.scratch/dev')+'/'))throw Error('OWNED_COHORT_CONFIGURATION_REQUIRED');
const postgres='sha256:979bccfde35445db618c65107d27d08c6c21339d8dc3ad4c830c6633947be0e4',nonce=randomBytes(16).toString('hex'),label='com.erpx.engineering-case',labelValue='native-ssr-20261031',env={PATH:process.env.PATH??'',DOCKER_HOST:dockerHost,NODE_ENV:'test' as const};
const docker=async(args:string[])=>exec('docker',args,{env,maxBuffer:1024*1024,timeout:30000});
const temp=await mkdtemp(join(tmpdir(),'erpx-native-cohort-driver-')),owned:string[]=[];let client='',holderStarted=false,run='';
async function waitFor(path:string,limit=240){for(let i=0;i<limit;i++){try{await docker(['exec',client,'test','-f',path]);return;}catch{await Bun.sleep(250);}}throw Error('COHORT_PHASE_TIMEOUT');}
try{
 const volume=JSON.parse((await docker(['volume','inspect',caseVolume])).stdout);if(volume.length!==1||volume[0].Labels?.[label]!==labelValue)throw Error('COHORT_CASE_VOLUME_INVALID');
 // The run is already bound to the reviewed release in this owned volume.
 const reader=(await docker(['run','--rm','--network','none','--mount',`type=volume,src=${caseVolume},dst=/case,readonly`,clientImage,'cat','/case/run'])).stdout.trim();
 if(!/^[a-f0-9]{12}$/.test(reader))throw Error('COHORT_RUN_INVALID');run=reader;
 await exec(process.execPath,['build',join(root,'scripts/testing/engineering/native-cohort-runtime-holder.ts'),'--target=bun','--outfile',join(temp,'holder.js')],{cwd:root});
 const pg=(await docker(['run','-d','--label',`${label}=${labelValue}`,'--label',`com.erpx.cohort-owner=${nonce}`,'--label',`com.erpx.native-engineering-run=${run}`,'--label',`com.erpx.native-engineering-nonce=${nonce}`,'--network','none','--tmpfs','/var/lib/postgresql:rw,nosuid,size=1g','-e','POSTGRES_HOST_AUTH_METHOD=trust',postgres])).stdout.trim();owned.push(pg);
 let ready=false;for(let i=0;i<120;i++){const logs=await docker(['logs',pg]);const text=logs.stdout+logs.stderr,marker=text.indexOf('PostgreSQL init process complete');if(marker>=0&&text.slice(marker).includes('database system is ready to accept connections')){ready=true;break;}await Bun.sleep(250);}if(!ready)throw Error('COHORT_POSTGRES_NOT_READY');
 await docker(['exec',pg,'psql','-XAt','-v','ON_ERROR_STOP=1','-U','postgres','-d','postgres','-c','CREATE ROLE erpx_test_admin LOGIN NOSUPERUSER NOCREATEROLE CREATEDB']);
 client=(await docker(['run','-d','--label',`${label}=${labelValue}`,'--label',`com.erpx.cohort-owner=${nonce}`,'--network',`container:${pg}`,'--mount',`type=volume,src=${caseVolume},dst=/case`,'--entrypoint','sleep',clientImage,'infinity'])).stdout.trim();owned.push(client);
 await docker(['cp',join(temp,'holder.js'),`${client}:/tmp/holder.js`]);await docker(['exec','-d','-e',`ERPX_ENGINEERING_RUN=${run}`,'-e','ERPX_ENGINEERING_CASE_ROOT=/case',client,'sh','-c','umask 077; bun /tmp/holder.js > /case/cohort-holder.log 2>&1']);holderStarted=true;
 const directory=`/case/.runtime/browser/${run}`;await waitFor(`${directory}/cohort-ready.json`);
 const localRoster=join(temp,'roster.json'),localReady=join(temp,'ready.json');await docker(['cp',`${client}:${directory}/native-engineering-leases.json`,localRoster]);await docker(['cp',`${client}:${directory}/cohort-ready.json`,localReady]);
 const roster=JSON.parse(await readFile(localRoster,'utf8')),readyInfo=JSON.parse(await readFile(localReady,'utf8'));
 await grantNativeEngineeringCatalogLocks({dockerHost,containerId:pg,run,nonce,sourceFingerprint:readyInfo.sourceFingerprint,fileManifestSha256:readyInfo.fileManifestSha256,roster});
 // The caller performs the normal owner seed and fixture activation while these
 // exact resources remain held; the descriptor itself is private and local.
 await writeFile(output,JSON.stringify({version:1,run,caseDirectory:directory,releaseDirectory:`${directory}/release`,databasesManifest:`${directory}/databases.json`,rosterPath:`${directory}/native-engineering-leases.json`,rosterSha256:readyInfo.rosterSha256,sourceFingerprint:readyInfo.sourceFingerprint,fileManifestSha256:readyInfo.fileManifestSha256,postgresContainer:pg,clientContainer:client}),{mode:0o600,flag:'wx'});await chmod(output,0o600);
 await waitFor(`${directory}/driver-stop`,14400);
}finally{
 let cleanupFailure:unknown;
 if(holderStarted&&client&&run){try{await docker(['exec',client,'touch',`/case/.runtime/browser/${run}/cohort-stop`]);await waitFor(`/case/.runtime/browser/${run}/cohort-cleaned`);}catch(error){cleanupFailure=error;}}
 for(const id of owned.reverse()){try{const info=JSON.parse((await docker(['inspect',id])).stdout);if(info.length!==1||info[0].Config.Labels?.[label]!==labelValue||info[0].Config.Labels?.['com.erpx.cohort-owner']!==nonce)throw Error('COHORT_CLEANUP_IDENTITY_CHANGED');await docker(['rm','-f',id]);}catch(error){cleanupFailure??=error;}}
 await rm(temp,{recursive:true,force:true});if(cleanupFailure)throw cleanupFailure;
}
