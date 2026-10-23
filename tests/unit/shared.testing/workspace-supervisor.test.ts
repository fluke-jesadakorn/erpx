import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer, request, type ClientRequest, type IncomingMessage } from 'node:http';
import { connect } from 'node:net';
import { expect, it } from 'vitest';
import { appDefinition, appIds } from '@erpx/contracts/apps';
import { consoleWorkspaceCutoverBlockers } from '../../../scripts/local/console-workspace-cutover';
import { startWorkspaceGateway, developmentApplicationBudget } from '../../../scripts/local/workspace-gateway-runtime';

it('validates local process budgets without silently increasing resource limits',()=>{
 expect(developmentApplicationBudget(4,undefined)).toBe(4);expect(developmentApplicationBudget(4,'6')).toBe(6);
 for(const value of ['', '0','13','04','6.0','+6',' 6','NaN'])expect(()=>developmentApplicationBudget(4,value)).toThrow('INVALID_DEVELOPMENT_APP_BUDGET');
 for(const value of [0,13,1.5])expect(()=>developmentApplicationBudget(value,'6')).toThrow('INVALID_WORKSPACE_SETTINGS');
});
const pause = (ms: number) => new Promise(done => setTimeout(done, ms));
it('requires every app mapping and the exact development environment before Console retirement without requiring Commercial enforcement', () => {
  const environment = { ERPX_ENVIRONMENT: 'local', ERPX_PRODUCT_ENVIRONMENT: 'development', ERPX_PUBLIC_ORIGIN: 'https://console-dev-erpx.jesadakorn.com', ERPX_PRODUCT_ROUTING: 'enabled', ERPX_PRODUCT_SSO: 'enabled' };
  expect(consoleWorkspaceCutoverBlockers(appIds, environment)).toEqual([]);
  const partial = consoleWorkspaceCutoverBlockers(['account', 'apps', 'learn', 'admin'], environment);
  expect(partial).toHaveLength(18);
  expect(partial).toContain('Application still uses compatibility mapping: work');
  expect(consoleWorkspaceCutoverBlockers([...appIds, 'compat'], environment)).toContain('Unknown application mapping: compat');
  expect(consoleWorkspaceCutoverBlockers([...appIds, 'work'], environment)).toContain('Duplicate active application mappings');
  for (const patch of [{ ERPX_ENVIRONMENT: 'production' }, { ERPX_PRODUCT_ENVIRONMENT: 'production' }, { ERPX_PUBLIC_ORIGIN: 'https://foreign.invalid' }, { ERPX_PUBLIC_ORIGIN: 'https://console-dev-erpx.jesadakorn.com/other' }, { ERPX_PRODUCT_ROUTING: 'disabled' }, { ERPX_PRODUCT_SSO: 'disabled' }])
    expect(consoleWorkspaceCutoverBlockers(appIds, { ...environment, ...patch }).length).toBeGreaterThan(0);
});
it('refuses premature public Console cutover before writing leases or touching the live gateway listener', async () => {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'erpx-cutover-')));
  try {
    await writeFile(join(root, 'package.json'), JSON.stringify({ name: 'erpx' }));
    await expect(startWorkspaceGateway({ root, node: process.execPath, port: 3100,
      environment: { NODE_ENV: 'development', ERPX_ENVIRONMENT: 'local', ERPX_PRODUCT_ENVIRONMENT: 'development', ERPX_PUBLIC_ORIGIN: 'https://console-dev-erpx.jesadakorn.com', ERPX_PRODUCT_ROUTING: 'enabled', ERPX_PRODUCT_SSO: 'enabled', ERPX_CONSOLE_RETIRED: 'enabled' },
      settings: () => ({ version: 1, activeApps: ['account', 'apps', 'learn', 'admin'], maxApps: 4, idleMinutes: 10, bundlers: {}, defaultBundler: 'webpack' }),
    })).rejects.toThrow('DEVELOPMENT_CONSOLE_CUTOVER_NOT_READY');
    await expect(readFile(join(root, '.scratch/dev/apps/leases.json'))).rejects.toMatchObject({ code: 'ENOENT' });
  } finally { await rm(root, { recursive: true, force: true }); }
});
async function free(port: number) {
  await new Promise<void>((done, reject) => {
    const socket = connect(port, '127.0.0.1');
    socket.once('connect', () => { socket.destroy(); reject(new Error('SUPERVISOR_TEST_PORT_OCCUPIED:' + port)); });
    socket.once('error', () => { socket.destroy(); done(); });
  });
}
function get(port: number, host: string, path = '/') {
  return new Promise<{ status: number; body: string }>((done, reject) => {
    const req = request({ host: '127.0.0.1', port, path, headers: { Host: host }, agent: false }, res => {
      let body = ''; res.on('data', chunk => body += chunk); res.once('end', () => done({ status: res.statusCode!, body }));
    }); req.once('error', reject); req.end();
  });
}
async function stream(host: string) {
  return new Promise<{ req: ClientRequest; res: IncomingMessage }>((done, reject) => {
    const req = request({ host: '127.0.0.1', port: 3104, path: '/stream', headers: { Host: host }, agent: false }, res => {
      res.once('data', () => { res.resume(); done({ req, res }); });
    }); req.once('error', reject); req.end();
  });
}
async function upgradeEcho(host: string) {
  const head=Buffer.from([0,255,1,2]),tail=Buffer.from([3,128,5]),expected=Buffer.concat([head,tail]);
  return await new Promise<{headers:string;body:Buffer}>((done,reject)=>{
    const socket=connect(3104,'127.0.0.1');let bytes=Buffer.alloc(0),finished=false;
    const timeout=setTimeout(()=>finish(new Error('UPGRADE_ECHO_TIMEOUT')),5000);
    function finish(error?:Error,result?:{headers:string;body:Buffer}){if(finished)return;finished=true;clearTimeout(timeout);socket.destroy();if(error)reject(error);else done(result!);}
    socket.once('error',error=>finish(error));
    socket.once('connect',()=>{socket.write(Buffer.concat([Buffer.from(`GET /socket HTTP/1.1\r\nHost: ${host}\r\nOrigin: https://${host}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n`),head]));setTimeout(()=>{if(!finished)socket.write(tail);},20);});
    socket.on('data',chunk=>{bytes=Buffer.concat([bytes,typeof chunk==='string'?Buffer.from(chunk):chunk]);const end=bytes.indexOf('\r\n\r\n');if(end>=0&&bytes.length-end-4>=expected.length)finish(undefined,{headers:bytes.subarray(0,end).toString(),body:bytes.subarray(end+4)});});
  });
}

