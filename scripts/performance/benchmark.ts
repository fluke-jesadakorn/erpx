import { resolve } from "node:path";
import { mkdir } from "node:fs/promises";
if (Bun.version !== "1.4.3") throw new Error("Benchmark requires Bun 1.4.3.");
const node = resolve(process.env.ERPX_NODE_BIN ?? ".runtime/node/node-v26.11.1-darwin-arm64/bin/node");
if (Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v26.11.1")
  throw new Error("Benchmark requires Node 26.11.1.");
await mkdir(".scratch/dev", { recursive: true });
const packages = await Bun.file("package.json").json();
const entry = process.argv[2] === "work-snapshot" ? "scripts/performance/benchmark-work-snapshot.ts" : "scripts/performance/benchmark-reads.ts";
const built = await Bun.build({ entrypoints: [entry], target: "node",
  external: [...Object.keys(packages.dependencies), ...Object.keys(packages.devDependencies)],
  outdir: ".scratch/dev", naming: "benchmark-reads.mjs" });
if (!built.success) throw new AggregateError(built.logs, "Benchmark build failed");
// Next's extensionless CommonJS entrypoints need explicit extensions in Node ESM.
for (const output of built.outputs) await Bun.write(output.path,
  (await output.text()).replaceAll('"next/navigation"', '"next/dist/client/components/navigation.react-server.js"').replaceAll('"next/headers"', '"next/headers.js"'));
const child = Bun.spawn([node, "--conditions=react-server", "--env-file=.env.local", resolve(".scratch/dev/benchmark-reads.mjs")], {
  env: { ...process.env, NODE_ENV: "development" }, stdout: "inherit", stderr: "inherit", stdin: "inherit" });
for (const signal of ["SIGINT", "SIGTERM"] as const) process.on(signal, () => child.kill(signal));
process.exitCode = await child.exited;
