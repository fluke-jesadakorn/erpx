import { createHash, randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstat, mkdir, readFile, realpath, rename, unlink, writeFile } from "node:fs/promises";
import { basename, dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { browserCases, assertBrowserCasePassed } from "../testing/browser-results";
import { requiredNativePhases, type NativeCiReceipt } from "../testing/browser-sharding";
import { loadCatalogue } from "../testing/catalogue";
import { verifyEngineeringCandidateInventory } from "../testing/engineering/engineering-candidate-inventory";
import { fullRegressionRunIdFromPath, validateFullRegressionBrowserIsolationArtifacts, validateFullRegressionCases, validateFullRegressionRollbackArtifacts } from "../testing/full-regression-receipt";
import { canonicalCaseFile, internalRequiredCases, loadRequiredCases } from "../testing/required-case-inventory";
import { verifyUiBrowserEvidence, type UiBrowserEvidenceWitness } from "../testing/ui-evidence-witness";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { productHostBrowserFile, productHostBusinessWorkflowForCase, productHostReadOnlyWorkflowForCase } from "../testing/product-host-cohort";
import catalogue from "../../tests/catalogue/suite-manifest.json";
import engineeringScenarios from "../../tests/catalogue/engineering-scenarios.json";
import productWorkflowReceipts from "../../tests/catalogue/product-workflow-receipts.json";

export const productMigrationReceiptSchema = "erpx-product-migration-receipt/v2" as const;
export const productMigrationReceiptManifestSchema = "erpx-product-migration-receipts/v2" as const;
export const productMigrationImportSchema = "erpx-product-migration-import/v2" as const;

export type ProductMigrationPhase = "implemented" | "verified" | "enabled";
export type ProductMigrationCase = {
  runner: "vitest" | "playwright";
  suite: string;
  file: string;
  title: string;
  project?: "desktop" | "mobile";
};
export type ProductMigrationEvidenceInput = { kind: string; path: string; sha256: string };
export type ProductMigrationEvidence = {
  schema: typeof productMigrationReceiptSchema;
  result: "passed";
  id: string;
  phase: Exclude<ProductMigrationPhase, "enabled">;
  candidateSha: string;
  runtimeFingerprint: string;
  verificationFingerprint: string;
  release: { id: string; fileManifestSha256: string };
  observations: (ProductMigrationCase & { status: "passed" })[];
  evidenceInputs: ProductMigrationEvidenceInput[];
  cleanup: {
    native: { passed: true; path: string; sha256: string };
    browserRuns: { run: string; passed: true; cleanupFailures: []; path: string; sha256: string; cases: ProductMigrationCase[] }[];
  };
};
export type ProductMigrationReceiptReference = {
  id: string;
  phase: ProductMigrationPhase;
  candidateSha: string;
  runtimeFingerprint: string;
  verificationFingerprint: string;
  release: { id: string; fileManifestSha256: string };
  path: string;
  sha256: string;
};
export type ProductMigrationReceiptManifest = {
  schema: typeof productMigrationReceiptManifestSchema;
  records: ProductMigrationReceiptReference[];
};

type EngineeringScenarioCase = { runner: "vitest" | "playwright"; file: string; title: string; project?: "desktop" | "mobile"; proof: string };
type EngineeringScenario = { id: string; review: string; cases: EngineeringScenarioCase[] };
const scenarios = new Map((engineeringScenarios.capabilities as EngineeringScenario[]).map(scenario => [scenario.id, scenario]));
type ProductWorkflowScenario = {
  id: string;
  cases: EngineeringScenarioCase[];
  blockers?: { implemented?: string; verified?: string };
};
const productWorkflows = productWorkflowReceipts.workflows as ProductWorkflowScenario[];
const productWorkflowScenarios = new Map(productWorkflows.map(workflow => [workflow.id, workflow]));

export type ProductWorkflowReceiptStatus = {
  native: { state: "mapped" } | { state: "unmapped"; blocker: string };
  browser: { state: "mapped" } | { state: "pending"; blocker: string };
};

export function productWorkflowReceiptIds(): string[] {
  if (productWorkflowReceipts.version !== 1 || !Array.isArray(productWorkflowReceipts.workflows) ||
    productWorkflowScenarios.size !== productWorkflows.length || productWorkflows.some(workflow =>
      typeof workflow.id !== "string" || !workflow.id.trim() || !Array.isArray(workflow.cases)))
    throw new Error("INVALID_PRODUCT_WORKFLOW_RECEIPT_REGISTER");
  return productWorkflows.map(workflow => workflow.id);
}

/** Explain exactly which receipt phases have reviewed cases and which still lack proof. */
export function productWorkflowReceiptMapping(id: string): ProductWorkflowReceiptStatus {
  productWorkflowReceiptIds();
  const workflow = productWorkflowScenarios.get(id);
  if (!workflow) throw new Error("UNSUPPORTED_PRODUCT_MIGRATION_WORKFLOW");
  const native = workflow.cases.some(test => test.runner === "vitest");
  const browser = workflow.cases.some(test => test.runner === "playwright");
  return {
    native: native ? { state: "mapped" } : {
      state: "unmapped",
      blocker: workflow.blockers?.implemented ?? "No exact native workflow case is registered.",
    },
    browser: browser ? { state: "mapped" } : {
      state: "pending",
      blocker: workflow.blockers?.verified ?? "No exact browser workflow case is registered.",
    },
  };
}

export function productWorkflowReceiptCases(id?: string): ProductMigrationCase[] {
  productWorkflowReceiptIds();
  const required = loadRequiredCases();
  const result: ProductMigrationCase[] = [];
  const selected = id === undefined ? productWorkflows : [productWorkflowScenarios.get(id)].filter((workflow): workflow is ProductWorkflowScenario => Boolean(workflow));
  if (id !== undefined && selected.length !== 1) throw new Error("UNSUPPORTED_PRODUCT_MIGRATION_WORKFLOW");
  for (const workflow of selected) {
    const seen = new Set<string>();
    for (const test of workflow.cases) {
      const allowedLanes = new Set<string>(test.runner === "vitest" ? ["unit", "integration"] : test.runner === "playwright" ? ["e2e", "journeys", "uat"] : []);
      if (!test.proof?.trim() || !test.title?.trim() || !test.file?.startsWith("tests/") || !allowedLanes.size ||
        (test.runner === "playwright" ? !["desktop", "mobile"].includes(test.project ?? "") : test.project !== undefined))
        throw new Error(`INVALID_PRODUCT_WORKFLOW_RECEIPT_CASE:${workflow.id}`);
      const owners = catalogue.suites.filter(suite => allowedLanes.has(suite.lane) && suite.files.includes(test.file));
      if (owners.length !== 1) throw new Error(`PRODUCT_WORKFLOW_RECEIPT_CASE_SUITE_BINDING_REQUIRED:${workflow.id}:${test.file}`);
      const identity = { runner: test.runner, file: test.file, title: test.title, ...(test.project ? { project: test.project } : {}) };
      if (!required.some(candidate => JSON.stringify([candidate.runner, candidate.file, candidate.title, candidate.project ?? ""]) ===
        JSON.stringify([identity.runner, identity.file, identity.title, identity.project ?? ""])))
        throw new Error(`PRODUCT_WORKFLOW_RECEIPT_CASE_NOT_REQUIRED:${workflow.id}:${test.file}:${test.title}`);
      const resolved = { ...identity, suite: owners[0].id } satisfies ProductMigrationCase;
      const key = caseKey(resolved);
      if (seen.has(key)) throw new Error(`DUPLICATE_PRODUCT_WORKFLOW_RECEIPT_CASE:${workflow.id}:${test.file}:${test.title}`);
      seen.add(key);
      result.push(resolved);
    }
    const native = workflow.cases.some(test => test.runner === "vitest");
    const browser = workflow.cases.some(test => test.runner === "playwright");
    if ((!native && !workflow.blockers?.implemented?.trim()) || (!browser && !workflow.blockers?.verified?.trim()))
      throw new Error(`PRODUCT_WORKFLOW_RECEIPT_BLOCKER_REQUIRED:${workflow.id}`);
  }
  return result;
}

function resolveScenarioCases(source: EngineeringScenarioCase[], requireNative = true) {
  const cases = source.flatMap(test => {
    const allowedLanes = new Set<string>(test.runner === "vitest" ? ["unit", "integration"] : test.runner === "playwright" ? ["e2e", "journeys", "uat"] : []);
    if (!test.proof?.trim() || !test.title?.trim() || !test.file?.startsWith("tests/") || !allowedLanes.size ||
      (test.runner === "playwright" ? !["desktop", "mobile"].includes(test.project ?? "") : test.project !== undefined))
      throw new Error("INVALID_PRODUCT_MIGRATION_SCENARIO_CASE");
    const owners = catalogue.suites.filter(suite => allowedLanes.has(suite.lane) && suite.files.includes(test.file));
    // Provider checks remain a separate opt-in boundary and never enter the
    // internal product migration receipt.
    if (!owners.length && test.runner === "vitest" && catalogue.suites.some(suite => suite.lane === "provider" && suite.files.includes(test.file)))
      return [];
    if (owners.length !== 1) throw new Error("PRODUCT_MIGRATION_CASE_SUITE_BINDING_REQUIRED");
    return [{
      runner: test.runner,
      suite: owners[0].id,
      file: test.file,
      title: test.title,
      ...(test.project ? { project: test.project } : {}),
    } satisfies ProductMigrationCase];
  });
  const native = cases.filter(test => test.runner === "vitest");
  const browser = cases.filter(test => test.runner === "playwright");
  if ((requireNative && !native.length) || new Set(cases.map(caseKey)).size !== cases.length) throw new Error("PRODUCT_MIGRATION_NATIVE_SCENARIO_REQUIRED");
  return { native, browser };
}

function reviewedScenarioCases(id: string) {
  const scenario = scenarios.get(id);
  if (!scenario || scenario.review !== "reviewed" || !Array.isArray(scenario.cases) || !scenario.cases.length)
    throw new Error("UNSUPPORTED_PRODUCT_MIGRATION_WORKFLOW");
  return resolveScenarioCases(scenario.cases);
}

function reviewedProductWorkflowCases(id: string) {
  const workflow = productWorkflowScenarios.get(id);
  if (!workflow) throw new Error("UNSUPPORTED_PRODUCT_MIGRATION_WORKFLOW");
  if (!workflow.cases.length) {
    const blocker = workflow.blockers?.implemented?.trim();
    if (!blocker) throw new Error("PRODUCT_MIGRATION_UNMAPPED_WORKFLOW_BLOCKER_REQUIRED");
    throw new Error(`PRODUCT_MIGRATION_WORKFLOW_UNMAPPED:${blocker}`);
  }
  const mapped = resolveScenarioCases(workflow.cases, false);
  if (!mapped.native.length) {
    const blocker = workflow.blockers?.implemented?.trim();
    if (!blocker) throw new Error("PRODUCT_MIGRATION_UNMAPPED_WORKFLOW_BLOCKER_REQUIRED");
    throw new Error(`PRODUCT_MIGRATION_WORKFLOW_UNMAPPED:${blocker}`);
  }
  return mapped;
}

function requiredProductWorkflowCases(id: string, phase: ProductMigrationPhase): ProductMigrationCase[] {
  const mapped = productWorkflowScenarios.has(id) ? reviewedProductWorkflowCases(id) : reviewedScenarioCases(id);
  if (phase === "implemented") return mapped.native;
  if (phase !== "verified") throw new Error("INVALID_PRODUCT_MIGRATION_PHASE");
  const browser = mapped.browser;
  if (!browser.length) {
    const blocker = productWorkflowScenarios.get(id)?.blockers?.verified?.trim();
    if (blocker) throw new Error(`PRODUCT_MIGRATION_BROWSER_WITNESS_REQUIRED:${blocker}`);
    throw new Error("PRODUCT_MIGRATION_BROWSER_WITNESS_REQUIRED");
  }
  return [...mapped.native, ...browser];
}

export function requiredProductMigrationCases(id: string, phase: ProductMigrationPhase): ProductMigrationCase[] {
  if (phase === "enabled") throw new Error("REAL_COMPANY_GRANT_AND_MFA_EVIDENCE_REQUIRED");
  if (phase !== "implemented" && phase !== "verified") throw new Error("INVALID_PRODUCT_MIGRATION_PHASE");
  return requiredProductWorkflowCases(id, phase);
}

const record = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const digest = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const candidateDigest = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{40,64}$/.test(value);
const caseKey = (item: ProductMigrationCase) => JSON.stringify([item.runner, item.suite, item.file, item.title, item.project ?? ""]);
const referenceKey = (item: ProductMigrationEvidenceInput) => `${item.path}:${item.sha256}`;

function assertCaseCoverage(observations: unknown, expected: ProductMigrationCase[]) {
  if (!Array.isArray(observations)) throw new Error("PRODUCT_MIGRATION_CASE_OBSERVATIONS_REQUIRED");
  const normalized = observations.map(value => {
    if (!record(value) || value.status !== "passed" || typeof value.runner !== "string" || typeof value.suite !== "string" ||
      typeof value.file !== "string" || typeof value.title !== "string" ||
      (value.project !== undefined && value.project !== "desktop" && value.project !== "mobile"))
      throw new Error("INVALID_PRODUCT_MIGRATION_CASE_OBSERVATION");
    return { runner: value.runner, suite: value.suite, file: value.file, title: value.title, ...(value.project ? { project: value.project } : {}), status: "passed" as const } as ProductMigrationCase & { status: "passed" };
  });
  if (normalized.length !== expected.length || new Set(normalized.map(caseKey)).size !== normalized.length ||
    JSON.stringify(normalized.map(caseKey).sort()) !== JSON.stringify(expected.map(caseKey).sort()))
    throw new Error("PRODUCT_MIGRATION_EXACT_CASE_COVERAGE_REQUIRED");
  return normalized;
}

/** Validate the durable normalized summary against the reviewed workflow cases. */
export function validateProductMigrationEvidence(value: unknown): ProductMigrationEvidence {
  if (!record(value) || value.schema !== productMigrationReceiptSchema || value.result !== "passed" || typeof value.id !== "string" ||
    (value.phase !== "implemented" && value.phase !== "verified")) throw new Error("INVALID_PRODUCT_MIGRATION_RECEIPT_V2");
  if (!candidateDigest(value.candidateSha) || !digest(value.runtimeFingerprint) || !digest(value.verificationFingerprint) ||
    !record(value.release) || typeof value.release.id !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,100}$/.test(value.release.id) || !digest(value.release.fileManifestSha256))
    throw new Error("PRODUCT_MIGRATION_CANDIDATE_BINDING_REQUIRED");
  const phase = value.phase as Exclude<ProductMigrationPhase, "enabled">;
  const observations = assertCaseCoverage(value.observations, requiredProductMigrationCases(value.id, phase));
  if (!Array.isArray(value.evidenceInputs) || !value.evidenceInputs.length) throw new Error("PRODUCT_MIGRATION_EVIDENCE_INPUTS_REQUIRED");
  const evidenceInputs = value.evidenceInputs.map(item => {
    if (!record(item) || typeof item.kind !== "string" || !/^[a-z][a-z0-9-]{1,40}$/.test(item.kind) ||
      typeof item.path !== "string" || !safeRepositoryPath(item.path) || !digest(item.sha256))
      throw new Error("INVALID_PRODUCT_MIGRATION_EVIDENCE_INPUT");
    return { kind: item.kind, path: item.path, sha256: item.sha256 };
  });
  if (new Set(evidenceInputs.map(referenceKey)).size !== evidenceInputs.length) throw new Error("DUPLICATE_PRODUCT_MIGRATION_EVIDENCE_INPUT");
  if (!record(value.cleanup) || !record(value.cleanup.native) || value.cleanup.native.passed !== true ||
    typeof value.cleanup.native.path !== "string" || !safeRepositoryPath(value.cleanup.native.path) || !digest(value.cleanup.native.sha256) ||
    !Array.isArray(value.cleanup.browserRuns)) throw new Error("PRODUCT_MIGRATION_CLEANUP_EVIDENCE_REQUIRED");
  const cleanup = value.cleanup as Record<string, unknown>;
  const nativeCleanup = cleanup.native as Record<string, unknown>;
  if (!evidenceInputs.some(item => item.path === nativeCleanup.path && item.sha256 === nativeCleanup.sha256))
    throw new Error("PRODUCT_MIGRATION_NATIVE_INPUT_DIGEST_UNBOUND");
  const browserRuns = (cleanup.browserRuns as unknown[]).map(item => {
    if (!record(item) || typeof item.run !== "string" || !/^[a-f0-9]{12}$/.test(item.run) || item.passed !== true ||
      !Array.isArray(item.cleanupFailures) || item.cleanupFailures.length !== 0 || typeof item.path !== "string" || !safeRepositoryPath(item.path) ||
      !digest(item.sha256) || !Array.isArray(item.cases))
      throw new Error("PRODUCT_MIGRATION_BROWSER_CLEANUP_REQUIRED");
    if (!evidenceInputs.some(input => input.path === item.path && input.sha256 === item.sha256)) throw new Error("PRODUCT_MIGRATION_BROWSER_INPUT_DIGEST_UNBOUND");
    const cases: ProductMigrationCase[] = item.cases.map(identity => {
      if (!record(identity) || identity.runner !== "playwright" || typeof identity.suite !== "string" || typeof identity.file !== "string" ||
        typeof identity.title !== "string" || (identity.project !== "desktop" && identity.project !== "mobile"))
        throw new Error("INVALID_PRODUCT_MIGRATION_BROWSER_CLEANUP_CASE");
      return { runner: "playwright" as const, suite: identity.suite, file: identity.file, title: identity.title, project: identity.project as "desktop" | "mobile" };
    });
    return { run: item.run, passed: true as const, cleanupFailures: [] as [], path: item.path, sha256: item.sha256, cases };
  });
  const expectedBrowser = phase === "verified" ? requiredProductMigrationCases(value.id, phase).filter(test => test.runner === "playwright") : [];
  const cleanupCases = browserRuns.flatMap(run => run.cases);
  if (JSON.stringify(cleanupCases.map(caseKey).sort()) !== JSON.stringify(expectedBrowser.map(caseKey).sort()) ||
    new Set(browserRuns.map(run => run.run)).size !== browserRuns.length || (phase === "verified" && browserRuns.length === 0))
    throw new Error("PRODUCT_MIGRATION_BROWSER_CLEANUP_CASES_REQUIRED");
  return {
    schema: productMigrationReceiptSchema,
    result: "passed",
    id: value.id,
    phase,
    candidateSha: value.candidateSha,
    runtimeFingerprint: value.runtimeFingerprint,
    verificationFingerprint: value.verificationFingerprint,
    release: { id: value.release.id, fileManifestSha256: value.release.fileManifestSha256 },
    observations,
    evidenceInputs,
    cleanup: { native: { passed: true, path: nativeCleanup.path as string, sha256: nativeCleanup.sha256 as string }, browserRuns },
  };
}

