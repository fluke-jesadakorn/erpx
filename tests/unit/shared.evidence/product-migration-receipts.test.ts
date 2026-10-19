import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { mkdtemp, mkdir, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  appendProductMigrationReceipt,
  assertCanonicalRepositoryFilePath,
  importProductMigrationReceipt,
  productMigrationReceiptManifestSchema,
  productMigrationBrowserCaseMatches,
  productMigrationReceiptMatchesCurrent,
  productMigrationReceiptSchema,
  requiredProductMigrationCases,
  productWorkflowReceiptCases,
  productWorkflowReceiptIds,
  productWorkflowReceiptMapping,
  readProductMigrationReceiptEvidence,
  validateProductMigrationEvidence,
  validateProductMigrationImport,
  validateProductMigrationReceiptManifest,
} from "../../../scripts/docs/product-migration-receipts";
import { requiredNativePhases } from "../../../scripts/testing/browser-sharding";
import { internalRequiredCases, loadRequiredCases, canonicalCaseFile } from "../../../scripts/testing/required-case-inventory";
import { serializeFullRegressionBrowserCases } from "../../../scripts/testing/browser-isolation-receipts";
import { assertBrowserCasePassed, browserCases } from "../../../scripts/testing/browser-results";
import { fullRegressionRunIdFromPath, requiredFullRegressionPhases, validateFullRegressionCases, validateFullRegressionReceipt, validateFullRegressionRollbackArtifacts } from "../../../scripts/testing/full-regression-receipt";
import { loadCatalogue } from "../../../scripts/testing/catalogue";
import { syntheticRollbackEvidenceValid } from "../../../scripts/testing/engineering/engineering-test-results";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../../../scripts/local/source-fingerprint";
import engineeringScenarios from "../../catalogue/engineering-scenarios.json";

const digest = (value: string) => value.repeat(64);
const rollbackReceipt = (runId: string, candidateReleaseId: string, fileManifestSha256: string) => ({
  scope: "isolated-synthetic-prior-release",
  candidateReleaseId,
  fixtureCandidateReleaseId: "2026-10-03T00-00-00-000Z",
  previousReleaseId: "2026-10-02T00-00-00-000Z",
  fileManifestSha256,
  candidateArtifactSha256: digest("5"),
  previousArtifactSha256: digest("6"),
  candidateServerSha256: digest("7"),
  previousServerSha256: digest("8"),
  evidencePath: ".scratch/dev/test-suites/full/" + runId + "/rollback-evidence/encrypted-recovery.json",
  evidenceSha256: digest("9"),
  testReportPath: ".scratch/dev/test-suites/full/" + runId + "/rollback-vitest.json",
  testReportSha256: digest("a"),
  switchedToPrevious: true as const,
  databaseAndFilesUnchanged: true as const,
  databaseStateBeforeSha256: digest("b"),
  databaseStateAfterSha256: digest("b"),
  privateFilesStateBeforeSha256: digest("c"),
  privateFilesStateAfterSha256: digest("c"),
});
const input = (kind: string, path: string) => ({ kind, path, sha256: digest("a") });
const implementationCase = {
  runner: "vitest" as const,
  suite: "assets.asset-maintenance.integration",
  file: "tests/integration/assets/asset-maintenance.test.ts",
  title: "tracks register, placement, condition and independently reviewed retirement",
};
const rollbackTargetTitle = "encrypts a complete isolated snapshot, restores immutable money and files, and rolls back only compatible releases";
const verifiedCases = requiredProductMigrationCases("assets/asset-register", "verified");

function fullRequiredCases(root = process.cwd()) {
  const suites = loadCatalogue(root).suites;
  return internalRequiredCases(loadRequiredCases(root), suites).map(test => {
    const owners = suites.filter(suite => suite.files.includes(test.file));
    if (owners.length !== 1) throw new Error(`FULL_RECEIPT_TEST_SUITE_BINDING_REQUIRED:${test.file}`);
    return { ...test, suite: owners[0].id };
  });
}

function rollbackArtifacts(runId: string, candidateReleaseId: string, fileManifestSha256: string, sourceFingerprint: string, root: string) {
  const summary = rollbackReceipt(runId, candidateReleaseId, fileManifestSha256);
  const evidence = {
    compiledCandidate: true,
    sourceFingerprint,
    fileManifestSha256,
    candidateReleaseId: summary.fixtureCandidateReleaseId,
    rollback: {
      scope: summary.scope,
      candidateReleaseId: summary.fixtureCandidateReleaseId,
      previousReleaseId: summary.previousReleaseId,
      candidateArtifactSha256: summary.candidateArtifactSha256,
      previousArtifactSha256: summary.previousArtifactSha256,
      candidateServerSha256: summary.candidateServerSha256,
      previousServerSha256: summary.previousServerSha256,
      switchedToPrevious: true,
      databaseAndFilesUnchanged: true,
      databaseStateBeforeSha256: summary.databaseStateBeforeSha256,
      databaseStateAfterSha256: summary.databaseStateAfterSha256,
      privateFilesStateBeforeSha256: summary.privateFilesStateBeforeSha256,
      privateFilesStateAfterSha256: summary.privateFilesStateAfterSha256,
    },
  };
  const report = {
    success: true,
    testResults: [{
      name: join(root, "tests/integration/shared.evidence/engineering-recovery.test.ts"),
      assertionResults: [{ fullName: rollbackTargetTitle, status: "passed" }],
    }],
  };
  const evidenceBytes = Buffer.from(JSON.stringify(evidence) + "\n");
  const reportBytes = Buffer.from(JSON.stringify(report) + "\n");
  return {
    rollback: {
      ...summary,
      evidenceSha256: createHash("sha256").update(evidenceBytes).digest("hex"),
      testReportSha256: createHash("sha256").update(reportBytes).digest("hex"),
    },
    evidenceBytes,
    reportBytes,
  };
}

async function writeFixtureFile(root: string, path: string, contents: string) {
  const absolute = join(root, path);
  await mkdir(dirname(absolute), { recursive: true });
  await writeFile(absolute, contents);
}

