import { resolve } from "node:path";
import { mkdir } from "node:fs/promises";
if (Bun.version !== "1.2.17") throw new Error("Benchmark requires Bun 1.2.17.");
const node = resolve(process.env.ERPX_NODE_BIN ?? ".runtime/node/node-v24.20.0-darwin-arm64/bin/node");
if (Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v24.20.0")
  throw new Error("Benchmark requires Node 24.20.0.");
await mkdir(".scratch/dev", { recursive: true });
const packages = await Bun.file("package.json").json();
const built = await Bun.build({ entrypoints: ["scripts/local/benchmark-reads.ts"], target: "node",
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
