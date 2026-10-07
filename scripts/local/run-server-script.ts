/** Run repository server utilities with the same server aliases as compiled jobs. */
import { mkdir, readFile } from "node:fs/promises";
import { dirname, relative, resolve, sep } from "node:path";
import { runtimeSourceFingerprint } from "./source-fingerprint";

const root = resolve(import.meta.dir, "../.."), requested = process.argv[2];
if (!requested) throw new Error("EXACT_REPOSITORY_SERVER_SCRIPT_REQUIRED");
const entry = resolve(root, requested);
if (!entry.startsWith(resolve(root, "scripts") + sep) || !entry.endsWith(".ts") || entry === resolve(import.meta.filename))
  throw new Error("REPOSITORY_SERVER_SCRIPT_REQUIRED");
const node = resolve(process.env.ERPX_NODE_BIN ?? resolve(root, ".runtime/node/node-v24.20.0-darwin-arm64/bin/node"));
if (Bun.version !== "1.2.17" || Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v24.20.0")
  throw new Error("PINNED_TOOLCHAIN_REQUIRED");
const directory = resolve(root, ".scratch/dev/server-scripts", relative(root, entry).replaceAll(/[^a-zA-Z0-9_-]/g, "_"));
await mkdir(directory, { recursive: true, mode: 0o700 });
const sourceFingerprint = await runtimeSourceFingerprint(root);
const built = await Bun.build({ entrypoints: [entry], outdir: directory, target: "node", format: "esm",
  conditions: ["react-server"], external: ["pg"], naming: "script.mjs",
  plugins: [{ name: "next-server-navigation", setup(build) {
    build.onResolve({ filter: /^next\/navigation$/ }, () => ({ path: resolve(root, "node_modules/next/dist/client/components/navigation.react-server.js") }));
    // A bundled CLI still owns the original repository, not its private output
    // directory. These entry files use Bun's import.meta.dir to locate it.
    build.onLoad({ filter: /\.ts$/ }, async args => {
      if (!args.path.startsWith(root + sep) || args.path.includes(sep + "node_modules" + sep)) return;
      const source = await readFile(args.path, "utf8");
      if (!source.includes("import.meta.dir") && !source.includes("import.meta.filename")) return;
      return { contents: source.replaceAll("import.meta.dir", JSON.stringify(dirname(args.path)))
        .replaceAll("import.meta.filename", JSON.stringify(args.path)), loader: "ts" };
    });
  } }],
});
if (!built.success) throw new Error(built.logs.map(log => log.message).join("\n"));
if (await runtimeSourceFingerprint(root) !== sourceFingerprint)
  throw new Error("SERVER_SCRIPT_SOURCE_CHANGED_DURING_COMPILATION");
// Keep Bun APIs available to provisioning/simulation commands while resolving
// server-only imports with the same aliases used by the pinned Node worker.
// Parent configuration is already resolved. Reloading workspace dotenv here
// would restore omitted live credentials inside an isolated child environment.
const child = Bun.spawn([process.execPath, "--env-file=/dev/null", resolve(directory, "script.mjs"), ...process.argv.slice(3)], {
  cwd: root, env: { ...process.env, ERPX_SERVER_SCRIPT_SOURCE_FINGERPRINT: sourceFingerprint }, stdin: "inherit", stdout: "inherit", stderr: "inherit",
});
for (const signal of ["SIGINT", "SIGTERM"] as const) process.on(signal, () => child.kill(signal));
process.exitCode = await child.exited;
