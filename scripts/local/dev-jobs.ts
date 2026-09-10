import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

// Use the production Node HTTPS implementation, including verified DNS pinning.
const node = resolve(".runtime/node/node-v24.20.0-darwin-arm64/bin/node");
if (Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v24.20.0")
  throw new Error("NODE_24_20_REQUIRED");
const outdir = resolve(".scratch/dev/worker");
await mkdir(outdir, { recursive: true });
const bundle = await Bun.build({
  entrypoints: [resolve("scripts/local/jobs.ts")], outdir,
  target: "node", format: "esm", conditions: ["react-server"],
  external: ["pg"], naming: "jobs.mjs",
});
if (!bundle.success) throw new Error("DEV_JOBS_BUILD_FAILED");
const child = Bun.spawn([node, resolve(outdir, "jobs.mjs")], {
  stdout: "inherit", stderr: "inherit", stdin: "ignore",
  env: {
    ...process.env,
    ERPX_PROCESS_ROLE: "jobs",
    ERPX_DATABASE_MANIFEST: resolve(process.env.ERPX_JOBS_DATABASE_MANIFEST ?? ".runtime/local-test/config/databases-jobs.json"),
  },
});
for (const signal of ["SIGINT", "SIGTERM"] as const)
  process.on(signal, () => child.kill(signal));
process.exitCode = await child.exited;
