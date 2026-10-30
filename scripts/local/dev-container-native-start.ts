/** Native processes for the isolated contributor workspace. Never a host launcher. */
import {createHash,randomBytes} from 'node:crypto';
import {chmodSync,chownSync,closeSync,copyFileSync,existsSync,lstatSync,mkdirSync,openSync,readFileSync,readlinkSync,renameSync,writeFileSync} from 'node:fs';
import {execFileSync,spawn} from 'node:child_process';
import {connect} from 'node:net';
import {linuxInitStartTime} from './app-process-leases';
import {hasPortListener} from './port-listeners';

type Role='api'|'worker'|'files';
const roles=['api','worker','files'] as const;
const verbFor=(role:Role)=>role==='api'?'private-api':role==='worker'?'service-count-worker':'field-files-worker';
const configFor=(role:Role)=>role==='api'?'api':'worker';
type Identity={pid:number;start:string;command:string};
type Lease={version:1;key:string;epoch:string;binary:string;renderer?:string;configuration:string;processes:Partial<Record<Role,Identity>>};
const hash=(value:Buffer|string)=>createHash('sha256').update(value).digest('hex');
const pause=()=>new Promise(done=>setTimeout(done,100));
function privateBytes(path:string){if(!lstatSync(path).isFile()||lstatSync(path).isSymbolicLink()||(lstatSync(path).mode&0o077)!==0)throw Error('PRIVATE_NATIVE_CONFIG_REQUIRED');return readFileSync(path);}
function writeLease(path:string,value:Lease){const tmp=`${path}.${randomBytes(16).toString('hex')}.tmp`;const fd=openSync(tmp,'wx',0o600);try{writeFileSync(fd,JSON.stringify(value));}finally{closeSync(fd);}renameSync(tmp,path);}
function processIdentity(pid:number):Identity|null{
 try{const stat=readFileSync(`/proc/${pid}/stat`,'utf8'),end=stat.lastIndexOf(')'),start=stat.slice(end+2).split(/\s+/)[19],command=readFileSync(`/proc/${pid}/cmdline`,'utf8').replaceAll('\0',' ').trim();if(!start)throw Error('NATIVE_PROCESS_IDENTITY_INVALID');return{pid,start,command};}
 catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return null;throw error;}
}
function nativeCwd(pid:number){return execFileSync('/usr/bin/setpriv',['--reuid=65532','--regid=65532','--clear-groups','/usr/bin/readlink',`/proc/${pid}/cwd`],{encoding:'utf8'}).trim();}
async function occupied(){return hasPortListener(44817)||await new Promise<boolean>(done=>{const socket=connect({host:'127.0.0.1',port:44817});socket.once('connect',()=>{socket.destroy();done(true);});socket.once('error',()=>done(false));});}
// The native API validates scanner readiness during assembly. Wait for the
// actual socket and fresh public signature metadata as the same worker user.
async function awaitScanner(){
 const probe=`const net=require('node:net'),fs=require('node:fs');async function cmd(x){return await new Promise((ok,no)=>{const s=net.createConnection('/run/erpx-scanner/clamd.sock');let b='';s.setTimeout(2000);s.on('connect',()=>s.write('z'+x+'\\0'));s.on('data',v=>{b+=v.toString();if(b.includes('\\0')){s.destroy();ok(b.split('\\0')[0]);}});s.on('timeout',()=>s.destroy(new Error('timeout')));s.on('end',()=>no(new Error('closed')));s.on('error',no);});} (async()=>{const names=['daily.cvd','daily.cld','main.cvd','main.cld'];let latest=0;for(const n of names){try{const s=fs.statSync('/run/erpx-clamav-definitions/'+n);if(s.isFile())latest=Math.max(latest,s.mtimeMs);}catch(e){if(e.code!=='ENOENT')throw e;}}const [ping,version]=await Promise.all([cmd('PING'),cmd('VERSION')]);if(ping!=='PONG'||!version.startsWith('ClamAV ')||!latest||latest>Date.now()||Date.now()-latest>86400000)process.exit(1);})().catch(()=>process.exit(1));`;
 const deadline=Date.now()+60_000;while(Date.now()<deadline){try{execFileSync('/usr/bin/setpriv',['--reuid=65532','--regid=65532','--groups=1000','/usr/local/bin/node','--eval',probe],{stdio:'ignore',timeout:4000});return;}catch{await pause();}}
 throw Error('NATIVE_SCANNER_NOT_READY');
}
function epoch(){const init=linuxInitStartTime(readFileSync('/proc/1/stat','utf8'));if(!init)throw Error('NATIVE_PID_NAMESPACE_REQUIRED');return[readFileSync('/proc/sys/kernel/random/boot_id','utf8').trim(),readlinkSync('/proc/self/ns/pid'),init].join(':');}
export async function ensureContributorNative(root:string,key:string,config:string,scratch:string,restart:boolean){
 if(process.platform!=='linux'||!existsSync('/.dockerenv')||root!=='/workspace/erpx'||process.cwd()!==root||process.getuid?.()!==0)throw Error('NATIVE_CONTRIBUTOR_WORKSPACE_REQUIRED');
 const state=JSON.parse(privateBytes(`${config}/native-leases.json`).toString()) as {key:string;sourceId:string;complete:boolean};
 if(state.key!==key||state.sourceId!==privateBytes(`${root}/.git/erpx-container-source-id`).toString().trim()||state.complete!==true)throw Error('NATIVE_WORKSPACE_LEASE_REQUIRED');
 const temporary='/run/erpx-native-files-tmp';mkdirSync(temporary,{recursive:true,mode:0o700});if(!lstatSync(temporary).isDirectory()||lstatSync(temporary).isSymbolicLink())throw Error('NATIVE_FILES_DIRECTORY_INVALID');chownSync(temporary,65532,65532);chmodSync(temporary,0o700);
 const api=privateBytes(`${config}/private-api.json`),worker=privateBytes(`${config}/worker.json`),configuration=hash(Buffer.concat([api,worker]));
 if(JSON.parse(api.toString()).listen!=='127.0.0.1:44817')throw Error('EXACT_NATIVE_LISTENER_REQUIRED');
 const target=`${root}/target/dev-native-${process.arch}-rust1991`,log=`${scratch}/native-build.log`,fd=openSync(log,'a',0o600),start=Date.now();
 let code:number|null;
 try{
  const child=spawn('/usr/local/cargo/bin/cargo',['build','--locked','-p','erpx-service','--no-default-features','--features','private-field-api,files-scan-worker','--bin','erpx-server'],{cwd:root,env:{NODE_ENV:'development',PATH:'/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin',HOME:'/root',CARGO_HOME:'/usr/local/cargo',RUSTUP_HOME:'/usr/local/rustup',CARGO_TARGET_DIR:target,CARGO_INCREMENTAL:'1',CARGO_PROFILE_DEV_DEBUG:'0',PKG_CONFIG_PATH:'/opt/erpx-vips/lib/pkgconfig',LD_LIBRARY_PATH:'/opt/erpx-vips/lib'},stdio:['ignore',fd,fd]});
  code=await new Promise<number|null>((done,reject)=>{child.once('error',reject);child.once('exit',done);});
 }finally{closeSync(fd);}
 writeFileSync(`${scratch}/native-build.json`,JSON.stringify({target,toolchain:'1.99.1',architecture:process.arch,elapsedMs:Date.now()-start,exitCode:code}),{mode:0o600});
 if(code!==0)throw Error('NATIVE_CONTRIBUTOR_BUILD_FAILED');
 const rendererFd=openSync(log,'a',0o600);
 const rendererProcess=spawn('/usr/local/cargo/bin/cargo',['rustc','--locked','-p','erpx-server-files','--features','preview-provider','--bin','erpx-render-preview','--','-C','link-arg=-Wl,-rpath,/opt/erpx-vips/lib'],{cwd:root,env:{NODE_ENV:'development',PATH:'/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin',HOME:'/root',CARGO_HOME:'/usr/local/cargo',RUSTUP_HOME:'/usr/local/rustup',CARGO_TARGET_DIR:target,CARGO_INCREMENTAL:'1',CARGO_PROFILE_DEV_DEBUG:'0',PKG_CONFIG_PATH:'/opt/erpx-vips/lib/pkgconfig',LD_LIBRARY_PATH:'/opt/erpx-vips/lib'},stdio:['ignore',rendererFd,rendererFd]});
 let rendererCode:number|null;try{rendererCode=await new Promise<number|null>((done,reject)=>{rendererProcess.once('error',reject);rendererProcess.once('exit',done);});}finally{closeSync(rendererFd);}if(rendererCode!==0)throw Error('NATIVE_PREVIEW_BUILD_FAILED');
 await awaitScanner();
 const compiled=`${target}/debug/erpx-server`,renderCompiled=`${target}/debug/erpx-render-preview`,binary=hash(readFileSync(compiled)),renderer=hash(readFileSync(renderCompiled)),currentEpoch=epoch(),leasePath=`${scratch}/native-processes.json`;
 let old:Lease|undefined;
 if(existsSync(leasePath)){
  old=JSON.parse(privateBytes(leasePath).toString()) as Lease;
  if(old.version!==1||old.key!==key||typeof old.epoch!=='string'||!old.processes||Object.keys(old.processes).some(role=>!roles.includes(role as Role)))throw Error('NATIVE_PROCESS_LEASE_INVALID');
 }
 const matching:Partial<Record<Role,Identity>>={};
 for(const role of roles){
  const prior=old?.processes[role];if(!prior)continue;
  if(!Number.isSafeInteger(prior.pid)||prior.pid<=1||typeof prior.start!=='string'||typeof prior.command!=='string')throw Error('NATIVE_PROCESS_LEASE_INVALID');
  if(old?.epoch!==currentEpoch)continue;
  const actual=processIdentity(prior.pid);if(!actual)continue;
  const verb=verbFor(role);
  if(actual.start!==prior.start||actual.command!==prior.command||actual.command!==`/run/erpx-native/bin/erpx-server ${verb} --development-config /run/erpx-native/config/${configFor(role)}.json`||nativeCwd(prior.pid)!==root||!/^Uid:\s+65532\s/m.test(readFileSync(`/proc/${prior.pid}/status`,'utf8'))||!/^Groups:\s+1000\s*$/m.test(readFileSync(`/proc/${prior.pid}/status`,'utf8')))throw Error('NATIVE_PROCESS_OWNERSHIP_CHANGED');
  matching[role]=actual;
 }
 // A crashed launcher may have spawned a child before recording it. Never
 // adopt or duplicate such a process; explicit keyed recovery replaces only
 // this container namespace and preserves all source/database/cache volumes.
 let observed:string[]=[];
 try{observed=execFileSync('/usr/bin/pgrep',['-u','65532','-f','^/run/erpx-native/bin/erpx-server (private-api|service-count-worker|field-files-worker) --development-config /run/erpx-native/config/(api|worker)\\.json$'],{encoding:'utf8'}).trim().split(/\s+/).filter(Boolean);}
 catch(error){if((error as {status?:number}).status!==1)throw error;}
 const recorded=new Set(Object.values(matching).map(value=>value.pid));
 if(observed.some(pid=>!recorded.has(Number(pid))))throw Error('UNRECORDED_NATIVE_PROCESS_USE_KEYED_RECOVER');
 if(await occupied()){
  const pids=execFileSync('/usr/bin/setpriv',['--reuid=65532','--regid=65532','--clear-groups','/usr/bin/lsof','-nP','-tiTCP:44817','-sTCP:LISTEN'],{encoding:'utf8'}).trim().split(/\s+/);
  if(!matching.api||pids.some(pid=>Number(pid)!==matching.api!.pid))throw Error('NATIVE_PORT_OCCUPIED_OR_UNRECORDED:44817; use explicit keyed dev-container recover; no process was stopped');
 }
 if(!restart&&old?.binary===binary&&old.renderer===renderer&&old.configuration===configuration&&matching.api&&matching.worker&&matching.files){
  if(hash(readFileSync('/run/erpx-native/bin/erpx-server'))!==binary||hash(readFileSync('/run/erpx-native/bin/erpx-render-preview'))!==renderer)throw Error('NATIVE_PUBLISHED_BINARY_CHANGED');
  const health=await fetch('http://127.0.0.1:44817/api/health',{signal:AbortSignal.timeout(5000)});if(health.status!==200)throw Error('NATIVE_CONTRIBUTOR_NOT_HEALTHY');return;
 }
 // Validate all ownership above before signaling any process. Rebuild is explicit
 // when source changes; automatic initial/recovery startup retains both owners.
 if(!restart&&(matching.api||matching.worker||matching.files)&&(old?.binary!==binary||old?.renderer!==renderer))throw Error('NATIVE_SOURCE_CHANGED_USE_RESTART_NATIVE');
 for(const role of ['files','worker','api'] as const){const prior=matching[role];if(!prior)continue;process.kill(prior.pid,'SIGTERM');const deadline=Date.now()+15_000;while(Date.now()<deadline){const actual=processIdentity(prior.pid);if(!actual||actual.start!==prior.start)break;await pause();}const live=processIdentity(prior.pid);if(live?.start===prior.start)throw Error('NATIVE_GRACEFUL_STOP_TIMEOUT');}
 mkdirSync('/run/erpx-native/bin',{recursive:true,mode:0o755});mkdirSync('/run/erpx-native/config',{recursive:true,mode:0o700});chownSync('/run/erpx-native/config',65532,65532);chmodSync('/run/erpx-native/config',0o700);
 for(const [name,bytes] of [['api',api],['worker',worker]] as const){const path=`/run/erpx-native/config/${name}.json`,temp=`${path}.${randomBytes(12).toString('hex')}`;writeFileSync(temp,bytes,{mode:0o600,flag:'wx'});chownSync(temp,65532,65532);renameSync(temp,path);}
 const path='/run/erpx-native/bin/erpx-server',temp=`${path}.${randomBytes(12).toString('hex')}`;copyFileSync(compiled,temp);chmodSync(temp,0o555);renameSync(temp,path);
 const rendererPath='/run/erpx-native/bin/erpx-render-preview',rendererTemp=`${rendererPath}.${randomBytes(12).toString('hex')}`;copyFileSync(renderCompiled,rendererTemp);chmodSync(rendererTemp,0o555);renameSync(rendererTemp,rendererPath);
 const lease:Lease={version:1,key,epoch:currentEpoch,binary,renderer,configuration,processes:{}};
 for(const role of roles){
  const fd=openSync(`${scratch}/native-${role}.log`,'a',0o600);
  try{const child=spawn('/usr/bin/setpriv',['--reuid=65532','--regid=65532','--groups=1000','--no-new-privs',path,verbFor(role),'--development-config',`/run/erpx-native/config/${configFor(role)}.json`],{cwd:root,env:{NODE_ENV:'development',PATH:'/usr/bin:/bin',LD_LIBRARY_PATH:'/opt/erpx-vips/lib'},detached:true,stdio:['ignore',fd,fd]});child.unref();if(!child.pid)throw Error('NATIVE_PROCESS_START_FAILED');for(let i=0;i<50;i++){await pause();const value=processIdentity(child.pid);if(value?.command.startsWith(`${path} `)){lease.processes[role]=value;break;}}if(!lease.processes[role])throw Error('NATIVE_PROCESS_START_FAILED');}
  finally{closeSync(fd);}
  // Persist each owned child so a failed second launch is recoverable.
  writeLease(leasePath,lease);
 }
 const deadline=Date.now()+30_000;let healthy=false;
 while(Date.now()<deadline){for(const role of roles){if(!processIdentity(lease.processes[role]!.pid))throw Error('NATIVE_PROCESS_EXITED');}try{healthy=(await fetch('http://127.0.0.1:44817/api/health',{signal:AbortSignal.timeout(1000)})).status===200;}catch{/* bounded initial pool and inventory validation */}if(healthy)break;await pause();}
 if(!healthy)throw Error('NATIVE_CONTRIBUTOR_NOT_HEALTHY');
 console.log('NATIVE_CONTRIBUTOR_LIVE: API liveness and worker process observed; capability readiness requires owner command/projection checks');
}
