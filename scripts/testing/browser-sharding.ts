import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
import catalogue from "../../tests/catalogue/suite-manifest.json";
import type { BrowserCase } from "./browser-results";
import { assertRequiredCaseInventory, assertRequiredCaseResults, type CaseObservation, type RequiredCaseIdentity } from "./required-case-inventory";
import { validateBrowserAcceptanceCohort, validateBrowserIsolation, type BrowserIsolationProof } from "./browser-isolation-receipts";
import { mergeBrowserAcceptanceReceipts } from "./browser-isolation-groups";
import type { BrowserRunReceipt } from "./browser-runner";
import { formsHostBrowserFile } from "./forms-host-cohort";

export type BrowserShard = { index: number; total: number };
export type BrowserCaseIdentity = { suite: string; file: string; title: string; project: "desktop" | "mobile" };
export type BrowserInventoryCase = BrowserCaseIdentity & { required: boolean; reason?: string };
export type BrowserInventoryEntry = BrowserInventoryCase;
export type BrowserCaseReceipt = BrowserCaseIdentity & { passed: boolean };
export type BrowserCaseExclusion = BrowserCaseIdentity & { required: false; reason: string };
export type CandidateReleaseIdentity = { id: string; fileManifestSha256: string };
export type NativeCiReceipt = { version: 1; kind: "native"; candidate: string; runtimeFingerprint: string; verificationFingerprint: string; release?: CandidateReleaseIdentity; passed: boolean; phases: { phase: string; passed: boolean }[]; cases: CaseObservation[] };
export type BrowserCiCohortReceipt = { group: "standard" | "executive-work" | "product-host" | "forms-host"; suiteIds: string[]; passed: boolean; forward?: BrowserRunReceipt; reverse?: BrowserRunReceipt; probe?: BrowserRunReceipt; forwardEvidenceDirectory?: string; reverseEvidenceDirectory?: string; probeEvidenceDirectory?: string; reason?: string };
export type BrowserCiReceipt = { version: 1; kind: "browser"; candidate: string; runtimeFingerprint: string; verificationFingerprint: string; release?: CandidateReleaseIdentity; passed: boolean; shard: BrowserShard; inventory: BrowserInventoryCase[]; receipts: BrowserCaseReceipt[]; exclusions: BrowserCaseExclusion[]; isolation: BrowserIsolationProof | null; cohorts: BrowserCiCohortReceipt[]; blocker?: { phase: string; reason: string } };
export const requiredNativePhases = ["architecture", "unit", "integration", "lint", "typecheck", "isolated-release"] as const;

const executiveWorkBrowserFile = "tests/e2e/shared.executive/executive-work-mcp.spec.ts";
const productHostBrowserFile = "tests/e2e/identity_access/product-hosts.spec.ts";
type BrowserCiCohortPlan = { group: BrowserCiCohortReceipt["group"]; suites: (typeof catalogue.suites)[number][] };

function requiredBrowserIsolationGroups(requiredSuiteIds: readonly string[]): BrowserCiCohortPlan[] {
  if (new Set(requiredSuiteIds).size !== requiredSuiteIds.length) throw new Error("BROWSER_REQUIRED_SUITE_SET_INVALID");
  const byId = new Map(catalogue.suites.map(suite => [suite.id, suite]));
  const suites = requiredSuiteIds.map(id => byId.get(id));
  if (suites.some(suite => !suite || !["e2e", "journeys", "uat"].includes(suite.lane))) throw new Error("BROWSER_REQUIRED_SUITE_UNKNOWN");
  const selected = suites as NonNullable<(typeof suites)[number]>[];
  const executive = selected.filter(suite => suite.files.includes(executiveWorkBrowserFile));
  const productHost = selected.filter(suite => suite.files.includes(productHostBrowserFile));
  const formsHost = selected.filter(suite => suite.files.includes(formsHostBrowserFile));
  if (executive.length && (executive.length !== 1 || executive[0].files.length !== 1 || executive[0].files[0] !== executiveWorkBrowserFile || executive[0].fixture !== "executive-work")) throw new Error("BROWSER_ISOLATION_GROUP_LAYOUT_REQUIRED");
  if (productHost.length && (productHost.length !== 1 || productHost[0].files.length !== 1 || productHost[0].files[0] !== productHostBrowserFile)) throw new Error("BROWSER_ISOLATION_GROUP_LAYOUT_REQUIRED");
  if (formsHost.length && (formsHost.length !== 1 || formsHost[0].files.length !== 1 || formsHost[0].files[0] !== formsHostBrowserFile || formsHost[0].fixture !== "engineering")) throw new Error("BROWSER_ISOLATION_GROUP_LAYOUT_REQUIRED");
  const special = new Set([...executive, ...productHost, ...formsHost].map(suite => suite.id));
  const standard = selected.filter(suite => !special.has(suite.id));
  return [
    ...(standard.length ? [{ group: "standard" as const, suites: standard }] : []),
    ...(executive.length ? [{ group: "executive-work" as const, suites: executive }] : []),
    ...(productHost.length ? [{ group: "product-host" as const, suites: productHost }] : []),
    ...(formsHost.length ? [{ group: "forms-host" as const, suites: formsHost }] : []),
  ];
}

