import { promises as fs, readFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runInNewContext } from "node:vm";
import { describe, expect, it } from "vitest";
import { clearDevelopmentCaches, guardDebugChunkCycles, guardDevelopmentTypeWrites, guardPendingPerformanceMetadata } from "../../../scripts/local/prepare-next-development";

describe.each(["webpack", "turbopack", "webpack-experimental", "turbopack-experimental"])("Next pending performance metadata: %s", variant => {
  const file = `node_modules/next/dist/compiled/react-server-dom-${variant}/cjs/react-server-dom-${variant.replace("-experimental", "")}-client.browser.development.js`;
  const upstream = readFileSync(file, "utf8")
    .replace("/* ERPX: pending development performance metadata */\n    ", "")
    .replaceAll(/\n\s*if \(null == (?:info|_info|_info2|candidateInfo|_candidateInfo)\) continue;/g, "");
  const patched = guardPendingPerformanceMetadata(upstream);
  function trace(source: string, debugInfo: unknown[]) {
    const start = source.indexOf("function flushComponentPerformance("), end = source.indexOf("function flushInitialRenderPerformance(", start);
    const model = Object.freeze({ content: "actual reports page" });
    const root = { status: "fulfilled", value: model, _debugInfo: debugInfo, _children: [] };
    const result = runInNewContext(`${source.slice(start, end)}; flushComponentPerformance({_rootEnvironmentName:'Server'},root,0,-Infinity,-Infinity)`, { root, isArrayImpl: Array.isArray, supportsUserTiming: false }, { timeout: 100 });
    expect(root.status).toBe("fulfilled"); expect(root.value).toBe(model);
    return result;
  }
  it("traces pending sparse metadata without changing reserved indexes or the rendered model", () => {
    const debugInfo = new Array(9); debugInfo[1] = { time: 10 }; debugInfo[3] = { name: "Reports", env: "Server" }; debugInfo[7] = { time: 20 };
    const before = Object.getOwnPropertyDescriptors(debugInfo);
    // Reproduce the installed browser failure on the exact upstream function.
    expect(() => trace(upstream, debugInfo)).toThrow(/time/);
    expect(() => trace(patched, debugInfo)).not.toThrow();
    expect(Object.getOwnPropertyDescriptors(debugInfo)).toEqual(before);
    debugInfo[2] = { name: "Later resolved metadata", env: "Server" };
    expect(() => trace(patched, debugInfo)).not.toThrow(); expect(debugInfo[3].name).toBe("Reports");
  });
  it("preserves ordinary dense performance results", () => {
    const metadata = [{ time: 10 }, { name: "Reports", env: "Server" }, { time: 20 }];
    expect(trace(patched, metadata)).toEqual(trace(upstream, metadata));
  });
  it("is idempotent and rejects changed upstream trace code", () => {
    expect(guardPendingPerformanceMetadata(patched)).toBe(patched);
    expect(() => guardPendingPerformanceMetadata("changed runtime")).toThrow("no longer matches");
    const invalid = upstream.replace("var candidateInfo = debugInfo[j];", "var candidateInfo = changed[j];");
    expect(() => guardPendingPerformanceMetadata(invalid)).toThrow("no longer matches");
  });
});

describe("Next workspace vendor cache invalidation", () => {
  it("invalidates owned root and app dev caches while preserving compiled outputs and unrelated directories", async () => {
    const directory = await fs.mkdtemp(join(tmpdir(), "erpx-next-cache-"));
    try {
      for (const app of ["", "apps/finance", "apps/launcher"]) {
        await fs.mkdir(join(directory, app, ".next/dev/cache"), { recursive: true });
        await fs.writeFile(join(directory, app, ".next/dev/cache/vendor"), "old vendor");
      }
      await fs.mkdir(join(directory, "apps/finance/.build"), { recursive: true });
      await fs.writeFile(join(directory, "apps/finance/.build/receipt"), "compiled artifact");
      await fs.mkdir(join(directory, "unrelated/.next/dev/cache"), { recursive: true });
      clearDevelopmentCaches(directory);
      for (const app of ["", "apps/finance", "apps/launcher"]) await expect(fs.access(join(directory, app, ".next/dev/cache"))).rejects.toThrow();
      expect(await fs.readFile(join(directory, "apps/finance/.build/receipt"), "utf8")).toBe("compiled artifact");
      await expect(fs.access(join(directory, "unrelated/.next/dev/cache"))).resolves.toBeUndefined();
    } finally { await fs.rm(directory, { recursive: true, force: true }); }
  });
  it("refuses a foreign app cache before deleting any owned cache", async () => {
    const directory = await fs.mkdtemp(join(tmpdir(), "erpx-next-cache-")), foreign = await fs.mkdtemp(join(tmpdir(), "erpx-next-foreign-"));
    try {
      await fs.mkdir(join(directory, ".next/dev/cache"), { recursive: true });
      await fs.writeFile(join(directory, ".next/dev/cache/vendor"), "owned");
      await fs.mkdir(join(directory, "apps/finance/.next/dev"), { recursive: true });
      await fs.writeFile(join(foreign, "sentinel"), "foreign");
      await fs.symlink(foreign, join(directory, "apps/finance/.next/dev/cache"));
      expect(() => clearDevelopmentCaches(directory)).toThrow("OWNED_DEVELOPMENT_CACHE_REQUIRED");
      expect(await fs.readFile(join(directory, ".next/dev/cache/vendor"), "utf8")).toBe("owned");
      expect(await fs.readFile(join(foreign, "sentinel"), "utf8")).toBe("foreign");
    } finally { await fs.rm(directory, { recursive: true, force: true }); await fs.rm(foreign, { recursive: true, force: true }); }
  });
});

