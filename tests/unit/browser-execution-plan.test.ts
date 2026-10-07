import { expect, it, vi } from "vitest";
import { EventEmitter } from "node:events";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Suite } from "../../scripts/testing/catalogue";
import type { BrowserRunReceipt } from "../../scripts/testing/browser-runner";
import { assertBrowserFaultProbeSelection, browserFaultProbeError, isBrowserFaultProbeCase, orderedBrowserExecutionCases, orderedBrowserExecutionGroups, parseBrowserCaseOrder, selectBrowserCaseOrder, triggerBrowserFaultProbe } from "../../scripts/testing/browser-execution-plan";
import { browserCaseKey, type BrowserCaseIdentity } from "../../scripts/testing/browser-sharding";

const suites = [{ id: "people", files: ["tests/e2e/people.spec.ts"] }, { id: "finance", files: ["tests/e2e/finance.spec.ts"] }];
const projects = ["desktop", "mobile"] as const;

it("reverses the complete suite project and case sequence without losing identities or mutating annotations", () => {
  const annotations = [{ type: "erpx-browser-projects", description: "retained original annotation" }];
  const cases = [{ title: "first", annotations }, { title: "second", annotations }];
  const flatten = (order: "forward" | "reverse") => orderedBrowserExecutionGroups(suites, [...projects], order).flatMap(({ suite, project }) => orderedBrowserExecutionCases(cases, order).map(test => browserCaseKey({ suite: suite.id, file: suite.files[0], project, title: test.title })));
  const original = JSON.stringify({ suites, projects, cases });
  const forward = flatten("forward"), reverse = flatten("reverse");
  expect(reverse).toEqual([...forward].reverse());
  expect(new Set(reverse).size).toBe(8);
  expect(JSON.stringify({ suites, projects, cases })).toBe(original);
  expect(orderedBrowserExecutionCases(cases, "reverse")[0].annotations).toBe(annotations);
  expect(orderedBrowserExecutionGroups(suites, [...projects], "forward")[0]).toEqual({ suite: suites[0], project: "desktop" });
});

it("rejects unknown execution orders and duplicate or missing suite project selections", () => {
  expect(parseBrowserCaseOrder("forward")).toBe("forward");
  expect(parseBrowserCaseOrder("reverse")).toBe("reverse");
  expect(selectBrowserCaseOrder(undefined, "reverse")).toBe("reverse");
  expect(() => selectBrowserCaseOrder("forward", "forward")).toThrow("DUPLICATE_BROWSER_CASE_ORDER");
  expect(() => selectBrowserCaseOrder("reverse", "forward")).toThrow("DUPLICATE_BROWSER_CASE_ORDER");
  for (const invalid of ["", "random", "Forward", "reverse,forward"]) expect(() => parseBrowserCaseOrder(invalid)).toThrow("INVALID_BROWSER_CASE_ORDER");
  expect(() => orderedBrowserExecutionGroups(suites, [], "forward")).toThrow("INVALID_BROWSER_EXECUTION_GROUPS");
  expect(() => orderedBrowserExecutionGroups(suites, ["desktop", "desktop"], "reverse")).toThrow("INVALID_BROWSER_EXECUTION_GROUPS");
  expect(() => orderedBrowserExecutionGroups([suites[0], suites[0]], ["desktop"], "reverse")).toThrow("INVALID_BROWSER_EXECUTION_GROUPS");
});

it("requires an exact declared fault probe in one registered suite project and preserves canonical case identity", () => {
  const probe: BrowserCaseIdentity = { suite: "people", file: "tests/e2e/people.spec.ts", project: "mobile", title: "first" };
  const group = orderedBrowserExecutionGroups([suites[0]], ["mobile"], "forward");
  expect(() => assertBrowserFaultProbeSelection(probe, group)).not.toThrow();
  expect(isBrowserFaultProbeCase("people", { ...probe, file: "/workspace/tests/e2e/people.spec.ts" }, probe)).toBe(true);
  for (const invalid of [{ ...probe, file: "tests/e2e/other.spec.ts" }, { ...probe, suite: "finance" }, { ...probe, project: "desktop" as const }, { ...probe, title: " " }]) expect(() => assertBrowserFaultProbeSelection(invalid, group)).toThrow("EXACT_BROWSER_FAULT_PROBE_SELECTION_REQUIRED");
  expect(() => assertBrowserFaultProbeSelection(probe, orderedBrowserExecutionGroups(suites, [...projects], "forward"))).toThrow("EXACT_BROWSER_FAULT_PROBE_SELECTION_REQUIRED");
  expect(() => assertBrowserFaultProbeSelection(undefined, group)).not.toThrow();
  const attachmentSuite = { id: "shared.attachments.attachments-v2.e2e", files: ["tests/e2e/attachments-v2.spec.ts"] };
  const actualReceipt: BrowserCaseIdentity = { suite: attachmentSuite.id, file: "attachments-v2.spec.ts", project: "desktop", title: "canonical Leave evidence supports resilient private files on desktop and mobile" };
  const attachmentGroup = orderedBrowserExecutionGroups([attachmentSuite], ["desktop"], "forward");
  const declaredInventory = { ...actualReceipt, file: attachmentSuite.files[0] };
  expect(() => assertBrowserFaultProbeSelection(actualReceipt, attachmentGroup)).not.toThrow();
  expect(isBrowserFaultProbeCase(attachmentSuite.id, declaredInventory, actualReceipt)).toBe(true);
  expect(isBrowserFaultProbeCase(attachmentSuite.id, actualReceipt, declaredInventory)).toBe(true);
  for (const foreignFile of ["different.spec.ts", "foreign/attachments-v2.spec.ts", "tests/e2e/nested/attachments-v2.spec.ts", "/foreign/attachments-v2.spec.ts"]) {
    expect(() => assertBrowserFaultProbeSelection({ ...actualReceipt, file: foreignFile }, attachmentGroup)).toThrow("EXACT_BROWSER_FAULT_PROBE_SELECTION_REQUIRED");
    expect(isBrowserFaultProbeCase(attachmentSuite.id, { ...actualReceipt, file: foreignFile }, declaredInventory)).toBe(false);
  }
  expect(isBrowserFaultProbeCase("foreign-suite", actualReceipt, declaredInventory)).toBe(false);
  expect(isBrowserFaultProbeCase(attachmentSuite.id, { ...actualReceipt, project: "mobile" }, declaredInventory)).toBe(false);
  expect(isBrowserFaultProbeCase(attachmentSuite.id, { ...actualReceipt, title: actualReceipt.title + " changed" }, declaredInventory)).toBe(false);
});