async function migrationImportFixture() {
  const root = await realpath(await mkdtemp(join(tmpdir(), "erpx-product-import-")));
  for (const directory of ["src", "public", "scripts", "fixtures", "tests/support", "migrations/postgres", "ops/macos/clamav", ".github/workflows", ".github/actions"])
    await mkdir(join(root, directory), { recursive: true });
  const sourceFiles = [
    "bun.lock", "package.json", "next.config.ts", "tsconfig.json", "postcss.config.mjs", "eslint.config.mjs",
    "tests/config/tsconfig.json", "tests/config/vitest.config.mts", "tests/config/vitest.unit.config.mts",
    "tests/config/vitest.integration.config.mts", "tests/config/playwright.config.ts",
    "tests/catalogue/engineering-scenarios.json", "tests/catalogue/legal-uat-engineering-cases.json",
    "tests/catalogue/engineering-gate-scenarios.json", "tests/catalogue/engineering-attachment-owner-cases.json",
    "tests/catalogue/uat-route-inventory.json", "tests/catalogue/required-cases.json", "tests/catalogue/required-browser-cases.json",
    "tests/catalogue/product-workflow-receipts.json",
    "tests/catalogue/suite-manifest.json", "ops/macos/install-attachment-runtime.py",
    "ops/macos/attachment-rollout.md", "ops/macos/erpxctl.py", "ops/macos/offsite.py", "ops/macos/recovery_status.py",
  ];
  for (const path of sourceFiles) {
    const content = path.startsWith("tests/catalogue/") ? readFileSync(resolve(process.cwd(), path), "utf8") : "fixture\n";
    await writeFixtureFile(root, path, content);
  }
  await writeFixtureFile(root, ".gitignore", "/public/vendor/pdfjs-*/\n");
  await writeFixtureFile(root, "public/vendor/pdfjs-fixture/pdf.worker.min.mjs", "fingerprinted ignored runtime asset\n");
  await writeFixtureFile(root, "docs/status/product-migration-receipts-v2.json", JSON.stringify({ schema: productMigrationReceiptManifestSchema, records: [] }, null, 2) + "\n");
  execFileSync("git", ["init", "-q"], { cwd: root });
  execFileSync("git", ["config", "user.name", "Receipt Fixture"], { cwd: root });
  execFileSync("git", ["config", "user.email", "receipt-fixture@example.test"], { cwd: root });
  execFileSync("git", ["add", "."], { cwd: root });
  execFileSync("git", ["commit", "-qm", "receipt fixture"], { cwd: root });
  const candidateSha = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
  const runtimeFingerprint = await runtimeSourceFingerprint(root);
  const verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  const releaseId = "2026-10-19T12-00-00-000Z";
  const releaseFiles = Buffer.from("{}\n");
  const fileManifestSha256 = createHash("sha256").update(releaseFiles).digest("hex");
  await writeFixtureFile(root, `.runtime/releases/${releaseId}/release-files.json`, releaseFiles.toString("utf8"));
  await writeFixtureFile(root, `.runtime/releases/${releaseId}/release.json`, JSON.stringify({ id: releaseId, sourceFingerprint: runtimeFingerprint, fileManifestSha256 }) + "\n");

  const ids = ["assets/asset-register", "finance/journals"];
  const nativeCases = [...new Map(ids.flatMap(id => requiredProductMigrationCases(id, "implemented")).map(test => [
    JSON.stringify([test.runner, test.file, test.title, test.project ?? ""]),
    { runner: test.runner, file: test.file, title: test.title, ...(test.project ? { project: test.project } : {}), status: "passed" },
  ])).values()];
  const nativePath = ".scratch/dev/test-suites/ci/native.json";
  const nativeReceipt = {
    version: 1,
    kind: "native",
    candidate: candidateSha,
    runtimeFingerprint,
    verificationFingerprint,
    release: { id: releaseId, fileManifestSha256 },
    passed: true,
    phases: requiredNativePhases.map(phase => ({ phase, passed: true })),
    cases: nativeCases,
  };
  const nativeBytes = Buffer.from(JSON.stringify(nativeReceipt) + "\n");
  await writeFixtureFile(root, nativePath, nativeBytes.toString("utf8"));
  return {
    root,
    ids,
    nativeBinding: { path: nativePath, sha256: createHash("sha256").update(nativeBytes).digest("hex") },
    candidateSha,
    runtimeFingerprint,
    verificationFingerprint,
    release: { id: releaseId, fileManifestSha256 },
  };
}

async function writeFullRegressionFixture(root: string, runId: string, binding: {
  candidate: string;
  runtimeFingerprint: string;
  verificationFingerprint: string;
  release: { id: string; fileManifestSha256: string };
}, cases = fullRequiredCases(root)) {
  const rollback = rollbackArtifacts(runId, binding.release.id, binding.release.fileManifestSha256, binding.runtimeFingerprint, root);
  const receipt = {
    version: 1,
    kind: "full-regression",
    scope: "internal-regression",
    runId,
    commit: binding.candidate,
    candidate: binding.candidate,
    runtimeFingerprint: binding.runtimeFingerprint,
    verificationFingerprint: binding.verificationFingerprint,
    release: binding.release,
    rollback: rollback.rollback,
    passed: true,
    phases: requiredFullRegressionPhases.map(phase => ({ phase, passed: true })),
    cases: cases.map(test => ({ ...test, status: "passed" })),
    providerRehearsals: [],
    providerSuites: [],
  };
  const runDirectory = `.scratch/dev/test-suites/full/${runId}`;
  const path = `${runDirectory}/receipt.json`;
  await writeFixtureFile(root, `${runDirectory}/rollback-vitest.json`, rollback.reportBytes.toString("utf8"));
  await writeFixtureFile(root, `${runDirectory}/rollback-evidence/encrypted-recovery.json`, rollback.evidenceBytes.toString("utf8"));
  const bytes = Buffer.from(JSON.stringify(receipt) + "\n");
  await writeFixtureFile(root, path, bytes.toString("utf8"));
  return { receipt, path, bytes };
}

function receipt(phase: "implemented" | "verified" = "implemented") {
  const cases = requiredProductMigrationCases("assets/asset-register", phase);
  const browserCases = cases.filter(test => test.runner === "playwright");
  const browserRuns = phase === "verified" ? browserCases.map((test, index) => ({
    run: `${index + 1}`.padStart(12, "0"),
    passed: true,
    cleanupFailures: [],
    path: `.scratch/dev/test-suites/browser/2026-10-19T12-00-00-000Z/receipt.json`,
    sha256: digest("b"),
    cases: [test],
  })) : [];
  const evidenceInputs = [input("native-ci", ".scratch/dev/test-suites/ci/native.json"),
    ...new Map(browserRuns.map(run => [run.path, { kind: "browser-run-receipt", path: run.path, sha256: run.sha256 }])).values()];
  return {
    schema: productMigrationReceiptSchema,
    result: "passed",
    id: "assets/asset-register",
    phase,
    candidateSha: "1".repeat(40),
    runtimeFingerprint: digest("2"),
    verificationFingerprint: digest("3"),
    release: { id: "2026-10-19T12-00-00-000Z", fileManifestSha256: digest("4") },
    observations: cases.map(test => ({ ...test, status: "passed" })),
    evidenceInputs,
    cleanup: {
      native: { passed: true, path: ".scratch/dev/test-suites/ci/native.json", sha256: digest("a") },
      browserRuns,
    },
  };
}

