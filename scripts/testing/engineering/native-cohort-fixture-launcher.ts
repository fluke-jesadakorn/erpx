/** Hold the real native fixture and relay one private owner-seed activation. */
import {spawn,execFile} from 'node:child_process';
import {open,readFile} from 'node:fs/promises';
import {promisify} from 'node:util';
const descriptorPath=process.env.ERPX_COHORT_DESCRIPTOR,dockerHost=process.env.ERPX_COHORT_DOCKER_HOST;
if(!descriptorPath||!dockerHost?.startsWith('unix:///'))throw Error('PRIVATE_COHORT_DESCRIPTOR_REQUIRED');
const fieldOwner=process.env.ERPX_FIELD_SERVICE_OWNER_PROFILE;
if(fieldOwner!==undefined&&fieldOwner!=='1')throw Error('INVALID_FIELD_OWNER_PROFILE');
const autoSeed=process.env.ERPX_COHORT_AUTO_SEED;
if(autoSeed!==undefined&&autoSeed!=='1')throw Error('INVALID_COHORT_AUTO_SEED');
const d=JSON.parse(await readFile(descriptorPath,'utf8'));
if(!/^[a-f0-9]{12}$/.test(d.run)||!/^[a-f0-9]{64}$/.test(d.clientContainer)||d.caseDirectory!==`/case/.runtime/browser/${d.run}`||!/^[a-f0-9]{64}$/.test(d.rosterSha256)||!/^[a-f0-9]{64}$/.test(d.sourceFingerprint)||!/^[a-f0-9]{64}$/.test(d.fileManifestSha256))throw Error('PRIVATE_COHORT_DESCRIPTOR_INVALID');
const exec=promisify(execFile),env={PATH:process.env.PATH??'',DOCKER_HOST:dockerHost,NODE_ENV:'test' as const},docker=async(args:string[])=>exec('docker',args,{env,timeout:30000,maxBuffer:65536});
const inspect=JSON.parse((await docker(['inspect',d.clientContainer])).stdout);if(inspect.length!==1||inspect[0].Config.Labels?.['com.erpx.engineering-case']!=='native-ssr-20261031'||inspect[0].HostConfig.NetworkMode!==`container:${d.postgresContainer}`)throw Error('COHORT_CLIENT_INVALID');
await docker(['exec',d.clientContainer,'mkdir','-p','/workspace']);
await docker(['exec',d.clientContainer,'sh','-c','if [ -L /workspace/erpx ]; then [ "$(readlink /workspace/erpx)" = /case/source ]; elif [ -e /workspace/erpx ]; then exit 1; else ln -s /case/source /workspace/erpx; fi']);
const output=`${d.caseDirectory}/fixture.json`,nonce=crypto.randomUUID(),variables={ERPX_TEST_ADMIN_URL:'postgresql://erpx_test_admin:synthetic@127.0.0.1:5432/postgres',ERPX_FIELD_ENGINEERING_ROSTER:d.rosterPath,ERPX_FIELD_ENGINEERING_ROSTER_SHA256:d.rosterSha256,ERPX_FIELD_ENGINEERING_RUN:d.run,ERPX_FIELD_ENGINEERING_SOURCE:d.sourceFingerprint,ERPX_FIELD_ENGINEERING_MANIFEST:d.fileManifestSha256,ERPX_FIELD_FILES_FIXTURE_OUTPUT:output,ERPX_FIELD_FILES_FIXTURE_NONCE:nonce,ERPX_FIELD_FILES_BROWSER_PROFILE:'1',ERPX_FIELD_FILES_MIXED_PROFILE:'1',ERPX_FIELD_PARTS_MIXED_PROFILE:'1',...(fieldOwner==='1'?{ERPX_FIELD_SERVICE_OWNER_PROFILE:'1'}:{}),...(process.env.ERPX_FIELD_COMMERCIAL_PROFILE==='1'?{ERPX_FIELD_COMMERCIAL_PROFILE:'1'}:{}),ERPX_FILES_PREVIEW_BIN:'/case/renderer',ERPX_HEIF_CONVERT:'/usr/bin/heif-convert',ERPX_PDFTOPPM_BIN:'/usr/bin/pdftoppm',ERPX_CLAMD_SOCKET:'/case/socket/clamd.sock',ERPX_CLAMAV_DATABASE_DIR:'/case/defs',LD_LIBRARY_PATH:'/opt/erpx-vips/lib'};
await docker(['exec','-e',`ERPX_EXPECTED_NONCE=${nonce}`,d.clientContainer,'sh','-c','umask 077; set -C; printf %s "$ERPX_EXPECTED_NONCE" > "$1"','sh',`${d.caseDirectory}/fixture-expected-nonce`]);
const log=await open(`${descriptorPath}.fixture.log`,'wx',0o600),args=['exec','-i',...Object.entries(variables).flatMap(([key,value])=>['-e',`${key}=${value}`]),d.clientContainer,'/case/field-files-provider'];
const child=spawn('docker',args,{env,stdio:['pipe','ignore',log.fd]}),input=child.stdin;if(!input)throw Error('COHORT_FIXTURE_STDIN_REQUIRED');let exited=false;child.once('exit',()=>{exited=true;});
async function wait(path:string,limit=14400){for(let i=0;i<limit;i++){if(exited)throw Error('COHORT_FIXTURE_EXITED');try{await docker(['exec',d.clientContainer,'test','-f',path]);return;}catch{await Bun.sleep(250);}}throw Error('COHORT_FIXTURE_TIMEOUT');}
try{
 await wait(`${output}.seed`);
 console.log('COHORT_PROVIDER_SEED_READY');
 if(autoSeed==='1'){
  const configPath=`${d.caseDirectory}/fixture-seed-config.json`;
  const config={run:d.run,nonce,rosterPath:d.rosterPath,rosterSha256:d.rosterSha256,fixtureSeedPath:`${output}.seed`,output:`${d.caseDirectory}/fixture-activation.json`};
  await docker(['exec',d.clientContainer,'bun','-e','require("node:fs").writeFileSync(process.argv[1],process.argv[2],{mode:0o600,flag:"wx"})',configPath,JSON.stringify(config)]);
  // This is a server CLI, outside Next's compiler. Apply its server-only /
  // client-only marker aliases without changing React's global conditions.
  const buildSeed='const result=await Bun.build({entrypoints:["/case/source/scripts/testing/engineering/native-cohort-parts-seed.ts"],target:"bun",outdir:process.argv[1],naming:"fixture-seed-cli.js",plugins:[{name:"owned-server-seed",setup(build){build.onResolve({filter:/^(server-only|client-only)$/},({path})=>({path:"/case/source/node_modules/next/dist/compiled/"+path+"/"+(path==="server-only"?"empty":"error")+".js"}));}}]});if(!result.success){console.error(result.logs);process.exit(1)}';
  // The provider consumes activation; it never creates it. Reuse the owned
  // seed's nonce/roster/database guards before relaying its actual actor JSON.
  async function seedCommand(args:string[],stage:string){
   const seed=spawn('docker',['exec',...(fieldOwner==='1'?['-e','ERPX_FIELD_SERVICE_OWNER_PROFILE=1']:[]),'-w','/case/source',d.clientContainer,'bun',...args],{env,stdio:['ignore',log.fd,log.fd]});
   const seedCode=await new Promise<number|null>((resolve,reject)=>{seed.once('error',reject);seed.once('exit',resolve);});
   if(seedCode!==0)throw Error(`${stage}: inspect the private fixture log`);
  }
  await seedCommand(['-e',buildSeed,d.caseDirectory],'COHORT_SEED_BUILD_FAILED');
  await seedCommand([`${d.caseDirectory}/fixture-seed-cli.js`,configPath],'COHORT_SEED_FAILED');
 }
 await wait(`${d.caseDirectory}/fixture-activation.json`);
 const stat=(await docker(['exec',d.clientContainer,'stat','-c','%a %s',`${d.caseDirectory}/fixture-activation.json`])).stdout.trim().split(' ');if(stat[0]!=='600'||Number(stat[1])>65536)throw Error('COHORT_ACTIVATION_INVALID');
 const activation=(await docker(['exec',d.clientContainer,'cat',`${d.caseDirectory}/fixture-activation.json`])).stdout;JSON.parse(activation);input.write(activation+'\n');
 console.log('COHORT_ACTIVATION_RELAYED');
 await wait(output);console.log('COHORT_FIXTURE_READY');await wait(`${d.caseDirectory}/fixture-stop`,fieldOwner==='1'?Number.POSITIVE_INFINITY:14400);input.write('quit\n');input.end();
 const code=await new Promise<number|null>(resolve=>child.once('exit',resolve));if(code!==0)throw Error('COHORT_FIXTURE_SHUTDOWN_FAILED');
}finally{if(!exited){input.end();child.kill('SIGTERM');}await log.close();}