it("throws the declared harness fault only for its exact case and never for a normal acceptance case", () => {
  const probe: BrowserCaseIdentity = { suite: "people", file: "tests/e2e/people.spec.ts", project: "desktop", title: "first" };
  expect(() => triggerBrowserFaultProbe("people", probe, probe)).toThrow(browserFaultProbeError);
  expect(() => triggerBrowserFaultProbe("finance", probe, probe)).not.toThrow();
  expect(() => triggerBrowserFaultProbe("people", { ...probe, project: "mobile" }, probe)).not.toThrow();
  expect(() => triggerBrowserFaultProbe("people", { ...probe, title: "second" }, probe)).not.toThrow();
  expect(() => triggerBrowserFaultProbe("people", probe, undefined)).not.toThrow();
  const attachment: BrowserCaseIdentity = { suite: "shared.attachments.attachments-v2.e2e", file: "attachments-v2.spec.ts", project: "desktop", title: "canonical Leave evidence supports resilient private files on desktop and mobile" };
  expect(() => triggerBrowserFaultProbe(attachment.suite, { ...attachment, file: "tests/e2e/attachments-v2.spec.ts" }, attachment)).toThrow(browserFaultProbeError);
  for (const different of [{ ...attachment, file: "foreign/attachments-v2.spec.ts" }, { ...attachment, project: "mobile" as const }, { ...attachment, title: attachment.title + " changed" }]) expect(() => triggerBrowserFaultProbe(attachment.suite, different, attachment)).not.toThrow();
  expect(() => triggerBrowserFaultProbe("foreign-suite", attachment, attachment)).not.toThrow();
});