async function prewarmHost(host: string, id: string) {
  return await new Promise<{code:number|null;stdout:string;stderr:string}>((done,reject)=>{
    const child=spawn(process.execPath,[fileURLToPath(new URL('../../../scripts/local/prewarm-apps.mjs',import.meta.url))],{stdio:['pipe','pipe','pipe']});
    let stdout='',stderr='';const timeout=setTimeout(()=>{child.kill('SIGTERM');reject(Error('PREWARM_FIXTURE_TIMEOUT'));},5000);
    child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);
    child.once('error',error=>{clearTimeout(timeout);reject(error);});
    child.once('close',code=>{clearTimeout(timeout);done({code,stdout,stderr});});
    child.stdin.end(JSON.stringify({version:1,port:3104,token:null,apps:[{id,host}]}));
  });
}

it('launches the stable Apps host and process identity from its launcher directory',async()=>{
  for(const port of [3104,3292])await free(port);
  const root=await realpath(await mkdtemp(join(tmpdir(),'erpx-launcher-')));
  let gateway:Awaited<ReturnType<typeof startWorkspaceGateway>>|undefined;
  try {
    await mkdir(join(root,'node_modules/next/dist/bin'),{recursive:true});await mkdir(join(root,'apps/launcher'),{recursive:true});
    await writeFile(join(root,'package.json'),JSON.stringify({name:'erpx'}));
    await writeFile(join(root,'node_modules/next/package.json'),JSON.stringify({name:'next',type:'commonjs'}));
    await writeFile(join(root,'node_modules/next/dist/bin/next'),`const{createServer}=require('node:http'),{join}=require('node:path'),{existsSync}=require('node:fs');if(process.env.ERPX_APP_ID!=='apps'||process.argv[3]!==join(process.cwd(),'apps/launcher')||!existsSync(process.argv[3]))process.exit(2);createServer((req,res)=>{res.setHeader('X-ERPX-App','apps');res.end('apps');}).listen(Number(process.env.ERPX_APP_PORT),'127.0.0.1');`);
    gateway=await startWorkspaceGateway({root,node:process.execPath,port:3104,preview:false,fallback:null,appPorts:new Map([['apps',3292]]),environment:{NODE_ENV:'development',PATH:process.env.PATH},settings:()=>({version:1,activeApps:['apps'],maxApps:1,idleMinutes:10,bundlers:{},defaultBundler:'turbopack'})});
    expect(await get(3104,new URL(appDefinition('apps').origin).host)).toEqual({status:200,body:'apps'});
    const lease=JSON.parse(await readFile(gateway.leasePath,'utf8'));expect(lease.apps[0].id).toBe('apps');expect(lease.apps[0].identity.command).toContain(root+'/apps/launcher ');
  }finally{await gateway?.close();await rm(root,{recursive:true,force:true});}
},15_000);

