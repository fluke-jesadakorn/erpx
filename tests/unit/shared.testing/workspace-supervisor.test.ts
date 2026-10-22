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
if(process.env.ERPX_PROCESS_ROLE!=='app')process.exit(2);const id=process.env.ERPX_APP_ID,port=Number(process.argv[process.argv.indexOf('--port')+1]);const previous=existsSync('starts.log')?readFileSync('starts.log','utf8'):'';appendFileSync('starts.log',id+'\\n');if(!previous.split('\\n').includes(id))setTimeout(()=>process.exit(1),id==='account'?80:800);let held;
createServer((req,res)=>{res.setHeader('X-ERPX-App',id);if(req.url==='/stream'){held=res;res.writeHead(200,{'Content-Type':'text/event-stream'});res.write('data: held\\n\\n');return;}if(req.url==='/release'){held?.end('data: done\\n\\n');held=undefined;}res.end(id);}).listen(port,'127.0.0.1');`);
    gateway = await startWorkspaceGateway({ root, node: process.execPath, port: 3104, preview: false, fallback: null,
      appPorts: new Map([['account', 3290], ['learn', 3291]]), environment: { NODE_ENV: "development", PATH: process.env.PATH, ERPX_PROCESS_ROLE: "jobs" },
      settings: () => ({ version: 1, activeApps: ['account', 'learn'], maxApps: 1, idleMinutes: 10, bundlers: {}, defaultBundler: 'webpack' }) });
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
    await gateway.prewarm('learn');await pause(1000);
    expect(await get(3104, learn)).toEqual({ status: 200, body: 'learn' });
    expect((await get(3104, new URL(appDefinition('finance').origin).host)).status).toBe(503);
    streams.push(await stream(learn));
    const pid = JSON.parse(await readFile(gateway.leasePath, 'utf8')).apps[0].pid as number;
    let closed = false; const closing = gateway.close().then(() => { closed = true; }); await pause(100);
    expect(closed).toBe(false); expect(() => process.kill(pid, 0)).not.toThrow();
    await get(3291, learn, '/release'); await closing;
    expect(closed).toBe(true); expect(JSON.parse(await readFile(gateway.leasePath, 'utf8')).apps).toEqual([]);
    expect(() => process.kill(pid, 0)).toThrow();
    expect((await readFile(join(root, 'starts.log'), 'utf8')).trim().split('\n')).toEqual(['account', 'account', 'learn', 'learn']);
  } finally {
    for (const item of streams) { item.req.destroy(); item.res.destroy(); }
    if(foreign)await new Promise<void>(done=>foreign!.close(()=>done()));
    await gateway?.close(); await rm(root, { recursive: true, force: true });
  }
}, 30_000);