/** Resolve a repository-relative artifact only when every path component is
 * a real local directory/file. This keeps durable evidence reads and writes
 * inside the checkout even when docs paths are replaced with symlinks. */
export async function assertCanonicalRepositoryFilePath(
  rootInput: string,
  relativePath: string,
  options: { createParents?: boolean; requireFile?: boolean } = {},
) {
  const root = resolve(rootInput);
  if (!safeRepositoryPath(relativePath) || await realpath(root) !== root) throw new Error("PRODUCT_MIGRATION_REPOSITORY_PATH_REQUIRED");
  let parent = root;
  for (const segment of relativePath.split("/").slice(0, -1)) {
    parent = resolve(parent, segment);
    let stat;
    try { stat = await lstat(parent); }
    catch (error) {
      if (!options.createParents || (error as NodeJS.ErrnoException).code !== "ENOENT") throw new Error("PRODUCT_MIGRATION_REPOSITORY_PATH_REQUIRED", { cause: error });
      await mkdir(parent, { mode: 0o700 });
      stat = await lstat(parent);
    }
    if (!stat.isDirectory() || stat.isSymbolicLink() || await realpath(parent) !== parent) throw new Error("PRODUCT_MIGRATION_REPOSITORY_PATH_REQUIRED");
  }
  const absolute = resolve(root, relativePath);
  if (!absolute.startsWith(root + sep)) throw new Error("PRODUCT_MIGRATION_REPOSITORY_PATH_REQUIRED");
  try {
    const stat = await lstat(absolute);
    if (!stat.isFile() || stat.isSymbolicLink() || await realpath(absolute) !== absolute) throw new Error("PRODUCT_MIGRATION_REPOSITORY_FILE_REQUIRED");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT" || options.requireFile) throw new Error("PRODUCT_MIGRATION_REPOSITORY_FILE_REQUIRED", { cause: error });
  }
  return absolute;
}

