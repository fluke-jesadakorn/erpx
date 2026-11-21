import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { pinnedNode } from "./app-task";

// Use the production Node HTTPS implementation, including verified DNS pinning.
const node = pinnedNode();
const outdir = resolve(".scratch/dev/worker");
await mkdir(outdir, { recursive: true });
const bundle = await Bun.build({
  entrypoints: [resolve("scripts/local/jobs.ts")], outdir,
  target: "node", format: "esm", conditions: ["react-server"],
  external: ["pg"], naming: "jobs.mjs",
  // Match Next's server-layer alias: its public navigation entry includes the
  // client AppRouterContext, which cannot initialize with react-server React.
  plugins: [{
    name: "next-server-navigation",
    setup(build) {
      build.onResolve({ filter: /^next\/navigation$/ }, () => ({
        path: resolve("node_modules/next/dist/client/components/navigation.react-server.js"),
      }));
    },
  }],
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
