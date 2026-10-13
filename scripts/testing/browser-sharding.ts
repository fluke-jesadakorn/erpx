import { createHash } from "node:crypto";
import catalogue from "../../tests/catalogue/suite-manifest.json";
import type { BrowserCase } from "./browser-results";
import { assertRequiredCaseInventory, assertRequiredCaseResults, type CaseObservation, type RequiredCaseIdentity } from "./required-case-inventory";
import { validateBrowserIsolation, type BrowserIsolationProof } from "./browser-isolation-receipts";

export type BrowserShard = { index: number; total: number };
export type BrowserCaseIdentity = { suite: string; file: string; title: string; project: "desktop" | "mobile" };
export type BrowserInventoryCase = BrowserCaseIdentity & { required: boolean; reason?: string };
export type BrowserInventoryEntry = BrowserInventoryCase;
export type BrowserCaseReceipt = BrowserCaseIdentity & { passed: boolean };
export type BrowserCaseExclusion = BrowserCaseIdentity & { required: false; reason: string };
export type CandidateReleaseIdentity = { id: string; fileManifestSha256: string };
export type NativeCiReceipt = { version: 1; kind: "native"; candidate: string; runtimeFingerprint: string; verificationFingerprint: string; release?: CandidateReleaseIdentity; passed: boolean; phases: { phase: string; passed: boolean }[]; cases: CaseObservation[] };
export type BrowserCiReceipt = { version: 1; kind: "browser"; candidate: string; runtimeFingerprint: string; verificationFingerprint: string; release?: CandidateReleaseIdentity; passed: boolean; shard: BrowserShard; inventory: BrowserInventoryCase[]; receipts: BrowserCaseReceipt[]; exclusions: BrowserCaseExclusion[]; isolation: BrowserIsolationProof | null };
export const requiredNativePhases = ["architecture", "unit", "integration", "lint", "typecheck", "isolated-release"] as const;

export function parseBrowserShard(value: string): BrowserShard {
  const match = /^(\d+)\/(\d+)$/.exec(value);
  if (!match) throw new Error("INVALID_BROWSER_SHARD");
  const shard = { index: Number(match[1]), total: Number(match[2]) };
  validateBrowserShard(shard);
  return shard;
}
function validateBrowserShard(shard: BrowserShard): void {
  if (!Number.isInteger(shard.index) || !Number.isInteger(shard.total) || shard.total < 1 || shard.total > 256 || shard.index < 1 || shard.index > shard.total) throw new Error("INVALID_BROWSER_SHARD");
}
export function browserCaseKey(test: BrowserCaseIdentity): string {
  const file = test.file.replaceAll("\\", "/");
  const registered = catalogue.suites.flatMap(suite => suite.files).filter(candidate => candidate === file || candidate.endsWith(`/${file}`));
  const index = file.indexOf("tests/");
  const relativeFile = registered.length === 1 ? registered[0]
    : index >= 0 ? file.slice(index) : !file.includes("/") ? `tests/e2e/${file}` : file;
  return JSON.stringify([test.suite, relativeFile, test.project, test.title]);
}
export function assignedBrowserShard(suiteId: string, wanted: Pick<BrowserCase, "file" | "title" | "project">, shard: BrowserShard): boolean {
  validateBrowserShard(shard);
  const digest = createHash("sha256").update(browserCaseKey({ suite: suiteId, ...wanted })).digest();
  return digest.readUInt32BE(0) % shard.total + 1 === shard.index;
}