export async function readProductMigrationReceiptEvidence(root: string, path: string, sha256: string) {
  if (!path.startsWith("docs/testing/evidence/product-migration-v2/") || !digest(sha256)) throw new Error("PRODUCT_MIGRATION_RECEIPT_EVIDENCE_BINDING_REQUIRED");
  const absolute = await assertCanonicalRepositoryFilePath(root, path, { requireFile: true });
  const stat = await lstat(absolute);
  if (stat.size > 128 * 1024 * 1024) throw new Error("PRODUCT_MIGRATION_RECEIPT_EVIDENCE_SIZE_INVALID");
  const bytes = await readFile(absolute);
  if (createHash("sha256").update(bytes).digest("hex") !== sha256) throw new Error("PRODUCT_MIGRATION_RECEIPT_EVIDENCE_DIGEST_MISMATCH");
  return bytes;
}

export function productMigrationReceiptMatchesCurrent(value: ProductMigrationReceiptReference, current: { runtimeFingerprint: string; verificationFingerprint: string }) {
  return value.runtimeFingerprint === current.runtimeFingerprint && value.verificationFingerprint === current.verificationFingerprint;
}

function safeRepositoryPath(path: string): boolean {
  return !isAbsolute(path) && !path.includes("\\") && path.split("/").every(part => part && part !== "." && part !== "..");
}

function safeEvidencePath(path: unknown, kind: "ci" | "browser" | "full" | "product-migration"): asserts path is string {
  const prefix = `.scratch/dev/test-suites/${kind}/`;
  if (typeof path !== "string" || !safeRepositoryPath(path) || !path.startsWith(prefix)) throw new Error("PRODUCT_MIGRATION_PRIVATE_EVIDENCE_PATH_REQUIRED");
}

async function readBoundEvidence(root: string, binding: { path: string; sha256: string }, kind: "ci" | "browser" | "full" | "product-migration") {
  safeEvidencePath(binding.path, kind);
  if (!digest(binding.sha256)) throw new Error("PRODUCT_MIGRATION_EVIDENCE_DIGEST_REQUIRED");
  const absolute = resolve(root, binding.path), rootAbsolute = resolve(root);
  if (!absolute.startsWith(rootAbsolute + sep)) throw new Error("PRODUCT_MIGRATION_PRIVATE_EVIDENCE_PATH_REQUIRED");
  const stat = await lstat(absolute);
  if (!stat.isFile() || stat.size > 128 * 1024 * 1024 || await realpath(absolute) !== absolute) throw new Error("PRODUCT_MIGRATION_REGULAR_EVIDENCE_REQUIRED");
  const bytes = await readFile(absolute);
  if (createHash("sha256").update(bytes).digest("hex") !== binding.sha256) throw new Error("PRODUCT_MIGRATION_EVIDENCE_DIGEST_MISMATCH");
  return { absolute, bytes };
}

function json(bytes: Buffer) {
  try { return JSON.parse(bytes.toString("utf8")) as unknown; }
  catch { throw new Error("PRODUCT_MIGRATION_EVIDENCE_JSON_REQUIRED"); }
}

function exactRelease(value: unknown, current: { id: string; fileManifestSha256: string }) {
  return record(value) && value.id === current.id && value.fileManifestSha256 === current.fileManifestSha256;
}

export type ProductMigrationImport = {
  schema: typeof productMigrationImportSchema;
  id: string;
  phase: ProductMigrationPhase;
  candidateSha: string;
  runtimeFingerprint: string;
  verificationFingerprint: string;
  release: { id: string; fileManifestSha256: string };
  nativeReceipt?: { path: string; sha256: string };
  fullReceipt?: { path: string; sha256: string };
  browserReceipts: { path: string; sha256: string }[];
};

export function validateProductMigrationImport(value: unknown): ProductMigrationImport {
  if (record(value) && value.schema === productMigrationImportSchema && value.phase === "enabled")
    throw new Error("REAL_COMPANY_GRANT_AND_MFA_EVIDENCE_REQUIRED");
  if (!record(value) || value.schema !== productMigrationImportSchema || typeof value.id !== "string" ||
    !["implemented", "verified"].includes(String(value.phase)) || !candidateDigest(value.candidateSha) ||
    !digest(value.runtimeFingerprint) || !digest(value.verificationFingerprint) || !record(value.release) ||
    typeof value.release.id !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,100}$/.test(value.release.id) || !digest(value.release.fileManifestSha256) ||
    !Array.isArray(value.browserReceipts))
    throw new Error("INVALID_PRODUCT_MIGRATION_IMPORT_V2");
  const phase = value.phase as "implemented" | "verified";
  const expectedBrowser = requiredProductMigrationCases(value.id, phase).filter(test => test.runner === "playwright");
  const hasNativeReceipt = Object.hasOwn(value, "nativeReceipt"), hasFullReceipt = Object.hasOwn(value, "fullReceipt");
  if (hasNativeReceipt === hasFullReceipt) throw new Error("INVALID_PRODUCT_MIGRATION_IMPORT_V2");
  const receiptBinding = hasNativeReceipt ? value.nativeReceipt : value.fullReceipt;
  if (!record(receiptBinding) || typeof receiptBinding.path !== "string" || !digest(receiptBinding.sha256)) throw new Error("INVALID_PRODUCT_MIGRATION_IMPORT_V2");
  if (!expectedBrowser.length && value.browserReceipts.length !== 0) throw new Error("IMPLEMENTED_RECEIPT_BROWSER_SCOPE_INVALID");
  if (expectedBrowser.length && value.browserReceipts.length === 0) throw new Error("VERIFIED_RECEIPT_BROWSER_WITNESS_REQUIRED");
  const browserReceipts = value.browserReceipts.map(item => {
    if (!record(item) || typeof item.path !== "string" || !digest(item.sha256)) throw new Error("INVALID_PRODUCT_MIGRATION_BROWSER_BINDING");
    return { path: item.path, sha256: item.sha256 };
  });
  if (new Set(browserReceipts.map(item => item.path)).size !== browserReceipts.length) throw new Error("DUPLICATE_PRODUCT_MIGRATION_BROWSER_BINDING");
  return {
    schema: productMigrationImportSchema,
    id: value.id,
    phase,
    candidateSha: value.candidateSha,
    runtimeFingerprint: value.runtimeFingerprint,
    verificationFingerprint: value.verificationFingerprint,
    release: { id: value.release.id, fileManifestSha256: value.release.fileManifestSha256 },
    ...(hasNativeReceipt ? { nativeReceipt: { path: receiptBinding.path, sha256: receiptBinding.sha256 } } : { fullReceipt: { path: receiptBinding.path, sha256: receiptBinding.sha256 } }),
    browserReceipts,
  };
}

export function productMigrationBrowserCaseMatches(actual: unknown, expected: ProductMigrationCase, suiteFiles: readonly string[]) {
  return record(actual) && actual.runner === expected.runner && actual.suite === expected.suite && typeof actual.file === "string" &&
    canonicalCaseFile(actual.file, suiteFiles) === expected.file && actual.title === expected.title &&
    (actual.project ?? undefined) === (expected.project ?? undefined);
}

export function productMigrationBrowserReceiptCaseMatches(actual: unknown, expected: ProductMigrationCase, suiteFiles: readonly string[]) {
  if (!record(actual) || expected.runner !== "playwright") return false;
  const browserCase = actual.runner === undefined ? { ...actual, runner: "playwright" } : actual;
  return productMigrationBrowserCaseMatches(browserCase, expected, suiteFiles);
}