function initialize(patched: string, chunk: object) {
  const start = patched.indexOf("function initializeDebugChunk(");
  const end = patched.indexOf("function initializeModelChunk(", start);
  const calls: unknown[] = [];
  const sandbox = {
    chunk, calls, isInitializingDebugInfo: false,
    initializeModelChunk: (debug: { status: string; value?: string }) => {
      calls.push("model"); debug.status = "fulfilled"; debug.value = "rendered";
    },
    initializeDebugInfo: (_response: unknown, value: unknown) => value,
    triggerErrorOnChunk: (_response: unknown, _chunk: unknown, error: unknown) => { throw error; },
  };
  runInNewContext(`${patched.slice(start, end)}; initializeDebugChunk({}, chunk);`, sandbox, { timeout: 100 });
  return { calls, sandbox };
}

describe.each([
  "react-server-dom-webpack/cjs/react-server-dom-webpack-client.node.development.js",
  "react-server-dom-webpack/cjs/react-server-dom-webpack-client.browser.development.js",
  "next-server/app-page.runtime.dev.js",
])("Next development debug chunk recovery: %s", (file) => {
  const patched = guardDebugChunkCycles(readFileSync(`node_modules/next/dist/compiled/${file}`, "utf8"));
  it("finishes when optional debug metadata has a self-cycle", () => {
    const debug = { status: "resolved_model", _debugChunk: null as unknown };
    debug._debugChunk = debug;
    const chunk = { _debugChunk: debug, _debugInfo: [], value: "page model" };
    expect(initialize(patched, chunk).calls).toEqual([]);
    expect(chunk._debugChunk).toBeNull();
    expect(chunk.value).toBe("page model");
  });

  it("finishes when two debug chunks form a cycle", () => {
    const a = { status: "resolved_model", _debugChunk: null as unknown };
    const b = { status: "pending", _debugChunk: a };
    a._debugChunk = b;
    const chunk = { _debugChunk: a, _debugInfo: [] };
    expect(initialize(patched, chunk).calls).toEqual([]);
    expect(chunk._debugChunk).toBeNull();
  });

  it("still initializes acyclic debug metadata and restores tracing state", () => {
    const chunk = { _debugChunk: { status: "resolved_model", _debugChunk: null }, _debugInfo: [] };
    const { calls, sandbox } = initialize(patched, chunk);
    expect(calls).toEqual(["model"]);
    expect(chunk._debugInfo).toEqual(["rendered"]);
    expect(sandbox.isInitializingDebugInfo).toBe(false);
  });

  it("bounds optional debug ancestry while leaving the actual page model and tracing state intact", () => {
    let ancestry: unknown = null;
    for (let index = 0; index < 8192; index++) ancestry = { status: "pending", _debugChunk: ancestry };
    const debug = { status: "resolved_model", _debugChunk: ancestry };
    const chunk = { _debugChunk: debug, _debugInfo: [], value: "actual page model" };
    const { calls, sandbox } = initialize(patched, chunk);
    expect(calls).toEqual([]);
    expect(chunk._debugChunk).toBeNull();
    expect(chunk._debugInfo).toEqual([]);
    expect(chunk.value).toBe("actual page model");
    expect(sandbox.isInitializingDebugInfo).toBe(false);
  });

  it("retains ordinary nontrivial debug ancestry and its original index", () => {
    let ancestry: unknown = null;
    for (let index = 0; index < 32; index++) ancestry = { status: "pending", _debugChunk: ancestry };
    const chunk = { _debugChunk: { status: "resolved_model", _debugChunk: ancestry }, _debugInfo: [] as unknown[] };
    expect(initialize(patched, chunk).calls).toEqual(["model"]);
    expect(chunk._debugInfo).toHaveLength(33);
    expect(chunk._debugInfo[32]).toBe("rendered");
  });

  it("is idempotent and fails explicitly if the upstream code changes", () => {
    expect(guardDebugChunkCycles(patched)).toBe(patched);
    expect(() => guardDebugChunkCycles("changed runtime")).toThrow("no longer matches");
  });
});