describe("current-source product migration receipt v2", () => {
  it("keeps current-source receipt status on runtime and verification fingerprints", () => {
    const reference = {
      id: "apps:dirty-product-switch",
      phase: "implemented" as const,
      candidateSha: "1".repeat(40),
      runtimeFingerprint: digest("2"),
      verificationFingerprint: digest("3"),
      release: { id: "release-1", fileManifestSha256: digest("4") },
      path: "docs/testing/evidence/product-migration-v2/current-implemented.json",
      sha256: digest("5"),
    };

    expect(productMigrationReceiptMatchesCurrent(reference, {
      runtimeFingerprint: digest("2"),
      verificationFingerprint: digest("3"),
    })).toBe(true);
    expect(productMigrationReceiptMatchesCurrent(reference, {
      runtimeFingerprint: digest("f"),
      verificationFingerprint: digest("3"),
    })).toBe(false);
    expect(productMigrationReceiptMatchesCurrent(reference, {
      runtimeFingerprint: digest("2"),
      verificationFingerprint: digest("f"),
    })).toBe(false);
  });

  it("canonicalizes raw browser receipt and report paths only through the registered suite", () => {
    const expected = requiredProductMigrationCases("apps:dirty-product-switch", "verified")
      .find(test => test.runner === "playwright" && test.project === "desktop")!;
    const suite = loadCatalogue().suites.find(test => test.id === expected.suite)!;
    const rawFile = "e2e/identity_access/product-hosts.spec.ts";
    const report = { suites: [{ specs: [{ file: rawFile, title: expected.title, tests: [{
      projectName: expected.project!, expectedStatus: "passed", status: "expected", results: [{ status: "passed" }],
    }] }] }] };
    const actual = browserCases(report)[0]!;
    const rawReceiptCase = { ...expected, file: rawFile, passed: true };
    const reportCase = { runner: "playwright", suite: expected.suite, ...actual };

    expect(suite.files).toContain(expected.file);
    expect(productMigrationBrowserCaseMatches(rawReceiptCase, expected, suite.files)).toBe(true);
    expect(productMigrationBrowserCaseMatches(reportCase, expected, suite.files)).toBe(true);
    expect(productMigrationBrowserCaseMatches({ ...rawReceiptCase, file: "e2e/identity_access/missing.spec.ts" }, expected, suite.files)).toBe(false);
    expect(productMigrationBrowserCaseMatches({ ...rawReceiptCase, title: `${expected.title} extra` }, expected, suite.files)).toBe(false);
    expect(productMigrationBrowserCaseMatches({ ...rawReceiptCase, project: "mobile" }, expected, suite.files)).toBe(false);
    expect(actual.title).toBe(expected.title);
    expect(actual.project).toBe(expected.project);
    expect(() => assertBrowserCasePassed(report, actual)).not.toThrow();
  });

  it("serializes full browser receipts with the registered suite file identity", () => {
    const suite = loadCatalogue().suites.find(test => test.id === "identity_access.product-hosts.e2e")!;
    const title = "isolated product-host switch preserves its exact browser identity";
    const [observed] = serializeFullRegressionBrowserCases([{
      suite: suite.id,
      file: "e2e/identity_access/product-hosts.spec.ts",
      title,
      project: "desktop",
      passed: true,
      required: false,
    }], [suite], "e2e");

    expect(observed).toEqual({
      runner: "playwright",
      suite: suite.id,
      file: "tests/e2e/identity_access/product-hosts.spec.ts",
      title,
      project: "desktop",
      status: "passed",
      required: false,
    });
  });

  it("binds exactly one native CI or run-specific full regression input", () => {
    const runId = "2026-10-19T12-00-00-000Z";
    const envelope = {
      schema: "erpx-product-migration-import/v2",
      id: "assets/asset-register",
      phase: "implemented",
      candidateSha: "1".repeat(40),
      runtimeFingerprint: digest("2"),
      verificationFingerprint: digest("3"),
      release: { id: runId, fileManifestSha256: digest("4") },
      fullReceipt: { path: `.scratch/dev/test-suites/full/${runId}/receipt.json`, sha256: digest("5") },
      browserReceipts: [],
    };

    const parsedFull = validateProductMigrationImport(envelope);
    expect(parsedFull.fullReceipt).toEqual(envelope.fullReceipt);
    expect(parsedFull.nativeReceipt).toBeUndefined();
    const withoutFull: Record<string, unknown> = { ...envelope };
    delete withoutFull.fullReceipt;
    const parsedNative = validateProductMigrationImport({ ...withoutFull, nativeReceipt: { path: ".scratch/dev/test-suites/ci/native.json", sha256: digest("6") } });
    expect(parsedNative.nativeReceipt).toEqual({ path: ".scratch/dev/test-suites/ci/native.json", sha256: digest("6") });
    expect(parsedNative.fullReceipt).toBeUndefined();
    expect(() => validateProductMigrationImport({ ...envelope, nativeReceipt: { path: ".scratch/dev/test-suites/ci/native.json", sha256: digest("6") } }))
      .toThrow("INVALID_PRODUCT_MIGRATION_IMPORT_V2");
    expect(() => validateProductMigrationImport(withoutFull)).toThrow("INVALID_PRODUCT_MIGRATION_IMPORT_V2");
  });

  it("accepts only a run-bound successful full regression receipt for the exact native workflow case", () => {
    const runId = "2026-10-19T12-00-00-000Z";
    const binding = {
      runId,
      candidate: "1".repeat(40),
      runtimeFingerprint: digest("2"),
      verificationFingerprint: digest("3"),
      release: { id: runId, fileManifestSha256: digest("4") },
    };
    const receipt = {
      version: 1,
      kind: "full-regression",
      scope: "internal-regression",
      runId,
      commit: binding.candidate,
      candidate: binding.candidate,
      runtimeFingerprint: binding.runtimeFingerprint,
      verificationFingerprint: binding.verificationFingerprint,
      release: binding.release,
      rollback: rollbackReceipt(runId, binding.release.id, binding.release.fileManifestSha256),
      passed: true,
      phases: requiredFullRegressionPhases.map(phase => ({ phase, passed: true })),
      cases: [{ ...implementationCase, status: "passed" }],
      providerRehearsals: [],
      providerSuites: [],
    };
    const path = `.scratch/dev/test-suites/full/${runId}/receipt.json`;

    expect(fullRegressionRunIdFromPath(path)).toBe(runId);
    expect(fullRegressionRunIdFromPath(".scratch/dev/test-suites/full/other/receipt.json")).toBeUndefined();
    expect(validateFullRegressionReceipt(receipt, binding, { ...implementationCase, status: "passed" })).toMatchObject(implementationCase);
    const journalCase = requiredProductMigrationCases("finance/journals", "implemented")[0];
    const twoWorkflowReceipt = { ...receipt, cases: [receipt.cases[0], { ...journalCase, status: "passed" }] };
    expect(validateFullRegressionCases(twoWorkflowReceipt, binding, [implementationCase, journalCase])).toHaveLength(2);
    expect(() => validateFullRegressionCases(receipt, binding, [implementationCase, journalCase]))
      .toThrow("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");

    const missingPhase = structuredClone(receipt);
    missingPhase.phases = missingPhase.phases.filter(item => item.phase !== "integration");
    expect(() => validateFullRegressionReceipt(missingPhase, binding, implementationCase)).toThrow("PRODUCT_MIGRATION_FULL_RUN_PHASES_REQUIRED");

    const duplicatePhase = structuredClone(receipt);
    duplicatePhase.phases[1] = { phase: "architecture", passed: true };
    expect(() => validateFullRegressionReceipt(duplicatePhase, binding, implementationCase)).toThrow("PRODUCT_MIGRATION_FULL_RUN_PHASES_REQUIRED");

    const wrongRelease = structuredClone(receipt);
    wrongRelease.release.fileManifestSha256 = digest("5");
    expect(() => validateFullRegressionReceipt(wrongRelease, binding, implementationCase)).toThrow("PRODUCT_MIGRATION_FULL_RUN_CANDIDATE_MISMATCH");

    const missingRollback = { ...structuredClone(receipt), rollback: null };
    expect(() => validateFullRegressionReceipt(missingRollback, binding, implementationCase)).toThrow("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_REQUIRED");

    const misleadingRollback = structuredClone(receipt);
    misleadingRollback.rollback.previousArtifactSha256 = misleadingRollback.rollback.candidateArtifactSha256;
    expect(() => validateFullRegressionReceipt(misleadingRollback, binding, implementationCase)).toThrow("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_REQUIRED");

    const wrongAssetCase = structuredClone(receipt);
    wrongAssetCase.cases[0].title += " (near match)";
    expect(() => validateFullRegressionReceipt(wrongAssetCase, binding, implementationCase)).toThrow("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");

    const wrongSuite = structuredClone(receipt);
    wrongSuite.cases[0].suite = "assets.different.integration";
    expect(() => validateFullRegressionReceipt(wrongSuite, binding, implementationCase)).toThrow("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");

    expect(() => validateFullRegressionReceipt(receipt, { ...binding, runId: "2026-10-19T12-01-00-000Z" }, implementationCase))
      .toThrow("PRODUCT_MIGRATION_FULL_RUN_CANDIDATE_MISMATCH");
  });

  it("accepts a full browser case only after canonicalizing its registered report path", () => {
    const runId = "2026-10-19T12-00-00-000Z";
    const binding = {
      runId,
      candidate: "1".repeat(40),
      runtimeFingerprint: digest("2"),
      verificationFingerprint: digest("3"),
      release: { id: runId, fileManifestSha256: digest("4") },
    };
    const base = {
      version: 1,
      kind: "full-regression",
      scope: "internal-regression",
      runId,
      commit: binding.candidate,
      candidate: binding.candidate,
      runtimeFingerprint: binding.runtimeFingerprint,
      verificationFingerprint: binding.verificationFingerprint,
      release: binding.release,
      rollback: rollbackReceipt(runId, binding.release.id, binding.release.fileManifestSha256),
      passed: true,
      phases: requiredFullRegressionPhases.map(phase => ({ phase, passed: true })),
      cases: [{ ...implementationCase, status: "passed" }],
      providerRehearsals: [],
      providerSuites: [],
    };
    const expected = requiredProductMigrationCases("apps:dirty-product-switch", "verified")
      .find(test => test.runner === "playwright" && test.project === "desktop")!;
    const suite = loadCatalogue().suites.find(test => test.id === expected.suite)!;
    const raw = { ...expected, file: "e2e/identity_access/product-hosts.spec.ts", status: "passed" };
    const canonical = { ...raw, file: canonicalCaseFile(raw.file, suite.files) };

    expect(() => validateFullRegressionReceipt({ ...base, cases: [...base.cases, raw] }, binding, expected))
      .toThrow("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");
    expect(validateFullRegressionReceipt({ ...base, cases: [...base.cases, canonical] }, binding, expected))
      .toMatchObject({ ...expected, status: "passed" });
  });

  it("requires every registered internal case exactly once with its owning suite in full-run imports", () => {
    const runId = "2026-10-19T12-00-00-000Z";
    const binding = {
      runId,
      candidate: "1".repeat(40),
      runtimeFingerprint: digest("2"),
      verificationFingerprint: digest("3"),
      release: { id: runId, fileManifestSha256: digest("4") },
    };
    const required = fullRequiredCases();
    const complete = {
      version: 1,
      kind: "full-regression",
      scope: "internal-regression",
      runId,
      commit: binding.candidate,
      candidate: binding.candidate,
      runtimeFingerprint: binding.runtimeFingerprint,
      verificationFingerprint: binding.verificationFingerprint,
      release: binding.release,
      rollback: rollbackReceipt(runId, binding.release.id, binding.release.fileManifestSha256),
      passed: true,
      phases: requiredFullRegressionPhases.map(phase => ({ phase, passed: true })),
      cases: required.map(test => ({ ...test, status: "passed" })),
      providerRehearsals: [],
      providerSuites: [],
    };

    expect(validateFullRegressionCases(complete, binding, required)).toHaveLength(required.length);
    const incomplete = { ...complete, cases: [{ ...implementationCase, status: "passed" }] };
    expect(() => validateFullRegressionCases(incomplete, binding, required)).toThrow("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");

    const first = required[0];
    const duplicate = { ...complete, cases: [...complete.cases, { ...first, suite: `${first.suite}.wrong`, status: "passed" }] };
    expect(() => validateFullRegressionCases(duplicate, binding, required)).toThrow("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");

    const wrongOwner = { ...complete, cases: complete.cases.map(test => test === complete.cases[0] ? { ...test, suite: `${test.suite}.wrong` } : test) };
    expect(() => validateFullRegressionCases(wrongOwner, binding, required)).toThrow("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");
  });

  it("validates full-run rollback artifacts and rejects hash, test, evidence, and claim mismatches", () => {
    const runId = "2026-10-19T12-00-00-000Z";
    const binding = {
      runId,
      candidate: "1".repeat(40),
      runtimeFingerprint: digest("2"),
      verificationFingerprint: digest("3"),
      release: { id: runId, fileManifestSha256: digest("4") },
    };
    const artifacts = rollbackArtifacts(runId, binding.release.id, binding.release.fileManifestSha256, binding.runtimeFingerprint, process.cwd());
    const receipt = { runId, rollback: artifacts.rollback };
    const evidence = {
      reportBytes: artifacts.reportBytes,
      evidenceBytes: artifacts.evidenceBytes,
    };

    expect(() => validateFullRegressionRollbackArtifacts(receipt, binding, evidence, process.cwd())).not.toThrow();
    expect(() => validateFullRegressionRollbackArtifacts(receipt, binding, {
      ...evidence,
      reportBytes: Buffer.from("{}\n"),
    }, process.cwd())).toThrow("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_DIGEST_MISMATCH");

    const failedReport = Buffer.from(JSON.stringify({
      success: true,
      testResults: [{
        name: join(process.cwd(), "tests/integration/shared.evidence/engineering-recovery.test.ts"),
        assertionResults: [{ fullName: rollbackTargetTitle, status: "failed" }],
      }],
    }) + "\n");
    const failedReceipt = { runId, rollback: { ...artifacts.rollback, testReportSha256: createHash("sha256").update(failedReport).digest("hex") } };
    expect(() => validateFullRegressionRollbackArtifacts(failedReceipt, binding, { ...evidence, reportBytes: failedReport }, process.cwd()))
      .toThrow("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_TEST_REQUIRED");

    const changedEvidence = Buffer.from(JSON.stringify({
      ...JSON.parse(artifacts.evidenceBytes.toString("utf8")),
      fileManifestSha256: digest("f"),
    }) + "\n");
    const changedEvidenceReceipt = { runId, rollback: { ...artifacts.rollback, evidenceSha256: createHash("sha256").update(changedEvidence).digest("hex") } };
    expect(() => validateFullRegressionRollbackArtifacts(changedEvidenceReceipt, binding, { ...evidence, evidenceBytes: changedEvidence }, process.cwd()))
      .toThrow("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_EVIDENCE_REQUIRED");

    const forgedClaim = { runId, rollback: { ...artifacts.rollback, previousReleaseId: "forged-prior-release" } };
    expect(() => validateFullRegressionRollbackArtifacts(forgedClaim, binding, evidence, process.cwd()))
      .toThrow("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_BINDING_MISMATCH");
  });

  it("accepts only candidate-bound evidence for a distinct isolated synthetic prior-release switch", () => {
    const sourceFingerprint = digest("1"), fileManifestSha256 = digest("2"), runId = "2026-10-19T12-00-00-000Z";
    const summary = rollbackReceipt(runId, runId, fileManifestSha256);
    const evidence = {
      compiledCandidate: true,
      sourceFingerprint,
      fileManifestSha256,
      candidateReleaseId: summary.fixtureCandidateReleaseId,
      rollback: {
        scope: summary.scope,
        candidateReleaseId: summary.fixtureCandidateReleaseId,
        previousReleaseId: summary.previousReleaseId,
        candidateArtifactSha256: summary.candidateArtifactSha256,
        previousArtifactSha256: summary.previousArtifactSha256,
        candidateServerSha256: summary.candidateServerSha256,
        previousServerSha256: summary.previousServerSha256,
        switchedToPrevious: true,
        databaseAndFilesUnchanged: true,
        databaseStateBeforeSha256: summary.databaseStateBeforeSha256,
        databaseStateAfterSha256: summary.databaseStateAfterSha256,
        privateFilesStateBeforeSha256: summary.privateFilesStateBeforeSha256,
        privateFilesStateAfterSha256: summary.privateFilesStateAfterSha256,
      },
    };
    expect(syntheticRollbackEvidenceValid(evidence, sourceFingerprint, fileManifestSha256)).toBe(true);
    expect(syntheticRollbackEvidenceValid({ ...evidence, fileManifestSha256: digest("3") }, sourceFingerprint, fileManifestSha256)).toBe(false);
    expect(syntheticRollbackEvidenceValid({ ...evidence, rollback: { ...evidence.rollback, previousServerSha256: evidence.rollback.candidateServerSha256 } },
      sourceFingerprint, fileManifestSha256)).toBe(false);
  });

  it("accepts only the exact Assets owner lifecycle case for implementation", () => {
    expect(requiredProductMigrationCases("assets/asset-register", "implemented")).toEqual([implementationCase]);
    expect(validateProductMigrationEvidence(receipt())).toMatchObject({
      schema: productMigrationReceiptSchema,
      id: "assets/asset-register",
      phase: "implemented",
      observations: [{ ...implementationCase, status: "passed" }],
    });
    for (const mutation of [
      (value: ReturnType<typeof receipt>) => { value.observations[0].title += " (similar)"; },
      (value: ReturnType<typeof receipt>) => { value.observations[0].file = "tests/unit/assets/asset-maintenance.test.ts"; },
      (value: ReturnType<typeof receipt>) => { value.candidateSha = "wrong"; },
      (value: ReturnType<typeof receipt>) => { value.cleanup.native.sha256 = digest("f"); },
    ]) {
      const changed = receipt();
      mutation(changed);
      expect(() => validateProductMigrationEvidence(changed)).toThrow();
    }
  });

  it("requires both UAT and product-host projects with clean browser teardown", () => {
    const verified = receipt("verified");
    expect(verifiedCases).toHaveLength(5);
    expect(validateProductMigrationEvidence(verified).observations).toHaveLength(5);

    const missingMobile = receipt("verified");
    missingMobile.observations = missingMobile.observations.filter(test => !(test.runner === "playwright" && test.project === "mobile"));
    expect(() => validateProductMigrationEvidence(missingMobile)).toThrow("PRODUCT_MIGRATION_EXACT_CASE_COVERAGE_REQUIRED");

    const wrongHost = receipt("verified");
    wrongHost.observations = wrongHost.observations.map(test => test.runner === "playwright" && test.suite === "identity_access.product-hosts.e2e"
      ? { ...test, title: `${test.title} outside the exact product host case` }
      : test);
    expect(() => validateProductMigrationEvidence(wrongHost)).toThrow("PRODUCT_MIGRATION_EXACT_CASE_COVERAGE_REQUIRED");

    const dirtyCleanup = receipt("verified");
    (dirtyCleanup.cleanup.browserRuns[0].cleanupFailures as string[]).push("owned case was not removed");
    expect(() => validateProductMigrationEvidence(dirtyCleanup)).toThrow("PRODUCT_MIGRATION_BROWSER_CLEANUP_REQUIRED");
  });

  it("treats an exact receipt re-import as idempotent and rejects a conflicting replacement", () => {
    const reference = {
      id: "assets/asset-register",
      phase: "implemented" as const,
      candidateSha: "1".repeat(40),
      runtimeFingerprint: digest("2"),
      verificationFingerprint: digest("3"),
      release: { id: "release-1", fileManifestSha256: digest("4") },
      path: "docs/testing/evidence/product-migration-v2/receipt.json",
      sha256: digest("5"),
    };
    const manifest = validateProductMigrationReceiptManifest({ schema: productMigrationReceiptManifestSchema, records: [reference] });

    expect(appendProductMigrationReceipt(manifest, reference)).toEqual({ status: "already-imported", reference });
    expect(() => appendProductMigrationReceipt(manifest, { ...reference, sha256: digest("6") })).toThrow("PRODUCT_MIGRATION_RECEIPT_CONFLICT");
  });

  it("imports separate workflows to distinct durable paths and keeps exact re-import idempotent", async () => {
    const fixture = await migrationImportFixture();
    try {
      expect(execFileSync("git", ["status", "--porcelain", "--ignored=matching", "--", "public"], { cwd: fixture.root, encoding: "utf8" }))
        .toContain("public/vendor/pdfjs-fixture/");
      const inputPaths = fixture.ids.map(id => `.scratch/dev/test-suites/product-migration/${id.replaceAll("/", "-")}.json`);
      for (let index = 0; index < fixture.ids.length; index += 1) {
        const input = {
          schema: "erpx-product-migration-import/v2",
          id: fixture.ids[index],
          phase: "implemented",
          candidateSha: fixture.candidateSha,
          runtimeFingerprint: fixture.runtimeFingerprint,
          verificationFingerprint: fixture.verificationFingerprint,
          release: fixture.release,
          nativeReceipt: fixture.nativeBinding,
          browserReceipts: [],
        };
        await writeFixtureFile(fixture.root, inputPaths[index], JSON.stringify(input) + "\n");
      }

      const first = await importProductMigrationReceipt(fixture.root, inputPaths[0]);
      const second = await importProductMigrationReceipt(fixture.root, inputPaths[1]);
      expect(first.status).toBe("imported");
      expect(second.status).toBe("imported");
      expect(first.reference.candidateSha).toBe(fixture.candidateSha);
      expect(JSON.parse(readFileSync(join(fixture.root, first.reference.path), "utf8")).candidateSha).toBe(fixture.candidateSha);

      const manifestPath = join(fixture.root, "docs/status/product-migration-receipts-v2.json");
      const manifest = JSON.parse(readFileSync(manifestPath, "utf8")) as { records: { id: string; path: string }[] };
      expect(manifest.records.map(row => row.id)).toEqual(fixture.ids);
      expect(new Set(manifest.records.map(row => row.path)).size).toBe(2);
      expect(manifest.records.every(row => row.path.startsWith("docs/testing/evidence/product-migration-v2/") && row.path.endsWith("-implemented.json"))).toBe(true);

      const repeated = await importProductMigrationReceipt(fixture.root, inputPaths[0]);
      expect(repeated.status).toBe("already-imported");
      expect(repeated.reference.path).toBe(manifest.records[0].path);
      expect(JSON.parse(readFileSync(manifestPath, "utf8")).records).toHaveLength(2);
    } finally {
      await rm(fixture.root, { recursive: true, force: true });
    }
  });

  it("imports a product case from a candidate-bound full regression receipt", async () => {
    const fixture = await migrationImportFixture();
    try {
      const runId = "2026-10-19T12-01-00-000Z";
      const full = await writeFullRegressionFixture(fixture.root, runId, {
        candidate: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
      });
      const importPath = `.scratch/dev/test-suites/product-migration/${fixture.ids[0].replaceAll("/", "-")}-full.json`;
      await writeFixtureFile(fixture.root, importPath, JSON.stringify({
        schema: "erpx-product-migration-import/v2",
        id: fixture.ids[0],
        phase: "implemented",
        candidateSha: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
        fullReceipt: { path: full.path, sha256: createHash("sha256").update(full.bytes).digest("hex") },
        browserReceipts: [],
      }) + "\n");

      await expect(importProductMigrationReceipt(fixture.root, importPath)).resolves.toMatchObject({ status: "imported" });

      const incompleteRunId = "2026-10-19T12-02-00-000Z";
      const incomplete = await writeFullRegressionFixture(fixture.root, incompleteRunId, {
        candidate: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
      }, [{ ...implementationCase, suite: implementationCase.suite }]);
      const incompleteImport = `.scratch/dev/test-suites/product-migration/${fixture.ids[1].replaceAll("/", "-")}-full.json`;
      await writeFixtureFile(fixture.root, incompleteImport, JSON.stringify({
        schema: "erpx-product-migration-import/v2",
        id: fixture.ids[1],
        phase: "implemented",
        candidateSha: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
        fullReceipt: { path: incomplete.path, sha256: createHash("sha256").update(incomplete.bytes).digest("hex") },
        browserReceipts: [],
      }) + "\n");
      await expect(importProductMigrationReceipt(fixture.root, incompleteImport)).rejects.toThrow("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");
    } finally {
      await rm(fixture.root, { recursive: true, force: true });
    }
  });

  it("requires the exact checkout candidate and matching source fingerprints for import", async () => {
    const fixture = await migrationImportFixture();
    try {
      const importPath = `.scratch/dev/test-suites/product-migration/${fixture.ids[0].replaceAll("/", "-")}-stale.json`;
      await writeFixtureFile(fixture.root, importPath, JSON.stringify({
        schema: "erpx-product-migration-import/v2",
        id: fixture.ids[0],
        phase: "implemented",
        candidateSha: fixture.candidateSha,
        runtimeFingerprint: digest("f"),
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
        nativeReceipt: fixture.nativeBinding,
        browserReceipts: [],
      }) + "\n");

      await expect(importProductMigrationReceipt(fixture.root, importPath)).rejects.toThrow("PRODUCT_MIGRATION_CURRENT_SOURCE_MISMATCH");

      const wrongCandidatePath = `.scratch/dev/test-suites/product-migration/${fixture.ids[0].replaceAll("/", "-")}-wrong-candidate.json`;
      await writeFixtureFile(fixture.root, wrongCandidatePath, JSON.stringify({
        schema: "erpx-product-migration-import/v2",
        id: fixture.ids[0],
        phase: "implemented",
        candidateSha: "f".repeat(40),
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
        nativeReceipt: fixture.nativeBinding,
        browserReceipts: [],
      }) + "\n");
      await expect(importProductMigrationReceipt(fixture.root, wrongCandidatePath)).rejects.toThrow("PRODUCT_MIGRATION_CURRENT_SOURCE_MISMATCH");
    } finally {
      await rm(fixture.root, { recursive: true, force: true });
    }
  });

  it("does not treat a development native test receipt as candidate acceptance", async () => {
    const fixture = await migrationImportFixture();
    try {
      const importPath = `.scratch/dev/test-suites/product-migration/${fixture.ids[0].replaceAll("/", "-")}-dev.json`;
      await writeFixtureFile(fixture.root, importPath, JSON.stringify({
        schema: "erpx-product-migration-import/v2",
        id: fixture.ids[0],
        phase: "implemented",
        candidateSha: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
        nativeReceipt: { path: ".scratch/dev/test-suites/development/native.json", sha256: digest("a") },
        browserReceipts: [],
      }) + "\n");

      await expect(importProductMigrationReceipt(fixture.root, importPath)).rejects.toThrow("PRODUCT_MIGRATION_OFFICIAL_NATIVE_RECEIPT_REQUIRED");
    } finally {
      await rm(fixture.root, { recursive: true, force: true });
    }
  });

  it("fails closed for enabled receipts until real Company grant and MFA evidence is supported", () => {
    expect(() => requiredProductMigrationCases("assets/asset-register", "enabled")).toThrow("REAL_COMPANY_GRANT_AND_MFA_EVIDENCE_REQUIRED");
    expect(() => validateProductMigrationImport({
      schema: "erpx-product-migration-import/v2",
      id: "assets/asset-register",
      phase: "enabled",
    })).toThrow("REAL_COMPANY_GRANT_AND_MFA_EVIDENCE_REQUIRED");
    expect(() => validateProductMigrationEvidence({ ...receipt(), phase: "enabled" })).toThrow("INVALID_PRODUCT_MIGRATION_RECEIPT_V2");
    expect(() => requiredProductMigrationCases("unknown/workflow", "implemented")).toThrow("UNSUPPORTED_PRODUCT_MIGRATION_WORKFLOW");
  });

  it("maps only required exact cases for the 17 non-owner workflows and preserves concrete blockers", () => {
    const ids = productWorkflowReceiptIds();
    expect(ids).toHaveLength(17);
    expect(new Set(ids).size).toBe(ids.length);

    const required = loadRequiredCases();
    const mappedCases = productWorkflowReceiptCases();
    expect(mappedCases.length).toBeGreaterThan(0);
    for (const test of mappedCases) {
      expect(test.suite).not.toBe("");
      expect(required).toContainEqual({ runner: test.runner, file: test.file, title: test.title, ...(test.project ? { project: test.project } : {}) });
    }

    const keys = (cases: ReturnType<typeof requiredProductMigrationCases>) => cases.map(test =>
      JSON.stringify([test.runner, test.suite, test.file, test.title, test.project ?? ""])).sort();
    for (const id of ids) {
      const mapping = productWorkflowReceiptMapping(id);
      const cases = productWorkflowReceiptCases(id);
      if (mapping.native.state === "mapped") {
        expect(keys(requiredProductMigrationCases(id, "implemented"))).toEqual(keys(cases.filter(test => test.runner === "vitest")));
      } else {
        expect(mapping.native.blocker.trim().length).toBeGreaterThan(20);
        expect(() => requiredProductMigrationCases(id, "implemented"))
          .toThrow(`PRODUCT_MIGRATION_WORKFLOW_UNMAPPED:${mapping.native.blocker}`);
      }
      if (mapping.browser.state === "mapped" && mapping.native.state === "mapped") {
        expect(keys(requiredProductMigrationCases(id, "verified"))).toEqual(keys(cases));
      } else if (mapping.native.state === "unmapped") {
        expect(() => requiredProductMigrationCases(id, "verified"))
          .toThrow(`PRODUCT_MIGRATION_WORKFLOW_UNMAPPED:${mapping.native.blocker}`);
      } else {
        if (mapping.browser.state !== "pending") throw new Error("EXPECTED_PENDING_BROWSER_WORKFLOW_MAPPING");
        expect(mapping.browser.blocker.trim().length).toBeGreaterThan(20);
        expect(() => requiredProductMigrationCases(id, "verified"))
          .toThrow(`PRODUCT_MIGRATION_BROWSER_WITNESS_REQUIRED:${mapping.browser.blocker}`);
      }
    }

    const appsBranch = requiredProductMigrationCases("apps:all-company-branch-department-product-selection", "verified")
      .filter(test => test.runner === "playwright" && test.title === "Apps opens a pre-entitled Branch workspace for its scoped user");
    expect(appsBranch.map(test => [test.file, test.project])).toEqual([
      ["tests/e2e/identity_access/product-hosts.spec.ts", "desktop"],
      ["tests/e2e/identity_access/product-hosts.spec.ts", "mobile"],
    ]);
    expect(productWorkflowReceiptMapping("forms:tenant-published-version-consent-submit-and-replay")).toMatchObject({
      native: { state: "mapped" },
      browser: { state: "pending", blocker: expect.stringContaining("Forms product host") },
    });
    expect(() => productWorkflowReceiptCases("unregistered:workflow")).toThrow("UNSUPPORTED_PRODUCT_MIGRATION_WORKFLOW");
  });

  it("binds all 187 reviewed owner workflows to exact native suites and only verifies workflows with browser witnesses", () => {
    expect(engineeringScenarios.capabilities).toHaveLength(187);
    expect(engineeringScenarios.registrySha256).toBe(createHash("sha256")
      .update(readFileSync(resolve(process.cwd(), "src/data/module-registry.json"))).digest("hex"));
    const ids = new Set<string>();
    let nativeOnly = 0;
    for (const scenario of engineeringScenarios.capabilities) {
      expect(ids.has(scenario.id)).toBe(false);
      ids.add(scenario.id);
      expect(scenario.review).toBe("reviewed");
      const implementation = requiredProductMigrationCases(scenario.id, "implemented");
      expect(implementation.length).toBeGreaterThan(0);
      expect(implementation.every(test => test.runner === "vitest" && test.suite.length > 0)).toBe(true);
      const browser = scenario.cases.filter(test => test.runner === "playwright");
      if (!browser.length) nativeOnly += 1;
      if (scenario.id === "assets/asset-register") {
        expect(requiredProductMigrationCases(scenario.id, "verified").filter(test => test.runner === "playwright")).toHaveLength(4);
      } else if (browser.length) {
        const verified = requiredProductMigrationCases(scenario.id, "verified");
        expect(verified.filter(test => test.runner === "playwright")).toHaveLength(browser.length);
      } else {
        expect(() => requiredProductMigrationCases(scenario.id, "verified")).toThrow("PRODUCT_MIGRATION_BROWSER_WITNESS_REQUIRED");
      }
    }
    expect(nativeOnly).toBe(55);
    expect(ids.size).toBe(187);
    const browserCandidates = [
      { id: "customer_authority/duplicate-review", suite: "customer_authority.demo-customer-duplicate-review.e2e", file: "tests/e2e/customer_authority/demo-customer-duplicate-review.spec.ts", title: "isolated demo independently reviews and merges duplicate customers" },
      { id: "finance/feature-chart-of-accounts", suite: "finance.demo-finance-chart-setup.e2e", file: "tests/e2e/finance/demo-finance-chart-setup.spec.ts", title: "isolated demo renders real chart and account setup controls" },
      { id: "finance/feature-receivables-aging", suite: "finance.demo-finance-aging.e2e", file: "tests/e2e/finance/demo-finance-aging.spec.ts", title: "isolated demo renders receivables and payables aging from live finance data" },
      { id: "finance/feature-opening-balances", suite: "finance.demo-finance-opening-import.e2e", file: "tests/e2e/finance/demo-finance-opening-import.spec.ts", title: "isolated demo imports and independently posts opening balances" },
      { id: "inventory/feature-issues-movements", suite: "inventory.demo-stock-transit.e2e", file: "tests/e2e/inventory/demo-stock-transit.spec.ts", title: "isolated demo carries an approved warehouse transfer through dispatch and partial receiving" },
      { id: "organization/feature-business-units", suite: "organization.demo-organization-lifecycle.e2e", file: "tests/e2e/organization/demo-organization-lifecycle.spec.ts", title: "isolated organization setup supports reviewed correction, safe suspension and retained history" },
      { id: "people_payroll/feature-expenses", suite: "people_payroll.expense.e2e", file: "tests/e2e/people_payroll/expense-v2.spec.ts", title: "expense goes through four independent people and two slip-backed payments" },
      { id: "collaboration/threads", suite: "collaboration.messaging.e2e", file: "tests/e2e/collaboration/messaging.spec.ts", title: "real accounts chat, reconnect, attach files, use teams and enforce private membership" },
    ];
    const requiredCases = loadRequiredCases();
    for (const candidate of browserCandidates) {
      const browser = requiredProductMigrationCases(candidate.id, "verified").filter(test => test.runner === "playwright");
      expect(browser).toEqual((["desktop", "mobile"] as const).map(project => ({
        runner: "playwright", suite: candidate.suite, file: candidate.file, title: candidate.title, project,
      })));
      for (const project of ["desktop", "mobile"] as const) expect(requiredCases).toContainEqual({
        runner: "playwright", file: candidate.file, title: candidate.title, project,
      });
    }
    expect(requiredCases.filter(test => test.runner === "playwright" &&
      test.file === "tests/journeys/shared.workplace-journeys/demo-workplace-platform-lifecycle.spec.ts" &&
      test.title === "isolated Marketing serves reviewed content and scanned public creative, captures visitor consent once and hands the immutable request to CRM before withdrawal"))
      .toEqual((["desktop", "mobile"] as const).map(project => ({
        runner: "playwright", file: "tests/journeys/shared.workplace-journeys/demo-workplace-platform-lifecycle.spec.ts",
        title: "isolated Marketing serves reviewed content and scanned public creative, captures visitor consent once and hands the immutable request to CRM before withdrawal",
        project,
      })));
    expect(requiredProductMigrationCases("commercial/invoices", "implemented")).toHaveLength(3);
    expect(requiredProductMigrationCases("commercial/invoices", "implemented").some(test => test.file.includes("/provider/"))).toBe(false);
  });

  it("keeps durable receipt reads and writes inside canonical repository files", async () => {
    const root = await realpath(await mkdtemp(join(tmpdir(), "erpx-product-receipt-")));
    try {
      const relative = "docs/testing/evidence/product-migration-v2/receipt.json";
      const absolute = await assertCanonicalRepositoryFilePath(root, relative, { createParents: true });
      const bytes = Buffer.from("{\"schema\":\"receipt\"}\n");
      await writeFile(absolute, bytes);
      const sha256 = createHash("sha256").update(bytes).digest("hex");
      expect((await readProductMigrationReceiptEvidence(root, relative, sha256)).equals(bytes)).toBe(true);

      await rm(absolute);
      const outside = join(root, "outside.json");
      await writeFile(outside, bytes);
      await symlink(outside, absolute);
      await expect(readProductMigrationReceiptEvidence(root, relative, sha256)).rejects.toThrow();

      const status = join(root, "docs/status");
      await rm(status, { recursive: true, force: true });
      const outsideDirectory = join(root, "outside-directory");
      await mkdir(outsideDirectory);
      await symlink(outsideDirectory, status);
      await expect(assertCanonicalRepositoryFilePath(root, "docs/status/receipts.json", { createParents: true })).rejects.toThrow();
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