function assertNativeProductMigrationSuiteBinding(expected: readonly ProductMigrationCase[]) {
  for (const test of expected) {
    const ownerSuite = catalogue.suites.find(suite => suite.id === test.suite);
    if (!ownerSuite || !["unit", "integration"].includes(ownerSuite.lane) || !ownerSuite.files.includes(test.file))
      throw new Error("PRODUCT_MIGRATION_NATIVE_SUITE_BINDING_REQUIRED");
  }
}

function internalFullRegressionCases(root: string) {
  const suites = loadCatalogue(root).suites;
  return internalRequiredCases(loadRequiredCases(root), suites).map(test => {
    const owners = suites.filter(suite => suite.files.includes(test.file));
    if (owners.length !== 1) throw new Error("PRODUCT_MIGRATION_FULL_RUN_CASE_SUITE_BINDING_REQUIRED");
    const suite = owners[0];
    if ((test.runner === "playwright") !== ["e2e", "journeys", "uat"].includes(suite.lane))
      throw new Error("PRODUCT_MIGRATION_FULL_RUN_CASE_SUITE_BINDING_REQUIRED");
    return { ...test, suite: suite.id };
  });
}

async function collectNative(root: string, binding: { path: string; sha256: string }, input: ProductMigrationImport) {
  if (binding.path !== ".scratch/dev/test-suites/ci/native.json") throw new Error("PRODUCT_MIGRATION_OFFICIAL_NATIVE_RECEIPT_REQUIRED");
  const artifact = await readBoundEvidence(root, binding, "ci"), receipt = json(artifact.bytes) as NativeCiReceipt;
  if (!record(receipt) || receipt.version !== 1 || receipt.kind !== "native" || receipt.candidate !== input.candidateSha ||
    receipt.runtimeFingerprint !== input.runtimeFingerprint || receipt.verificationFingerprint !== input.verificationFingerprint ||
    receipt.passed !== true || !exactRelease(receipt.release, input.release) || !Array.isArray(receipt.phases) || !Array.isArray(receipt.cases))
    throw new Error("PRODUCT_MIGRATION_NATIVE_CANDIDATE_MISMATCH");
  if (receipt.phases.length !== requiredNativePhases.length || requiredNativePhases.some(phase => receipt.phases.filter(row => row.phase === phase && row.passed === true).length !== 1))
    throw new Error("PRODUCT_MIGRATION_NATIVE_ACCEPTANCE_REQUIRED");
  const expected = requiredProductMigrationCases(input.id, "implemented");
  assertNativeProductMigrationSuiteBinding(expected);
  const observations = expected.map(test => {
    const matched = receipt.cases.filter(item => item.runner === test.runner && item.file === test.file && item.title === test.title &&
      (item.project ?? undefined) === test.project && item.status === "passed");
    if (matched.length !== 1) throw new Error("PRODUCT_MIGRATION_EXACT_NATIVE_CASE_REQUIRED");
    return { ...test, status: "passed" as const };
  });
  return {
    observations,
    input: { kind: "native-ci", path: binding.path, sha256: binding.sha256 },
  };
}

async function collectFullRegression(root: string, binding: { path: string; sha256: string }, input: ProductMigrationImport) {
  const expected = requiredProductMigrationCases(input.id, "implemented");
  assertNativeProductMigrationSuiteBinding(expected);
  const runId = fullRegressionRunIdFromPath(binding.path);
  if (!runId) throw new Error("PRODUCT_MIGRATION_OFFICIAL_FULL_RUN_RECEIPT_REQUIRED");
  const artifact = await readBoundEvidence(root, binding, "full");
  const fullReceipt = json(artifact.bytes);
  const fullBinding = {
    runId,
    candidate: input.candidateSha,
    runtimeFingerprint: input.runtimeFingerprint,
    verificationFingerprint: input.verificationFingerprint,
    release: input.release,
  };
  const allRequired = internalFullRegressionCases(root);
  const validatedAll = validateFullRegressionCases(fullReceipt, fullBinding, allRequired, root);
  if (!record(fullReceipt) || !record(fullReceipt.browserIsolation) || typeof fullReceipt.browserIsolation.evidencePath !== "string" ||
    typeof fullReceipt.browserIsolation.evidenceSha256 !== "string") throw new Error("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_REQUIRED");
  let browserIsolationArtifact: Awaited<ReturnType<typeof readBoundEvidence>>;
  try {
    browserIsolationArtifact = await readBoundEvidence(root, {
      path: fullReceipt.browserIsolation.evidencePath,
      sha256: fullReceipt.browserIsolation.evidenceSha256,
    }, "full");
  } catch (error) {
    if (error instanceof Error && error.message === "PRODUCT_MIGRATION_EVIDENCE_DIGEST_MISMATCH")
      throw new Error("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_DIGEST_MISMATCH", { cause: error });
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_ARTIFACT_REQUIRED", { cause: error });
  }
  await validateFullRegressionBrowserIsolationArtifacts(fullReceipt, fullBinding, browserIsolationArtifact.bytes, root);
  const byIdentity = new Map(validatedAll.map(test => [caseKey(test), test]));
  const observations = expected.map(test => {
    const observation = byIdentity.get(caseKey(test));
    if (!observation || observation.suite !== test.suite) throw new Error("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");
    return { ...test, status: "passed" as const };
  });
  if (!record(fullReceipt) || !record(fullReceipt.rollback)) throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_REQUIRED");
  const rollback = fullReceipt.rollback;
  const runDirectory = `.scratch/dev/test-suites/full/${runId}`;
  const reportPath = `${runDirectory}/rollback-vitest.json`;
  const evidencePath = `${runDirectory}/rollback-evidence/encrypted-recovery.json`;
  if (rollback.testReportPath !== reportPath || rollback.evidencePath !== evidencePath ||
    typeof rollback.testReportSha256 !== "string" || typeof rollback.evidenceSha256 !== "string")
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_REQUIRED");
  const [reportArtifact, evidenceArtifact] = await Promise.all([
    readBoundEvidence(root, { path: reportPath, sha256: rollback.testReportSha256 }, "full"),
    readBoundEvidence(root, { path: evidencePath, sha256: rollback.evidenceSha256 }, "full"),
  ]);
  validateFullRegressionRollbackArtifacts(fullReceipt, fullBinding, {
    reportBytes: reportArtifact.bytes,
    evidenceBytes: evidenceArtifact.bytes,
  }, root);
  return {
    observations,
    input: { kind: "full-regression-receipt", path: binding.path, sha256: binding.sha256 },
  };
}

function browserReceiptRecord(value: unknown) {
  if (!record(value) || value.version !== 1 || value.passed !== true || value.interrupted !== null || !Array.isArray(value.cleanupFailures) ||
    value.cleanupFailures.length !== 0 || !Array.isArray(value.receipts) || !Array.isArray(value.suites) ||
    !record(value.execution) || value.execution.mode !== "acceptance") throw new Error("PRODUCT_MIGRATION_BROWSER_CLEANUP_OR_EXECUTION_REQUIRED");
  return value as Record<string, unknown> & { receipts: unknown[]; suites: unknown[] };
}

function browserAttachments(report:unknown,wanted:ProductMigrationCase,suiteFiles:readonly string[]):unknown[]{
  if(!record(report)||!Array.isArray(report.suites)||!wanted.project)throw new Error("PRODUCT_MIGRATION_BROWSER_ATTACHMENTS_REQUIRED");
  const matches:unknown[][]=[];
  const visit=(suite:unknown,parents:string[])=>{
    if(!record(suite))return;const isFile=typeof suite.title==="string"&&/\.spec\.[cm]?[jt]sx?$/.test(suite.title),names=typeof suite.title==="string"&&!isFile?[...parents,suite.title]:parents;
    for(const spec of Array.isArray(suite.specs)?suite.specs:[]){
      if(!record(spec)||typeof spec.file!=="string"||typeof spec.title!=="string"||canonicalCaseFile(spec.file,suiteFiles)!==canonicalCaseFile(wanted.file,suiteFiles)||[...names,spec.title].join(" ")!==wanted.title)continue;
      for(const test of Array.isArray(spec.tests)?spec.tests:[])if(record(test)&&test.projectName===wanted.project&&Array.isArray(test.results))for(const result of test.results)if(record(result)&&result.status==="passed"&&Array.isArray(result.attachments))matches.push(result.attachments);
    }
    for(const child of Array.isArray(suite.suites)?suite.suites:[])visit(child,names);
  };
  for(const suite of report.suites)visit(suite,[]);
  if(matches.length!==1)throw new Error("PRODUCT_MIGRATION_BROWSER_ATTACHMENTS_REQUIRED");return matches[0]!;
}