it("rejects late fingerprint interruption while retaining completed cleanup and genuine declared-fault receipts", async () => {
  const mockedModules = ["node:child_process", "node:net", "../../scripts/testing/catalogue", "../../scripts/local/source-fingerprint", "../../scripts/testing/required-case-inventory", "../../scripts/testing/browser-prerequisites", "../../scripts/testing/browser-evidence"];
  const suite: Suite = { id: "controlled.e2e", owner: "shared.test-infrastructure", modules: [], lane: "e2e", fixture: "general-company", files: ["tests/e2e/controlled.spec.ts"], requires: { jobs: false, scanner: false } };
  const identity: BrowserCaseIdentity = { suite: suite.id, file: suite.files[0], title: "retained business case", project: "desktop" };
  const fingerprint = "a".repeat(64);
  for (const boundary of ["none", "runtime", "verification", "probe"] as const) {
    const root = await mkdtemp(join(tmpdir(), "erpx-browser-interruption-"));
    const listenersBefore = new Set(process.listeners("SIGINT"));
    const retained: string[] = [], destroyed: string[] = [];
    let executed = false, liveChildren = 0, blocked = false;
    let reached!: () => void, release!: () => void;
    const atBoundary = new Promise<void>(done => { reached = done; });
    const fingerprintPending = new Promise<void>(done => { release = done; });
    const source = async (kind: "runtime" | "verification") => {
      if (executed && boundary === kind && !blocked) { blocked = true; reached(); await fingerprintPending; }
      return fingerprint;
    };
    try {
      vi.resetModules();
      vi.stubGlobal("Bun", { version: "1.2.17", spawnSync: () => ({ stdout: Buffer.from("v24.20.0\n") }) });
      vi.stubGlobal("fetch", vi.fn(async () => ({ status: 200 })));
      vi.doMock("../../scripts/testing/catalogue", () => ({ repositoryRoot: root }));
      vi.doMock("../../scripts/local/source-fingerprint", () => ({ runtimeSourceFingerprint: () => source("runtime"), verificationSourceFingerprint: () => source("verification") }));
      vi.doMock("../../scripts/testing/required-case-inventory", async () => ({ ...await vi.importActual<typeof import("../../scripts/testing/required-case-inventory")>("../../scripts/testing/required-case-inventory"), loadRequiredCases: () => [{ runner: "playwright", file: identity.file, title: identity.title, project: identity.project }] }));
      vi.doMock("../../scripts/testing/browser-prerequisites", () => ({ prepareBrowserCasePrerequisites: async () => ({}) }));
      vi.doMock("../../scripts/testing/browser-evidence", () => ({ retainBrowserFixtureEvidence: async (_root: string, run: string) => { retained.push(run); } }));
      vi.doMock("node:net", () => ({ createServer: () => {
        const server = { once: () => server, listen: (_port: number, _host: string, done: () => void) => { done(); }, close: (done: () => void) => { done(); } };
        return server;
      } }));
      vi.doMock("node:child_process", () => ({ spawn: (_command: string, args: string[], options: { env: Record<string, string> }) => {
        const child = Object.assign(new EventEmitter(), { kill: vi.fn() });
        liveChildren++;
        setImmediate(async () => {
          try {
            const run = options.env.ERPX_BROWSER_SIMULATION_RUN_ID, directory = join(root, ".runtime/browser", run);
            if (args.includes("prepare")) {
              await mkdir(directory, { recursive: true });
              await writeFile(join(directory, "state.json"), JSON.stringify({ run, origin: "http://127.0.0.1:3102", ready: true, sourceFingerprint: fingerprint, pid: process.pid }));
            } else if (args.includes("destroy")) { await rm(directory, { recursive: true }); destroyed.push(run); }
            else if (args.includes("--reporter=json")) {
              const collection = args.includes("--list");
              await writeFile(options.env.PLAYWRIGHT_JSON_OUTPUT_NAME, JSON.stringify({ suites: [{ title: "controlled.spec.ts", specs: [{ file: identity.file, title: identity.title, tests: [{ projectName: identity.project, expectedStatus: "passed", status: "expected", results: collection ? [] : [{ status: "passed" }] }] }] }] }));
              if (!collection) executed = true;
            }
            liveChildren--; child.emit("exit", 0);
          } catch (error) { liveChildren--; child.emit("error", error); }
        });
        return child;
      } }));
      const runner = await import("../../scripts/testing/browser-runner");
      const result = runner.runBrowserSuites([suite], { projects: ["desktop"], ...(boundary === "probe" ? { faultProbe: identity } : {}) }).then(value => ({ value, error: undefined }), error => ({ value: undefined, error }));
      if (boundary === "runtime" || boundary === "verification") {
        await atBoundary;
        expect(liveChildren).toBe(0);
        const signal = process.listeners("SIGINT").filter(listener => !listenersBefore.has(listener));
        expect(signal).toHaveLength(1);
        signal[0].call(process, "SIGINT");
        release();
      }
      const outcome = await result;
      const evidence = join(root, ".scratch/dev/test-suites/browser", (await readdir(join(root, ".scratch/dev/test-suites/browser")))[0]);
      const receipt = JSON.parse(await readFile(join(evidence, "receipt.json"), "utf8")) as BrowserRunReceipt;
      expect(receipt.cleanupFailures).toEqual([]);
      expect(receipt.receipts).toHaveLength(1);
      expect(receipt.receipts[0].cleanup).toEqual({ completed: true, ownedDirectoryAbsent: true });
      expect(retained).toHaveLength(3);
      expect(new Set(destroyed)).toEqual(new Set(retained));
      expect(retained.every(run => !existsSync(join(root, ".runtime/browser", run)))).toBe(true);
      if (boundary === "none") {
        expect(outcome.error).toBeUndefined(); expect(outcome.value?.cases).toBe(1);
        expect(receipt.passed).toBe(true); expect(receipt.interrupted).toBeNull();
      } else if (boundary === "probe") {
        expect(outcome.error).toBeInstanceOf(runner.BrowserFaultProbeError);
        expect(receipt.passed).toBe(false); expect(receipt.interrupted).toBeNull();
        expect(receipt.receipts[0]).toMatchObject({ passed: false, faultProbe: true, failure: browserFaultProbeError });
      } else {
        expect({ error: outcome.error instanceof Error ? outcome.error.message : null, result: outcome.value?.cases ?? null, passed: receipt.passed, interrupted: receipt.interrupted })
          .toEqual({ error: "BROWSER_RUN_INTERRUPTED:SIGINT", result: null, passed: false, interrupted: "SIGINT" });
      }
    } finally {
      release();
      for (const specifier of mockedModules) vi.doUnmock(specifier);
      vi.unstubAllGlobals(); vi.resetModules();
      await rm(root, { recursive: true, force: true });
    }
  }
});
