import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { mkdtemp, mkdir, readFile, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  appendProductMigrationReceipt,
  assertCanonicalRepositoryFilePath,
  importProductMigrationReceipt,
  productMigrationReceiptManifestSchema,
  productMigrationBrowserCaseMatches,
  productMigrationBrowserReceiptCaseMatches,
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
import { assignedBrowserShard, requiredNativePhases } from "../../../scripts/testing/browser-sharding";
import { internalRequiredCases, loadRequiredCases, canonicalCaseFile, type RequiredCaseIdentity } from "../../../scripts/testing/required-case-inventory";
import { serializeFullRegressionBrowserCases } from "../../../scripts/testing/browser-isolation-receipts";
import { assertBrowserCasePassed, browserCases } from "../../../scripts/testing/browser-results";
import { fullRegressionRunIdFromPath, requiredFullRegressionPhases, validateFullRegressionBrowserIsolationArtifacts, validateFullRegressionCases, validateFullRegressionReceipt, validateFullRegressionRollbackArtifacts } from "../../../scripts/testing/full-regression-receipt";
import { loadCatalogue } from "../../../scripts/testing/catalogue";
import { partitionBrowserIsolationSuites } from "../../../scripts/testing/browser-isolation-groups";
import { companyAdminOwnerReadsProductHostCase, productHostBusinessWorkflowForCase, productHostBrowserFile, productHostReadOnlyWorkflowForCase } from "../../../scripts/testing/product-host-cohort";
import { productHostSuiteId, validateProductHostCommercialCohorts } from "../../../scripts/testing/product-host-commercial-evidence";
import type { BrowserRunReceipt } from "../../../scripts/testing/browser-runner";
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
const assetImplementationCases = [
  implementationCase,
  {
    runner: "vitest" as const,
    suite: "finance.assets.integration",
    file: "tests/integration/finance/finance-assets.test.ts",
    title: "atomic Finance fixed asset lifecycle recognizes cost then posts consecutive straight-line months to expense and accumulated depreciation with exact residual rounding",
  },
  {
    runner: "vitest" as const,
    suite: "finance.assets.integration",
    file: "tests/integration/finance/finance-assets.test.ts",
    title: "atomic Finance fixed asset lifecycle posts reviewed disposal gain or loss, reverses asset and accumulated balances, and retains the immutable receipt after reload",
  },
  {
    runner: "vitest" as const,
    suite: "finance.assets.integration",
    file: "tests/integration/finance/finance-assets.test.ts",
    title: "Assets-only recognition and disposal settle retained acquisitions while general Finance stays unavailable",
  },
];
const rollbackTargetTitle = "encrypts a complete isolated snapshot, restores immutable money and files, and rolls back only compatible releases";
function fullRequiredCases(root = process.cwd()) {
  const suites = loadCatalogue(root).suites;
  return internalRequiredCases(loadRequiredCases(root), suites).map(test => {
    const owners = suites.filter(suite => suite.files.includes(test.file));
    if (owners.length !== 1) throw new Error(`FULL_RECEIPT_TEST_SUITE_BINDING_REQUIRED:${test.file}`);
    return { ...test, suite: owners[0].id };
  });
}

function fullBrowserIsolationGroups(root = process.cwd()) {
  return partitionBrowserIsolationSuites(loadCatalogue(root).suites.filter(suite => ["e2e", "journeys", "uat"].includes(suite.lane)));
}

function browserIsolationSummary(runId: string, evidenceSha256 = digest("d")) {
  return {
    passed: true,
    completedGroups: fullBrowserIsolationGroups().map(group => group.id),
    pendingGroups: [],
    evidencePath: `.scratch/dev/test-suites/full/${runId}/browser-isolation.json`,
    evidenceSha256,
  };
}

type FixtureBrowserCase = {
  suite: string;
  file: string;
  title: string;
  project: "desktop" | "mobile";
  run?: string;
  report?: string;
  reportSha256?: string;
  productHostLease?: { path: string; sha256: string };
  productHostCommercial?: { path: string; sha256: string; bytes: number };
  productHostReadOnly?: { path: string; sha256: string; bytes: number };
  passed?: boolean;
  faultProbe?: true;
  failure?: string;
  cleanup?: { completed: boolean; ownedDirectoryAbsent: boolean };
};
type FixtureBrowserReceipt = {
  version: 1;
  passed: boolean;
  runtimeFingerprint: string;
  verificationFingerprint: string;
  shard: null;
  interrupted: null;
  suites: string[];
  cleanupFailures: string[];
  inventory: (FixtureBrowserCase & { required: boolean })[];
  receipts: FixtureBrowserCase[];
  execution: { caseOrder: "forward" | "reverse"; mode: "acceptance" | "fault-probe"; faultProbe?: Omit<FixtureBrowserCase, "run" | "passed" | "faultProbe" | "failure" | "cleanup"> };
  evidencePath?: string;
};

async function writeBrowserIsolationFixture(root: string, runId: string, binding: {
  runtimeFingerprint: string;
  verificationFingerprint: string;
  release: { id: string; fileManifestSha256: string };
}, cases: readonly (RequiredCaseIdentity & { suite: string })[]) {
  const groups = fullBrowserIsolationGroups(root), browserCases = cases.filter(test => test.runner === "playwright");
  let nextRun = 0;
  let nextEvidenceStamp = 0;
  const newRun = () => createHash("sha256").update(`${runId}:${nextRun++}`).digest("hex").slice(0, 12);
  const retained = async (stage: "forward" | "reverse" | "probe", groupId: string, receipt: Record<string, unknown>) => {
    const runParts = /^(\d{4}-\d{2}-\d{2})T(\d{2})-(\d{2})-(\d{2})-(\d{3})Z$/.exec(runId);
    if (!runParts) throw new Error("TEST_FULL_RUN_TIMESTAMP_REQUIRED");
    const evidenceTime = new Date(`${runParts[1]}T${runParts[2]}:${runParts[3]}:${runParts[4]}.${runParts[5]}Z`);
    evidenceTime.setTime(evidenceTime.getTime() + ++nextEvidenceStamp);
    const browserStamp = evidenceTime.toISOString().replace(/[:.]/g, "-");
    const evidencePath = `.scratch/dev/test-suites/browser/${browserStamp}`;
    receipt.evidencePath = evidencePath;
    if (groupId === "product-host" && stage !== "probe") {
      const browserReceipt = receipt as FixtureBrowserReceipt;
      for (const test of browserReceipt.receipts) {
        if (test.suite !== productHostSuiteId || test.passed !== true || !test.run || !test.file || !test.title) continue;
        const casePath = join(evidencePath, test.run), caseDirectory = resolve(root, casePath), artifactDirectory = join(casePath, "artifacts");
        const businessWorkflow = productHostBusinessWorkflowForCase(test.file, test.title);
        const readOnlyWorkflow = productHostReadOnlyWorkflowForCase(test.file, test.title);
        const lease = {
          schema: "erpx-owned-product-host-browser/v1",
          run: test.run,
          sourceFingerprint: binding.runtimeFingerprint,
          verificationFingerprint: binding.verificationFingerprint,
          release: binding.release,
          scopedBusinessWorkflow: businessWorkflow,
          readOnlyBusiness: businessWorkflow === null,
          readOnlyWorkflow,
          isolatedNormalCommercial: readOnlyWorkflow === null,
          wholeProductAcceptance: false,
        };
        const leasePath = join(caseDirectory, "product-host-lease.json"), leaseBytes = Buffer.from(JSON.stringify(lease) + "\n");
        await mkdir(dirname(leasePath), { recursive: true });
        await writeFile(leasePath, leaseBytes, { mode: 0o600 });
        const attachments: { name: string; contentType: string; path: string }[] = [];
        if (readOnlyWorkflow === "admin.company-brand-organization-iam") {
          const readOnlyPages = [
            { owner: "brand", path: "/holdings/simulation/administration/brand/identity", observation: "Published Company identity", screenshot: "company-admin-brand-identity" },
            { owner: "organization", path: "/holdings/simulation/administration/technology/organization", observation: "H001 Company organization node", screenshot: "company-admin-organization" },
            { owner: "iam", path: "/holdings/simulation/administration/technology/settings/roles", observation: "Existing holding_owner assignment", screenshot: "company-admin-iam-roles" },
          ] as const;
          const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aWZ0AAAAASUVORK5CYII=", "base64");
          for (const page of readOnlyPages) {
            const screenshotPath = resolve(root, artifactDirectory, `${page.screenshot}.png`);
            await writeFixtureFile(root, relative(root, screenshotPath), png);
            attachments.push({ name: page.screenshot, contentType: "image/png", path: screenshotPath });
          }
          const proof = {
            schema: "erpx-product-host-company-admin-owner-reads/v1",
            result: "passed",
            workflow: readOnlyWorkflow,
            run: test.run,
            sourceFingerprint: binding.runtimeFingerprint,
            verificationFingerprint: binding.verificationFingerprint,
            release: lease.release,
            project: test.project,
            authority: {
              companyId: "h001-company",
              userId: "h001-owner",
              assignmentId: "h001-owner-assignment",
              role: "holding_owner",
              sameOwnerAssignmentAcrossPages: true,
              ownerAssignmentUnchanged: true,
            },
            pages: readOnlyPages.map(({ owner, path, observation, screenshot }) => ({ owner, path, observation, screenshot })),
            blockedWrites: [],
          };
          const attachmentPath = resolve(root, artifactDirectory, "company-admin-owner-reads.json");
          const proofBytes = Buffer.from(JSON.stringify(proof, null, 2) + "\n");
          await writeFixtureFile(root, relative(root, attachmentPath), proofBytes);
          attachments.unshift({ name: "company-admin-owner-reads", contentType: "application/json", path: attachmentPath });
          test.productHostReadOnly = {
            path: relative(root, attachmentPath).replaceAll("\\", "/"),
            sha256: createHash("sha256").update(proofBytes).digest("hex"),
            bytes: proofBytes.length,
          };
        } else {
          const attachmentPath = resolve(root, artifactDirectory, "isolated-normal-commercial.json");
          const commercial = {
            run: test.run,
            sourceFingerprint: binding.runtimeFingerprint,
            release: lease.release,
            changes: [],
            noInstalledWrites: true,
            noSqlGrants: true,
            normalOwnerEndpoints: true,
          };
          await writeFixtureFile(root, join(artifactDirectory, "isolated-normal-commercial.json"), JSON.stringify(commercial) + "\n");
          attachments.push({ name: "isolated-normal-commercial", contentType: "application/json", path: attachmentPath });
        }
        const report = {
          suites: [{ title: productHostBrowserFile.split("/").at(-1), specs: [{ file: test.file, title: test.title, tests: [{
            projectName: test.project, expectedStatus: "passed", status: "expected", results: [{ status: "passed", attachments }],
          }] }] }],
        };
        const reportPath = resolve(root, casePath, "result.json"), reportBytes = Buffer.from(JSON.stringify(report) + "\n");
        await writeFixtureFile(root, join(casePath, "result.json"), reportBytes.toString("utf8"));
        test.report = reportPath;
        test.reportSha256 = createHash("sha256").update(reportBytes).digest("hex");
        test.productHostLease = { path: leasePath, sha256: createHash("sha256").update(leaseBytes).digest("hex") };
        if (readOnlyWorkflow === null) {
          const attachmentPath = resolve(root, artifactDirectory, "isolated-normal-commercial.json");
          const commercialBytes = await readFile(attachmentPath);
          test.productHostCommercial = {
            path: relative(root, attachmentPath).replaceAll("\\", "/"),
            sha256: createHash("sha256").update(commercialBytes).digest("hex"),
            bytes: commercialBytes.length,
          };
        }
      }
    }
    await writeFixtureFile(root, join(evidencePath, "receipt.json"), JSON.stringify(receipt, null, 2) + "\n");
    return resolve(root, evidencePath);
  };
  const groupsEvidence = [];
  for (const group of groups) {
    const suiteIds = group.suites.map(suite => suite.id), groupCases = browserCases.filter(test => suiteIds.includes(test.suite));
    const identities = groupCases.map(test => ({ suite: test.suite, file: test.file, title: test.title, project: test.project! }));
    if (!identities.length) throw new Error(`TEST_BROWSER_ISOLATION_GROUP_WITHOUT_CASES:${group.id}`);
    const makeAcceptance = (caseOrder: "forward" | "reverse", mode: "acceptance" | "fault-probe", tests: typeof identities, suiteList = suiteIds, faultProbe?: typeof identities[number]): FixtureBrowserReceipt => ({
      version: 1,
      passed: true,
      runtimeFingerprint: binding.runtimeFingerprint,
      verificationFingerprint: binding.verificationFingerprint,
      shard: null,
      interrupted: null,
      suites: suiteList,
      cleanupFailures: [],
      inventory: tests.map(test => ({ ...test, required: true })),
      receipts: tests.map(test => ({ ...test, run: newRun(), passed: true, cleanup: { completed: true, ownedDirectoryAbsent: true } })),
      execution: { caseOrder, mode, ...(faultProbe ? { faultProbe } : {}) },
    });
    const forward = makeAcceptance("forward", "acceptance", identities);
    const reverse = makeAcceptance("reverse", "acceptance", [...identities].reverse());
    const probeIdentity = identities[0];
    const probe = makeAcceptance("forward", "fault-probe", [probeIdentity], [probeIdentity.suite], probeIdentity);
    probe.passed = false;
    probe.receipts[0] = { ...probe.receipts[0], passed: false, faultProbe: true, failure: "BROWSER_DECLARED_FAULT_PROBE" };
    const forwardDirectory = await retained("forward", group.id, forward);
    const reverseDirectory = await retained("reverse", group.id, reverse);
    const probeDirectory = await retained("probe", group.id, probe);
    for (const [stage, evidenceDirectory] of [["forward", forwardDirectory], ["probe", probeDirectory], ["reverse", reverseDirectory]] as const) {
      await writeFixtureFile(root, `.scratch/dev/test-suites/full/${runId}/browser-${group.id}-${stage}.json`,
        JSON.stringify({ evidenceDirectory }, null, 2) + "\n");
    }
    groupsEvidence.push({
      group: group.id,
      acceptance: { cases: identities.length, rerunCases: identities.length, faultProbe: true, caseOrders: ["forward", "reverse"] },
      proof: { forward, reverse, probe },
      evidenceDirectories: { forward: forwardDirectory, reverse: reverseDirectory, probe: probeDirectory },
    });
  }
  const bytes = Buffer.from(JSON.stringify({ groups: groupsEvidence }, null, 2) + "\n");
  await writeFixtureFile(root, `.scratch/dev/test-suites/full/${runId}/browser-isolation.json`, bytes.toString("utf8"));
  return { bytes, summary: browserIsolationSummary(runId, createHash("sha256").update(bytes).digest("hex")) };
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

async function writeFixtureFile(root: string, path: string, contents: string | Uint8Array) {
  const absolute = join(root, path);
  await mkdir(dirname(absolute), { recursive: true });
  await writeFile(absolute, contents);
}

async function migrationImportFixture() {
  const root = await realpath(await mkdtemp(join(tmpdir(), "erpx-product-import-")));
  for (const directory of ["src", "public", "scripts", "fixtures", "tests/support", "migrations/postgres", "ops/macos/clamav", ".github/workflows", ".github/actions"])
    await mkdir(join(root, directory), { recursive: true });
  const sourceFiles = [
    "bun.lock", "package.json", "next.config.ts", "tsconfig.base.json", "tsconfig.json", "postcss.config.mjs", "eslint.config.mjs",
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
  const browserIsolation = await writeBrowserIsolationFixture(root, runId, binding, fullRequiredCases(root));
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
    browserIsolation: browserIsolation.summary,
    providerRehearsals: [],
    providerSuites: [],
  };
  const runDirectory = `.scratch/dev/test-suites/full/${runId}`;
  const path = `${runDirectory}/receipt.json`;
  await writeFixtureFile(root, `${runDirectory}/rollback-vitest.json`, rollback.reportBytes.toString("utf8"));
  await writeFixtureFile(root, `${runDirectory}/rollback-evidence/encrypted-recovery.json`, rollback.evidenceBytes.toString("utf8"));
  const bytes = Buffer.from(JSON.stringify(receipt) + "\n");
  await writeFixtureFile(root, path, bytes.toString("utf8"));
  return { receipt, path, bytes, browserIsolationBytes: browserIsolation.bytes };
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
    const browserReceiptCase = { ...rawReceiptCase, runner: undefined };
    const reportCase = { runner: "playwright", suite: expected.suite, ...actual };

    expect(suite.files).toContain(expected.file);
    expect(productMigrationBrowserCaseMatches(rawReceiptCase, expected, suite.files)).toBe(true);
    expect(productMigrationBrowserCaseMatches(browserReceiptCase, expected, suite.files)).toBe(false);
    expect(productMigrationBrowserReceiptCaseMatches(browserReceiptCase, expected, suite.files)).toBe(true);
    expect(productMigrationBrowserReceiptCaseMatches({ ...browserReceiptCase, runner: "vitest" }, expected, suite.files)).toBe(false);
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
      browserIsolation: browserIsolationSummary(runId),
      providerRehearsals: [],
      providerSuites: [],
    };
    const path = `.scratch/dev/test-suites/full/${runId}/receipt.json`;

    expect(fullRegressionRunIdFromPath(path)).toBe(runId);
    expect(fullRegressionRunIdFromPath(".scratch/dev/test-suites/full/other/receipt.json")).toBeUndefined();
    expect(validateFullRegressionReceipt(receipt, binding, { ...implementationCase, status: "passed" })).toMatchObject(implementationCase);
    const missingIsolation = { ...receipt } as Record<string, unknown>;
    delete missingIsolation.browserIsolation;
    expect(() => validateFullRegressionReceipt(missingIsolation, binding, { ...implementationCase, status: "passed" }))
      .toThrow("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_REQUIRED");
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
      browserIsolation: browserIsolationSummary(runId),
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
      browserIsolation: browserIsolationSummary(runId),
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
    expect(requiredProductMigrationCases("assets/asset-register", "implemented")).toEqual(assetImplementationCases);
    expect(validateProductMigrationEvidence(receipt())).toMatchObject({
      schema: productMigrationReceiptSchema,
      id: "assets/asset-register",
      phase: "implemented",
      observations: assetImplementationCases.map(test => ({ ...test, status: "passed" })),
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

  it("does not promote partial Assets UI observations to verified workflow evidence", () => {
    expect(() => requiredProductMigrationCases("assets/asset-register", "verified"))
      .toThrow("PRODUCT_MIGRATION_BROWSER_WITNESS_REQUIRED");
    expect(() => receipt("verified")).toThrow("PRODUCT_MIGRATION_BROWSER_WITNESS_REQUIRED");
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

  it("requires a present, byte-bound browser-isolation artifact and persisted exact cohort receipts", async () => {
    const fixture = await migrationImportFixture();
    try {
      const runId = "2026-10-19T12-03-00-000Z";
      const full = await writeFullRegressionFixture(fixture.root, runId, {
        candidate: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
      });
      const importPath = `.scratch/dev/test-suites/product-migration/${fixture.ids[0].replaceAll("/", "-")}-missing-isolation.json`;
      const writeImport = async (receipt: typeof full) => writeFixtureFile(fixture.root, importPath, JSON.stringify({
        schema: "erpx-product-migration-import/v2",
        id: fixture.ids[0],
        phase: "implemented",
        candidateSha: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
        fullReceipt: { path: receipt.path, sha256: createHash("sha256").update(receipt.bytes).digest("hex") },
        browserReceipts: [],
      }) + "\n");
      await writeImport(full);
      await rm(join(fixture.root, full.receipt.browserIsolation!.evidencePath), { force: true });
      await expect(importProductMigrationReceipt(fixture.root, importPath))
        .rejects.toThrow("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_ARTIFACT_REQUIRED");

      const changedRunId = "2026-10-19T12-04-00-000Z";
      const changed = await writeFullRegressionFixture(fixture.root, changedRunId, {
        candidate: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
      });
      await writeImport(changed);
      await writeFixtureFile(fixture.root, changed.receipt.browserIsolation!.evidencePath, "{\"groups\":[]}\n");
      await expect(importProductMigrationReceipt(fixture.root, importPath))
        .rejects.toThrow("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_DIGEST_MISMATCH");
    } finally {
      await rm(fixture.root, { recursive: true, force: true });
    }
  });

  it("binds browser isolation receipts to the current full run and the probe's exact owner suite", async () => {
    const fixture = await migrationImportFixture();
    try {
      const runId = "2026-10-19T12-05-00-000Z";
      const binding = {
        runId,
        candidate: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
      };
      const full = await writeFullRegressionFixture(fixture.root, runId, binding);
      const hashArtifact = (groups: unknown) => Buffer.from(JSON.stringify(groups, null, 2) + "\n");
      const boundReceipt = (bytes: Uint8Array) => ({
        ...full.receipt,
        browserIsolation: { ...full.receipt.browserIsolation!, evidenceSha256: createHash("sha256").update(bytes).digest("hex") },
      });

      type GroupEvidence = {
        proof: {
          forward: { evidencePath: string; suites: string[] };
          probe: { suites: string[]; execution: { faultProbe: { suite: string } } };
        };
        evidenceDirectories: { forward: string };
      };
      const staleArtifact = JSON.parse(full.browserIsolationBytes.toString("utf8")) as { groups: GroupEvidence[] };
      const staleGroup = staleArtifact.groups[0];
      const staleEvidencePath = staleGroup.proof.forward.evidencePath.replace(/-\d{3}Z$/, "-999Z");
      staleGroup.proof.forward.evidencePath = staleEvidencePath;
      staleGroup.evidenceDirectories.forward = resolve(fixture.root, staleEvidencePath);
      const staleBytes = hashArtifact(staleArtifact);
      await expect(validateFullRegressionBrowserIsolationArtifacts(boundReceipt(staleBytes), binding, staleBytes, fixture.root))
        .rejects.toThrow("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_EVIDENCE_REQUIRED");

      const wrongProbeArtifact = JSON.parse(full.browserIsolationBytes.toString("utf8")) as { groups: GroupEvidence[] };
      const probeGroup = wrongProbeArtifact.groups[0];
      const wrongProbeSuite = probeGroup.proof.forward.suites.find(suite => suite !== probeGroup.proof.probe.execution.faultProbe.suite);
      if (!wrongProbeSuite) throw new Error("PRODUCT_MIGRATION_TEST_SECOND_PROBE_SUITE_REQUIRED");
      probeGroup.proof.probe.suites = [wrongProbeSuite];
      const wrongProbeBytes = hashArtifact(wrongProbeArtifact);
      await expect(validateFullRegressionBrowserIsolationArtifacts(boundReceipt(wrongProbeBytes), binding, wrongProbeBytes, fixture.root))
        .rejects.toThrow("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_SUITE_BINDING_REQUIRED");
    } finally {
      await rm(fixture.root, { recursive: true, force: true });
    }
  });

  it("requires bound Product Host leases and workflow-specific evidence in CI and full receipts", async () => {
    const fixture = await migrationImportFixture();
    try {
      const runId = "2026-10-19T12-07-00-000Z";
      const binding = {
        runId,
        candidate: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
      };
      const full = await writeFullRegressionFixture(fixture.root, runId, binding);
      type ProductGroup = { group: string; proof: { forward: BrowserRunReceipt; reverse: BrowserRunReceipt } };
      const artifact = JSON.parse(full.browserIsolationBytes.toString("utf8")) as { groups: ProductGroup[] };
      const productGroup = artifact.groups.find(group => group.group === "product-host");
      if (!productGroup) throw new Error("PRODUCT_HOST_TEST_GROUP_REQUIRED");
      const expected = { runtimeFingerprint: fixture.runtimeFingerprint, verificationFingerprint: fixture.verificationFingerprint, release: fixture.release };
      const ciGroup = (forward: BrowserRunReceipt, reverse: BrowserRunReceipt) => ({
        group: "product-host" as const, suiteIds: forward.suites, passed: true, forward, reverse,
      });
      await expect(validateProductHostCommercialCohorts(fixture.root, [ciGroup(productGroup.proof.forward, productGroup.proof.reverse)], expected, true))
        .resolves.toHaveLength(productGroup.proof.forward.receipts.length + productGroup.proof.reverse.receipts.length);

      const invalidRun = structuredClone(productGroup.proof.forward);
      const invalidRunCase = invalidRun.receipts.find(test => test.suite === productHostSuiteId);
      if (!invalidRunCase) throw new Error("PRODUCT_HOST_TEST_CASE_REQUIRED");
      invalidRunCase.run = "../outside";
      await expect(validateProductHostCommercialCohorts(fixture.root, [ciGroup(invalidRun, productGroup.proof.reverse)], expected, true))
        .rejects.toThrow("PRODUCT_HOST_CASE_RECEIPT_REQUIRED");

      const missingLease = structuredClone(productGroup.proof.forward);
      const missingLeaseCase = missingLease.receipts.find(test => test.suite === productHostSuiteId);
      if (!missingLeaseCase) throw new Error("PRODUCT_HOST_TEST_CASE_REQUIRED");
      delete missingLeaseCase.productHostLease;
      await expect(validateProductHostCommercialCohorts(fixture.root, [ciGroup(missingLease, productGroup.proof.reverse)], expected, true))
        .rejects.toThrow("PRODUCT_HOST_CASE_RECEIPT_REQUIRED");

      const missingReadOnlyProof = structuredClone(productGroup.proof.forward);
      const readOnlyCase = missingReadOnlyProof.receipts.find(test => test.suite === productHostSuiteId &&
        test.title === companyAdminOwnerReadsProductHostCase);
      if (!readOnlyCase) throw new Error("PRODUCT_HOST_READ_ONLY_TEST_CASE_REQUIRED");
      delete readOnlyCase.productHostReadOnly;
      await expect(validateProductHostCommercialCohorts(fixture.root, [ciGroup(missingReadOnlyProof, productGroup.proof.reverse)], expected, true))
        .rejects.toThrow("PRODUCT_HOST_READ_ONLY_CASE_RECEIPT_REQUIRED");

      const mixedWorkflowScope = structuredClone(productGroup.proof.forward);
      const financeCase = mixedWorkflowScope.receipts.find(test => test.suite === productHostSuiteId &&
        productHostBusinessWorkflowForCase(test.file ?? "", test.title ?? "") === "finance.sales-invoice");
      if (!financeCase?.productHostLease) throw new Error("PRODUCT_HOST_FINANCE_CASE_LEASE_REQUIRED");
      const originalLeaseBytes = await readFile(financeCase.productHostLease.path);
      const mixedLease = JSON.parse(originalLeaseBytes.toString("utf8")) as Record<string, unknown>;
      mixedLease.readOnlyWorkflow = "admin.company-brand-organization-iam";
      const mixedLeaseBytes = Buffer.from(JSON.stringify(mixedLease) + "\n");
      try {
        await writeFile(financeCase.productHostLease.path, mixedLeaseBytes);
        const mixedLeaseSha256 = createHash("sha256").update(mixedLeaseBytes).digest("hex");
        financeCase.productHostLease.sha256 = mixedLeaseSha256;
        if (financeCase.uiEvidence?.productHostLease) financeCase.uiEvidence.productHostLease.sha256 = mixedLeaseSha256;
        await expect(validateProductHostCommercialCohorts(fixture.root, [ciGroup(mixedWorkflowScope, productGroup.proof.reverse)], expected, true))
          .rejects.toThrow("PRODUCT_HOST_LEASE_SCOPE_OR_CANDIDATE_MISMATCH");
      } finally {
        await writeFile(financeCase.productHostLease.path, originalLeaseBytes);
      }

      const missingAttachment = structuredClone(productGroup.proof.forward);
      const attachmentCase = missingAttachment.receipts.find(test => test.suite === productHostSuiteId &&
        productHostReadOnlyWorkflowForCase(test.file ?? "", test.title ?? "") === null);
      if (!attachmentCase?.report) throw new Error("PRODUCT_HOST_TEST_REPORT_REQUIRED");
      const reportPath = attachmentCase.report;
      const report = JSON.parse(await readFile(reportPath, "utf8")) as {
        suites: { specs: { tests: { results: { attachments: { name: string }[] }[] }[] }[] }[];
      };
      const results = report.suites[0]?.specs[0]?.tests[0]?.results;
      if (!results?.[0]) throw new Error("PRODUCT_HOST_TEST_RESULT_REQUIRED");
      results[0].attachments = results[0].attachments.filter(item => item.name !== "isolated-normal-commercial");
      const reportBytes = Buffer.from(JSON.stringify(report) + "\n");
      await writeFile(reportPath, reportBytes);
      attachmentCase.reportSha256 = createHash("sha256").update(reportBytes).digest("hex");
      await expect(validateProductHostCommercialCohorts(fixture.root, [ciGroup(missingAttachment, productGroup.proof.reverse)], expected, true))
        .rejects.toThrow("PRODUCT_HOST_COMMERCIAL_ATTACHMENT_REQUIRED");

      const brokenArtifact = JSON.parse(full.browserIsolationBytes.toString("utf8")) as { groups: ProductGroup[] };
      const brokenProductGroup = brokenArtifact.groups.find(group => group.group === "product-host");
      if (!brokenProductGroup) throw new Error("PRODUCT_HOST_TEST_GROUP_REQUIRED");
      const brokenForward = brokenProductGroup.proof.forward;
      const brokenCase = brokenForward.receipts.find(test => test.suite === productHostSuiteId);
      if (!brokenCase) throw new Error("PRODUCT_HOST_TEST_CASE_REQUIRED");
      delete brokenCase.productHostLease;
      await writeFixtureFile(fixture.root, join(brokenForward.evidencePath!, "receipt.json"), JSON.stringify(brokenForward, null, 2) + "\n");
      const brokenBytes = Buffer.from(JSON.stringify(brokenArtifact, null, 2) + "\n");
      await writeFixtureFile(fixture.root, full.receipt.browserIsolation!.evidencePath, brokenBytes.toString("utf8"));
      const brokenSummary = {
        ...full.receipt,
        browserIsolation: { ...full.receipt.browserIsolation!, evidenceSha256: createHash("sha256").update(brokenBytes).digest("hex") },
      };
      await expect(validateFullRegressionBrowserIsolationArtifacts(brokenSummary, binding, brokenBytes, fixture.root))
        .rejects.toThrow("PRODUCT_HOST_CASE_RECEIPT_REQUIRED");
    } finally {
      await rm(fixture.root, { recursive: true, force: true });
    }
  });

  it("accepts a 64-way Product Host shard only when both assigned inventories and receipts are empty", async () => {
    const root = process.cwd(), shard = { index: 3, total: 64 };
    const cases = fullRequiredCases(root).filter(test => test.runner === "playwright" && test.suite === productHostSuiteId);
    const inventory = cases.map(test => ({
      suite: test.suite,
      file: test.file,
      title: test.title,
      project: test.project as "desktop" | "mobile",
      required: true,
    }));
    expect(inventory.filter(test => assignedBrowserShard(test.suite, test, shard))).toHaveLength(0);
    const makeReceipt = (caseOrder: "forward" | "reverse", receiptShard: BrowserRunReceipt["shard"] = shard, receipts: BrowserRunReceipt["receipts"] = []): BrowserRunReceipt => ({
      version: 1,
      passed: true,
      runtimeFingerprint: digest("a"),
      verificationFingerprint: digest("b"),
      shard: receiptShard,
      interrupted: null,
      suites: [productHostSuiteId],
      cleanupFailures: [],
      inventory,
      receipts,
      execution: { caseOrder, mode: "acceptance" },
      evidencePath: ".scratch/dev/test-suites/browser/2026-10-20T12-00-00-000Z",
    });
    const expected = { runtimeFingerprint: digest("a"), verificationFingerprint: digest("b"), release: { id: "release", fileManifestSha256: digest("c") } };
    const group = (forward: BrowserRunReceipt, reverse: BrowserRunReceipt) => ({
      group: "product-host" as const,
      suiteIds: [productHostSuiteId],
      passed: true,
      forward,
      reverse,
    });

    await expect(validateProductHostCommercialCohorts(root, [group(makeReceipt("forward"), makeReceipt("reverse"))], expected, true))
      .resolves.toEqual([]);

    const forgedReceipt: BrowserRunReceipt["receipts"][number] = {
      suite: productHostSuiteId,
      file: inventory[0].file,
      title: inventory[0].title,
      project: inventory[0].project,
      run: "0123456789ab",
      passed: true,
    };
    await expect(validateProductHostCommercialCohorts(root, [group(
      makeReceipt("forward", shard, [forgedReceipt]),
      makeReceipt("reverse"),
    )], expected, true)).rejects.toThrow("PRODUCT_HOST_REQUIRED_CASE_COVERAGE_REQUIRED");

    await expect(validateProductHostCommercialCohorts(root, [group(
      makeReceipt("forward", null),
      makeReceipt("reverse", null),
    )], expected, true)).rejects.toThrow("PRODUCT_HOST_REQUIRED_CASE_COVERAGE_REQUIRED");
  });

  it("requires run-local browser stage markers to be present, exact and non-symlinked", async () => {
    const fixture = await migrationImportFixture();
    try {
      const runId = "2026-10-19T12-06-00-000Z";
      const binding = {
        runId,
        candidate: fixture.candidateSha,
        runtimeFingerprint: fixture.runtimeFingerprint,
        verificationFingerprint: fixture.verificationFingerprint,
        release: fixture.release,
      };
      const full = await writeFullRegressionFixture(fixture.root, runId, binding);
      const bytes = full.browserIsolationBytes;
      const summary = {
        ...full.receipt,
        browserIsolation: { ...full.receipt.browserIsolation!, evidenceSha256: createHash("sha256").update(bytes).digest("hex") },
      };
      const browserArtifact = JSON.parse(bytes.toString("utf8")) as { groups: { group: string }[] };
      const markerPath = `.scratch/dev/test-suites/full/${runId}/browser-${browserArtifact.groups[0].group}-forward.json`;
      const validate = () => validateFullRegressionBrowserIsolationArtifacts(summary, binding, bytes, fixture.root);

      await rm(join(fixture.root, markerPath), { force: true });
      await expect(validate()).rejects.toThrow("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_EVIDENCE_REQUIRED");

      await writeFixtureFile(fixture.root, markerPath, JSON.stringify({ evidenceDirectory: join(fixture.root, ".scratch/dev/test-suites/browser/old-forward") }) + "\n");
      await expect(validate()).rejects.toThrow("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_EVIDENCE_REQUIRED");

      const markerTarget = `.scratch/dev/test-suites/full/${runId}/marker-target.json`;
      await writeFixtureFile(fixture.root, markerTarget, JSON.stringify({ evidenceDirectory: "unused" }) + "\n");
      await rm(join(fixture.root, markerPath), { force: true });
      await symlink(join(fixture.root, markerTarget), join(fixture.root, markerPath));
      await expect(validate()).rejects.toThrow("PRODUCT_MIGRATION_FULL_RUN_BROWSER_ISOLATION_EVIDENCE_REQUIRED");
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

  it("maps only required exact cases for the 19 non-owner workflows and preserves concrete blockers", () => {
    const ids = productWorkflowReceiptIds();
    expect(ids).toHaveLength(19);
    expect(ids).toEqual(expect.arrayContaining([
      "legal-portal:scoped-shared-version-access-and-revocation",
      "legal-portal:private-document-download-and-access-history",
    ]));
    expect(ids).not.toContain("legal-portal:signing-fresh-mfa-version-bound-consent-and-signature-evidence");
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
    const formsId = "forms:tenant-published-version-consent-submit-and-replay";
    expect(productWorkflowReceiptMapping(formsId)).toEqual({
      native: { state: "mapped" },
      browser: { state: "mapped" },
    });
    expect(requiredProductMigrationCases(formsId, "verified").filter(test => test.runner === "playwright")).toEqual(
      (["desktop", "mobile"] as const).map(project => ({
        runner: "playwright",
        suite: "marketing.forms-host.e2e",
        file: "tests/e2e/marketing/forms-host.spec.ts",
        title: "isolated anonymous Forms host submits the published version and replays exact consent once",
        project,
      })),
    );
    expect(requiredProductMigrationCases(formsId, "implemented")).toEqual([
      {
        runner: "vitest",
        suite: "marketing.public-marketing-form.integration",
        file: "tests/integration/marketing/public-marketing-form.test.ts",
        title: "requires the rendered version for a new Forms-host submission while retaining exact legacy receipts and current publication authority",
      },
    ]);
    for (const test of requiredProductMigrationCases(formsId, "verified")) {
      expect(required).toContainEqual({ runner: test.runner, file: test.file, title: test.title, ...(test.project ? { project: test.project } : {}) });
      expect(test.suite).toBe(test.runner === "playwright" ? "marketing.forms-host.e2e" : "marketing.public-marketing-form.integration");
    }
    const executiveId = "work:company-executive-and-scoped-mcp";
    expect(productWorkflowReceiptMapping(executiveId)).toEqual({
      native: { state: "mapped" },
      browser: { state: "mapped" },
    });
    expect(requiredProductMigrationCases(executiveId, "verified").filter(test => test.runner === "playwright")).toEqual([{
      runner: "playwright",
      suite: "shared.executive.executive-work-mcp.e2e",
      file: "tests/e2e/shared.executive/executive-work-mcp.spec.ts",
      title: "isolated Work Executive UI reads a seeded connection through scoped MCP",
      project: "desktop",
    }]);

    const salesInvoiceId = "finance:sales-invoice-workspace";
    expect(productWorkflowReceiptMapping(salesInvoiceId)).toEqual({
      native: { state: "mapped" },
      browser: { state: "mapped" },
    });
    const salesInvoiceCases = requiredProductMigrationCases(salesInvoiceId, "verified");
    const salesInvoiceUnitCases = [
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "Finance draft boundary blocks same-view invoice query navigation while a marked draft is dirty"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "marks state-managed Finance form edits in the same dirty-navigation boundary"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "Finance draft boundary blocks navigation and explicit discard while a Finance acknowledgement is unresolved"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales command registers its original request with FinanceDraftBoundary through ambiguous retry and preserves newer edits"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales command releases the boundary lock after a definitive first response"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice Escape uses Finance confirmation and discards only its own form"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice Close uses Finance confirmation and discards only its own form"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice Add service is dirty and preserves the close confirmation"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice Remove service is dirty when it is the only edit after an acknowledged baseline"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice scoped discard refuses a replaced owner root and retains other Finance drafts"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice retains its native dismissal guard for pending commands"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice retains its native dismissal guard for unresolved commands"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice drawer preserves the submitted command and newer service edits while the command is unresolved"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice clean close removes only its open flag and retains current repeated filters, cursor and hash"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice discard close removes only its open flag and retains current repeated filters, cursor and hash"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice clean close keeps another dirty Sales owner mounted without URL replacement"],
      ["finance.navigation.unit", "tests/unit/finance/finance-navigation.test.ts", "actual Sales invoice inline close and already-absent open flag do not navigate"],
      ["finance.record-projection-services.unit", "tests/unit/finance/finance-product-presentation.test.ts", "routes direct Finance invoice setup failures through the access boundary before rendering the form"],
    ] as const;
    expect(salesInvoiceCases.filter(test => test.runner === "vitest")).toEqual([
      {
        runner: "vitest",
        suite: "finance.sales-workspace.integration",
        file: "tests/integration/finance/finance-sales-workspace.test.ts",
        title: "Sales-only grants can create and replay standalone service invoices while draining blocks new invoices",
      },
      {
        runner: "vitest",
        suite: "finance.sales-workspace.integration",
        file: "tests/integration/finance/finance-sales-workspace.test.ts",
        title: "Sales-only standalone invoice posting fails closed when completion outbox evidence is missing",
      },
      ...salesInvoiceUnitCases.map(([suite, file, title]) => ({ runner: "vitest" as const, suite, file, title })),
    ]);
    expect(salesInvoiceCases.filter(test => test.runner === "playwright")).toEqual([
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        suite: "finance.sales-workspace.e2e",
        file: "tests/e2e/finance/finance-sales-workspace.spec.ts",
        title: "English sales workspace opens drawers, calculates invoices and keeps direct forms available",
        project,
      })),
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        suite: "shared.ui.uat-route-matrix.uat",
        file: "tests/uat/shared.ui/uat-route-matrix.spec.ts",
        title: "Finance invoice drawer observations native prepared invoice drawers retain dirty work and keyboard focus across the appearance matrix",
        project,
      })),
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        suite: "identity_access.product-hosts.e2e",
        file: "tests/e2e/identity_access/product-hosts.spec.ts",
        title: "isolated Finance native invoice drawer keeps dirty work across the appearance matrix",
        project,
      })),
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        suite: "identity_access.product-hosts.e2e",
        file: "tests/e2e/identity_access/product-hosts.spec.ts",
        title: "isolated Finance host creates a completion-backed sales invoice through Account and Apps",
        project,
      })),
    ]);
    expect(requiredProductMigrationCases("finance:sales-invoice-workspace", "implemented")).toEqual(
      salesInvoiceCases.filter(test => test.runner === "vitest"),
    );
    expect(requiredProductMigrationCases("finance/feature-payments", "verified").filter(test =>
      test.runner === "playwright" && test.file === "tests/e2e/finance/finance-sales-workspace.spec.ts")).toEqual([]);
    for (const test of salesInvoiceCases) {
      expect(required).toContainEqual({ runner: test.runner, file: test.file, title: test.title, ...(test.project ? { project: test.project } : {}) });
    }
    expect(() => productWorkflowReceiptCases("unregistered:workflow")).toThrow("UNSUPPORTED_PRODUCT_MIGRATION_WORKFLOW");
  });

  it("binds all 187 reviewed owner workflows to exact native suites and only verifies workflows with browser witnesses", () => {
    expect(engineeringScenarios.capabilities).toHaveLength(187);
    expect(engineeringScenarios.registrySha256).toBe(createHash("sha256")
      .update(readFileSync(resolve(process.cwd(), "packages/catalogue/src/module-registry.json"))).digest("hex"));
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
      if (browser.length) {
        const verified = requiredProductMigrationCases(scenario.id, "verified");
        expect(verified.filter(test => test.runner === "playwright")).toHaveLength(browser.length);
      } else {
        expect(() => requiredProductMigrationCases(scenario.id, "verified")).toThrow("PRODUCT_MIGRATION_BROWSER_WITNESS_REQUIRED");
      }
    }
    expect(nativeOnly).toBe(107);
    expect(ids.size).toBe(187);
    for (const id of ["customer_360/feature-customer-notes", "crm/activities"]) {
      expect(requiredProductMigrationCases(id, "verified").filter(test => test.runner === "playwright").map(test => test.project).sort()).toEqual(["desktop", "mobile"]);
    }
    const rejectedNearFitMappings = [
      "assets/asset-register",
      "commercial/entitlements",
      "commercial/feature-catalog-plans",
      "identity_access/assignments",
      "identity_access/feature-people-principals",
      "identity_access/feature-roles-permissions",
      "identity_access/feature-directory-connectors",
      "organization/organization-tree",
      "organization/feature-business-units",
      "platform_administration/feature-projections-events",
      "platform_administration/feature-migration-runs",
      "platform_administration/feature-platform-audit",
      "assets/feature-placement",
      "assets/feature-maintenance-lifecycle",
      "maintenance/work-orders",
      "maintenance/feature-completion",
      "maintenance/feature-maintenance-history",
      "inventory/feature-receipts",
      "inventory/feature-issues-movements",
      "operations/feature-exception-queue",
      "operations/feature-operations-summary",
      "quality/inspections",
      "quality/results",
      "quality/release-gates",
      "manufacturing/orders",
      "manufacturing/material-readiness",
      "manufacturing/quality-disposition",
      "manufacturing/feature-labor-output",
      "manufacturing/feature-costs-genealogy",
      "projects/dependencies",
      "projects/feature-projects-wbs",
      "projects/feature-handoffs",
      "projects/feature-time-costs",
      "projects/feature-project-graph",
      "learning/feature-assignments",
      "learning/feature-attendance-results",
      "field_service/service-orders",
      "field_service/feature-agreements",
      "field_service/feature-dispatch",
      "field_service/feature-visits",
      "agent/agent-catalog",
      "agent/versions",
      "agent/assignments",
      "agent/feature-field-suggestions",
      "collaboration/mentions",
      "collaboration/feature-comments-moderation",
      "collaboration/feature-attachments",
      "collaboration/feature-record-relations",
      "pms/cashier",
      "customer_360/service-timeline",
      "customer_360/feature-customer-profile",
      "customer_authority/access-history",
      "customer_authority/feature-identity-changes",
      "pos/feature-register-session",
      "pos/feature-sales-baskets",
      "pos/feature-payments-receipts",
      "sales/quotes",
      "sales/orders",
      "sales/fulfillment",
      "crm/feature-leads",
      "marketing/feature-forms-profiles",
      "marketing/feature-engagement",
    ];
    for (const id of rejectedNearFitMappings) {
      expect(engineeringScenarios.capabilities.find(scenario => scenario.id === id)?.cases.some(test => test.runner === "playwright")).toBe(false);
      expect(() => requiredProductMigrationCases(id, "verified")).toThrow("PRODUCT_MIGRATION_BROWSER_WITNESS_REQUIRED");
    }
    const browserCandidates = [
      { id: "pms/dashboard", cases: [{ suite: "pms.owner-read-workflows.e2e", file: "tests/e2e/pms/pms-owner-read-workflows.spec.ts", title: "pms/dashboard renders the selected hotel's live room status through its owner-scoped read" }] },
      { id: "customer_authority/duplicate-review", cases: [{ suite: "customer_authority.demo-customer-duplicate-review.e2e", file: "tests/e2e/customer_authority/demo-customer-duplicate-review.spec.ts", title: "isolated demo independently reviews and merges duplicate customers" }] },
      { id: "finance/feature-chart-of-accounts", cases: [{ suite: "finance.demo-finance-chart-setup.e2e", file: "tests/e2e/finance/demo-finance-chart-setup.spec.ts", title: "isolated demo renders real chart and account setup controls" }] },
      { id: "finance/feature-receivables-aging", cases: [{ suite: "finance.demo-finance-aging.e2e", file: "tests/e2e/finance/demo-finance-aging.spec.ts", title: "isolated demo renders receivables and payables aging from live finance data" }] },
      { id: "finance/feature-opening-balances", cases: [{ suite: "finance.demo-finance-opening-import.e2e", file: "tests/e2e/finance/demo-finance-opening-import.spec.ts", title: "isolated demo imports and independently posts opening balances" }] },
      { id: "finance/feature-payments", cases: [
        { suite: "shared.money-people-journeys.demo-engineering-money-people.journeys", file: "tests/journeys/shared.money-people-journeys/demo-engineering-money-people.spec.ts", title: "isolated normal-login Finance and workforce owner journeys creates an itemized supplier bill and independently withholds pays issues amends and financially cancels retained 50 tawi copies" },
        { suite: "finance.demo-controlled-payment.e2e", file: "tests/e2e/finance/demo-controlled-payment.spec.ts", title: "isolated demo completes a three-person generic payment through exact bank evidence" },
      ] },
      { id: "sales/feature-cancellations", cases: [{ suite: "sales.demo-sales-to-collection.journeys", file: "tests/journeys/sales/demo-sales-to-collection.spec.ts", title: "isolated Sales cancels before stock movement and requires a controlled return after delivery" }] },
      { id: "people_payroll/feature-expenses", cases: [{ suite: "people_payroll.expense.e2e", file: "tests/e2e/people_payroll/expense-v2.spec.ts", title: "expense goes through four independent people and two slip-backed payments" }] },
      { id: "collaboration/threads", cases: [{ suite: "collaboration.messaging.e2e", file: "tests/e2e/collaboration/messaging.spec.ts", title: "real accounts chat, reconnect, attach files, use teams and enforce private membership" }] },
    ];
    const requiredCases = loadRequiredCases();
    for (const candidate of browserCandidates) {
      const browser = requiredProductMigrationCases(candidate.id, "verified").filter(test => test.runner === "playwright");
      expect(browser).toEqual(candidate.cases.flatMap(test => (["desktop", "mobile"] as const).map(project => ({
        runner: "playwright", ...test, project,
      }))));
      for (const test of candidate.cases) for (const project of ["desktop", "mobile"] as const) expect(requiredCases).toContainEqual({
        runner: "playwright", file: test.file, title: test.title, project,
      });
    }
    const salesWorkspace = requiredProductMigrationCases("finance:sales-invoice-workspace", "verified").filter(test =>
      test.runner === "playwright" && test.file === "tests/e2e/finance/finance-sales-workspace.spec.ts");
    expect(salesWorkspace).toEqual((["desktop", "mobile"] as const).map(project => ({
      runner: "playwright", suite: "finance.sales-workspace.e2e", file: "tests/e2e/finance/finance-sales-workspace.spec.ts",
      title: "English sales workspace opens drawers, calculates invoices and keeps direct forms available", project,
    })));
    for (const project of ["desktop", "mobile"] as const) expect(requiredCases).toContainEqual({
      runner: "playwright", file: "tests/e2e/finance/finance-sales-workspace.spec.ts",
      title: "English sales workspace opens drawers, calculates invoices and keeps direct forms available", project,
    });
    expect(requiredProductMigrationCases("finance:sales-invoice-workspace", "verified").filter(test =>
      test.runner === "playwright" && test.file === productHostBrowserFile && test.title === "isolated Finance host creates a completion-backed sales invoice through Account and Apps"))
      .toEqual((["desktop", "mobile"] as const).map(project => ({ runner: "playwright", suite: productHostSuiteId, file: productHostBrowserFile,
        title: "isolated Finance host creates a completion-backed sales invoice through Account and Apps", project })));
    for (const project of ["desktop", "mobile"] as const) expect(requiredCases).toContainEqual({
      runner: "playwright", file: productHostBrowserFile,
      title: "isolated Finance host creates a completion-backed sales invoice through Account and Apps", project,
    });
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