async function browserWitness(root: string, value: Record<string, unknown> & { receipts: unknown[]; suites: unknown[] }, wanted: ProductMigrationCase, input: ProductMigrationImport, evidenceInputs: ProductMigrationEvidenceInput[]) {
  if (value.runtimeFingerprint !== input.runtimeFingerprint || value.verificationFingerprint !== input.verificationFingerprint) throw new Error("PRODUCT_MIGRATION_BROWSER_FINGERPRINT_MISMATCH");
  const owners = catalogue.suites.filter(suite => suite.id === wanted.suite && suite.files.includes(wanted.file));
  if (owners.length !== 1) throw new Error("PRODUCT_MIGRATION_BROWSER_SUITE_BINDING_REQUIRED");
  const suiteFiles = owners[0].files;
  const matching = value.receipts.filter(item => productMigrationBrowserReceiptCaseMatches(item, wanted, suiteFiles) && record(item) && item.passed === true);
  if (matching.length !== 1) throw new Error("PRODUCT_MIGRATION_EXACT_BROWSER_CASE_REQUIRED");
  const result = matching[0] as Record<string, unknown>;
  const businessWorkflow = productHostBusinessWorkflowForCase(wanted.file, wanted.title);
  if (typeof result.run !== "string" || !/^[a-f0-9]{12}$/.test(result.run) || typeof result.report !== "string" ||
    !record(result.cleanup) || result.cleanup.completed !== true || result.cleanup.ownedDirectoryAbsent !== true ||
    !record(result.uiEvidence) || result.uiEvidence.schema !== "erpx-ui-original-browser-leaves/v1" || result.uiEvidence.run !== result.run)
    throw new Error("PRODUCT_MIGRATION_UI_WITNESS_REQUIRED");
  const caseReportPath = resolve(root, result.report), browserRoot = resolve(root, ".scratch/dev/test-suites/browser");
  if (!caseReportPath.startsWith(browserRoot + sep) || basename(caseReportPath) !== "result.json" || basename(dirname(caseReportPath)) !== result.run)
    throw new Error("PRODUCT_MIGRATION_CASE_REPORT_PATH_REQUIRED");
  const witness = result.uiEvidence as unknown as UiBrowserEvidenceWitness;
  const witnessWithFormsHost = witness as UiBrowserEvidenceWitness & { formsHostLease?: { path: string; sha256: string; bytes: number } };
  await verifyUiBrowserEvidence(root, witness);
  const stateFile = await readBoundEvidence(root, witness.state, "browser"), state = json(stateFile.bytes);
  if (!record(state) || state.run !== result.run || state.sourceFingerprint !== input.runtimeFingerprint || !exactRelease(state.release, input.release))
    throw new Error("PRODUCT_MIGRATION_COMPILED_RELEASE_WITNESS_MISMATCH");
  const reportFile = await readBoundEvidence(root, witness.report, "browser"), report = json(reportFile.bytes) as { suites?: unknown[] };
  const actual = browserCases(report as Parameters<typeof browserCases>[0]);
  if (actual.length !== 1) throw new Error("PRODUCT_MIGRATION_PLAYWRIGHT_REPORT_IDENTITY_MISMATCH");
  const reportCase = { runner: "playwright", suite: wanted.suite, ...actual[0] };
  if (!productMigrationBrowserCaseMatches(reportCase, wanted, suiteFiles))
    throw new Error("PRODUCT_MIGRATION_PLAYWRIGHT_REPORT_IDENTITY_MISMATCH");
  assertBrowserCasePassed(report as Parameters<typeof assertBrowserCasePassed>[0], actual[0]);
  if (caseReportPath !== reportFile.absolute) throw new Error("PRODUCT_MIGRATION_PLAYWRIGHT_REPORT_BINDING_MISMATCH");
  const productHost = wanted.suite === "identity_access.product-hosts.e2e";
  if (productHost) {
    const financeInvoiceHost = businessWorkflow === "finance.sales-invoice";
    if (!witness.productHostLease || !record(result.productHostLease) || result.productHostLease.path !== witness.productHostLease.path ||
      result.productHostLease.sha256 !== witness.productHostLease.sha256) throw new Error("PRODUCT_MIGRATION_PRODUCT_HOST_WITNESS_REQUIRED");
    const leaseFile = await readBoundEvidence(root, witness.productHostLease, "browser"), lease = json(leaseFile.bytes);
    if (!record(lease) || lease.schema !== "erpx-owned-product-host-browser/v1" || lease.run !== result.run ||
      lease.sourceFingerprint !== input.runtimeFingerprint || lease.verificationFingerprint !== input.verificationFingerprint ||
      !exactRelease(lease.release, input.release) || lease.wholeProductAcceptance !== false || lease.readOnlyBusiness !== !financeInvoiceHost ||
      lease.scopedBusinessWorkflow !== (financeInvoiceHost ? businessWorkflow : null) || lease.readOnlyWorkflow !== productHostReadOnlyWorkflowForCase(wanted.file, wanted.title) ||
      lease.isolatedNormalCommercial !== true || !Array.isArray(lease.origins) ||
      !lease.origins.includes(financeInvoiceHost ? "https://finance-dev-erpx.jesadakorn.com" : "https://assets-dev-erpx.jesadakorn.com"))
      throw new Error("PRODUCT_MIGRATION_PRODUCT_HOST_SCOPE_MISMATCH");
    if (!record(result.productHostLease) || result.productHostLease.path !== witness.productHostLease.path || result.productHostLease.sha256 !== witness.productHostLease.sha256)
      throw new Error("PRODUCT_MIGRATION_PRODUCT_HOST_BINDING_MISMATCH");
    if (financeInvoiceHost) {
      const business = result.productHostBusiness;
      if (wanted.file !== productHostBrowserFile || wanted.suite !== "identity_access.product-hosts.e2e" || !record(business) ||
        Object.keys(business).sort().join(",") !== "fixtureState,proof,screenshot" || !record(business.proof) || !record(business.fixtureState) || !record(business.screenshot))
        throw new Error("PRODUCT_MIGRATION_FINANCE_PRODUCT_HOST_PROOF_REQUIRED");
      const businessProofBinding = business.proof, businessStateBinding = business.fixtureState, businessScreenshotBinding = business.screenshot;
      const attachments = browserAttachments(report, wanted, suiteFiles);
      const readBusinessAttachment = async (name: string, contentType: string, binding: unknown) => {
        if (!record(binding) || typeof binding.path !== "string" || !digest(binding.sha256) || !Number.isSafeInteger(binding.bytes) || Number(binding.bytes) < 1)
          throw new Error("PRODUCT_MIGRATION_FINANCE_PRODUCT_HOST_BINDING_REQUIRED");
        const matches = attachments.filter(item => record(item) && item.name === name);
        const attachment = matches[0];
        const attachmentPath = record(attachment) ? attachment.path : undefined;
        if (matches.length !== 1 || !record(attachment) || attachment.contentType !== contentType || typeof attachmentPath !== "string" || !isAbsolute(attachmentPath))
          throw new Error("PRODUCT_MIGRATION_FINANCE_PRODUCT_HOST_ATTACHMENT_REQUIRED");
        const absolute = resolve(attachmentPath), relativePath = relative(root, absolute).split(sep).join("/"), artifactRoot = resolve(root, ".scratch/dev/test-suites/browser", String(result.run), "artifacts");
        if (!absolute.startsWith(artifactRoot + sep) || !safeRepositoryPath(relativePath) || binding.path !== relativePath)
          throw new Error("PRODUCT_MIGRATION_FINANCE_PRODUCT_HOST_ATTACHMENT_BINDING_REQUIRED");
        const file = await readBoundEvidence(root, { path: relativePath, sha256: binding.sha256 }, "browser");
        if (file.bytes.length !== binding.bytes || resolve(root, binding.path) !== absolute) throw new Error("PRODUCT_MIGRATION_FINANCE_PRODUCT_HOST_ATTACHMENT_BINDING_REQUIRED");
        return file;
      };
      const [proofFile, fixtureFile, screenshotFile] = await Promise.all([
        readBusinessAttachment("finance-product-host-sales-invoice", "application/json", businessProofBinding),
        readBusinessAttachment("finance-product-host-simulation-state", "application/json", businessStateBinding),
        readBusinessAttachment("finance-product-host-sales-invoice-screen", "image/png", businessScreenshotBinding),
      ]);
      if (witness.observations.length !== 1 || witness.screenshots.length !== 1 ||
        !witness.screenshots.some(item => item.path === businessScreenshotBinding.path &&
          item.sha256 === businessScreenshotBinding.sha256 && item.bytes === businessScreenshotBinding.bytes))
        throw new Error("PRODUCT_MIGRATION_FINANCE_PRODUCT_HOST_UI_OBSERVATION_REQUIRED");
      const uiObservationFile = await readBoundEvidence(root, witness.observations[0]!, "browser"), uiObservation = json(uiObservationFile.bytes);
      const proof = json(proofFile.bytes), fixtureState = json(fixtureFile.bytes), proofRelease = record(proof) ? proof.release : undefined;
      const fixture = record(fixtureState) && record(fixtureState.fixture) ? fixtureState.fixture : undefined;
      const completion = record(proof) ? proof.completionEvidence : undefined;
      const requestBody = record(proof) && typeof proof.requestBody === "string" ? proof.requestBody : "";
      let request: unknown; try { request = JSON.parse(requestBody); } catch { request = undefined; }
      const requestLine = record(request) && Array.isArray(request.lines) && request.lines.length === 1 ? request.lines[0] : undefined;
      const invoiceDate = record(request) ? request.invoiceDate : undefined;
      const validInvoiceDate = typeof invoiceDate === "string" && /^\d{4}-\d{2}-\d{2}$/.test(invoiceDate) &&
        !Number.isNaN(Date.parse(`${invoiceDate}T00:00:00.000Z`)) && new Date(`${invoiceDate}T00:00:00.000Z`).toISOString().slice(0, 10) === invoiceDate;
      const leaseUi = record(lease.ui) ? lease.ui : undefined, leaseBackend = leaseUi && record(leaseUi.backend) ? leaseUi.backend : undefined;
      const leaseState = leaseBackend && record(leaseBackend.state) ? leaseBackend.state : undefined;
      const proofKeys = ["companyId", "completionEvidence", "contentType", "currency", "disposableCohortCleanup", "fixturePartyId", "idempotencyKey", "invoiceId", "invoiceNumber", "origin", "project", "release", "requestBody", "requestSha256", "run", "schema", "sourceFingerprint", "total", "userId", "verificationFingerprint", "workflow", "wholeProductAcceptance"].sort().join(",");
      if (!record(proof) || Object.keys(proof).sort().join(",") !== proofKeys || proof.schema !== "erpx-product-host-finance-sales-invoice/v1" ||
        proof.workflow !== businessWorkflow || proof.run !== result.run || proof.sourceFingerprint !== input.runtimeFingerprint ||
        proof.verificationFingerprint !== input.verificationFingerprint || !exactRelease(proofRelease, input.release) || proof.project !== wanted.project ||
        proof.origin !== "https://finance-dev-erpx.jesadakorn.com" || typeof proof.companyId !== "string" || !proof.companyId ||
        typeof proof.userId !== "string" || !proof.userId || typeof proof.fixturePartyId !== "string" || !proof.fixturePartyId ||
        typeof proof.invoiceId !== "string" || !proof.invoiceId || typeof proof.invoiceNumber !== "string" || !/^INV-\d{4}-\d{6,}$/.test(proof.invoiceNumber) ||
        proof.currency !== "THB" || proof.total !== "214.00" || proof.contentType !== "application/json" || typeof proof.requestBody !== "string" ||
        typeof proof.idempotencyKey !== "string" ||
        !/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(proof.idempotencyKey) ||
        !digest(proof.requestSha256) || proof.requestSha256 !== createHash("sha256").update(proof.requestBody).digest("hex") ||
        !record(request) || Object.keys(request).sort().join(",") !== "completionConfirmed,completionNote,currency,dueDate,invoiceDate,lines,partyId" ||
        request.partyId !== proof.fixturePartyId || request.currency !== "THB" || request.completionConfirmed !== true ||
        request.completionNote !== `Fictional Finance Product Host completion evidence ${result.run}` || !validInvoiceDate || request.dueDate !== invoiceDate ||
        !record(requestLine) || Object.keys(requestLine).sort().join(",") !== "description,quantity,taxBps,unitPrice" ||
        requestLine.description !== `Fictional Finance Product Host completed service ${result.run}` || requestLine.quantity !== "2" ||
        requestLine.unitPrice !== "100" || requestLine.taxBps !== 700 || proof.disposableCohortCleanup !== true || proof.wholeProductAcceptance !== false ||
        !record(completion) || Object.keys(completion).sort().join(",") !== "confirmedAt,confirmedBy,invoiceRevision,note" ||
        completion.note !== `Fictional Finance Product Host completion evidence ${result.run}` || completion.confirmedBy !== proof.userId ||
        !Number.isSafeInteger(completion.confirmedAt) || Number(completion.confirmedAt) <= 0 || completion.invoiceRevision !== 1 ||
        !record(fixtureState) || fixtureState.run !== result.run || fixtureState.prefix !== `erpx_browser_${result.run}` ||
        fixtureState.productHosts !== true || fixtureState.port !== 3104 || fixtureState.origin !== "http://127.0.0.1:3104" ||
        fixtureState.ready !== true || fixtureState.diagnostic === true || fixtureState.sourceFingerprint !== input.runtimeFingerprint ||
        !exactRelease(fixtureState.release, input.release) || !fixture || fixture.companyId !== proof.companyId || fixture.customerId !== proof.fixturePartyId ||
        !record(uiObservation) || uiObservation.schema !== "erpx-ui-browser-observation/v1" || uiObservation.result !== "passed" ||
        uiObservation.run !== result.run || uiObservation.sourceFingerprint !== input.runtimeFingerprint ||
        uiObservation.verificationFingerprint !== input.verificationFingerprint || typeof uiObservation.url !== "string" ||
        new URL(uiObservation.url).origin !== "https://finance-dev-erpx.jesadakorn.com" ||
        !record(uiObservation.ownerContext) || uiObservation.ownerContext.companyId !== proof.companyId || uiObservation.ownerContext.userId !== proof.userId ||
        typeof uiObservation.ownerContext.assignmentId !== "string" || !uiObservation.ownerContext.assignmentId ||
        typeof uiObservation.ownerContext.revision !== "string" || !/^\d+$/.test(uiObservation.ownerContext.revision) ||
        !record(uiObservation.screenshot) || uiObservation.screenshot.path !== businessScreenshotBinding.path ||
        uiObservation.screenshot.sha256 !== businessScreenshotBinding.sha256 || uiObservation.screenshot.bytes !== businessScreenshotBinding.bytes ||
        fixtureFile.bytes.equals(stateFile.bytes) !== true || !leaseState || leaseState.path !== resolve(root, ".runtime/browser", result.run, "state.json") ||
        leaseState.sha256 !== createHash("sha256").update(fixtureFile.bytes).digest("hex") || leaseState.bytes !== fixtureFile.bytes.length)
        throw new Error("PRODUCT_MIGRATION_FINANCE_PRODUCT_HOST_PROOF_MISMATCH");
      for (const [kind, file] of [["product-host-finance-invoice-proof", proofFile], ["product-host-finance-fixture-state", fixtureFile], ["product-host-finance-invoice-screenshot", screenshotFile]] as const)
        if (!evidenceInputs.some(item => item.path === relative(root, file.absolute).split(sep).join("/") && item.sha256 === createHash("sha256").update(file.bytes).digest("hex")))
          evidenceInputs.push({ kind, path: relative(root, file.absolute).split(sep).join("/"), sha256: createHash("sha256").update(file.bytes).digest("hex") });
    } else if (result.productHostBusiness !== undefined) throw new Error("UNEXPECTED_PRODUCT_MIGRATION_PRODUCT_HOST_BUSINESS_PROOF");
  } else if (witness.productHostLease || result.productHostLease) throw new Error("UNEXPECTED_PRODUCT_HOST_WITNESS");
  const formsHost = wanted.suite === "marketing.forms-host.e2e";
  if (formsHost) {
    const { formsHostBrowserFile, formsHostCaseTitle, formsHostOrigin, formsHostWorkflow, readFormsHostLease } = await import("../testing/forms-host-cohort");
    if (wanted.file !== formsHostBrowserFile || wanted.title !== formsHostCaseTitle || !witnessWithFormsHost.formsHostLease ||
      !record(result.formsHostLease) || result.formsHostLease.path !== witnessWithFormsHost.formsHostLease.path ||
      result.formsHostLease.sha256 !== witnessWithFormsHost.formsHostLease.sha256)
      throw new Error("PRODUCT_MIGRATION_FORMS_HOST_WITNESS_REQUIRED");
    const leaseFile = await readBoundEvidence(root, witnessWithFormsHost.formsHostLease, "browser");
    const lease = await readFormsHostLease(root, leaseFile.absolute);
    const stateRecord = record(state) ? state : undefined;
    const engineering = stateRecord && record(stateRecord.fixture) && record(stateRecord.fixture.engineeringWorkplacePlatform)
      ? stateRecord.fixture.engineeringWorkplacePlatform : undefined;
    const marketing = engineering && record(engineering.marketing) ? engineering.marketing : undefined;
    const requestEvidence = lease.requestEvidence;
    if (!stateRecord || stateRecord.formsHosts !== true || stateRecord.productHosts === true || stateRecord.executiveWorkHosts === true ||
      stateRecord.origin !== "http://127.0.0.1:3104" || !exactRelease(stateRecord.release, input.release) || !marketing ||
      lease.run !== result.run || lease.sourceFingerprint !== input.runtimeFingerprint ||
      lease.verificationFingerprint !== input.verificationFingerprint || !exactRelease(lease.release, input.release) ||
      lease.workflow !== formsHostWorkflow || lease.origin !== formsHostOrigin || lease.formsOnly !== true ||
      lease.productHosts !== false || lease.executiveWorkHosts !== false || lease.commercial !== false || lease.mfa !== false ||
      lease.apply !== false || lease.packageEnforcement !== false || lease.wholeProductAcceptance !== false ||
      lease.fixture.slug !== "simulation" || lease.fixture.formId !== marketing.formId ||
      lease.fixture.formVersion !== marketing.formVersion || lease.fixture.mediaPublicationId !== marketing.publicationId ||
      lease.ui.backend.state.path !== resolve(root, ".runtime/browser", result.run, "state.json") ||
      lease.ui.backend.state.sha256 !== createHash("sha256").update(stateFile.bytes).digest("hex") ||
      lease.ui.backend.state.bytes !== stateFile.bytes.length || lease.ui.backend.pid !== stateRecord.pid ||
      lease.ui.backend.jobsPid !== stateRecord.jobsPid || requestEvidence.acceptedPosts !== 2 || requestEvidence.replayCount !== 1 ||
      requestEvidence.refusalCount !== 0 || requestEvidence.refusalLog.length !== 0 || !digest(requestEvidence.bodySha256) ||
      !digest(requestEvidence.idempotencyKeySha256))
      throw new Error("PRODUCT_MIGRATION_FORMS_HOST_SCOPE_MISMATCH");
    const retainedDatabasePath = relative(root, lease.databases.path).split(sep).join("/");
    const retainedGuidePath = relative(root, lease.fixtureGuide.path).split(sep).join("/");
    const [databaseArtifact, guideArtifact] = await Promise.all([
      readBoundEvidence(root, { path: retainedDatabasePath, sha256: lease.databases.sha256 }, "browser"),
      readBoundEvidence(root, { path: retainedGuidePath, sha256: lease.fixtureGuide.sha256 }, "browser"),
    ]);
    if (databaseArtifact.bytes.length !== lease.databases.bytes || guideArtifact.bytes.length !== lease.fixtureGuide.bytes)
      throw new Error("PRODUCT_MIGRATION_FORMS_HOST_RETAINED_FIXTURE_MISMATCH");
    const planPath = resolve(root, lease.plan.path), privatePlanRoot = resolve(root, ".scratch/dev");
    if (!planPath.startsWith(privatePlanRoot + sep) || await realpath(planPath) !== planPath) throw new Error("PRODUCT_MIGRATION_FORMS_HOST_ARM_PATH_REQUIRED");
    const planStat = await lstat(planPath);
    if (!planStat.isFile() || planStat.isSymbolicLink() || (planStat.mode & 0o077) !== 0) throw new Error("PRODUCT_MIGRATION_FORMS_HOST_PRIVATE_ARM_REQUIRED");
    const planBytes = await readFile(planPath);
    if (createHash("sha256").update(planBytes).digest("hex") !== lease.plan.sha256) throw new Error("PRODUCT_MIGRATION_FORMS_HOST_ARM_DIGEST_MISMATCH");
    const plan = json(planBytes), tls = record(plan) && record(plan.tls) ? plan.tls : undefined;
    const tools = record(plan) && record(plan.tools) ? plan.tools : undefined;
    const planBindingMatches = (value: unknown, binding: { path: string; sha256: string }) => record(value) &&
      typeof value.path === "string" && resolve(root, value.path) === resolve(root, binding.path) && value.sha256 === binding.sha256;
    if (!record(plan) || plan.schema !== "erpx-forms-host-arm/v1" || plan.version !== 1 || plan.enabled !== true ||
      plan.workflow !== formsHostWorkflow || plan.runtimeFingerprint !== input.runtimeFingerprint ||
      plan.verificationFingerprint !== input.verificationFingerprint || !tls || !tools ||
      !planBindingMatches(tls.key, lease.key) || !planBindingMatches(tls.certificate, lease.certificate) ||
      !planBindingMatches(tls.ca, lease.ca) || !planBindingMatches(tools.chromium, lease.chromium) ||
      !record(tls.key) || !record(tls.certificate) || !record(tls.ca) || !record(tools.node) ||
      tools.node.sha256 !== lease.ui.node.sha256 || resolve(root, String(tools.node.path)) !== resolve(root, lease.ui.node.path) ||
      tls.spki !== lease.spki || lease.certificateSha256 !== tls.certificate.sha256 ||
      resolve(root, String(plan.release && record(plan.release) ? plan.release.path : "")) !== resolve(root, ".runtime/releases", input.release.id, "release.json") ||
      resolve(root, String(plan.releaseFiles && record(plan.releaseFiles) ? plan.releaseFiles.path : "")) !== resolve(root, ".runtime/releases", input.release.id, "release-files.json") ||
      !record(plan.release) || !record(plan.releaseFiles) || plan.releaseFiles.sha256 !== input.release.fileManifestSha256)
      throw new Error("PRODUCT_MIGRATION_FORMS_HOST_ARM_BINDING_MISMATCH");
    for (const [kind, absolutePath, sha256] of [
      ["forms-host-databases", lease.databases.path, lease.databases.sha256],
      ["forms-host-fixture-guide", lease.fixtureGuide.path, lease.fixtureGuide.sha256],
      ["forms-host-plan", lease.plan.path, lease.plan.sha256],
      ["forms-host-tls-key", lease.key.path, lease.key.sha256],
    ] as const) {
      const path = relative(root, absolutePath).split(sep).join("/");
      if (!safeRepositoryPath(path)) throw new Error("PRODUCT_MIGRATION_FORMS_HOST_PRIVATE_INPUT_PATH_REQUIRED");
      if (!evidenceInputs.some(item => item.path === path && item.sha256 === sha256)) evidenceInputs.push({ kind, path, sha256 });
    }
  } else if (witnessWithFormsHost.formsHostLease || result.formsHostLease) throw new Error("UNEXPECTED_FORMS_HOST_WITNESS");
  for (const binding of [witness.report, witness.state, ...witness.observations, ...witness.screenshots, ...(witness.productHostLease ? [witness.productHostLease] : []), ...(witnessWithFormsHost.formsHostLease ? [witnessWithFormsHost.formsHostLease] : [])]) {
    const path = binding.path;
    if (!path.startsWith(".scratch/dev/test-suites/browser/")) throw new Error("PRODUCT_MIGRATION_UI_LEAF_PATH_REQUIRED");
    const kind = path === witness.report.path ? "browser-playwright-report" : path === witness.state.path ? "browser-state" :
      path === witness.productHostLease?.path ? "product-host-lease" : path === witnessWithFormsHost.formsHostLease?.path ? "forms-host-lease" :
        path.endsWith(".png") ? "browser-screenshot" : "browser-observation";
    if (!evidenceInputs.some(item => item.path === path && item.sha256 === binding.sha256)) evidenceInputs.push({ kind, path, sha256: binding.sha256 });
  }
  return { observation: { ...wanted, status: "passed" as const }, run: result.run };
}

