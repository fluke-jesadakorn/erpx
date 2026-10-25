// Reuse revalidated assets across restarts of the same source. A source change
// gets a new deployment ID, retaining Next's version-skew protection.
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { prepareNextDevelopment } from "./prepare-next-development";
if (Bun.version !== "1.2.17") throw new Error("Development requires Bun 1.2.17.");
prepareNextDevelopment();
const node = resolve(process.env.ERPX_NODE_BIN ?? ".runtime/node/node-v24.20.0-darwin-arm64/bin/node");
if (Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v24.20.0")
  throw new Error("Development requires Node 24.20.0.");
const fingerprint = createHash("sha256").update("node24.20.0:bun1.2.17");
const sources = readdirSync("src", { recursive: true, withFileTypes: true })
  .filter(entry => entry.isFile()).map(entry => resolve(entry.parentPath, entry.name));
for (const file of [...sources, "bun.lock", "package.json", "next.config.ts", "tsconfig.json", "postcss.config.mjs", "scripts/local/dev.ts", "scripts/local/prepare-next-development.ts"].sort()) {
  fingerprint.update(file).update("\0").update(readFileSync(file)).update("\0");
}
for (const key of Object.keys(process.env).filter(key => key.startsWith("NEXT_PUBLIC_")).sort())
  fingerprint.update(key).update("\0").update(process.env[key] ?? "").update("\0");
const deploymentId = `dev-${fingerprint.digest("hex").slice(0, 24)}`;
// The full workspace graph exceeded Next's restart threshold even at 8 GiB,
// interrupting RSC/API requests with 502s and forcing browser reloads. Give
// compilation 16 GiB of headroom; smaller machines can explicitly use 4/8 GiB.
const devArgs = process.argv.slice(2);
if (!devArgs.some(arg => ["--webpack", "--turbopack", "--turbo"].includes(arg)))
  devArgs.unshift("--webpack");
const heapMiB = process.env.ERPX_DEV_HEAP_MIB ?? "16384";
if (!["4096", "8192", "16384"].includes(heapMiB)) throw new Error("ERPX_DEV_HEAP_MIB must be 4096, 8192 or 16384.");
const inheritedNodeOptions = process.env.NODE_OPTIONS ?? "";
const nodeOptions = /(?:^|\s)--max-old-space-size(?:=|\s+)\d+/.test(inheritedNodeOptions)
  ? inheritedNodeOptions
  : `${inheritedNodeOptions} --max-old-space-size=${heapMiB}`.trim();
const child = Bun.spawn([node, resolve("node_modules/next/dist/bin/next"), "dev", "-p", "3100", ...devArgs], {
  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
  env: {
    ...process.env,
    NEXT_DEPLOYMENT_ID: deploymentId,
    NODE_OPTIONS: nodeOptions,
  },
});
for (const signal of ["SIGINT", "SIGTERM"] as const)
  process.on(signal, () => child.kill(signal));
process.exitCode = await child.exited;
