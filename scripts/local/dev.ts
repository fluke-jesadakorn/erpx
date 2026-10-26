import { hasPortListener } from "./port-listeners";
import { existsSync, mkdirSync, openSync, closeSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { connect } from "node:net";
import { execFileSync } from "node:child_process";
import { appDefinition } from "@erpx/contracts/apps";
import {holdingRoutingEnabled,holdingHosts} from '@erpx/contracts/holding-hosts';
import { prepareNextDevelopment } from "./prepare-next-development";
import { pinnedNode } from "./app-task";

const root = resolve(import.meta.dirname, "../..");
if (Bun.version !== "1.4.3") throw new Error("Development requires Bun 1.4.3.");
const args = process.argv.slice(2).filter(value => value !== "--");
const selected: string[] = [];
for (const arg of args) {
  if (!arg.startsWith("--app=")) throw new Error(`UNKNOWN_DEVELOPMENT_OPTION:${arg}`);
  for (const id of arg.slice(6).split(",")) { appDefinition(id); selected.push(id); }
}
const occupied = hasPortListener(3100) || await new Promise<boolean>(done => {
  const socket = connect({ host: "127.0.0.1", port: 3100 });
  socket.once("connect", () => { socket.destroy(); done(true); }); socket.once("error", () => done(false));
});
if (occupied) {
  const listeners = execFileSync("lsof", ["-nP", "-tiTCP:3100", "-sTCP:LISTEN"], { encoding: "utf8" }).trim().split(/\s+/).filter(Boolean);
  if (!listeners.length) throw new Error("DEVELOPMENT_PORT_OWNER_UNAVAILABLE:3100");
  for (const pid of listeners) {
    const cwd = execFileSync("lsof", ["-a", "-p", pid, "-d", "cwd", "-Fn"], { encoding: "utf8" }).split("\n").find(line => line.startsWith("n"))?.slice(1);
    const command = execFileSync("ps", ["-p", pid, "-o", "command="], { encoding: "utf8" });
    if (cwd !== root || !/(?:gateway\.mjs|next-server|next dev)/.test(command)) throw new Error("DEVELOPMENT_PORT_OCCUPIED_BY_ANOTHER_PROJECT:3100");
  }
  if (selected.length) {
    const previewFile = resolve(root, ".scratch/dev/apps/preview.json");
    if (!existsSync(previewFile)) throw new Error("EXISTING_DEVELOPMENT_GATEWAY_REQUIRED_FOR_APP_PREWARM");
    const token = JSON.parse(readFileSync(previewFile,"utf8")).token;
    execFileSync(pinnedNode(),[resolve(root,'scripts/local/prewarm-apps.mjs')],{cwd:root,
      input:JSON.stringify({version:1,port:3100,token,apps:[...new Set(selected)].map(id=>holdingRoutingEnabled(process.env)&&id!=='account'?{id,host:holdingHosts[0].hostname,path:`/__erpx/${id}/api/health`}:{id,host:new URL(appDefinition(id).origin).host})}),stdio:['pipe','pipe','inherit']});
  }
  console.log("Reusing the existing ERPX development server on 3100."); process.exit(0);
}
const directory = resolve(root, ".scratch/dev"); mkdirSync(directory, { recursive: true, mode: 0o700 });
const lock = resolve(directory, "gateway-start.lock");
if (existsSync(lock)) {
  const pid = Number(readFileSync(lock, "utf8"));
  let alive = false; try { process.kill(pid, 0); alive = true; } catch { /* stale owned startup */ }
  if (alive) throw new Error("DEVELOPMENT_GATEWAY_START_IN_PROGRESS");
  unlinkSync(lock);
}
const descriptor = openSync(lock, "wx", 0o600); writeFileSync(descriptor, String(process.pid)); closeSync(descriptor);
let styleWatcher:ReturnType<typeof import('./workspace-style-watcher').watchWorkspaceStyleSources>|undefined;
try {
  prepareNextDevelopment(root);
  const styles=await import('../assets/workspace-styles');
  styleWatcher=(await import('./workspace-style-watcher')).watchWorkspaceStyleSources(root,styles.refreshWorkspaceStyles().files);
  const bundled = await Bun.build({ entrypoints: [resolve(root, "scripts/local/dev-gateway.ts")], outdir: directory,
    target: "node", format: "esm", naming: "gateway.mjs", env: "disable" });
  if (!bundled.success) throw new Error(bundled.logs.map(log => log.message).join("\n"));
  const node = pinnedNode();
  const child = Bun.spawn([node, resolve(directory, "gateway.mjs")], { cwd: root, stdin: "inherit", stdout: "inherit", stderr: "inherit",
    env: { ...process.env, ERPX_REPOSITORY_ROOT: root, ERPX_NODE_BIN: node,
      ERPX_DEV_PREWARM: [...new Set(selected)].join(","), PATH: dirname(process.execPath) + ":" + process.env.PATH } });
  writeFileSync(resolve(directory, "gateway.pid"), String(child.pid) + "\n", { mode: 0o600 });
  // The one-off parser/bundler graph is no longer needed. Keep only the small
  // watcher text snapshot while the independent Node gateway serves requests.
  Bun.gc(true);
  for (const signal of ["SIGTERM", "SIGINT"] as const) process.on(signal, () => child.kill(signal));
  process.exitCode = await child.exited;
} finally { styleWatcher?.close();if (existsSync(lock) && readFileSync(lock, "utf8") === String(process.pid)) unlinkSync(lock); }
