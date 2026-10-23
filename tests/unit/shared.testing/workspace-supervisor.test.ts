import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer, request, type ClientRequest, type IncomingMessage } from 'node:http';
import { connect } from 'node:net';
import { expect, it } from 'vitest';
import { appDefinition } from '@erpx/contracts/apps';
import { startWorkspaceGateway } from '../../../scripts/local/workspace-gateway-runtime';

const pause = (ms: number) => new Promise(done => setTimeout(done, ms));
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
if(process.env.ERPX_PROCESS_ROLE!=='app')process.exit(2);const id=process.env.ERPX_APP_ID,port=Number(process.argv[process.argv.indexOf('--port')+1]);const previous=existsSync('starts.log')?readFileSync('starts.log','utf8'):'';appendFileSync('starts.log',id+'\\n');if(!previous.split('\\n').includes(id))setTimeout(()=>process.exit(1),id==='account'?80:800);let held,readyObserved=false;
const server=createServer((req,res)=>{res.setHeader('X-ERPX-App',id);if(req.url==='/cancelled-upgrade')appendFileSync('cancelled-upgrade.log','forwarded');if(req.url==='/api/health'){res.write('probing');res.once('finish',()=>readyObserved=true);setTimeout(()=>res.end(id),40);return;}if(req.url==='/stream'){held=res;res.writeHead(200,{'Content-Type':'text/event-stream'});res.write('data: held\\n\\n');return;}if(req.url==='/release'){held?.end('data: done\\n\\n');held=undefined;}res.end(readyObserved?id:'health response was cancelled');});
server.on('upgrade',(req,socket,head)=>{if(req.url==='/cancelled-upgrade')appendFileSync('cancelled-upgrade.log','forwarded');socket.on('error',()=>socket.destroy());socket.on('end',()=>socket.end());const accept=require('node:crypto').createHash('sha1').update(String(req.headers['sec-websocket-key'])+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');socket.write('HTTP/1.1 101 Switching Protocols\\r\\nConnection: Upgrade\\r\\nUpgrade: websocket\\r\\nSec-WebSocket-Accept: '+accept+'\\r\\nX-Source-Host: '+req.headers.host+'\\r\\nX-Source-Origin: '+req.headers.origin+'\\r\\n\\r\\n');if(head.length)socket.write(head);socket.on('data',chunk=>socket.write(chunk));});server.listen(port,'127.0.0.1');`);
    let budget=1;
    gateway = await startWorkspaceGateway({ root, node: process.execPath, port: 3104, preview: false, fallback: null,
      appPorts: new Map([['account', 3290], ['learn', 3291]]), environment: { NODE_ENV: "development", PATH: process.env.PATH, ERPX_PROCESS_ROLE: "jobs" },
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