describe("Next development generated type files", () => {
type Writer = (temporary: string) => Promise<void>;
type Write = (path: string, writer: Writer) => Promise<void>;
const upstream = readFileSync("node_modules/next/dist/server/lib/router-utils/setup-dev-bundler.js", "utf8");

function guardedWrite(): Write {
  const patched = guardDevelopmentTypeWrites(upstream);
  const start = patched.indexOf("/* ERPX: atomic development route type writes */");
  const end = patched.indexOf("/* ERPX: end atomic development route type writes */", start);
  return runInNewContext(`${patched.slice(start, end)}; erpxWriteGeneratedType`, {
    _fs: { default: { promises: fs } }, process: { pid: process.pid },
    require: (name: string) => {
      if (name !== "node:crypto") throw new Error("Unexpected development helper import");
      return { randomUUID };
    },
  }) as Write;
}

it("keeps a complete type file visible during a real interrupted write and orders overlapping generations", async () => {
  for (const protectedWrite of [false, true]) {
    const directory = await fs.mkdtemp(join(tmpdir(), "erpx-dev-type-write-"));
    const path = join(directory, "routes.d.ts");
    const initial = "export type Route = '/before';\n";
    const latest = "export type Route = '/latest';\n";
    const prefix = "export type Route = '/";
    const suffix = "older';\n" + "// longer generated content\n".repeat(100);
    let enter!: () => void, release!: () => void;
    const entered = new Promise<void>((resolve) => { enter = resolve; });
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const write: Write = protectedWrite ? guardedWrite() : async (file, writer) => writer(file);
    let pending: Promise<void> | undefined;
    try {
      await fs.writeFile(path, initial);
      pending = write(path, async (file) => {
        const handle = await fs.open(file, "w");
        try {
          await handle.write(prefix); enter(); await gate;
          await handle.write(suffix);
        } finally { await handle.close(); }
      });
      await entered;
      expect(await fs.readFile(path, "utf8")).toBe(protectedWrite ? initial : prefix);
      if (protectedWrite) {
        let secondStarted = false;
        const next = write(path, async (file) => { secondStarted = true; await fs.writeFile(file, latest); });
        expect(secondStarted).toBe(false);
        release(); await Promise.all([pending, next]);
        expect(await fs.readFile(path, "utf8")).toBe(latest);
        expect(await fs.readdir(directory)).toEqual(["routes.d.ts"]);
      } else { release(); await pending; }
    } finally {
      release(); await pending?.catch(() => {});
      await fs.rm(directory, { recursive: true, force: true });
    }
  }
});

it("preserves the previous file and recovers the queue after a failed generation", async () => {
  const directory = await fs.mkdtemp(join(tmpdir(), "erpx-dev-type-failure-"));
  const path = join(directory, "validator.ts"), write = guardedWrite();
  try {
    await fs.writeFile(path, "export {};\n");
    await expect(write(path, async (file) => {
      await fs.writeFile(file, "unfinished"); throw new Error("GENERATION_FAILED");
    })).rejects.toThrow("GENERATION_FAILED");
    expect(await fs.readFile(path, "utf8")).toBe("export {};\n");
    await write(path, async (file) => { await fs.writeFile(file, "export type Ready = true;\n"); });
    expect(await fs.readFile(path, "utf8")).toBe("export type Ready = true;\n");
    expect(await fs.readdir(directory)).toEqual(["validator.ts"]);
  } finally { await fs.rm(directory, { recursive: true, force: true }); }
});

it("patches only the pinned development callers and detects upstream changes", () => {
  const patched = guardDevelopmentTypeWrites(upstream);
  expect(guardDevelopmentTypeWrites(patched)).toBe(patched);
  expect(patched).toContain("writeRouteTypesManifest)(routeTypesManifest, temporary, opts.nextConfig)");
  expect(patched).toContain("writeValidatorFile)(routeTypesManifest, temporary, Boolean(nextConfig.experimental.strictRouteTypes))");
  expect(() => guardDevelopmentTypeWrites("changed development runtime")).toThrow("no longer matches");
});

});
