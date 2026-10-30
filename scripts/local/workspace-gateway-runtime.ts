import { hasPortListener } from "./port-listeners";
import { createServer, request as httpRequest, type IncomingMessage, type ServerResponse } from "node:http";
import { connect } from "node:net";
import { PassThrough, type Duplex } from "node:stream";
import { spawn, type ChildProcess } from "node:child_process";
import { createWriteStream, readFileSync, mkdirSync, writeFileSync, renameSync, existsSync, readlinkSync } from "node:fs";
import { isAbsolute, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { recoverGatewayApps, inspectProcessIdentity, assertRetiredLeasePortsAvailable, linuxInitStartTime, type ProcessIdentity, type PidNamespace } from "./app-process-leases";
import { randomBytes, timingSafeEqual } from "node:crypto";
import { appDefinitions, appDirectory, appForHost, unregisteredDevelopmentProductHost } from "@erpx/contracts/apps";
import {holdingRoutingEnabled,holdingHostForHost,contributorExactOrigin,contributorOriginProfile,originProfile} from '@erpx/feature-runtime/lib/holding-hosts';
import {holdingGatewayApp,holdingGatewayLocation} from './holding-gateway';
import { consoleWorkspaceCutoverBlockers } from "./console-workspace-cutover";
import {consoleRetirementResult,developmentConsoleRetired} from '@erpx/contracts/console-retirement';

/** Optional local budget; isolated fixtures always retain their declared cap. */
export function developmentApplicationBudget(configured:number,override:string|undefined){
 if(!Number.isInteger(configured)||configured<1||configured>12)throw new Error('INVALID_WORKSPACE_SETTINGS');
 if(override===undefined)return configured;
 if(!/^(?:[1-9]|1[0-2])$/.test(override))throw new Error('INVALID_DEVELOPMENT_APP_BUDGET');
 return Number(override);
}

export type WorkspaceGatewaySettings = { version: number; activeApps: string[]; maxApps: number; idleMinutes: number; bundlers: Record<string, string>; defaultBundler: string };
type RunningApp = { id: string; port: number; child: ChildProcess; started: Promise<void>; requests: number; upgraded: Set<Duplex>; lastUsed: number; identity: ProcessIdentity; startupAttempt: number; ready: boolean; servedRequest: boolean; intentionalStop: boolean };
export type WorkspaceGatewayOptions = {
 root:string;node:string;port:3100|3104;environment?:NodeJS.ProcessEnv;
 settings?:()=>WorkspaceGatewaySettings;appPorts?:ReadonlyMap<string,number>;
 fallback?:"compat"|null;preview?:boolean;prewarm?:readonly string[];
};
/** Process supervision and streaming transport contain no business services. */
export async function startWorkspaceGateway(options:WorkspaceGatewayOptions) {
if(!isAbsolute(options.root))throw new Error("ABSOLUTE_WORKSPACE_ROOT_REQUIRED");
const repository = resolve(options.root), node = options.node, environment = {...(options.environment ?? process.env)};
if(JSON.parse(readFileSync(resolve(repository,"package.json"),"utf8")).name!=="erpx")throw new Error("ERPX_WORKSPACE_ROOT_REQUIRED");
if(options.fallback!==undefined && options.fallback!==null && options.fallback!=="compat")throw new Error("INVALID_GATEWAY_FALLBACK");
if (process.version !== "v26.11.1" || !node || !node.startsWith("/") || execFileSync(node,["--version"],{encoding:"utf8"}).trim()!=="v26.11.1") throw new Error("PINNED_GATEWAY_RUNTIME_REQUIRED");
if (![3100,3104].includes(options.port)) throw new Error("OWNED_GATEWAY_PORT_REQUIRED");
// Fail before lease recovery, file writes or app startup on premature cutover.
settings();
const assignedPorts = new Map(options.appPorts ?? [["compat",3200],...appDefinitions.map(app=>[app.id,app.port] as [string,number])]);
const observedPorts = new Set<number>();
for (const [id,port] of assignedPorts) {
 if ((id!=="compat" && !appDefinitions.some(app=>app.id===id)) || !Number.isInteger(port) || port<3200 || port>3299 || observedPorts.has(port)) throw new Error("INVALID_GATEWAY_APP_PORT_ASSIGNMENT");
 observedPorts.add(port);
}
const logs = resolve(repository, ".scratch/dev/apps"); mkdirSync(logs, { recursive: true, mode: 0o700 });
function processIdentity(pid: number): ProcessIdentity | null {
  return inspectProcessIdentity(pid, () => {
    const command = execFileSync("ps", ["-p", String(pid), "-o", "command="], {encoding:"utf8",stdio:["ignore","pipe","ignore"]}).trim();
    const started = execFileSync("ps", ["-p", String(pid), "-o", "lstart="], {encoding:"utf8",stdio:["ignore","pipe","ignore"]}).trim();
    const cwd = execFileSync("lsof", ["-a", "-p", String(pid), "-d", "cwd", "-Fn"], {encoding:"utf8",stdio:["ignore","pipe","ignore"]}).split("\n").find(line=>line.startsWith("n"))?.slice(1);
    if (!command || !started || !cwd) throw new Error("PROCESS_IDENTITY_UNAVAILABLE");
    return {command,cwd,started};
  }, processId => {process.kill(processId,0);});
}
function currentPidNamespace(): PidNamespace | undefined {
  if (process.platform !== 'linux') return undefined;
  try { const bootId=readFileSync('/proc/sys/kernel/random/boot_id','utf8').trim(),namespace=readlinkSync('/proc/self/ns/pid'),initStartTime=linuxInitStartTime(readFileSync('/proc/1/stat','utf8')); return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(bootId)&&/^pid:\[\d+\]$/.test(namespace)&&initStartTime?{bootId,namespace,initStartTime}:undefined; }
  catch { return undefined; } // Unproven namespace retains strict PID checks.
}
const pidNamespace=currentPidNamespace(), priorLease=resolve(logs,"leases.json");
if(existsSync(priorLease)) {
 const previous=JSON.parse(readFileSync(priorLease,"utf8"));
 const oldNamespace=await recoverGatewayApps(previous,repository,{
  identity:processIdentity,
  stop:pid=>{process.kill(pid,"SIGTERM");},
  waitForExit:async pid=>{const deadline=Date.now()+30_000;while(processIdentity(pid)){if(Date.now()>deadline)throw new Error("OWNED_APP_SHUTDOWN_TIMEOUT");await new Promise(done=>setTimeout(done,100));}},
 },assignedPorts,pidNamespace);
 if(oldNamespace) await assertRetiredLeasePortsAvailable(previous,options.port,async port=>hasPortListener(port)||await listenerExists(port));
}
// A routing preview changes the selected implementation, never host/session authority.
const preview = randomBytes(24).toString("hex");
if(options.preview!==false)writeFileSync(resolve(logs, "preview.json"), JSON.stringify({ token: preview, scope: "development-routing-only" }) + "\n", { mode: 0o600 });
const running = new Map<string, RunningApp>(), pendingDemand = new Map<string, number>(); let stopping = false;
function holdDemand(id: string) {
 pendingDemand.set(id,(pendingDemand.get(id)??0)+1);let released=false;
 return()=>{if(released)return;released=true;const remaining=(pendingDemand.get(id)??1)-1;if(remaining)pendingDemand.set(id,remaining);else pendingDemand.delete(id);};
}
function settings(): WorkspaceGatewaySettings {
  const configured = options.settings ? options.settings() : JSON.parse(readFileSync(resolve(repository, "scripts/local/workspace-settings.json"), "utf8")) as WorkspaceGatewaySettings;
  const value = options.settings ? configured : {...configured,maxApps:developmentApplicationBudget(configured.maxApps,options.port===3100?environment.ERPX_DEV_MAX_APPS:undefined)};
  if (value.version !== 1 || !Array.isArray(value.activeApps) || value.activeApps.some(id => !appDefinitions.some(app => app.id === id))
    || !Number.isInteger(value.maxApps) || value.maxApps < 1 || value.maxApps > 12 || !Number.isFinite(value.idleMinutes) || value.idleMinutes < 1
    || !["webpack", "turbopack"].includes(value.defaultBundler)) throw new Error("INVALID_WORKSPACE_SETTINGS");
  if (options.port === 3100 && environment.ERPX_CONSOLE_RETIRED === "enabled") {
    const blockers = consoleWorkspaceCutoverBlockers(value.activeApps, environment);
    if (blockers.length) throw new Error("DEVELOPMENT_CONSOLE_CUTOVER_NOT_READY:" + blockers.join("; "));
  }
  return value;
}
function listenerExists(port: number): Promise<boolean> {
  return new Promise(resolveResult => {
    const socket = connect({ host: "127.0.0.1", port });
    const done = (result: boolean) => { socket.destroy(); resolveResult(result); };
    socket.once("connect", () => done(true)); socket.once("error", () => done(false)); socket.setTimeout(500, () => done(false));
  });
}
function leases() {
  const temporary = resolve(logs, `leases-${process.pid}.tmp`), target = resolve(logs, "leases.json");
  writeFileSync(temporary, JSON.stringify({ version: pidNamespace ? 3 : 2, ...(pidNamespace ? {pidNamespace} : {}), gatewayPid: process.pid, root: repository,
    apps: [...running.values()].map(app => ({ id: app.id, pid: app.child.pid, port: app.port, identity: app.identity })) }, null, 2) + "\n", { mode: 0o600 });
  renameSync(temporary, target);
}
function stopApp(app: RunningApp) {
  if (app.requests || app.upgraded.size || pendingDemand.has(app.id)) return false;
  app.intentionalStop=true; app.child.kill("SIGTERM"); return true;
}
async function acquire(id: string, startupAttempt = 0): Promise<RunningApp> {
  if (stopping) throw new Error("DEVELOPMENT_STOPPING");
  const existing = running.get(id);
  if (existing && !existing.intentionalStop && existing.child.exitCode===null && !existing.child.signalCode) { existing.lastUsed = Date.now(); await existing.started; return existing; }
  if(existing?.intentionalStop && existing.child.exitCode===null && !existing.child.signalCode)await new Promise<void>((done,reject)=>{const timeout=setTimeout(()=>reject(new Error('APP_SHUTDOWN_TIMEOUT')),10_000);existing.child.once('exit',()=>{clearTimeout(timeout);done();});});
  const config = settings(), port = assignedPorts.get(id);
  if (!port) throw new Error("UNKNOWN_GATEWAY_APP");
  if (running.size >= config.maxApps) {
    const idle = [...running.values()].filter(app => !app.requests && !app.upgraded.size && !pendingDemand.has(app.id)).sort((a, b) => a.lastUsed - b.lastUsed)[0];
    if (!idle) throw new Error("DEVELOPMENT_APP_CAPACITY:close an unused app or raise maxApps");
    stopApp(idle);
    await new Promise<void>((done, reject) => { const timeout = setTimeout(() => reject(new Error("APP_SHUTDOWN_TIMEOUT")), 10_000); idle.child.once("exit", () => { clearTimeout(timeout); done(); }); });
  }
  if (hasPortListener(port) || await listenerExists(port)) throw new Error(`APP_PORT_OCCUPIED:${id}:${port}`);
  if (stopping) throw new Error("DEVELOPMENT_STOPPING");
  const bundler = config.bundlers[id] ?? config.defaultBundler;
  if (!["webpack", "turbopack"].includes(bundler)) throw new Error("INVALID_APP_BUNDLER");
  const log = createWriteStream(resolve(logs, `${id}.log`), { flags: "a", mode: 0o600 });
  const child = spawn(node!, [resolve(repository, "node_modules/next/dist/bin/next"), "dev", resolve(repository, appDirectory(id)), "--port", String(port), "--hostname", "127.0.0.1", `--${bundler}`], {
    cwd: repository, stdio: ["ignore", "pipe", "pipe"], env: { ...environment, NODE_ENV: "development",
      ERPX_APP_ID: id, ERPX_APP_PORT: String(port), ERPX_PROCESS_ROLE: "app", ERPX_REPOSITORY_ROOT: repository,
      ERPX_DEV_PG_POOL_MAX: "1", ERPX_CONTROL_PG_POOL_MAX: "2",
      NODE_OPTIONS: `--max-old-space-size=${id === "compat" ? "16384" : environment.ERPX_DEV_APP_HEAP_MIB ?? "4096"}` },
  });
  child.once("error",error=>console.error(`APP_PROCESS_START_FAILED:${id}:${(error as NodeJS.ErrnoException).code??"UNKNOWN"}`));
  child.stdout!.pipe(log, { end: false }); child.stderr!.pipe(log, { end: false });
  let identity: ProcessIdentity | null;
  try { identity=processIdentity(child.pid!); }
  catch(error) { child.kill("SIGTERM");log.end();throw error; }
  if(!identity){child.kill("SIGTERM");log.end();throw new Error("APP_PROCESS_IDENTITY_UNAVAILABLE");}
  const app: RunningApp = { id, port, child, identity, started: Promise.resolve(), requests: 0, upgraded: new Set(), lastUsed: Date.now(), startupAttempt, ready: false, servedRequest: false, intentionalStop: false };
  running.set(id, app); leases();
  child.once("exit", code => {
    const stoppedUnusedStartup = code===1 && app.ready && !app.intentionalStop && !app.servedRequest && app.startupAttempt===0 && !stopping;
    for(const socket of app.upgraded)socket.destroy();
    if(running.get(id)===app)running.delete(id);log.end();leases();
    // A CLI reminder can stop after its first health response. Restart an
    // unused prewarm once; no application request or mutation is replayed.
    if(stoppedUnusedStartup)queueMicrotask(()=>{void ensure(id,1).catch(error=>console.error(error.message));});
  });
  app.started = (async () => {
    const deadline = Date.now() + 120_000;
    let healthy = false;
    while (!healthy) {
      if (stopping || child.exitCode !== null || child.signalCode || Date.now() > deadline) {
        const exited = child.exitCode !== null || !!child.signalCode;
        child.kill("SIGTERM");
        throw Object.assign(new Error(`APP_STARTUP_FAILED:${id}`), {startupExited:exited});
      }
      if (await listenerExists(port)) {
        try {
          const response = await fetch(`http://127.0.0.1:${port}/api/health`, {signal:AbortSignal.timeout(10_000)});
          const accepted = [200,503].includes(response.status) && response.headers.get("x-erpx-app") === id;
          await response.arrayBuffer();
          healthy = accepted;
        } catch { /* The owned listener can still be compiling its first route. */ }
      }
      if (!healthy) await new Promise(done => setTimeout(done, 150));
    }
    // The pinned CLI can exit after its first healthy response. Keep the first
    // request queued through that startup window; never replay its stream.
    await new Promise(done=>setTimeout(done,startupAttempt===0?2000:500));
    if(child.exitCode!==null || child.signalCode || running.get(id)!==app)throw Object.assign(new Error(`APP_STARTUP_FAILED:${id}`),{startupExited:true});
    app.ready=true;
    console.log(`ERPX app HTTP ready: ${id} at 127.0.0.1:${port}`);
  })();
  try { await app.started; return app; }
  catch(error) {
    if(child.exitCode===null && !child.signalCode) {
      child.kill("SIGTERM");
      await new Promise<void>(done=>child.once("exit",()=>done()));
    }
    if(running.get(id)===app)running.delete(id);
    leases();throw error;
  }
}
const starting = new Map<string, Promise<RunningApp>>();
let launchQueue: Promise<unknown> = Promise.resolve();
async function ensure(id: string, startupAttempt = 0) {
  if(stopping)throw new Error('DEVELOPMENT_STOPPING');
  const ready=running.get(id);
  if(ready?.ready&&!ready.intentionalStop&&ready.child.exitCode===null&&!ready.child.signalCode){ready.lastUsed=Date.now();return ready;}
  const pending = starting.get(id); if (pending) return pending;
  const next = launchQueue.then(async () => {
    try { return await acquire(id,startupAttempt); }
    catch(error) {
      // Next's pinned CLI may stop once to show its upgrade recommendation.
      // Retry only an exited startup, before any client request was forwarded.
      if(startupAttempt!==0 || stopping || !(error instanceof Error) || error.message!==`APP_STARTUP_FAILED:${id}` || !(error as Error & {startupExited?:boolean}).startupExited)throw error;
      console.warn(`Retrying exited application startup once before serving requests: ${id}. Review its application log.`);
      return acquire(id,1);
    }
  }); launchQueue = next.catch(() => {}); starting.set(id, next);
  try { return await next; } finally { if (starting.get(id) === next) starting.delete(id); }
}
// The local gateway origin is the same retired Console surface. Restrict the
// alias to this listener so a private production or another project port is
// never treated as a development Console origin.
function retirementHost(host: string | undefined) {
  return host && [`127.0.0.1:${options.port}`, `localhost:${options.port}`, `[::1]:${options.port}`].includes(host)
    ? 'console-dev-erpx.jesadakorn.com' : host;
}
function target(req: IncomingMessage) {
  if (holdingRoutingEnabled(environment) && holdingHostForHost(req.headers.host)) {
    const id = holdingGatewayApp(req.headers.host!, req.url ?? '/', req.headers.referer);
    if (!id || !settings().activeApps.includes(id)) throw new Error('HOLDING_ROUTE_NOT_FOUND');
    return id;
  }
  if(developmentConsoleRetired(environment)&&retirementHost(req.headers.host)==='console-dev-erpx.jesadakorn.com'&&['/api/health','/api/ready'].includes((req.url??'').split('?')[0])){
    settings();return 'account';
  }
  const app = appForHost(req.headers.host ?? "",environment,originProfile());
  const cookies = (req.headers.cookie ?? "").split(";").map(value=>value.trim()).filter(value=>value.startsWith("__Host-erpx-dev-preview="));
  const header = req.headers["x-erpx-workspace-preview"] ?? (cookies.length===1 ? cookies[0].slice("__Host-erpx-dev-preview=".length) : undefined);
  const candidate = typeof header === "string" && /^[a-f0-9]{48}$/.test(header) ? Buffer.from(header) : null;
  const previewed = candidate && timingSafeEqual(candidate, Buffer.from(preview));
  if(app && (settings().activeApps.includes(app.id) || options.preview!==false && previewed))return app.id;
  if(options.fallback===null)throw new Error("HOST_OUTSIDE_DEVELOPMENT_PROFILE");
  return options.fallback ?? "compat";
}
function failed(res: ServerResponse, error: unknown) {
  if (error instanceof Error && error.message === 'HOLDING_ROUTE_NOT_FOUND' && !res.headersSent) { res.writeHead(404, {'Cache-Control':'private, no-store','Content-Type':'text/plain; charset=utf-8'});res.end('Not Found\n');return; }
  console.error(error instanceof Error ? error.message : "GATEWAY_REQUEST_FAILED");
  if (!res.headersSent) { res.writeHead(503, { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "no-store" }); res.end(error instanceof Error&&error.message.startsWith("DEVELOPMENT_APP_CAPACITY:")?"Development app capacity reached. Close an unused development app tab or increase ERPX_DEV_MAX_APPS in your local configuration.":"Development application unavailable. See the owned application log."); }
  else res.destroy();
}
const server = createServer(async (req, res) => {
  if (contributorOriginProfile() && !contributorExactOrigin(req.headers.host) &&
      !([`127.0.0.1:${options.port}`,`localhost:${options.port}`,`[::1]:${options.port}`].includes(req.headers.host??'') && ['/api/health','/api/ready'].includes((req.url??'').split('?')[0]))) {
    res.writeHead(404,{'Cache-Control':'private, no-store'}); res.end(); return;
  }
  if(holdingRoutingEnabled(environment)&&!holdingHostForHost(req.headers.host)&&!appForHost(req.headers.host??'',environment,originProfile())&&
    !['console-dev-erpx.jesadakorn.com','dev-erpx.jesadakorn.com',`127.0.0.1:${options.port}`,`localhost:${options.port}`,`[::1]:${options.port}`].includes(req.headers.host??'')){
    res.writeHead(404,{'Cache-Control':'private, no-store','Content-Type':'text/plain; charset=utf-8'});res.end('Not Found\n');return;
  }
  if (unregisteredDevelopmentProductHost(req.headers.host,environment,originProfile())) {
    res.writeHead(404, { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "private, no-store" });
    res.end("Not Found\n"); return;
  }
  try{
    const retired=consoleRetirementResult({host:retirementHost(req.headers.host),method:req.method??'GET',url:req.url??'/',accept:req.headers.accept},environment);
    if(retired){settings();res.writeHead(retired.status,retired.headers);res.end(retired.body);return;}
  }catch(error){failed(res,error);return;}
  // A temporary host-only routing preference lets the normal browser review a
  // candidate app before its public host mapping changes. It grants no access.
  const previewPath = (req.url ?? "").split("?")[0];
  if (previewPath.startsWith("/.erpx-dev/preview/")) {
    const token=previewPath.slice("/.erpx-dev/preview/".length), clear=token==="clear";
    if (options.preview===false || req.method!=="GET" || !appForHost(req.headers.host ?? "",environment,originProfile()) || !clear && (!/^[a-f0-9]{48}$/.test(token) || !timingSafeEqual(Buffer.from(token),Buffer.from(preview)))) { res.writeHead(404,{"Cache-Control":"no-store"});res.end();return; }
    res.writeHead(303,{Location:"/","Cache-Control":"private, no-store","Referrer-Policy":"no-referrer",
      "Set-Cookie":`__Host-erpx-dev-preview=${clear ? "" : preview}; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=${clear ? 0 : 3600}`});res.end();return;
  }
  let app: RunningApp | undefined, releaseDemand:(()=>void)|undefined;
  try {
    const id=target(req);releaseDemand=holdDemand(id);
    app = await ensure(id); if (req.destroyed) return;
    app.requests++;releaseDemand(); if(!["/api/health","/api/ready"].includes((req.url??"").split("?")[0]))app.servedRequest=true; app.lastUsed = Date.now(); const selected = app;
    let finished = false; const finish = () => { if (!finished) { finished = true; selected.requests--; selected.lastUsed = Date.now(); } };
    res.once("finish", finish); res.once("close", finish);
    // A failed upstream is never retried: this stream may be a business mutation.
    const headers = { ...req.headers }; delete headers["x-erpx-workspace-preview"];
    if (contributorExactOrigin(req.headers.host)) { delete headers["forwarded"]; delete headers["x-forwarded-for"]; delete headers["x-forwarded-server"]; delete headers["x-original-host"]; delete headers["x-original-proto"]; headers["x-forwarded-host"] = req.headers.host; headers["x-forwarded-port"] = "3443"; headers["x-forwarded-proto"] = "https"; }
    const transportPrefix=`/__erpx/${id}`;
    const holdingWorker=holdingRoutingEnabled(environment)&&!!holdingHostForHost(req.headers.host)&&(req.url??'').split('?')[0]===transportPrefix+'/erpx-push-sw.js';
    const upstreamPath=holdingRoutingEnabled(environment)&&holdingHostForHost(req.headers.host)&&((req.url??'').startsWith(transportPrefix+'/api/')||holdingWorker)?req.url!.slice(transportPrefix.length):req.url;
    const upstream = httpRequest({ host: "127.0.0.1", port: app.port, method: req.method, path: upstreamPath, headers }, response => {
      if(holdingWorker&&response.statusCode===200)response.headers['service-worker-allowed']='/';
      if(id!=='compat'&&holdingRoutingEnabled(environment)&&response.headers.location)response.headers.location=holdingGatewayLocation(req.headers.host??'',req.url??'/',response.headers.location,id);
      res.writeHead(response.statusCode ?? 502, response.statusMessage, response.headers); response.pipe(res);
      response.once("error", error => failed(res, error));
    });
    req.once("aborted", () => upstream.destroy()); upstream.once("error", error => failed(res, error));
    res.once("close", () => { if (!res.writableFinished) upstream.destroy(); }); req.pipe(upstream);
  } catch (error) { failed(res, error); } finally {releaseDemand?.();}
});
server.on("upgrade", async (req, downstream, head) => {
  if (contributorOriginProfile() && !contributorExactOrigin(req.headers.host)) {
    downstream.end('HTTP/1.1 404 Not Found\r\nConnection: close\r\nCache-Control: private, no-store\r\nContent-Length: 0\r\n\r\n'); return;
  }
  const contributorOrigin = contributorExactOrigin(req.headers.host);
  if (contributorOrigin && req.headers.origin !== contributorOrigin) {
    downstream.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\nCache-Control: private, no-store\r\nContent-Length: 0\r\n\r\n'); return;
  }
  if(holdingRoutingEnabled(environment)&&!holdingHostForHost(req.headers.host)&&!appForHost(req.headers.host??'',environment,originProfile())&&
    !['console-dev-erpx.jesadakorn.com','dev-erpx.jesadakorn.com',`127.0.0.1:${options.port}`,`localhost:${options.port}`,`[::1]:${options.port}`].includes(req.headers.host??'')){
    downstream.end('HTTP/1.1 404 Not Found\r\nConnection: close\r\nCache-Control: private, no-store\r\nContent-Length: 0\r\n\r\n');return;
  }
  if (unregisteredDevelopmentProductHost(req.headers.host,environment,originProfile())) {
    downstream.end("HTTP/1.1 404 Not Found\r\nConnection: close\r\nCache-Control: private, no-store\r\nContent-Length: 0\r\n\r\n"); return;
  }
  let releaseDemand:(()=>void)|undefined, releaseHandshake:(()=>void)|undefined;
  const waiting=new PassThrough({highWaterMark:64*1024});
  downstream.once('error',()=>downstream.destroy());downstream.once('end',()=>downstream.destroy());
  downstream.once('close',()=>{releaseDemand?.();releaseHandshake?.();waiting.destroy();});
  waiting.once('error',()=>downstream.destroy());downstream.pipe(waiting);
  try {
    // Health exceptions apply to HTTP reads; retired origins never upgrade.
    if(consoleRetirementResult({host:retirementHost(req.headers.host),method:req.method??'GET',url:'/',accept:req.headers.accept},environment)){
      settings();downstream.unpipe(waiting);waiting.destroy();downstream.end('HTTP/1.1 404 Not Found\r\nConnection: close\r\nCache-Control: private, no-store\r\nContent-Length: 0\r\n\r\n');return;
    }
    const id=target(req);releaseDemand=holdDemand(id);
    const app = await ensure(id);if(downstream.destroyed)return;app.requests++;releaseDemand();app.servedRequest=true; app.lastUsed = Date.now();
    let finished=false;releaseHandshake=()=>{if(finished)return;finished=true;app.requests--;app.lastUsed=Date.now();};
    downstream.once('close',()=>releaseHandshake?.());
    const headers = { ...req.headers }; delete headers["x-erpx-workspace-preview"];
    if (contributorExactOrigin(req.headers.host)) { delete headers["forwarded"]; delete headers["x-forwarded-for"]; delete headers["x-forwarded-server"]; delete headers["x-original-host"]; delete headers["x-original-proto"]; headers["x-forwarded-host"] = req.headers.host; headers["x-forwarded-port"] = "3443"; headers["x-forwarded-proto"] = "https"; }
    const upstream = httpRequest({ host: "127.0.0.1", port: app.port, path: req.url, method: req.method, headers });
    downstream.once('close',()=>upstream.destroy());
    upstream.once("upgrade", (response, socket, upstreamHead) => {
      if(downstream.destroyed){releaseHandshake?.();socket.destroy();return;}
      app.upgraded.add(downstream);releaseHandshake?.(); downstream.once("close", () => { app.upgraded.delete(downstream); socket.destroy(); }); socket.once("close", () => downstream.destroy());
      downstream.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\n` + response.rawHeaders.reduce((text, value, index, values) => index % 2 ? text : text + value + ": " + values[index + 1] + "\r\n", "") + "\r\n");
      if (upstreamHead.length) downstream.write(upstreamHead); if (head.length) socket.write(head);
      socket.pipe(downstream); waiting.pipe(socket); socket.once("error", () => downstream.destroy()); downstream.once("error", () => socket.destroy());
    });
    upstream.once("response", response => { response.once('end',()=>releaseHandshake?.());response.once('error',()=>{releaseHandshake?.();downstream.destroy();});downstream.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\nConnection: close\r\n\r\n`); response.pipe(downstream); });
    upstream.once("error", () => {releaseHandshake?.();downstream.destroy();}); upstream.end();
  } catch { releaseHandshake?.();downstream.end("HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n\r\n"); }finally{releaseDemand?.();}
});
const idle = setInterval(() => {
  let limit:number;
  try {limit=settings().idleMinutes*60_000;} catch(error) {
    // Invalid live settings reject requests; they must not crash the supervisor
    // or choose processes to stop using an unvalidated configuration.
    console.error(error instanceof Error?error.message:'INVALID_WORKSPACE_SETTINGS');return;
  }
  for (const app of running.values()) if (Date.now() - app.lastUsed > limit) stopApp(app);
}, 30_000); idle.unref();
let stopped:Promise<void>|undefined;
function stop() {
 if(stopped)return stopped;
 stopping=true;clearInterval(idle);
 const serverClosed=new Promise<void>(done=>server.close(()=>done()));
 for(const app of running.values())for(const socket of app.upgraded)socket.end();
 const appsClosed=new Promise<void>(done=>{
  const drain=setInterval(()=>{
   for(const app of running.values())stopApp(app);
   if(!running.size&&!starting.size){clearInterval(drain);leases();done();}
  },100);
  for(const app of running.values())stopApp(app);
 });
 stopped=Promise.all([serverClosed,appsClosed]).then(()=>{});return stopped;
}
await new Promise<void>((done,reject)=>{
 server.once("error",reject);
 server.listen(options.port,"127.0.0.1",()=>{server.off("error",reject);leases();done();});
});
server.on("error",error=>{console.error(error.message);void stop();});
for(const id of options.prewarm??[])void ensure(id).catch(error=>console.error(error.message));
return {server,close:stop,prewarm:async(id:string)=>{await ensure(id);},leasePath:resolve(logs,"leases.json")};
}