it('retires Console HTTP and upgrades before app startup and sends health only to Account',async()=>{
 for(const port of [3104,3293])await free(port);
 const root=await realpath(await mkdtemp(join(tmpdir(),'erpx-retired-console-')));
 let gateway:Awaited<ReturnType<typeof startWorkspaceGateway>>|undefined;
 try{
  await mkdir(join(root,'node_modules/next/dist/bin'),{recursive:true});await mkdir(join(root,'apps/account'),{recursive:true});
  await writeFile(join(root,'package.json'),JSON.stringify({name:'erpx'}));await writeFile(join(root,'node_modules/next/package.json'),JSON.stringify({name:'next',type:'commonjs'}));
  await writeFile(join(root,'node_modules/next/dist/bin/next'),`const{createServer}=require('node:http'),{appendFileSync}=require('node:fs');if(process.env.ERPX_APP_ID!=='account')process.exit(2);createServer((req,res)=>{appendFileSync('account-requests.log',req.method+' '+req.url+'\\n');res.setHeader('X-ERPX-App','account');res.end('account-health');}).listen(Number(process.env.ERPX_APP_PORT),'127.0.0.1');`);
  gateway=await startWorkspaceGateway({root,node:process.execPath,port:3104,preview:false,fallback:null,appPorts:new Map([['account',3293]]),
   environment:{PATH:process.env.PATH,NODE_ENV:'development',ERPX_ENVIRONMENT:'local',ERPX_PRODUCT_ENVIRONMENT:'development',ERPX_PUBLIC_ORIGIN:'https://console-dev-erpx.jesadakorn.com',ERPX_PRODUCT_ROUTING:'enabled',ERPX_PRODUCT_SSO:'enabled',ERPX_CONSOLE_RETIRED:'enabled'},
   settings:()=>({version:1,activeApps:appIds,maxApps:4,idleMinutes:10,bundlers:{},defaultBundler:'webpack'})});
  for(const host of ['console-dev-erpx.jesadakorn.com','127.0.0.1:3104','localhost:3104','[::1]:3104'])for(const method of ['GET','HEAD','POST','PUT','PATCH','DELETE','OPTIONS']){
   const result=await new Promise<{status:number;body:string;location:unknown;cookie:unknown}>((done,reject)=>{
    const req=request({host:'127.0.0.1',port:3104,path:'/api/old-command?token=must-not-replay',method,headers:{Host:host},agent:false},res=>{
     let body='';res.on('data',chunk=>body+=chunk);res.on('end',()=>done({status:res.statusCode!,body,location:res.headers.location,cookie:res.headers['set-cookie']}));
    });req.on('error',reject);req.end(method==='POST'?'private-command-body':undefined);
   });
   expect(result.status).toBe(410);expect(result.location).toBeUndefined();expect(result.cookie).toBeUndefined();expect(result.body).not.toContain('must-not-replay');expect(result.body).not.toContain('private-command-body');
  }
  for(const host of ['console-dev-erpx.jesadakorn.com','127.0.0.1:3104','localhost:3104','[::1]:3104'])for(const path of ['/_next/hmr','/api/health','/api/ready']){
  const upgrade=await new Promise<string>((done,reject)=>{const socket=connect(3104,'127.0.0.1');let text='';socket.on('connect',()=>socket.write(`GET ${path} HTTP/1.1\r\nHost: ${host}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n`));socket.on('data',chunk=>text+=chunk);socket.on('end',()=>done(text));socket.on('error',reject);});
  expect(upgrade).toContain('HTTP/1.1 410 Gone');}
  await expect(readFile(join(root,'account-requests.log'))).rejects.toMatchObject({code:'ENOENT'});
  for(const host of ['console-dev-erpx.jesadakorn.com','127.0.0.1:3104','localhost:3104','[::1]:3104'])expect(await get(3104,host,'/api/health')).toEqual({status:200,body:'account-health'});
  const reads=(await readFile(join(root,'account-requests.log'),'utf8')).trim().split('\n');expect(reads.length).toBeGreaterThanOrEqual(2);expect(new Set(reads)).toEqual(new Set(['GET /api/health']));
  const leases=JSON.parse(await readFile(gateway.leasePath,'utf8'));expect(leases.apps.map((app:{id:string})=>app.id)).toEqual(['account']);
 }finally{await gateway?.close();await rm(root,{recursive:true,force:true});}
},20_000);

