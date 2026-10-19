import { createHash, randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstat, mkdir, readFile, realpath, rename, unlink, writeFile } from "node:fs/promises";
import { basename, dirname, isAbsolute, resolve, sep } from "node:path";
import { browserCases, assertBrowserCasePassed } from "../testing/browser-results";
import { requiredNativePhases, type NativeCiReceipt } from "../testing/browser-sharding";
import { loadCatalogue } from "../testing/catalogue";
import { verifyEngineeringCandidateInventory } from "../testing/engineering/engineering-candidate-inventory";
import { fullRegressionRunIdFromPath, validateFullRegressionCases, validateFullRegressionRollbackArtifacts } from "../testing/full-regression-receipt";
import { internalRequiredCases, loadRequiredCases } from "../testing/required-case-inventory";
import { verifyUiBrowserEvidence, type UiBrowserEvidenceWitness } from "../testing/ui-evidence-witness";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
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

const uatAssetRegisterTitle = "asset register native state observations real native records, filters, validation, pending and draft guards across the appearance matrix";
const productHostAssetRegisterTitle = "isolated Assets native drawer and dirty product switch observations across the appearance matrix";
const verifiedBrowserCases: ProductMigrationCase[] = [
  ...(["desktop", "mobile"] as const).map(project => ({
    runner: "playwright" as const,
    suite: "shared.ui.uat-route-matrix.uat",
    file: "tests/uat/shared.ui/uat-route-matrix.spec.ts",
    title: uatAssetRegisterTitle,
    project,
  })),
  ...(["desktop", "mobile"] as const).map(project => ({
    runner: "playwright" as const,
    suite: "identity_access.product-hosts.e2e",
    file: "tests/e2e/identity_access/product-hosts.spec.ts",
    title: productHostAssetRegisterTitle,
    project,
  })),
];

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
  const browser = id === "assets/asset-register" ? verifiedBrowserCases : mapped.browser;
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

