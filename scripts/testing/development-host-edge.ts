import { createServer as createHttpServer, request as httpRequest } from 'node:http';
import { createServer as createHttpsServer } from 'node:https';
import { Socket, type AddressInfo } from 'node:net';
import { X509Certificate, createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { chmod, lstat, mkdtemp, readFile, realpath, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { hasPortListener } from '../local/port-listeners';
import { appDefinition } from '@erpx/contracts/apps';
import { workspaceBrowserApps } from './development-host-profile';

export type DevelopmentHostEdgeInput = { root: string; run: string; sourceFingerprint: string; entryApp: string };
const hash = (value: string | Buffer) => createHash('sha256').update(value).digest('hex');
function assert(value: unknown, code: string): asserts value { if (!value) throw new Error(code); }

/** Test-only CONNECT proxy: never resolves or forwards to a public network. */
export async function startDevelopmentHostEdge(input: DevelopmentHostEdgeInput) {
  assert(process.version === 'v26.11.1' && /^[a-f0-9]{12}$/.test(input.run), 'PINNED_DEVELOPMENT_HOST_EDGE_REQUIRED');
  const root = resolve(input.root), directory = join(root, '.runtime/browser', input.run), sourceRoot = join(directory, 'app');
  assert(await realpath(directory) === directory, 'OWNED_DEVELOPMENT_HOST_DIRECTORY_REQUIRED');
  const state = JSON.parse(await readFile(join(directory, 'state.json'), 'utf8'));
  const runtimeBytes = await readFile(join(directory, 'workspace-runtime.json')), runtime = JSON.parse(runtimeBytes.toString());
  const apps = workspaceBrowserApps(JSON.stringify(state.workspaceApps));
  assert(Boolean(runtime.consoleRetired)===Boolean(state.workspaceConsoleRetired),'DEVELOPMENT_CONSOLE_PROFILE_CHANGED');
  assert((runtime.fixtureProfile??null)===(state.workspaceFixtureProfile??null),'WORKSPACE_FIXTURE_PROFILE_CHANGED');
  assert(apps && state.run === input.run && state.prefix === `erpx_browser_${input.run}` && state.executionMode === 'development-smoke' && state.ready && !state.release && !state.diagnostic && state.port === 3104 && state.sourceFingerprint === input.sourceFingerprint, 'DEVELOPMENT_HOST_FIXTURE_STATE_REQUIRED');
  assert(runtime.schema === 'erpx-development-workspace-runtime/v1' && runtime.acceptance === false && runtime.run === input.run && runtime.sourceRoot === sourceRoot && runtime.pid === state.pid && runtime.gateway === 3104 && runtime.sourceFingerprint === input.sourceFingerprint && JSON.stringify(runtime.apps) === JSON.stringify(apps), 'DEVELOPMENT_HOST_GATEWAY_IDENTITY_REQUIRED');
  const listeners = execFileSync('lsof', ['-nP', '-tiTCP:3104', '-sTCP:LISTEN'], { encoding: 'utf8' }).trim().split(/\s+/).map(Number);
  assert(listeners.length === 1 && listeners[0] === runtime.pid, 'OWNED_DEVELOPMENT_HOST_GATEWAY_LISTENER_REQUIRED');
  const manifest = JSON.parse(await readFile(join(directory, 'databases.json'), 'utf8'));
  const slots = ['CONTROL_DB', 'CONTROL_READ_DB', 'H001_DB', 'H001_READ_DB', 'H002_DB', 'H002_READ_DB', 'H003_DB', 'H003_READ_DB'];
  assert(Object.keys(manifest.databases).length === 8 && slots.every(binding => {
    const url = new URL(manifest.databases[binding]);
    return url.hostname === '127.0.0.1' && url.port === '5432' && url.username === 'erpx_test_admin' && url.pathname === `/erpx_browser_${input.run}_${binding.toLowerCase()}`;
  }), 'DEVELOPMENT_HOST_OWNED_DATABASES_REQUIRED');
  const origins = apps[0] === 'compat' ? ['https://console-dev-erpx.jesadakorn.com'] : apps.map(id => appDefinition(id).origin);
  assert(apps.includes(input.entryApp), 'DEVELOPMENT_HOST_ENTRY_APP_REQUIRED');
  assert(JSON.stringify([...runtime.origins].sort()) === JSON.stringify([...origins].sort()), 'DEVELOPMENT_HOST_ORIGIN_REGISTER_CHANGED');
  const hosts = origins.map(origin => new URL(origin).host);
  // Pinned Node owns HTTP probes; Bun's current native client cannot run them.
  const entryOrigin = input.entryApp === 'compat' ? origins[0] : appDefinition(input.entryApp).origin;
  await new Promise<void>((done, reject) => {
    const health = httpRequest({ hostname: '127.0.0.1', port: 3104, path: '/api/health', headers: { Host: new URL(entryOrigin).host }, timeout: 120_000 }, response => {
      response.resume(); response.once('end', () => {
        if (![200, 503].includes(response.statusCode ?? 0) || response.headers['x-erpx-app'] !== input.entryApp) reject(new Error(`DEVELOPMENT_HOST_APP_PROCESS_REQUIRED:${response.statusCode}:${response.headers['x-erpx-app']??'missing'}`)); else done();
      }); response.once('error', reject);
    });
    health.once('timeout', () => health.destroy(new Error('DEVELOPMENT_HOST_APP_START_TIMEOUT'))); health.once('error', reject); health.end();
  });
  assert(!hasPortListener(3102), 'DEVELOPMENT_HOST_PROXY_PORT_OCCUPIED');
  const leasePath = join(directory, 'workspace-edge.json');
  try {
    const previous = JSON.parse(await readFile(leasePath, 'utf8'));
    assert(previous.schema === 'erpx-development-host-edge/v1' && previous.acceptance === false && previous.run === input.run && previous.sourceFingerprint === input.sourceFingerprint && Number.isSafeInteger(previous.pid) && previous.pid > 0, 'OWNED_DEVELOPMENT_HOST_EDGE_LEASE_REQUIRED');
    try { process.kill(previous.pid, 0); throw new Error('DEVELOPMENT_HOST_EDGE_OWNER_STILL_ALIVE'); } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error; }
  } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  const tlsDirectory = await mkdtemp(join(directory, 'workspace-tls-')); await chmod(tlsDirectory, 0o700);
  const keyPath = join(tlsDirectory, 'key.pem'), certificatePath = join(tlsDirectory, 'certificate.pem'), configuration = join(tlsDirectory, 'openssl.cnf');
  await writeFile(configuration, `[req]\nprompt=no\ndistinguished_name=subject\nx509_extensions=extensions\n[subject]\nCN=ERPX isolated development fixture\n[extensions]\nsubjectAltName=${hosts.map(host => 'DNS:' + host).join(',')}\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,digitalSignature,keyEncipherment,keyCertSign\nextendedKeyUsage=serverAuth\n`, { mode: 0o600, flag: 'wx' });
  execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1', '-keyout', keyPath, '-out', certificatePath, '-config', configuration], { stdio: ['ignore', 'ignore', 'pipe'] });
  await chmod(keyPath, 0o600); await chmod(certificatePath, 0o600);
  const certificateBytes = await readFile(certificatePath), certificate = new X509Certificate(certificateBytes);
  assert(hosts.every(host => certificate.checkHost(host) === host) && ((await lstat(keyPath)).mode & 0o077) === 0, 'DEVELOPMENT_HOST_PRIVATE_CERTIFICATE_REQUIRED');
  const spki = hash(certificate.publicKey.export({ type: 'spki', format: 'der' }));
  const sockets = new Set<Socket>(), refusals: { reason: string; method: string; pathHash: string }[] = [];
  const refused = (reason: string, method: string, path: string) => refusals.push({ reason, method, pathHash: hash(path) });
  const tls = createHttpsServer({ key: await readFile(keyPath), cert: certificateBytes, minVersion: 'TLSv1.3' }, (req, res) => {
    const host = String(req.headers.host ?? '');
    try {
      const url = new URL(req.url ?? '/', `https://${host}`);
      assert(hosts.includes(host) && url.host === host && !url.username && !url.password && req.socket.remoteAddress === '127.0.0.1', 'HOST_OUTSIDE_DEVELOPMENT_PROFILE');
      assert(!Object.keys(req.headers).some(name => name === 'forwarded' || name.startsWith('x-forwarded-')), 'CALLER_FORWARDING_REFUSED');
      assert(req.headers.origin === undefined || origins.includes(String(req.headers.origin)), 'ORIGIN_OUTSIDE_DEVELOPMENT_PROFILE');
      const headers = { ...req.headers, host, 'x-forwarded-host': host, 'x-forwarded-proto': 'https', 'x-forwarded-port': '443' }; delete headers['proxy-authorization'];
      const upstream = httpRequest({ host: '127.0.0.1', port: 3104, method: req.method, path: url.pathname + url.search, headers }, response => {
        for (const location of Array.isArray(response.headers.location) ? response.headers.location : response.headers.location ? [response.headers.location] : []) {
          if (!origins.includes(new URL(location, url).origin)) { refused('REDIRECT_OUTSIDE_DEVELOPMENT_PROFILE', req.method ?? '', req.url ?? ''); response.destroy(); res.writeHead(502); res.end(); return; }
        }
        res.writeHead(response.statusCode ?? 502, response.headers); response.pipe(res); response.once('error', () => res.destroy());
      });
      req.once('aborted', () => upstream.destroy()); res.once('close', () => { if (!res.writableFinished) upstream.destroy(); });
      upstream.once('error', () => { if (!res.headersSent) { res.writeHead(503); res.end(); } else res.destroy(); }); req.pipe(upstream);
    } catch (error) { refused(error instanceof Error ? error.message : 'REQUEST_REFUSED', req.method ?? '', req.url ?? ''); res.writeHead(403); res.end(); }
  });
  tls.on('upgrade', (req, downstream, head) => {
    const host = String(req.headers.host ?? '');
    if (!hosts.includes(host) || req.socket.remoteAddress !== '127.0.0.1' || Object.keys(req.headers).some(name => name === 'forwarded' || name.startsWith('x-forwarded-')) || req.headers.origin && !origins.includes(String(req.headers.origin))) { refused('WEBSOCKET_TARGET_REFUSED', req.method ?? '', req.url ?? ''); downstream.destroy(); return; }
    const headers = { ...req.headers, host, 'x-forwarded-host': host, 'x-forwarded-proto': 'https', 'x-forwarded-port': '443' };
    const upstream = httpRequest({ host: '127.0.0.1', port: 3104, method: req.method, path: req.url, headers });
    upstream.once('upgrade', (response, socket, upstreamHead) => {
      downstream.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\n` + response.rawHeaders.reduce((text, value, index, values) => index % 2 ? text : text + value + ': ' + values[index + 1] + '\r\n', '') + '\r\n');
      if (upstreamHead.length) downstream.write(upstreamHead); if (head.length) socket.write(head);
      socket.pipe(downstream); downstream.pipe(socket); socket.once('close', () => downstream.destroy()); downstream.once('close', () => socket.destroy()); socket.once('error', () => downstream.destroy()); downstream.once('error', () => socket.destroy());
    });
    upstream.once('response', response => { downstream.end(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\nConnection: close\r\n\r\n`); response.destroy(); }); upstream.once('error', () => downstream.destroy()); upstream.end();
  });
  const proxy = createHttpServer((_req, res) => { res.writeHead(403); res.end(); });
  proxy.on('connection', socket => { sockets.add(socket); socket.once('close', () => sockets.delete(socket)); });
  proxy.on('connect', (req, socket, head) => {
    const match = /^([a-z0-9.-]+):443$/.exec(req.url ?? '');
    if (!(socket instanceof Socket) || !match || !hosts.includes(match[1]) || socket.remoteAddress !== '127.0.0.1') { refused('CONNECT_TARGET_REFUSED', 'CONNECT', req.url ?? ''); socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n'); return; }
    socket.write('HTTP/1.1 200 Connection Established\r\n\r\n'); if (head.length) socket.unshift(head); tls.emit('connection', socket);
  });
  await new Promise<void>((done, reject) => { proxy.once('error', reject); proxy.listen(3102, '127.0.0.1', () => done()); });
  const address = proxy.address() as AddressInfo;
  assert(address.address === '127.0.0.1' && address.port === 3102, 'DEVELOPMENT_HOST_ACTUAL_PROXY_REQUIRED');
  const lease = { schema: 'erpx-development-host-edge/v1', acceptance: false, run: input.run, pid: process.pid, port: 3102, upstream: 3104, origins, apps, fixtureProfile:state.workspaceFixtureProfile, consoleRetired:Boolean(state.workspaceConsoleRetired), spki, certificate: { path: certificatePath, sha256: hash(certificateBytes) }, runtime: { path: join(directory, 'workspace-runtime.json'), sha256: hash(runtimeBytes) }, sourceFingerprint: input.sourceFingerprint };
  try { await writeFile(leasePath, JSON.stringify(lease, null, 2) + '\n', { mode: 0o600 }); } catch (error) { for (const socket of sockets) socket.destroy(); await new Promise<void>(done => proxy.close(() => done())); throw error; }
  let closing: Promise<void> | undefined;
  return { leasePath, lease, refusals, close() { return closing ??= (async () => { for (const socket of sockets) socket.destroy(); await new Promise<void>(done => proxy.close(() => done())); await writeFile(join(directory, 'workspace-edge-result.json'), JSON.stringify({ acceptance: false, refusals, closed: true }, null, 2) + '\n', { mode: 0o600 }); })(); } };
}

if (import.meta.main) {
  const input = JSON.parse(await readFile(process.argv[2], 'utf8')) as DevelopmentHostEdgeInput;
  const edge = await startDevelopmentHostEdge(input);
  process.send?.({ kind: 'ready', path: edge.leasePath });
  let stopping = false; const stop = async () => { if (stopping) return; stopping = true; await edge.close(); process.exit(0); };
  process.on('SIGTERM', () => { void stop(); }); process.on('SIGINT', () => { void stop(); }); process.on('disconnect', () => { void stop(); });
}
