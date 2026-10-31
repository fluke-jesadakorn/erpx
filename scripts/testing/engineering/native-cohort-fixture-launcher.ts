/** Hold the real native fixture and relay one private owner-seed activation. */
import {spawn,execFile} from 'node:child_process';
import {open,readFile} from 'node:fs/promises';
import {promisify} from 'node:util';
const descriptorPath=process.env.ERPX_COHORT_DESCRIPTOR,dockerHost=process.env.ERPX_COHORT_DOCKER_HOST;
if(!descriptorPath||!dockerHost?.startsWith('unix:///'))throw Error('PRIVATE_COHORT_DESCRIPTOR_REQUIRED');
const d=JSON.parse(await readFile(descriptorPath,'utf8'));
if(!/^[a-f0-9]{12}$/.test(d.run)||!/^[a-f0-9]{64}$/.test(d.clientContainer)||d.caseDirectory!==`/case/.runtime/browser/${d.run}`||!/^[a-f0-9]{64}$/.test(d.rosterSha256)||!/^[a-f0-9]{64}$/.test(d.sourceFingerprint)||!/^[a-f0-9]{64}$/.test(d.fileManifestSha256))throw Error('PRIVATE_COHORT_DESCRIPTOR_INVALID');
const exec=promisify(execFile),env={PATH:process.env.PATH??'',DOCKER_HOST:dockerHost,NODE_ENV:'test' as const},docker=async(args:string[])=>exec('docker',args,{env,timeout:30000,maxBuffer:65536});
const inspect=JSON.parse((await docker(['inspect',d.clientContainer])).stdout);if(inspect.length!==1||inspect[0].Config.Labels?.['com.erpx.engineering-case']!=='native-ssr-20261031'||inspect[0].HostConfig.NetworkMode!==`container:${d.postgresContainer}`)throw Error('COHORT_CLIENT_INVALID');
await docker(['exec',d.clientContainer,'mkdir','-p','/workspace']);await docker(['exec',d.clientContainer,'ln','-s','/case/source','/workspace/erpx']);
const output=`${d.caseDirectory}/fixture.json`,nonce=crypto.randomUUID(),variables={ERPX_TEST_ADMIN_URL:'postgresql://erpx_test_admin:synthetic@127.0.0.1:5432/postgres',ERPX_FIELD_ENGINEERING_ROSTER:d.rosterPath,ERPX_FIELD_ENGINEERING_ROSTER_SHA256:d.rosterSha256,ERPX_FIELD_ENGINEERING_RUN:d.run,ERPX_FIELD_ENGINEERING_SOURCE:d.sourceFingerprint,ERPX_FIELD_ENGINEERING_MANIFEST:d.fileManifestSha256,ERPX_FIELD_FILES_FIXTURE_OUTPUT:output,ERPX_FIELD_FILES_FIXTURE_NONCE:nonce,ERPX_FIELD_FILES_BROWSER_PROFILE:'1',ERPX_FIELD_FILES_MIXED_PROFILE:'1',ERPX_FIELD_PARTS_MIXED_PROFILE:'1',ERPX_FILES_PREVIEW_BIN:'/case/renderer',ERPX_HEIF_CONVERT:'/usr/bin/heif-convert',ERPX_PDFTOPPM_BIN:'/usr/bin/pdftoppm',ERPX_CLAMD_SOCKET:'/case/socket/clamd.sock',ERPX_CLAMAV_DATABASE_DIR:'/case/defs',LD_LIBRARY_PATH:'/opt/erpx-vips/lib'};
await docker(['exec','-e',`ERPX_EXPECTED_NONCE=${nonce}`,d.clientContainer,'sh','-c','umask 077; set -C; printf %s "$ERPX_EXPECTED_NONCE" > "$1"','sh',`${d.caseDirectory}/fixture-expected-nonce`]);
const log=await open(`${descriptorPath}.fixture.log`,'wx',0o600),args=['exec','-i',...Object.entries(variables).flatMap(([key,value])=>['-e',`${key}=${value}`]),d.clientContainer,'/case/field-files-provider'];
const child=spawn('docker',args,{env,stdio:['pipe','ignore',log.fd]}),input=child.stdin;if(!input)throw Error('COHORT_FIXTURE_STDIN_REQUIRED');let exited=false;child.once('exit',()=>{exited=true;});
async function wait(path:string){for(let i=0;i<14400;i++){if(exited)throw Error('COHORT_FIXTURE_EXITED');try{await docker(['exec',d.clientContainer,'test','-f',path]);return;}catch{await Bun.sleep(250);}}throw Error('COHORT_FIXTURE_TIMEOUT');}
try{
 await wait(`${output}.seed`);
 await wait(`${d.caseDirectory}/fixture-activation.json`);
 const stat=(await docker(['exec',d.clientContainer,'stat','-c','%a %s',`${d.caseDirectory}/fixture-activation.json`])).stdout.trim().split(' ');if(stat[0]!=='600'||Number(stat[1])>65536)throw Error('COHORT_ACTIVATION_INVALID');
 const activation=(await docker(['exec',d.clientContainer,'cat',`${d.caseDirectory}/fixture-activation.json`])).stdout;JSON.parse(activation);input.write(activation+'\n');
 await wait(output);await wait(`${d.caseDirectory}/fixture-stop`);input.write('quit\n');input.end();
 const code=await new Promise<number|null>(resolve=>child.once('exit',resolve));if(code!==0)throw Error('COHORT_FIXTURE_SHUTDOWN_FAILED');
}finally{if(!exited){input.end();child.kill('SIGTERM');}await log.close();}
