import { createServer, request as httpRequest, type IncomingMessage, type ServerResponse } from "node:http";
import { connect } from "node:net";
import type { Duplex } from "node:stream";
import { spawn, type ChildProcess } from "node:child_process";
import { createWriteStream, readFileSync, mkdirSync, writeFileSync, renameSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { recoverGatewayApps, type ProcessIdentity } from "./app-process-leases";
import { randomBytes, timingSafeEqual } from "node:crypto";
import { appDefinitions, appForHost } from "@erpx/contracts/apps";

type Settings = { version: number; activeApps: string[]; maxApps: number; idleMinutes: number; bundlers: Record<string, string>; defaultBundler: string };
type RunningApp = { id: string; port: number; child: ChildProcess; started: Promise<void>; requests: number; upgraded: Set<Duplex>; lastUsed: number; identity: ProcessIdentity };
const root = process.env.ERPX_REPOSITORY_ROOT;
if (!root || !process.env.ERPX_NODE_BIN || process.version !== "v26.11.1") throw new Error("PINNED_GATEWAY_RUNTIME_REQUIRED");
const repository = root, node = process.env.ERPX_NODE_BIN;
const logs = resolve(repository, ".scratch/dev/apps"); mkdirSync(logs, { recursive: true, mode: 0o700 });
function processIdentity(pid: number): ProcessIdentity | null {
  try {
    const command = execFileSync("ps", ["-p", String(pid), "-o", "command="], {encoding:"utf8",stdio:["ignore","pipe","ignore"]}).trim();
    const started = execFileSync("ps", ["-p", String(pid), "-o", "lstart="], {encoding:"utf8",stdio:["ignore","pipe","ignore"]}).trim();
    const cwd = execFileSync("lsof", ["-a", "-p", String(pid), "-d", "cwd", "-Fn"], {encoding:"utf8",stdio:["ignore","pipe","ignore"]}).split("\n").find(line=>line.startsWith("n"))?.slice(1);
    if (!command || !started || !cwd) throw new Error("PROCESS_IDENTITY_UNAVAILABLE");
    return {command,cwd,started};
  } catch (error) {
    try { process.kill(pid,0); } catch (probe) { if ((probe as NodeJS.ErrnoException).code === "ESRCH") return null; }
    throw error;
  }
}
const priorLease=resolve(logs,"leases.json");
if(existsSync(priorLease)) await recoverGatewayApps(JSON.parse(readFileSync(priorLease,"utf8")),repository,{
  identity:processIdentity,
  stop:pid=>{process.kill(pid,"SIGTERM");},
  waitForExit:async pid=>{const deadline=Date.now()+30_000;while(processIdentity(pid)){if(Date.now()>deadline)throw new Error("OWNED_APP_SHUTDOWN_TIMEOUT");await new Promise(done=>setTimeout(done,100));}},
});
// A routing preview changes the selected implementation, never host/session authority.
const preview = randomBytes(24).toString("hex");
writeFileSync(resolve(logs, "preview.json"), JSON.stringify({ token: preview, scope: "development-routing-only" }) + "\n", { mode: 0o600 });
const running = new Map<string, RunningApp>(); let stopping = false;
function settings(): Settings {
  const value = JSON.parse(readFileSync(resolve(repository, "scripts/local/workspace-settings.json"), "utf8")) as Settings;
  if (value.version !== 1 || !Array.isArray(value.activeApps) || value.activeApps.some(id => !appDefinitions.some(app => app.id === id))
    || !Number.isInteger(value.maxApps) || value.maxApps < 1 || value.maxApps > 12 || value.idleMinutes < 1
    || !["webpack", "turbopack"].includes(value.defaultBundler)) throw new Error("INVALID_WORKSPACE_SETTINGS");
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
  writeFileSync(temporary, JSON.stringify({ version: 2, gatewayPid: process.pid, root: repository,
    apps: [...running.values()].map(app => ({ id: app.id, pid: app.child.pid, port: app.port, identity: app.identity })) }, null, 2) + "\n", { mode: 0o600 });
  renameSync(temporary, target);
}
function stopApp(app: RunningApp) {
  if (app.requests || app.upgraded.size) return false;
  app.child.kill("SIGTERM"); return true;
}
async function acquire(id: string): Promise<RunningApp> {
  if (stopping) throw new Error("DEVELOPMENT_STOPPING");
  const existing = running.get(id);
  if (existing) { existing.lastUsed = Date.now(); await existing.started; return existing; }
  const config = settings(), port = id === "compat" ? 3200 : appDefinitions.find(app => app.id === id)?.port;
  if (!port) throw new Error("UNKNOWN_GATEWAY_APP");
  if (running.size >= config.maxApps) {
    const idle = [...running.values()].filter(app => !app.requests && !app.upgraded.size).sort((a, b) => a.lastUsed - b.lastUsed)[0];
    if (!idle) throw new Error("DEVELOPMENT_APP_CAPACITY:close an unused app or raise maxApps");
    stopApp(idle);
    await new Promise<void>((done, reject) => { const timeout = setTimeout(() => reject(new Error("APP_SHUTDOWN_TIMEOUT")), 10_000); idle.child.once("exit", () => { clearTimeout(timeout); done(); }); });
  }
  if (await listenerExists(port)) throw new Error(`APP_PORT_OCCUPIED:${id}:${port}`);
  if (stopping) throw new Error("DEVELOPMENT_STOPPING");
  const bundler = config.bundlers[id] ?? config.defaultBundler;
  if (!["webpack", "turbopack"].includes(bundler)) throw new Error("INVALID_APP_BUNDLER");
  const log = createWriteStream(resolve(logs, `${id}.log`), { flags: "a", mode: 0o600 });
  const child = spawn(node!, [resolve(repository, "node_modules/next/dist/bin/next"), "dev", resolve(repository, "apps", id), "--port", String(port), "--hostname", "127.0.0.1", `--${bundler}`], {
    cwd: repository, stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, NODE_ENV: "development",
      ERPX_APP_ID: id, ERPX_REPOSITORY_ROOT: repository,
      ERPX_DEV_PG_POOL_MAX: "1", ERPX_CONTROL_PG_POOL_MAX: "2",
      NODE_OPTIONS: `--max-old-space-size=${id === "compat" ? "16384" : process.env.ERPX_DEV_APP_HEAP_MIB ?? "4096"}` },
  });
  child.stdout!.pipe(log, { end: false }); child.stderr!.pipe(log, { end: false });
  let identity: ProcessIdentity | null;
  try { identity=processIdentity(child.pid!); }
  catch(error) { child.kill("SIGTERM");log.end();throw error; }
  if(!identity){child.kill("SIGTERM");log.end();throw new Error("APP_PROCESS_IDENTITY_UNAVAILABLE");}
  const app: RunningApp = { id, port, child, identity, started: Promise.resolve(), requests: 0, upgraded: new Set(), lastUsed: Date.now() };
  running.set(id, app); leases();
  child.once("exit", () => { for (const socket of app.upgraded) socket.destroy(); if (running.get(id) === app) running.delete(id); log.end(); leases(); });
  app.started = (async () => {
    const deadline = Date.now() + 120_000;
    let healthy = false;
    while (!healthy) {
      if (child.exitCode !== null || child.signalCode || Date.now() > deadline) {
        const exited = child.exitCode !== null || !!child.signalCode;
        child.kill("SIGTERM");
        throw Object.assign(new Error(`APP_STARTUP_FAILED:${id}`), {startupExited:exited});
      }
      if (await listenerExists(port)) {
        try {
          const response = await fetch(`http://127.0.0.1:${port}/api/health`, {signal:AbortSignal.timeout(10_000)});
          healthy = [200,503].includes(response.status) && response.headers.get("x-erpx-app") === id;
          await response.body?.cancel();
        } catch { /* The owned listener can still be compiling its first route. */ }
      }
      if (!healthy) await new Promise(done => setTimeout(done, 150));
    }
    if (!running.has(id)) throw new Error(`APP_STARTUP_FAILED:${id}`);
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
async function ensure(id: string) {
  const pending = starting.get(id); if (pending) return pending;
  const next = launchQueue.then(async () => {
    try { return await acquire(id); }
    catch(error) {
      // Next's pinned CLI may stop once to show its upgrade recommendation.
      // Retry only an exited startup, before any client request was forwarded.
      if(stopping || !(error instanceof Error) || error.message!==`APP_STARTUP_FAILED:${id}` || !(error as Error & {startupExited?:boolean}).startupExited)throw error;
      console.warn(`Retrying exited application startup once before serving requests: ${id}. Review its application log.`);
      return acquire(id);
    }
  }); launchQueue = next.catch(() => {}); starting.set(id, next);
  try { return await next; } finally { if (starting.get(id) === next) starting.delete(id); }
}
function target(req: IncomingMessage) {
  const app = appForHost(req.headers.host ?? "");
  const cookies = (req.headers.cookie ?? "").split(";").map(value=>value.trim()).filter(value=>value.startsWith("__Host-erpx-dev-preview="));
  const header = req.headers["x-erpx-workspace-preview"] ?? (cookies.length===1 ? cookies[0].slice("__Host-erpx-dev-preview=".length) : undefined);
  const candidate = typeof header === "string" && /^[a-f0-9]{48}$/.test(header) ? Buffer.from(header) : null;
  const previewed = candidate && timingSafeEqual(candidate, Buffer.from(preview));
  return app && (settings().activeApps.includes(app.id) || previewed) ? app.id : "compat";
}
function failed(res: ServerResponse, error: unknown) {
  console.error(error instanceof Error ? error.message : "GATEWAY_REQUEST_FAILED");
  if (!res.headersSent) { res.writeHead(503, { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "no-store" }); res.end("Development application unavailable. See the owned application log."); }
  else res.destroy();
}
const server = createServer(async (req, res) => {
  // A temporary host-only routing preference lets the normal browser review a
  // candidate app before its public host mapping changes. It grants no access.
  const previewPath = (req.url ?? "").split("?")[0];
  if (previewPath.startsWith("/.erpx-dev/preview/")) {
    const token=previewPath.slice("/.erpx-dev/preview/".length), clear=token==="clear";
    if (req.method!=="GET" || !appForHost(req.headers.host ?? "") || !clear && (!/^[a-f0-9]{48}$/.test(token) || !timingSafeEqual(Buffer.from(token),Buffer.from(preview)))) { res.writeHead(404,{"Cache-Control":"no-store"});res.end();return; }
    res.writeHead(303,{Location:"/","Cache-Control":"private, no-store","Referrer-Policy":"no-referrer",
      "Set-Cookie":`__Host-erpx-dev-preview=${clear ? "" : preview}; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=${clear ? 0 : 3600}`});res.end();return;
  }
  let app: RunningApp | undefined;
  try {
    app = await ensure(target(req)); if (req.destroyed) return;
    app.requests++; app.lastUsed = Date.now(); const selected = app;
    let finished = false; const finish = () => { if (!finished) { finished = true; selected.requests--; selected.lastUsed = Date.now(); } };
    res.once("finish", finish); res.once("close", finish);
    // A failed upstream is never retried: this stream may be a business mutation.
    const headers = { ...req.headers }; delete headers["x-erpx-workspace-preview"];
    const upstream = httpRequest({ host: "127.0.0.1", port: app.port, method: req.method, path: req.url, headers }, response => {
      res.writeHead(response.statusCode ?? 502, response.statusMessage, response.headers); response.pipe(res);
      response.once("error", error => failed(res, error));
    });
    req.once("aborted", () => upstream.destroy()); upstream.once("error", error => failed(res, error));
    res.once("close", () => { if (!res.writableFinished) upstream.destroy(); }); req.pipe(upstream);
  } catch (error) { failed(res, error); }
});
server.on("upgrade", async (req, downstream, head) => {
  try {
    const app = await ensure(target(req)); app.lastUsed = Date.now();
    const headers = { ...req.headers }; delete headers["x-erpx-workspace-preview"];
    const upstream = httpRequest({ host: "127.0.0.1", port: app.port, path: req.url, method: req.method, headers });
    upstream.once("upgrade", (response, socket, upstreamHead) => {
      app.upgraded.add(downstream); downstream.once("close", () => { app.upgraded.delete(downstream); socket.destroy(); }); socket.once("close", () => downstream.destroy());
      downstream.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\n` + response.rawHeaders.reduce((text, value, index, values) => index % 2 ? text : text + value + ": " + values[index + 1] + "\r\n", "") + "\r\n");
      if (upstreamHead.length) downstream.write(upstreamHead); if (head.length) socket.write(head);
      socket.pipe(downstream); downstream.pipe(socket); socket.once("error", () => downstream.destroy()); downstream.once("error", () => socket.destroy());
    });
    upstream.once("response", response => { downstream.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\nConnection: close\r\n\r\n`); response.pipe(downstream); });
    upstream.once("error", () => downstream.destroy()); upstream.end();
  } catch { downstream.end("HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n\r\n"); }
});
const idle = setInterval(() => { const limit = settings().idleMinutes * 60_000; for (const app of running.values()) if (Date.now() - app.lastUsed > limit) stopApp(app); }, 30_000); idle.unref();
async function stop() {
  if (stopping) return; stopping = true; clearInterval(idle); server.close();
  for (const app of running.values()) for (const socket of app.upgraded) socket.end();
  // Requests and streamed responses finish before their app is stopped. Never
  // impose a timer that cuts an in-flight mutation or response in half.
  const drain = setInterval(() => {
    for (const app of running.values()) stopApp(app);
    if (!running.size && !starting.size) { clearInterval(drain); leases(); }
  }, 100);
  for (const app of running.values()) stopApp(app);
}
process.on("SIGTERM", stop); process.on("SIGINT", stop);
server.once("error", error => { console.error(error.message); void stop(); process.exitCode = 1; });
server.listen(3100, "127.0.0.1", () => {
  leases(); console.log("ERPX development gateway: http://127.0.0.1:3100");
  for (const id of (process.env.ERPX_DEV_PREWARM ?? "").split(",").filter(Boolean)) void ensure(id).catch(error => console.error(error.message));
});