function receiptReference(value: unknown): ProductMigrationReceiptReference {
  if (!record(value) || typeof value.id !== "string" || !["implemented", "verified", "enabled"].includes(String(value.phase)) ||
    !candidateDigest(value.candidateSha) || !digest(value.runtimeFingerprint) || !digest(value.verificationFingerprint) ||
    !record(value.release) || typeof value.release.id !== "string" || !digest(value.release.fileManifestSha256) ||
    typeof value.path !== "string" || !safeRepositoryPath(value.path) || !value.path.startsWith("docs/testing/evidence/product-migration-v2/") || !digest(value.sha256))
    throw new Error("INVALID_PRODUCT_MIGRATION_RECEIPT_REFERENCE");
  return {
    id: value.id,
    phase: value.phase as ProductMigrationPhase,
    candidateSha: value.candidateSha,
    runtimeFingerprint: value.runtimeFingerprint,
    verificationFingerprint: value.verificationFingerprint,
    release: { id: value.release.id, fileManifestSha256: value.release.fileManifestSha256 },
    path: value.path,
    sha256: value.sha256,
  };
}

function productMigrationReceiptEvidencePath(candidateSha: string, id: string, phase: Exclude<ProductMigrationPhase, "enabled">) {
  const slug = id.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 80).replace(/-$/g, "") || "workflow";
  const identity = createHash("sha256").update(id).digest("hex");
  return `docs/testing/evidence/product-migration-v2/${candidateSha}-${slug}-${identity}-${phase}.json`;
}