it('cleans an owned cold startup during shutdown without waiting for its readiness deadline or replaying the request',async()=>{
 for(const port of [3104,3294])await free(port);
 const root=await realpath(await mkdtemp(join(tmpdir(),'erpx-stop-startup-')));
 let gateway:Awaited<ReturnType<typeof startWorkspaceGateway>>|undefined;
 try{
  await mkdir(join(root,'node_modules/next/dist/bin'),{recursive:true});await mkdir(join(root,'apps/account'),{recursive:true});
  await writeFile(join(root,'package.json'),JSON.stringify({name:'erpx'}));await writeFile(join(root,'node_modules/next/package.json'),JSON.stringify({name:'next',type:'commonjs'}));
  await writeFile(join(root,'node_modules/next/dist/bin/next'),`require('node:http').createServer((req,res)=>{res.statusCode=503;res.end('not-ready');}).listen(Number(process.env.ERPX_APP_PORT),'127.0.0.1');`);
  gateway=await startWorkspaceGateway({root,node:process.execPath,port:3104,preview:false,fallback:null,appPorts:new Map([['account',3294]]),environment:{PATH:process.env.PATH,NODE_ENV:'development'},settings:()=>({version:1,activeApps:['account'],maxApps:1,idleMinutes:10,bundlers:{},defaultBundler:'webpack'})});
  const response=get(3104,new URL(appDefinition('account').origin).host),deadline=Date.now()+3000;
  while(true){try{if(JSON.parse(await readFile(gateway.leasePath,'utf8')).apps.length)break;}catch{}if(Date.now()>deadline)throw Error('OWNED_STARTUP_NOT_REACHED');await pause(20);}
  const started=performance.now();await gateway.close();expect(performance.now()-started).toBeLessThan(3000);expect((await response).status).toBe(503);
  await free(3294);expect(JSON.parse(await readFile(gateway.leasePath,'utf8')).apps).toEqual([]);
 }finally{await gateway?.close();await rm(root,{recursive:true,force:true});}
},10_000);

