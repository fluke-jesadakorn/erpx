import { createHash } from 'node:crypto';
import { mkdir, open, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { request } from 'node:http';
import { join, resolve } from 'node:path';
import { appDefinition } from '@erpx/contracts/apps';
import { pinnedNode } from '../local/app-task';
import { verifyAppArtifact } from '../local/app-artifact-cache';

// Static artifact validation intentionally has no installed database manifest.
// A degraded health response here is evidence of HTTP startup only.
const root = resolve(import.meta.dirname, '../..');
const id = process.argv.find(arg => arg.startsWith('--app='))?.slice(6);
if (!id) throw new Error('EXPLICIT_APP_REQUIRED');
const app = appDefinition(id), output = join(root, app.directory, '.build');
const receipt = JSON.parse(await readFile(join(output, 'artifact.json'), 'utf8'));
if (receipt.schema !== 'erpx-app-artifact/v2' || receipt.app !== id || !await verifyAppArtifact(output, receipt)) throw new Error('VERIFIED_APP_ARTIFACT_REQUIRED');
const directory = join(root, '.scratch/dev/artifact-smoke', id, new Date().toISOString().replace(/[:.]/g, '-'));
await mkdir(directory, { recursive: true, mode: 0o700 });
async function available(port: number) {
  return await new Promise<boolean>(done => { const probe = createServer(); probe.once('error', () => done(false)); probe.listen(port, '127.0.0.1', () => probe.close(() => done(true))); });
}
let port = 3299; while (port >= 3250 && !await available(port)) port--;
if (port < 3250) throw new Error('NO_FREE_ARTIFACT_SMOKE_PORT');
const log = await open(join(directory, 'server.log'), 'a', 0o600);
const child = Bun.spawn([pinnedNode(), join(output, receipt.entry)], { cwd: join(output, 'standalone'), stdin: 'ignore', stdout: log.fd, stderr: log.fd,
  env: { PATH: process.env.PATH, HOME: process.env.HOME, NODE_ENV: 'production', PORT: String(port), ERPX_DATA_DIR: directory,
    ERPX_DATABASE_MANIFEST: join(directory, 'intentionally-missing-databases.json'), ERPX_PRODUCT_ROUTING: 'enabled', ERPX_PRODUCT_SSO: 'enabled', ERPX_PRODUCT_ENVIRONMENT: 'development', ERPX_ENVIRONMENT: 'local' } });
const samples: { path: string; status: number; sha256?: string }[] = [];
// Keep every probe on the owned socket. HTTP redirects must never turn an
// unavailable disposable artifact into a successful installed-host response.
function localResponse(path:string,timeout:number){
  return new Promise<{status:number;bytes:Buffer}>((done,reject)=>{
    const probe=request({hostname:'127.0.0.1',port,path,headers:{Host:new URL(app.origin).host,'X-Forwarded-Proto':'https'}},response=>{
      const chunks:Buffer[]=[];
      response.on('data',chunk=>chunks.push(Buffer.from(chunk)));
      response.on('error',reject);
      response.on('end',()=>done({status:response.statusCode??0,bytes:Buffer.concat(chunks)}));
    });
    probe.setTimeout(timeout,()=>probe.destroy(new Error('ARTIFACT_LOCAL_HTTP_TIMEOUT')));
    probe.on('error',reject);probe.end();
  });
}
let passed = false;
try {
  await writeFile(join(directory, 'lease.json'), JSON.stringify({ root, pid: child.pid, port, app: id, buildFingerprint: receipt.buildFingerprint }) + '\n', { mode: 0o600 });
  const deadline = Date.now() + 30_000;
  let ready = false;
  while (!ready && Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error('ARTIFACT_SERVER_EXITED');
    try { const health = await localResponse('/api/health',1000); ready = [200, 503].includes(health.status); if (ready) samples.push({ path: '/api/health', status: health.status }); } catch { /* Wait for the owned child to bind. */ }
    if (!ready) await Bun.sleep(100);
  }
  if (!ready) throw new Error('ARTIFACT_HTTP_STARTUP_TIMEOUT');
  const staticPrefix = `standalone/${app.directory}/.build/static/`;
  const css = Object.keys(receipt.files).find(file => file.startsWith(staticPrefix) && file.endsWith('.css'));
  const asset = Object.keys(receipt.files).find(file => file.startsWith('standalone/public/brand/') && /\.(?:svg|png)$/.test(file));
  if (!css || !asset) throw new Error('ARTIFACT_CSS_AND_BRAND_REQUIRED');
  const pdfAssets=['pdf.worker.min.mjs','cmaps/78-EUC-H.bcmap','standard_fonts/LiberationSans-Regular.ttf','wasm/openjpeg.wasm'].map(name=>`standalone/public/vendor/pdfjs-6.4.299/${name}`);
  if(pdfAssets.some(file=>!receipt.files[file]))throw new Error('ARTIFACT_PDF_RUNTIME_ASSETS_REQUIRED');
  for (const [file, path] of [[css, '/_next/static/' + css.slice(staticPrefix.length)], [asset, '/' + asset.slice('standalone/public/'.length)],...pdfAssets.map(file=>[file,'/'+file.slice('standalone/public/'.length)])]) {
    const response = await localResponse(path,10_000);
    const digest = createHash('sha256').update(response.bytes).digest('hex');
    samples.push({ path, status: response.status, sha256: digest });
    if (response.status !== 200 || digest !== receipt.files[file]) throw new Error('ARTIFACT_STATIC_CONTENT_MISMATCH:' + path);
  }
  passed = true;
} finally {
  if (child.exitCode === null) child.kill('SIGTERM'); await child.exited; await log.close();
  const path = join(directory, 'receipt.json');
  await writeFile(path, JSON.stringify({ scope: 'development-artifact-static', acceptance: false, passed, app: id, buildFingerprint: receipt.buildFingerprint, database: 'explicitly-unavailable', transport:'owned-loopback-http-no-redirects',port, samples }, null, 2) + '\n', { mode: 0o600 });
  console.log(JSON.stringify({ passed, acceptance: false, receipt: path }));
}