export function validateProductMigrationReceiptManifest(value: unknown): ProductMigrationReceiptManifest {
  if (!record(value) || value.schema !== productMigrationReceiptManifestSchema || !Array.isArray(value.records)) throw new Error("INVALID_PRODUCT_MIGRATION_RECEIPT_MANIFEST_V2");
  const records = value.records.map(receiptReference), seen = new Set<string>();
  for (const reference of records) {
    const key = `${reference.id}:${reference.phase}:${reference.candidateSha}:${reference.runtimeFingerprint}:${reference.verificationFingerprint}`;
    if (seen.has(key)) throw new Error("DUPLICATE_PRODUCT_MIGRATION_RECEIPT_REFERENCE");
    seen.add(key);
  }
  return { schema: productMigrationReceiptManifestSchema, records };
}

export function appendProductMigrationReceipt(manifest: ProductMigrationReceiptManifest, reference: ProductMigrationReceiptReference) {
  const sameKey = manifest.records.find(item => item.id === reference.id && item.phase === reference.phase && item.candidateSha === reference.candidateSha &&
    item.runtimeFingerprint === reference.runtimeFingerprint && item.verificationFingerprint === reference.verificationFingerprint);
  if (sameKey) {
    if (sameKey.path !== reference.path || sameKey.sha256 !== reference.sha256) throw new Error("PRODUCT_MIGRATION_RECEIPT_CONFLICT");
    return { status: "already-imported" as const, reference };
  }
  return {
    status: "appended" as const,
    manifest: validateProductMigrationReceiptManifest({ ...manifest, records: [...manifest.records, reference] }),
  };
}

