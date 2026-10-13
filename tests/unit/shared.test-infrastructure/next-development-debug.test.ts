import { promises as fs, readFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runInNewContext } from "node:vm";
import { describe, expect, it } from "vitest";
import { guardDebugChunkCycles, guardDevelopmentTypeWrites } from "../../../scripts/local/prepare-next-development";

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