it('deduplicates startup, refuses capacity without interrupting streams, and drains owned apps before closing', async () => {
  for (const port of [3104, 3290, 3291]) await free(port);
  const root = await realpath(await mkdtemp(join(tmpdir(), 'erpx-supervisor-')));
  let gateway: Awaited<ReturnType<typeof startWorkspaceGateway>> | undefined;
  let foreign: ReturnType<typeof createServer> | undefined;
  const streams: { req: ClientRequest; res: IncomingMessage }[] = [];
  try {
    await mkdir(join(root, 'node_modules/next/dist/bin'), { recursive: true });
    await mkdir(join(root, 'apps/account'), { recursive: true }); await mkdir(join(root, 'apps/learn'), { recursive: true });
    await writeFile(join(root, 'package.json'), '{"name":"erpx","private":true}');
    await writeFile(join(root, 'node_modules/next/package.json'), '{"name":"next","type":"commonjs"}');
    // This process fixture speaks HTTP and owns no database or business service.
    await writeFile(join(root, 'node_modules/next/dist/bin/next'), `const{createServer}=require('node:http'),{appendFileSync,existsSync,readFileSync}=require('node:fs');
if(process.env.ERPX_PROCESS_ROLE!=='app')process.exit(2);const id=process.env.ERPX_APP_ID,port=Number(process.argv[process.argv.indexOf('--port')+1]);if(process.env.ERPX_APP_PORT!==String(port))process.exit(3);const previous=existsSync('starts.log')?readFileSync('starts.log','utf8'):'';appendFileSync('starts.log',id+'\\n');if(!previous.split('\\n').includes(id))setTimeout(()=>process.exit(1),id==='account'?80:800);let held,readyObserved=false;
const server=createServer((req,res)=>{res.setHeader('X-ERPX-App',id);if(req.url==='/cancelled-upgrade')appendFileSync('cancelled-upgrade.log','forwarded');if(req.url==='/api/health'){res.write('probing');res.once('finish',()=>readyObserved=true);setTimeout(()=>res.end(id),40);return;}if(req.url==='/stream'){held=res;res.writeHead(200,{'Content-Type':'text/event-stream'});res.write('data: held\\n\\n');return;}if(req.url==='/release'){held?.end('data: done\\n\\n');held=undefined;}res.end(readyObserved?id:'health response was cancelled');});
server.on('upgrade',(req,socket,head)=>{if(req.url==='/cancelled-upgrade')appendFileSync('cancelled-upgrade.log','forwarded');socket.on('error',()=>socket.destroy());socket.on('end',()=>socket.end());const accept=require('node:crypto').createHash('sha1').update(String(req.headers['sec-websocket-key'])+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');socket.write('HTTP/1.1 101 Switching Protocols\\r\\nConnection: Upgrade\\r\\nUpgrade: websocket\\r\\nSec-WebSocket-Accept: '+accept+'\\r\\nX-Source-Host: '+req.headers.host+'\\r\\nX-Source-Origin: '+req.headers.origin+'\\r\\n\\r\\n');if(head.length)socket.write(head);socket.on('data',chunk=>socket.write(chunk));});server.listen(port,'127.0.0.1');`);
    let budget=1;
    gateway = await startWorkspaceGateway({ root, node: process.execPath, port: 3104, preview: false, fallback: null,
      appPorts: new Map([['account', 3290], ['learn', 3291]]), environment: { NODE_ENV: "development", PATH: process.env.PATH, ERPX_PROCESS_ROLE: "jobs", ERPX_DEV_MAX_APPS:"12" },
      settings: () => ({ version: 1, activeApps: ['account', 'learn'], maxApps: budget, idleMinutes: 10, bundlers: {}, defaultBundler: 'webpack' }) });
    const account = new URL(appDefinition('account').origin).host, learn = new URL(appDefinition('learn').origin).host;
    foreign = createServer();
    await new Promise<void>((done,reject)=>{foreign!.once('error',reject);foreign!.listen({port:3290,host:'::1',ipv6Only:true},()=>done());});
    expect((await get(3104,account)).status).toBe(503);
    expect(foreign.listening).toBe(true);
    await new Promise<void>(done=>foreign!.close(()=>done()));foreign=undefined;
    const first = await Promise.all([get(3104, account), get(3104, account), get(3104, account)]);
    expect(first.map(result => result.body)).toEqual(['account', 'account', 'account']);
    expect((await readFile(join(root, 'starts.log'), 'utf8')).trim().split('\n')).toEqual(['account', 'account']);
    const warmed=await prewarmHost(account,'account');expect(warmed.code,warmed.stderr).toBe(0);expect(JSON.parse(warmed.stdout).prewarmed).toEqual(['account']);
    const mismatched=await prewarmHost(account,'learn');expect(mismatched.code).toBe(1);expect(mismatched.stderr).toContain('APP_PREWARM_FAILED:learn');
    expect((await readFile(join(root,'starts.log'),'utf8')).trim().split('\n')).toEqual(['account','account']);
    streams.push(await stream(account));
    expect((await get(3104, learn)).status).toBe(503);
    expect(JSON.parse(await readFile(gateway.leasePath, 'utf8')).apps.map((app: { id: string }) => app.id)).toEqual(['account']);
    streams[0].req.destroy(); streams[0].res.destroy(); await pause(50);
    await gateway.prewarm('learn');
    expect(await get(3104, learn)).toEqual({ status: 200, body: 'learn' });
    budget=2;
    const cold=gateway.prewarm('account'),startedDeadline=Date.now()+10_000;
    while((await readFile(join(root,'starts.log'),'utf8')).trim().split('\n').filter(id=>id==='account').length<3){if(Date.now()>startedDeadline)throw Error('EXPECTED_COLD_ACCOUNT_START');await pause(10);}
    const cancelled=connect(3104,'127.0.0.1');
    await new Promise<void>((done,reject)=>{cancelled.once('connect',done);cancelled.once('error',reject);});
    cancelled.write(`GET /cancelled-upgrade HTTP/1.1\r\nHost: ${account}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n`);
    await pause(20);cancelled.destroy();
    const hot=get(3104,learn);
    expect(await Promise.race([hot.then(()=> 'hot'),cold.then(()=> 'cold')])).toBe('hot');
    expect(await hot).toEqual({status:200,body:'learn'});await cold;
    await expect(readFile(join(root,'cancelled-upgrade.log'))).rejects.toMatchObject({code:'ENOENT'});
    const echoed=await upgradeEcho(account);
    expect(echoed.headers).toContain('HTTP/1.1 101');expect(echoed.headers).toContain(`X-Source-Host: ${account}`);expect(echoed.headers).toContain(`X-Source-Origin: https://${account}`);
    expect(echoed.body).toEqual(Buffer.from([0,255,1,2,3,128,5]));
    expect((await get(3104, new URL(appDefinition('finance').origin).host)).status).toBe(503);
    streams.push(await stream(learn));
    const pid = JSON.parse(await readFile(gateway.leasePath, 'utf8')).apps[0].pid as number;
    let closed = false; const closing = gateway.close().then(() => { closed = true; }); await pause(100);
    expect(closed).toBe(false); expect(() => process.kill(pid, 0)).not.toThrow();
    await get(3291, learn, '/release'); await closing;
    expect(closed).toBe(true); expect(JSON.parse(await readFile(gateway.leasePath, 'utf8')).apps).toEqual([]);
    expect(() => process.kill(pid, 0)).toThrow();
    expect((await readFile(join(root, 'starts.log'), 'utf8')).trim().split('\n')).toEqual(['account', 'account', 'learn', 'learn', 'account']);
  } finally {
    for (const item of streams) { item.req.destroy(); item.res.destroy(); }
    if(foreign)await new Promise<void>(done=>foreign!.close(()=>done()));
    await gateway?.close(); await rm(root, { recursive: true, force: true });
  }
}, 30_000);