export async function importProductMigrationReceipt(rootInput: string, inputPath: string) {
  const root = resolve(rootInput);
  const inputBinding = { path: inputPath.replaceAll("\\", "/"), sha256: "" };
  safeEvidencePath(inputBinding.path, "product-migration");
  const inputAbsolute = resolve(root, inputBinding.path), inputStat = await lstat(inputAbsolute);
  if (!inputStat.isFile() || await realpath(inputAbsolute) !== inputAbsolute) throw new Error("PRODUCT_MIGRATION_IMPORT_FILE_REQUIRED");
  const importBytes = await readFile(inputAbsolute);
  inputBinding.sha256 = createHash("sha256").update(importBytes).digest("hex");
  const input = validateProductMigrationImport(json(importBytes));
  if (input.phase === "enabled") throw new Error("REAL_COMPANY_GRANT_AND_MFA_EVIDENCE_REQUIRED");
  const candidateSha = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
  const runtimeFingerprintNow = await runtimeSourceFingerprint(root), verificationFingerprintNow = await verificationSourceFingerprint(root, runtimeFingerprintNow);
  if (candidateSha !== input.candidateSha || runtimeFingerprintNow !== input.runtimeFingerprint || verificationFingerprintNow !== input.verificationFingerprint)
    throw new Error("PRODUCT_MIGRATION_CURRENT_SOURCE_MISMATCH");
  const candidatePaths = ["src", "public", "scripts", "fixtures", "tests", "migrations/postgres", "ops/macos", "bun.lock", "package.json", "next.config.ts", "tsconfig.json", "postcss.config.mjs", ".github", "eslint.config.mjs"];
  // Ignore Git-ignored generated assets here: the source fingerprints above bind all
  // runtime and verification files, including ignored assets such as the local PDF.js
  // runtime. Tracked edits and non-ignored untracked source still require a clean commit.
  if (execFileSync("git", ["status", "--porcelain", "--untracked-files=all", "--", ...candidatePaths], { cwd: root, encoding: "utf8" }).trim())
    throw new Error("PRODUCT_MIGRATION_CLEAN_CANDIDATE_REQUIRED");
  const releaseDirectory = resolve(root, ".runtime/releases", input.release.id);
  if (!releaseDirectory.startsWith(resolve(root, ".runtime/releases") + sep)) throw new Error("PRODUCT_MIGRATION_RELEASE_PATH_INVALID");
  if (await realpath(releaseDirectory) !== releaseDirectory) throw new Error("PRODUCT_MIGRATION_RELEASE_PATH_INVALID");
  const releaseJsonPath = await assertCanonicalRepositoryFilePath(root, `.runtime/releases/${input.release.id}/release.json`, { requireFile: true });
  const releaseManifestPath = await assertCanonicalRepositoryFilePath(root, `.runtime/releases/${input.release.id}/release-files.json`, { requireFile: true });
  const releaseJson = json(await readFile(releaseJsonPath));
  const releaseManifestBytes = await readFile(releaseManifestPath);
  if (!record(releaseJson) || releaseJson.id !== input.release.id || releaseJson.sourceFingerprint !== input.runtimeFingerprint ||
    releaseJson.fileManifestSha256 !== input.release.fileManifestSha256 || createHash("sha256").update(releaseManifestBytes).digest("hex") !== input.release.fileManifestSha256)
    throw new Error("PRODUCT_MIGRATION_RELEASE_BINDING_MISMATCH");
  await verifyEngineeringCandidateInventory(releaseDirectory, input.release.fileManifestSha256);
  const nativeBinding = input.nativeReceipt ?? input.fullReceipt;
  if (!nativeBinding) throw new Error("PRODUCT_MIGRATION_NATIVE_INPUT_REQUIRED");
  const native = input.nativeReceipt
    ? await collectNative(root, nativeBinding, input)
    : await collectFullRegression(root, nativeBinding, input);
  const inputs: ProductMigrationEvidenceInput[] = [
    { kind: "import-manifest", path: inputBinding.path, sha256: inputBinding.sha256 },
    native.input,
    { kind: "compiled-release-json", path: `.runtime/releases/${input.release.id}/release.json`, sha256: createHash("sha256").update(await readFile(releaseJsonPath)).digest("hex") },
    { kind: "compiled-release-manifest", path: `.runtime/releases/${input.release.id}/release-files.json`, sha256: input.release.fileManifestSha256 },
  ];
  const observations: ProductMigrationEvidence["observations"][number][] = [...native.observations];
  const browserRuns: ProductMigrationEvidence["cleanup"]["browserRuns"] = [];
  const browserCaseEvidence = new Map<string, { observation: ProductMigrationEvidence["observations"][number]; run: string }>();
  for (const browserBinding of input.browserReceipts) {
    const browserParts = browserBinding.path.split("/");
    if (basename(browserBinding.path) !== "receipt.json" || browserParts.length !== 6 ||
      !/^\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}-\d{3}Z$/.test(browserParts[4]))
      throw new Error("PRODUCT_MIGRATION_OFFICIAL_BROWSER_RECEIPT_REQUIRED");
    const file = await readBoundEvidence(root, browserBinding, "browser"), receipt = browserReceiptRecord(json(file.bytes));
    if (!record(receipt) || receipt.runtimeFingerprint !== input.runtimeFingerprint || receipt.verificationFingerprint !== input.verificationFingerprint)
      throw new Error("PRODUCT_MIGRATION_BROWSER_CANDIDATE_MISMATCH");
    const matchedExpected = requiredProductMigrationCases(input.id, input.phase).filter(test => test.runner === "playwright" && receipt.suites.includes(test.suite));
    if (!matchedExpected.length) throw new Error("PRODUCT_MIGRATION_UNRELATED_BROWSER_RECEIPT");
    for (const expected of matchedExpected) {
      const witnessed = await browserWitness(root, receipt, expected, input, inputs);
      if (browserCaseEvidence.has(caseKey(expected))) throw new Error("DUPLICATE_PRODUCT_MIGRATION_BROWSER_CASE");
      browserCaseEvidence.set(caseKey(expected), witnessed);
      observations.push(witnessed.observation);
      browserRuns.push({ run: witnessed.run, passed: true, cleanupFailures: [], path: browserBinding.path, sha256: browserBinding.sha256, cases: [expected] });
    }
    inputs.push({ kind: "browser-run-receipt", path: browserBinding.path, sha256: browserBinding.sha256 });
  }
  assertCaseCoverage(observations, requiredProductMigrationCases(input.id, input.phase));
  const summary: ProductMigrationEvidence = validateProductMigrationEvidence({
    schema: productMigrationReceiptSchema,
    result: "passed",
    id: input.id,
    phase: input.phase,
    candidateSha: input.candidateSha,
    runtimeFingerprint: runtimeFingerprintNow,
    verificationFingerprint: verificationFingerprintNow,
    release: input.release,
    observations,
    evidenceInputs: inputs,
    cleanup: { native: { passed: true, path: nativeBinding.path, sha256: nativeBinding.sha256 }, browserRuns },
  });
  const manifestRelative = "docs/status/product-migration-receipts-v2.json";
  const lockRelative = `${manifestRelative}.lock`;
  const lockPath = await assertCanonicalRepositoryFilePath(root, lockRelative);
  try { await writeFile(lockPath, `${process.pid}\n`, { flag: "wx", mode: 0o600 }); }
  catch (error) { throw new Error("PRODUCT_MIGRATION_RECEIPT_MANIFEST_LOCKED", { cause: error }); }
  try {
    const manifestPath = await assertCanonicalRepositoryFilePath(root, manifestRelative, { requireFile: true });
    const manifestBytes = await readFile(manifestPath), manifest = validateProductMigrationReceiptManifest(json(manifestBytes));
    if (summary.phase === "verified" && !manifest.records.some(item => item.id === summary.id && item.phase === "implemented" &&
      item.candidateSha === summary.candidateSha &&
      item.runtimeFingerprint === summary.runtimeFingerprint && item.verificationFingerprint === summary.verificationFingerprint))
      throw new Error("PRODUCT_MIGRATION_SAME_SOURCE_IMPLEMENTATION_REQUIRED");
    const evidenceRelative = productMigrationReceiptEvidencePath(summary.candidateSha, summary.id, summary.phase);
    const evidencePath = await assertCanonicalRepositoryFilePath(root, evidenceRelative, { createParents: true });
    const evidenceBytes = Buffer.from(`${JSON.stringify(summary, null, 2)}\n`);
    const evidenceSha = createHash("sha256").update(evidenceBytes).digest("hex");
    try { await writeFile(evidencePath, evidenceBytes, { flag: "wx", mode: 0o600 }); }
    catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "EEXIST" ||
        !(await readProductMigrationReceiptEvidence(root, evidenceRelative, evidenceSha)).equals(evidenceBytes))
        throw new Error("PRODUCT_MIGRATION_EVIDENCE_IS_IMMUTABLE", { cause: error });
    }
    const reference: ProductMigrationReceiptReference = {
      id: summary.id,
      phase: summary.phase,
      candidateSha: summary.candidateSha,
      runtimeFingerprint: summary.runtimeFingerprint,
      verificationFingerprint: summary.verificationFingerprint,
      release: summary.release,
      path: evidenceRelative,
      sha256: evidenceSha,
    };
    const append = appendProductMigrationReceipt(manifest, reference);
    if (append.status === "already-imported") return { status: append.status, reference };
    const next = append.manifest;
    const temporaryRelative = `${manifestRelative}.tmp-${process.pid}-${randomUUID()}`;
    const temporary = await assertCanonicalRepositoryFilePath(root, temporaryRelative);
    let temporaryCreated = false;
    try {
      await writeFile(temporary, `${JSON.stringify(next, null, 2)}\n`, { flag: "wx", mode: 0o600 });
      temporaryCreated = true;
      await assertCanonicalRepositoryFilePath(root, temporaryRelative, { requireFile: true });
      await assertCanonicalRepositoryFilePath(root, manifestRelative, { requireFile: true });
      if (!(await readFile(manifestPath)).equals(manifestBytes)) throw new Error("PRODUCT_MIGRATION_RECEIPT_MANIFEST_CHANGED");
      await rename(temporary, manifestPath);
    } finally { if (temporaryCreated) await unlink(temporary).catch(() => {}); }
    return { status: "imported", reference };
  } finally {
    try {
      await assertCanonicalRepositoryFilePath(root, lockRelative, { requireFile: true });
      await unlink(lockPath);
    } catch { /* A replaced lock path stays fail-closed for the next importer. */ }
  }
}