export function productMigrationReceiptMatchesCurrent(value: ProductMigrationReceiptReference, current: { candidateSha: string; runtimeFingerprint: string; verificationFingerprint: string }) {
  return value.candidateSha === current.candidateSha && value.runtimeFingerprint === current.runtimeFingerprint && value.verificationFingerprint === current.verificationFingerprint;
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

function sameCase(actual: unknown, expected: ProductMigrationCase) {
  return record(actual) && actual.runner === expected.runner && actual.suite === expected.suite && actual.file === expected.file && actual.title === expected.title &&
    (actual.project ?? undefined) === (expected.project ?? undefined);
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
  const validatedAll = validateFullRegressionCases(fullReceipt, fullBinding, allRequired);
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

async function browserWitness(root: string, value: Record<string, unknown> & { receipts: unknown[]; suites: unknown[] }, wanted: ProductMigrationCase, input: ProductMigrationImport, evidenceInputs: ProductMigrationEvidenceInput[]) {
  if (value.runtimeFingerprint !== input.runtimeFingerprint || value.verificationFingerprint !== input.verificationFingerprint) throw new Error("PRODUCT_MIGRATION_BROWSER_FINGERPRINT_MISMATCH");
  const matching = value.receipts.filter(item => sameCase(item, wanted) && record(item) && item.passed === true);
  if (matching.length !== 1) throw new Error("PRODUCT_MIGRATION_EXACT_BROWSER_CASE_REQUIRED");
  const result = matching[0] as Record<string, unknown>;
  if (typeof result.run !== "string" || !/^[a-f0-9]{12}$/.test(result.run) || typeof result.report !== "string" ||
    !record(result.cleanup) || result.cleanup.completed !== true || result.cleanup.ownedDirectoryAbsent !== true ||
    !record(result.uiEvidence) || result.uiEvidence.schema !== "erpx-ui-original-browser-leaves/v1" || result.uiEvidence.run !== result.run)
    throw new Error("PRODUCT_MIGRATION_UI_WITNESS_REQUIRED");
  const caseReportPath = resolve(root, result.report), browserRoot = resolve(root, ".scratch/dev/test-suites/browser");
  if (!caseReportPath.startsWith(browserRoot + sep) || basename(caseReportPath) !== "result.json" || basename(dirname(caseReportPath)) !== result.run)
    throw new Error("PRODUCT_MIGRATION_CASE_REPORT_PATH_REQUIRED");
  const witness = result.uiEvidence as unknown as UiBrowserEvidenceWitness;
  await verifyUiBrowserEvidence(root, witness);
  const stateFile = await readBoundEvidence(root, witness.state, "browser"), state = json(stateFile.bytes);
  if (!record(state) || state.run !== result.run || state.sourceFingerprint !== input.runtimeFingerprint || !exactRelease(state.release, input.release))
    throw new Error("PRODUCT_MIGRATION_COMPILED_RELEASE_WITNESS_MISMATCH");
  const reportFile = await readBoundEvidence(root, witness.report, "browser"), report = json(reportFile.bytes) as { suites?: unknown[] };
  const actual = browserCases(report as Parameters<typeof browserCases>[0]);
  if (actual.length !== 1 || actual[0].file !== wanted.file || actual[0].title !== wanted.title || actual[0].project !== wanted.project)
    throw new Error("PRODUCT_MIGRATION_PLAYWRIGHT_REPORT_IDENTITY_MISMATCH");
  assertBrowserCasePassed(report as Parameters<typeof assertBrowserCasePassed>[0], actual[0]);
  if (caseReportPath !== reportFile.absolute) throw new Error("PRODUCT_MIGRATION_PLAYWRIGHT_REPORT_BINDING_MISMATCH");
  const productHost = wanted.suite === "identity_access.product-hosts.e2e";
  if (productHost) {
    if (!witness.productHostLease || !record(result.productHostLease) || result.productHostLease.path !== witness.productHostLease.path ||
      result.productHostLease.sha256 !== witness.productHostLease.sha256) throw new Error("PRODUCT_MIGRATION_PRODUCT_HOST_WITNESS_REQUIRED");
    const leaseFile = await readBoundEvidence(root, witness.productHostLease, "browser"), lease = json(leaseFile.bytes);
    if (!record(lease) || lease.schema !== "erpx-owned-product-host-browser/v1" || lease.run !== result.run ||
      lease.sourceFingerprint !== input.runtimeFingerprint || lease.verificationFingerprint !== input.verificationFingerprint ||
      !exactRelease(lease.release, input.release) || lease.wholeProductAcceptance !== false || lease.readOnlyBusiness !== true ||
      lease.scopedBusinessWorkflow !== null || lease.isolatedNormalCommercial !== true ||
      !Array.isArray(lease.origins) || !lease.origins.includes("https://assets-dev-erpx.jesadakorn.com"))
      throw new Error("PRODUCT_MIGRATION_PRODUCT_HOST_SCOPE_MISMATCH");
    if (!record(result.productHostLease) || result.productHostLease.path !== witness.productHostLease.path || result.productHostLease.sha256 !== witness.productHostLease.sha256)
      throw new Error("PRODUCT_MIGRATION_PRODUCT_HOST_BINDING_MISMATCH");
  } else if (witness.productHostLease || result.productHostLease) throw new Error("UNEXPECTED_PRODUCT_HOST_WITNESS");
  for (const binding of [witness.report, witness.state, ...witness.observations, ...witness.screenshots, ...(witness.productHostLease ? [witness.productHostLease] : [])]) {
    const path = binding.path;
    if (!path.startsWith(".scratch/dev/test-suites/browser/")) throw new Error("PRODUCT_MIGRATION_UI_LEAF_PATH_REQUIRED");
    const kind = path === witness.report.path ? "browser-playwright-report" : path === witness.state.path ? "browser-state" :
      path === witness.productHostLease?.path ? "product-host-lease" : path.endsWith(".png") ? "browser-screenshot" : "browser-observation";
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
  if (execFileSync("git", ["status", "--porcelain", "--untracked-files=all", "--ignored=matching", "--", ...candidatePaths], { cwd: root, encoding: "utf8" }).trim())
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
    candidateSha,
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
      item.candidateSha === summary.candidateSha && item.runtimeFingerprint === summary.runtimeFingerprint && item.verificationFingerprint === summary.verificationFingerprint))
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