/** Aggregation is acceptance only when every shard proves the same complete candidate inventory. */
export function aggregateCiRegression(input: { native: NativeCiReceipt; browsers: BrowserCiReceipt[]; candidate: string; total: number; requiredSuiteIds: string[]; requiredCases: RequiredCaseIdentity[] }) {
  const { native, browsers, candidate, total, requiredSuiteIds, requiredCases } = input;
  validateBrowserShard({ index: 1, total });
  if (!/^[a-f0-9]{40,64}$/.test(candidate) || native.version !== 1 || native.kind !== "native" || native.candidate !== candidate || !native.passed) throw new Error("NATIVE_CANDIDATE_REGRESSION_REQUIRED");
  if (!/^[a-f0-9]{64}$/.test(native.runtimeFingerprint) || !/^[a-f0-9]{64}$/.test(native.verificationFingerprint)) throw new Error("CANDIDATE_FINGERPRINT_REQUIRED");
  if (!native.release?.id || !/^[a-f0-9]{64}$/.test(native.release.fileManifestSha256)) throw new Error("NATIVE_CANDIDATE_RELEASE_REQUIRED");
  if (native.phases.length !== requiredNativePhases.length || requiredNativePhases.some(phase => native.phases.filter(result => result.phase === phase && result.passed).length !== 1)) throw new Error("NATIVE_REGRESSION_PHASE_MISSING");
  if (!Array.isArray(native.cases) || !native.cases.length || native.cases.some(test => test.runner !== "vitest" || test.project !== undefined || typeof test.file !== "string" || typeof test.title !== "string" || !test.title || test.status !== "passed")) throw new Error("NATIVE_CASE_INVENTORY_REQUIRED");
  assertRequiredCaseResults(requiredCases, native.cases, { runner: "vitest", files: requiredCases.filter(test => test.runner === "vitest").map(test => test.file) });
  if (browsers.length !== total || new Set(browsers.map(receipt => receipt.shard.index)).size !== total) throw new Error("BROWSER_SHARDS_INCOMPLETE");
  let inventorySignature: string | undefined;
  const required = new Set<string>(), declaredExclusions = new Map<string, string>(), passed = new Set<string>(), excluded = new Set<string>();
  const suiteProjects = new Set<string>();
  for (const receipt of browsers) {
    validateBrowserShard(receipt.shard);
    if (receipt.version !== 1 || receipt.kind !== "browser" || receipt.candidate !== candidate || !receipt.passed || receipt.shard.total !== total || receipt.runtimeFingerprint !== native.runtimeFingerprint || receipt.verificationFingerprint !== native.verificationFingerprint || receipt.release?.id !== native.release.id || receipt.release?.fileManifestSha256 !== native.release.fileManifestSha256) throw new Error("BROWSER_SHARD_CANDIDATE_MISMATCH");
    if (!Array.isArray(receipt.inventory) || !receipt.inventory.length || !Array.isArray(receipt.receipts) || !Array.isArray(receipt.exclusions)) throw new Error("BROWSER_SHARD_INVENTORY_REQUIRED");
    assertRequiredCaseInventory(requiredCases, receipt.inventory.map(test => ({ ...test, runner: "playwright" })), { runner: "playwright", files: requiredCases.filter(test => test.runner === "playwright").map(test => test.file), projects: ["desktop", "mobile"] });
    const entries = receipt.inventory.map(test => {
      if (!test.suite || !test.file || !test.title || !["desktop", "mobile"].includes(test.project) || !requiredSuiteIds.includes(test.suite) || typeof test.required !== "boolean" || (!test.required && !test.reason?.trim())) throw new Error("INVALID_BROWSER_INVENTORY");
      const key = browserCaseKey(test);
      suiteProjects.add(JSON.stringify([test.suite, test.project]));
      if (test.required) required.add(key); else declaredExclusions.set(key, test.reason!);
      return [key, test.required, test.reason ?? null] as const;
    }).sort((a, b) => a[0].localeCompare(b[0]));
    if (new Set(entries.map(entry => entry[0])).size !== entries.length) throw new Error("DUPLICATE_BROWSER_INVENTORY_CASE");
    const signature = JSON.stringify(entries);
    if (inventorySignature !== undefined && signature !== inventorySignature) throw new Error("BROWSER_SHARD_INVENTORY_MISMATCH");
    inventorySignature = signature;
    for (const test of receipt.receipts) {
      const key = browserCaseKey(test);
      if (!required.has(key) || !test.passed || !assignedBrowserShard(test.suite, test, receipt.shard)) throw new Error("UNEXPECTED_OR_FAILED_BROWSER_CASE");
      if (passed.has(key)) throw new Error("DUPLICATE_BROWSER_CASE_RECEIPT");
      passed.add(key);
    }
    for (const test of receipt.exclusions) {
      const key = browserCaseKey(test);
      if (test.required !== false || test.reason !== declaredExclusions.get(key) || !test.reason?.trim() || !assignedBrowserShard(test.suite, test, receipt.shard)) throw new Error("UNDECLARED_BROWSER_CASE_EXCLUSION");
      if (excluded.has(key)) throw new Error("DUPLICATE_BROWSER_CASE_EXCLUSION");
      excluded.add(key);
    }
  }
  if (requiredSuiteIds.some(suite => ["desktop", "mobile"].some(project => !suiteProjects.has(JSON.stringify([suite, project]))))) throw new Error("BROWSER_SUITE_PROJECT_MISSING");
  if (required.size !== passed.size || declaredExclusions.size !== excluded.size) throw new Error("FULL_BROWSER_CASE_COVERAGE_REQUIRED");
  let rerunCases = 0;
  for (const receipt of browsers) {
    if (!receipt.isolation) throw new Error("ISOLATION_PROOF_REQUIRED");
    const isolated = validateBrowserIsolation(receipt.isolation, { runtimeFingerprint: native.runtimeFingerprint, verificationFingerprint: native.verificationFingerprint, shard: receipt.shard });
    const inventoryKeys = (entries: BrowserInventoryCase[]) => entries.map(test => JSON.stringify([browserCaseKey(test), test.required, test.reason ?? null])).sort();
    const passKeys = (entries: BrowserCaseReceipt[]) => entries.map(test => JSON.stringify([browserCaseKey(test), test.passed])).sort();
    if (JSON.stringify(inventoryKeys(receipt.inventory)) !== JSON.stringify(inventoryKeys(receipt.isolation.forward.inventory)) ||
      JSON.stringify(passKeys(receipt.receipts)) !== JSON.stringify(passKeys(receipt.isolation.forward.receipts.filter(test => test.required !== false) as BrowserCaseReceipt[]))) throw new Error("CI_ISOLATION_FORWARD_RECEIPT_MISMATCH");
    if (isolated.cases !== receipt.receipts.length) throw new Error("CI_ISOLATION_FORWARD_RECEIPT_MISMATCH");
    rerunCases += isolated.rerunCases;
  }
  return { version: 1, scope: "internal-regression", passed: true, candidate, runtimeFingerprint: native.runtimeFingerprint, verificationFingerprint: native.verificationFingerprint, release: native.release, shards: total, cases: passed.size, rerunCases, faultProbes: browsers.length, caseOrders: ["forward", "reverse"], explicitProjectExclusions: excluded.size };
}