function assertBrowserCohorts(receipt: BrowserCiReceipt, expected: readonly BrowserCiCohortPlan[]) {
  const cohorts = receipt.cohorts;
  const probeCohorts = Array.isArray(cohorts) ? cohorts.filter(cohort => cohort.probe !== undefined) : [];
  const probeEvidenceCohorts = Array.isArray(cohorts) ? cohorts.filter(cohort => typeof cohort.probeEvidenceDirectory === "string" && cohort.probeEvidenceDirectory.trim()) : [];
  if (receipt.blocker || !Array.isArray(cohorts) || cohorts.length !== expected.length || cohorts.some((cohort, index) =>
    !cohort || cohort.group !== expected[index].group || cohort.passed !== true ||
    JSON.stringify(cohort.suiteIds) !== JSON.stringify(expected[index].suites.map(suite => suite.id)) ||
    typeof cohort.forwardEvidenceDirectory !== "string" || !cohort.forwardEvidenceDirectory.trim() ||
    typeof cohort.reverseEvidenceDirectory !== "string" || !cohort.reverseEvidenceDirectory.trim() ||
    !cohort.forward || !cohort.reverse) ||
    probeCohorts.length !== 1 || probeEvidenceCohorts.length !== 1 || probeCohorts[0] !== probeEvidenceCohorts[0]) {
    throw new Error("BROWSER_COHORT_PREREQUISITE_REQUIRED");
  }
  const evidenceEntries=cohorts.flatMap(cohort=>[
    {path:cohort.forwardEvidenceDirectory!,receipt:cohort.forward!},
    {path:cohort.reverseEvidenceDirectory!,receipt:cohort.reverse!},
    ...(cohort.probe?[{path:cohort.probeEvidenceDirectory!,receipt:cohort.probe}]:[]),
  ]);
  const evidencePathPattern=/^\.scratch\/dev\/test-suites\/browser\/\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}-\d{3}Z$/;
  if(evidenceEntries.some(entry=>!evidencePathPattern.test(entry.path)||entry.receipt.evidencePath!==entry.path)||
    new Set(evidenceEntries.map(entry=>entry.path)).size!==evidenceEntries.length)throw new Error("BROWSER_COHORT_EVIDENCE_BINDING_REQUIRED");
  return cohorts;
}

export function assertBrowserProbeCohortBinding(
  cohorts:readonly Pick<BrowserCiCohortReceipt,"group"|"suiteIds"|"probe">[],
  expected:readonly {group:BrowserCiCohortReceipt["group"];suiteIds:readonly string[]}[],
) {
  const owners=cohorts.filter(cohort=>cohort.probe!==undefined);
  const probe=owners[0]?.probe, suite=probe?.execution?.faultProbe?.suite;
  const planned=expected.find(group=>group.suiteIds.includes(suite??""));
  if(owners.length!==1||!suite||!planned||owners[0].group!==planned.group||!owners[0].suiteIds.includes(suite))
    throw new Error("CI_ISOLATION_PROBE_COHORT_MISMATCH");
  return owners[0];
}

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
  const requiredGroups = requiredBrowserIsolationGroups(requiredSuiteIds);
  const suiteById = new Map(catalogue.suites.map(suite => [suite.id, suite]));
  const registeredBrowserSuites = requiredSuiteIds.map(id => suiteById.get(id)).filter((suite): suite is (typeof catalogue.suites)[number] => Boolean(suite));
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
    assertBrowserCohorts(receipt, requiredGroups);
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
    const cohorts = receipt.cohorts;
    for (let index = 0; index < requiredGroups.length; index++) {
      const plan = requiredGroups[index], cohort = cohorts[index];
      validateBrowserAcceptanceCohort(cohort.forward!, { suites: plan.suites, runtimeFingerprint: native.runtimeFingerprint, verificationFingerprint: native.verificationFingerprint, shard: receipt.shard, caseOrder: "forward" });
      validateBrowserAcceptanceCohort(cohort.reverse!, { suites: plan.suites, runtimeFingerprint: native.runtimeFingerprint, verificationFingerprint: native.verificationFingerprint, shard: receipt.shard, caseOrder: "reverse" });
    }
    const probeCohort=assertBrowserProbeCohortBinding(cohorts,requiredGroups.map(({group,suites})=>({group,suiteIds:suites.map(suite=>suite.id)})));
    const probe = probeCohort.probe!;
    const probeSuite = probe.execution?.faultProbe?.suite;
    const probePlan = requiredGroups.find(plan => plan.suites.some(suite => suite.id === probeSuite));
    if (!probeSuite || !probePlan || JSON.stringify(probe.suites) !== JSON.stringify([probeSuite]) ||
        probe.inventory.some(test => test.suite !== probeSuite || test.project !== probe.execution.faultProbe?.project) ||
        !isDeepStrictEqual(probe, receipt.isolation.probe)) throw new Error("CI_ISOLATION_PROBE_COHORT_MISMATCH");
    const reconstructedForward = mergeBrowserAcceptanceReceipts(registeredBrowserSuites, requiredGroups.map((plan, index) => ({ suites: plan.suites, receipt: cohorts[index].forward! })), "forward", receipt.shard);
    const reconstructedReverse = mergeBrowserAcceptanceReceipts(registeredBrowserSuites, requiredGroups.map((plan, index) => ({ suites: plan.suites, receipt: cohorts[index].reverse! })), "reverse", receipt.shard);
    if (!isDeepStrictEqual(reconstructedForward, receipt.isolation.forward) || !isDeepStrictEqual(reconstructedReverse, receipt.isolation.reverse)) {
      throw new Error("CI_ISOLATION_GROUP_RECEIPT_MISMATCH");
    }
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
