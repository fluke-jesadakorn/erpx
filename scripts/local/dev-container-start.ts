/** Start only the leased contributor workspace and its selected development apps. */
import { closeSync, existsSync, lstatSync, mkdirSync, openSync, readFileSync, readdirSync, readlinkSync, rmdirSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { spawn, execFileSync } from 'node:child_process';
import { connect } from 'node:net';
import { resolve } from 'node:path';
import { appDefinition } from '@erpx/contracts/apps';
import { hasPortListener } from './port-listeners';

const root='/workspace/erpx', key=process.env.ERPX_DEV_WORKSPACE_KEY??'', args=process.argv.slice(2);
if(process.platform!=='linux'||!existsSync('/.dockerenv')||resolve(process.cwd())!==root||!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(key)||args.length>1||args.length===1&&!args[0].startsWith('--app='))throw Error('ISOLATED_KEYED_WORKSPACE_REQUIRED');
const selectionFile=`${root}/.runtime/local/config/selected-app`;if(existsSync(selectionFile)&&(statSync(selectionFile).mode&0o077)!==0)throw Error('PRIVATE_APP_SELECTION_REQUIRED');const app=args.length?args[0].slice(6):existsSync(selectionFile)?readFileSync(selectionFile,'utf8').trim():'finance';appDefinition(app);
for(const name of Object.keys(process.env))if((name.startsWith('ERPX_')&&name!=='ERPX_DEV_WORKSPACE_KEY')||name==='DATABASE_URL'||name==='BETTER_AUTH_SECRET')throw Error('EXTERNAL_RUNTIME_ENVIRONMENT_FORBIDDEN');
for(const name of readdirSync(root))if((name==='.env'||name.startsWith('.env.'))&&name!=='.env.example')throw Error('WORKSPACE_ENV_FILES_FORBIDDEN');
const lease='/run/erpx-secrets/workspace-key',config=`${root}/.runtime/local/config`,sourceId=`${root}/.git/erpx-container-source-id`,phasePath=`${config}/bootstrap-phase.json`,authPath=`${config}/bootstrap-auth-secret`;
for(const file of [lease,sourceId,phasePath,authPath,`${config}/databases.json`])if(!existsSync(file)||(statSync(file).mode&0o077)!==0)throw Error('COMPLETED_PRIVATE_BOOTSTRAP_REQUIRED');
const phase=JSON.parse(readFileSync(phasePath,'utf8')) as {key?:string;sourceId?:string;phase?:string},auth=readFileSync(authPath,'utf8').trim();
if(readFileSync(lease,'utf8').trim()!==key||phase.key!==key||!/^[a-f0-9]{64}$/.test(phase.sourceId??'')||phase.sourceId!==readFileSync(sourceId,'utf8').trim()||phase.phase!=='complete'||!/^[a-f0-9]{64}$/.test(auth))throw Error('WORKSPACE_LEASE_OR_BOOTSTRAP_MISMATCH');
if(Bun.version!=='1.4.3'||execFileSync('/usr/local/bin/node',['--version'],{encoding:'utf8'}).trim()!=='v26.11.1')throw Error('PINNED_TOOLCHAIN_REQUIRED');
const env:NodeJS.ProcessEnv={NODE_ENV:'development',PATH:'/usr/local/bin:/usr/bin:/bin',HOME:'/root',ERPX_NODE_BIN:'/usr/local/bin/node',ERPX_BUN_BIN:'/usr/local/bin/bun',ERPX_ENVIRONMENT:'local',ERPX_PRODUCT_ENVIRONMENT:'development',ERPX_PRODUCT_ROUTING:'enabled',ERPX_PRODUCT_SSO:'enabled',ERPX_HOLDING_ROUTING:'enabled',NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE:'container-localhost',ERPX_DATA_DIR:`${root}/.runtime/local`,ERPX_DATABASE_MANIFEST:`${config}/databases.json`,ERPX_PUBLIC_ORIGIN:'https://account.erpx.localhost:3443',ERPX_CLAMD_SOCKET:'/run/erpx-scanner/clamd.sock',ERPX_CLAMAV_DATABASE_DIR:'/run/erpx-clamav-definitions',ERPX_CLAMAV_VERSION:'1.5.4',ERPX_HEIF_CONVERT:'/usr/bin/heif-convert',ERPX_PDFTOPPM_BIN:'/usr/bin/pdftoppm',ERPX_JOBS_DATABASE_MANIFEST:`${config}/databases-jobs.json`,BETTER_AUTH_SECRET:auth};
function assertOwnedGateway(){
 const pids=execFileSync('lsof',['-nP','-tiTCP:3100','-sTCP:LISTEN'],{encoding:'utf8'}).trim().split(/\s+/).filter(Boolean);if(!pids.length)throw Error('DEVELOPMENT_PORT_OWNER_UNAVAILABLE:3100');
 for(const pid of pids){const cwd=execFileSync('lsof',['-a','-p',pid,'-d','cwd','-Fn'],{encoding:'utf8'}).split('\n').find(line=>line.startsWith('n'))?.slice(1),command=execFileSync('ps',['-p',pid,'-o','args='],{encoding:'utf8'});if(cwd!==root||!command.includes(`${root}/.scratch/dev/gateway.mjs`))throw Error('DEVELOPMENT_PORT_OCCUPIED_BY_ANOTHER_PROJECT:3100');}
}
const selected=[...new Set(['account','apps',app])],argv=['--no-env-file','run','dev','--',`--app=${selected.join(',')}`],scratch=`${root}/.scratch/dev`;
mkdirSync(scratch,{recursive:true,mode:0o700});const lock=`${root}/.runtime/dev-container-start.lock`;
try{mkdirSync(lock,{mode:0o700});}catch{throw Error('DEVELOPMENT_START_LOCK_HELD_OR_STALE');}
try{
 const jobsPidPath=`${scratch}/container-jobs.pid`;let jobsRunning=false;
 if(existsSync(jobsPidPath)){
  const pid=Number(readFileSync(jobsPidPath,'utf8').trim());if(!Number.isSafeInteger(pid)||pid<2)throw Error('DEVELOPMENT_JOBS_PID_INVALID');
  let command='',cwd='';try{command=readFileSync(`/proc/${pid}/cmdline`,'utf8').replaceAll('\0',' ');cwd=readlinkSync(`/proc/${pid}/cwd`);}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}
  if(command.includes('scripts/local/dev-jobs.ts')){if(cwd!==root)throw Error('DEVELOPMENT_JOBS_OWNER_MISMATCH');jobsRunning=true;}
 }
 if(!jobsRunning){const fd=openSync(`${scratch}/container-jobs.log`,'a',0o600);try{const child=spawn('/usr/local/bin/bun',['--no-env-file','scripts/local/dev-jobs.ts'],{cwd:root,env,detached:true,stdio:['ignore',fd,fd]});child.unref();if(!child.pid)throw Error('DEVELOPMENT_JOBS_START_FAILED');writeFileSync(jobsPidPath,`${child.pid}\n`,{mode:0o600});}finally{closeSync(fd);}}
 const occupied=hasPortListener(3100)||await new Promise<boolean>(done=>{const socket=connect({host:'127.0.0.1',port:3100});socket.once('connect',()=>{socket.destroy();done(true);});socket.once('error',()=>done(false));});
 if(occupied){
  // The existing supervisor performs its own exact listener/cwd checks before prewarming.
  const child=spawn('/usr/local/bin/bun',argv,{cwd:root,env,stdio:['ignore','inherit','inherit']});const code=await new Promise<number|null>((done,reject)=>{child.once('error',reject);child.once('exit',done);});if(code!==0)throw Error('EXISTING_DEVELOPMENT_PREWARM_FAILED');
 }else{
  // A source volume survives container recreation, but its old PID namespace does not.
  // A recycled PID must not make the supervisor's persisted lock look active.
  const gatewayLock=`${scratch}/gateway-start.lock`;
  if(existsSync(gatewayLock)){
   if(!lstatSync(gatewayLock).isFile())throw Error('DEVELOPMENT_GATEWAY_LOCK_INVALID');
   const pid=Number(readFileSync(gatewayLock,'utf8').trim());if(!Number.isSafeInteger(pid)||pid<2)throw Error('DEVELOPMENT_GATEWAY_LOCK_INVALID');
   let command='';try{command=readFileSync(`/proc/${pid}/cmdline`,'utf8').replaceAll('\0',' ');}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}
   if(command.includes('scripts/local/dev.ts'))throw Error('DEVELOPMENT_GATEWAY_START_IN_PROGRESS');
   unlinkSync(gatewayLock);
  }
  const fd=openSync(`${scratch}/container-gateway.log`,'a',0o600);try{const child=spawn('/usr/local/bin/bun',argv,{cwd:root,env,detached:true,stdio:['ignore',fd,fd]});child.unref();if(!child.pid)throw Error('DEVELOPMENT_START_FAILED');writeFileSync(`${scratch}/container-gateway.pid`,`${child.pid}\n`,{mode:0o600});}finally{closeSync(fd);}
 }
 const deadline=Date.now()+120_000;let healthy=false;
 while(Date.now()<deadline){if(hasPortListener(3100))assertOwnedGateway();try{const response=await fetch('http://127.0.0.1:3100/api/health',{headers:{host:'account.erpx.localhost:3443'},signal:AbortSignal.timeout(2000)});if(response.status===200){healthy=true;break;}}catch{/* wait for the owned supervisor */}await new Promise(done=>setTimeout(done,250));}
 if(!healthy||!hasPortListener(3100))throw Error('DEVELOPMENT_GATEWAY_NOT_READY');assertOwnedGateway();
 const previewPath=`${scratch}/apps/preview.json`;if(!existsSync(previewPath)||(statSync(previewPath).mode&0o077)!==0)throw Error('DEVELOPMENT_PREWARM_AUTHORITY_REQUIRED');const preview=JSON.parse(readFileSync(previewPath,'utf8')) as {token?:string};
 const prewarm=spawn('/usr/local/bin/node',[`${root}/scripts/local/prewarm-apps.mjs`],{cwd:root,env,stdio:['pipe','ignore','ignore']});prewarm.stdin.end(JSON.stringify({version:1,port:3100,token:preview.token,apps:selected.map(id=>({id,host:id==='account'?'account.erpx.localhost:3443':'acme.erpx.localhost:3443',...(id==='account'?{}:{path:`/__erpx/${id}/api/health`})}))}));const prewarmCode=await new Promise<number|null>((done,reject)=>{prewarm.once('error',reject);prewarm.once('exit',done);});if(prewarmCode!==0||!hasPortListener(3100))throw Error('DEVELOPMENT_PREWARM_FAILED');assertOwnedGateway();
 writeFileSync(selectionFile,`${app}\n`,{mode:0o600});
 console.log(`DEVELOPMENT_READY: isolated gateway 3100; prewarm ${selected.join(',')}; contributor HTTPS on loopback 3443; explicit local CA trust required`);
}finally{rmdirSync(lock);}
