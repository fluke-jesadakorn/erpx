import { resolve } from "node:path";
import { existsSync, readFileSync } from "node:fs";
import { appDefinition } from "@erpx/contracts/apps";

const root = resolve(import.meta.dirname, "../..");
export function pinnedNode() {
  const node = process.env.ERPX_NODE_BIN ?? resolve(root, ".runtime/node/node-v26.11.1-darwin-arm64/bin/node");
  if (!existsSync(node) || Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v26.11.1") throw new Error("NODE_26_11_1_REQUIRED");
  return node;
}
export function appTaskArguments(id: string, mode: "dev" | "build", port?: number, bundler = "webpack") {
  const directory = id === "compat" ? "apps/compat" : appDefinition(id).directory;
  if (!["webpack", "turbopack"].includes(bundler)) throw new Error("UNKNOWN_APP_BUNDLER");
  const argv = [resolve(root, "node_modules/next/dist/bin/next"), mode, resolve(root, directory), `--${bundler}`];
  if (mode === "dev") {
    if (!port || port < 3200 || port > 3299) throw new Error("OWNED_APP_PORT_REQUIRED");
    argv.push("--port", String(port), "--hostname", "127.0.0.1");
  }
  return argv;
}
if (import.meta.main) {
  const [mode, ...args] = process.argv.slice(2), selected = args.find(arg => arg.startsWith("--app="))?.slice(6);
  if (!selected || !["dev", "build"].includes(mode)) throw new Error("EXPLICIT_APP_TASK_REQUIRED");
  if(mode === "build"){
    if(selected === "compat")throw new Error("COMPATIBILITY_RELEASE_REQUIRES_EXPLICIT_REVIEW");
    (await import('../assets/workspace-styles')).refreshWorkspaceStyles();
    const settings=JSON.parse(readFileSync(resolve(root,"scripts/local/workspace-settings.json"),"utf8"));
    await (await import("./isolated-app-build")).buildIsolatedApp(selected,settings.bundlers[selected]??"webpack");
    process.exit(0);
  }
  throw new Error('DEVELOPMENT_REQUIRES_ROOT_SUPERVISOR:use bun run dev -- --app='+selected);
}
