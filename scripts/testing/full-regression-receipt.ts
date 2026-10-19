import { createHash } from "node:crypto";
import { vitestObservations, syntheticRollbackEvidenceValid } from "./engineering/engineering-test-results";
import { requiredNativePhases, type CandidateReleaseIdentity } from "./browser-sharding";
import type { CaseObservation, RequiredCaseIdentity } from "./required-case-inventory";

export const requiredFullRegressionPhases = [
  ...requiredNativePhases,
  "rollback",
  "e2e",
  "journeys",
  "uat",
  "browser-isolation",
] as const;

export type FullRegressionRollback = {
  scope: "isolated-synthetic-prior-release";
  candidateReleaseId: string;
  fixtureCandidateReleaseId: string;
  previousReleaseId: string;
  fileManifestSha256: string;
  candidateArtifactSha256: string;
  previousArtifactSha256: string;
  candidateServerSha256: string;
  previousServerSha256: string;
  evidencePath: string;
  evidenceSha256: string;
  testReportPath: string;
  testReportSha256: string;
  switchedToPrevious: true;
  databaseAndFilesUnchanged: true;
  databaseStateBeforeSha256: string;
  databaseStateAfterSha256: string;
  privateFilesStateBeforeSha256: string;
  privateFilesStateAfterSha256: string;
};

export type FullRegressionReceipt = {
  version: 1;
  kind: "full-regression";
  scope: "internal-regression";
  runId: string;
  commit: string;
  candidate: string;
  runtimeFingerprint: string;
  verificationFingerprint: string;
  release: CandidateReleaseIdentity | null;
  rollback: FullRegressionRollback | null;
  passed: boolean;
  phases: { phase: string; passed: boolean }[];
  cases: (CaseObservation & { suite: string })[];
  providerRehearsals: unknown[];
  providerSuites: unknown[];
};

export type FullRegressionReceiptBinding = {
  runId: string;
  candidate: string;
  runtimeFingerprint: string;
  verificationFingerprint: string;
  release: CandidateReleaseIdentity;
};

const record = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const digest = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const candidateDigest = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{40,64}$/.test(value);
function validRollbackReceipt(value:unknown,binding:FullRegressionReceiptBinding):value is FullRegressionRollback {
  return record(value)&&value.scope==="isolated-synthetic-prior-release"&&value.candidateReleaseId===binding.release.id&&
    typeof value.fixtureCandidateReleaseId==="string"&&typeof value.previousReleaseId==="string"&&value.fixtureCandidateReleaseId!==value.previousReleaseId&&
    value.fileManifestSha256===binding.release.fileManifestSha256&&
    digest(value.candidateArtifactSha256)&&digest(value.previousArtifactSha256)&&value.candidateArtifactSha256!==value.previousArtifactSha256&&
    digest(value.candidateServerSha256)&&digest(value.previousServerSha256)&&value.candidateServerSha256!==value.previousServerSha256&&
    value.evidencePath===".scratch/dev/test-suites/full/"+binding.runId+"/rollback-evidence/encrypted-recovery.json"&&digest(value.evidenceSha256)&&
    value.testReportPath===".scratch/dev/test-suites/full/"+binding.runId+"/rollback-vitest.json"&&digest(value.testReportSha256)&&
    value.switchedToPrevious===true&&value.databaseAndFilesUnchanged===true&&
    digest(value.databaseStateBeforeSha256)&&value.databaseStateBeforeSha256===value.databaseStateAfterSha256&&
    digest(value.privateFilesStateBeforeSha256)&&value.privateFilesStateBeforeSha256===value.privateFilesStateAfterSha256;
}

/** Verify the rollback claims in a full-run receipt against its canonical child artifacts. */
export function validateFullRegressionRollbackArtifacts(
  value: unknown,
  binding: FullRegressionReceiptBinding,
  artifacts: { reportBytes: Uint8Array; evidenceBytes: Uint8Array },
  root: string,
): void {
  if (!record(value) || value.runId !== binding.runId || !validRollbackReceipt(value.rollback, binding))
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_REQUIRED");

  const reportPath = `.scratch/dev/test-suites/full/${binding.runId}/rollback-vitest.json`;
  const evidencePath = `.scratch/dev/test-suites/full/${binding.runId}/rollback-evidence/encrypted-recovery.json`;
  const rollback = value.rollback;
  const reportSha256 = createHash("sha256").update(artifacts.reportBytes).digest("hex");
  const evidenceSha256 = createHash("sha256").update(artifacts.evidenceBytes).digest("hex");
  if (rollback.testReportPath !== reportPath || rollback.evidencePath !== evidencePath ||
    rollback.testReportSha256 !== reportSha256 || rollback.evidenceSha256 !== evidenceSha256)
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_DIGEST_MISMATCH");

  let report: unknown, evidence: unknown;
  try {
    report = JSON.parse(Buffer.from(artifacts.reportBytes).toString("utf8")) as unknown;
    evidence = JSON.parse(Buffer.from(artifacts.evidenceBytes).toString("utf8")) as unknown;
  } catch {
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_ARTIFACTS_INVALID");
  }
  if (!record(report) || report.success !== true)
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_TEST_REQUIRED");

  let observations: ReturnType<typeof vitestObservations>;
  try {
    observations = vitestObservations(root, report as Parameters<typeof vitestObservations>[1]);
  } catch {
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_TEST_REQUIRED");
  }
  const target = observations.filter(test => test.file === "tests/integration/shared.evidence/engineering-recovery.test.ts" &&
    test.title === "encrypts a complete isolated snapshot, restores immutable money and files, and rolls back only compatible releases");
  if (observations.some(test => test.status === "failed") || target.length !== 1 || target[0].status !== "passed")
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_TEST_REQUIRED");

  if (!syntheticRollbackEvidenceValid(evidence, binding.runtimeFingerprint, binding.release.fileManifestSha256) || !record(evidence) ||
    !record(evidence.rollback))
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_EVIDENCE_REQUIRED");
  const proof = evidence.rollback;
  if (rollback.scope !== proof.scope || rollback.candidateReleaseId !== binding.release.id ||
    rollback.fixtureCandidateReleaseId !== evidence.candidateReleaseId || rollback.previousReleaseId !== proof.previousReleaseId ||
    rollback.fileManifestSha256 !== evidence.fileManifestSha256 || rollback.candidateArtifactSha256 !== proof.candidateArtifactSha256 ||
    rollback.previousArtifactSha256 !== proof.previousArtifactSha256 || rollback.candidateServerSha256 !== proof.candidateServerSha256 ||
    rollback.previousServerSha256 !== proof.previousServerSha256 || rollback.switchedToPrevious !== proof.switchedToPrevious ||
    rollback.databaseAndFilesUnchanged !== proof.databaseAndFilesUnchanged ||
    rollback.databaseStateBeforeSha256 !== proof.databaseStateBeforeSha256 || rollback.databaseStateAfterSha256 !== proof.databaseStateAfterSha256 ||
    rollback.privateFilesStateBeforeSha256 !== proof.privateFilesStateBeforeSha256 ||
    rollback.privateFilesStateAfterSha256 !== proof.privateFilesStateAfterSha256)
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_BINDING_MISMATCH");
}

export function fullRegressionRunIdFromPath(path: string) {
  return /^\.scratch\/dev\/test-suites\/full\/(\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}-\d{3}Z)\/receipt\.json$/.exec(path)?.[1];
}

export function validateFullRegressionCases<T extends RequiredCaseIdentity & { suite: string }>(value: unknown, binding: FullRegressionReceiptBinding, requiredCases: readonly T[]): (T & { status: "passed" })[] {
  if (!record(value) || value.version !== 1 || value.kind !== "full-regression" || value.scope !== "internal-regression" ||
    value.runId !== binding.runId || value.commit !== binding.candidate || value.candidate !== binding.candidate || value.runtimeFingerprint !== binding.runtimeFingerprint ||
    value.verificationFingerprint !== binding.verificationFingerprint || value.passed !== true || !candidateDigest(value.candidate) ||
    !digest(value.runtimeFingerprint) || !digest(value.verificationFingerprint) || !record(value.release) ||
    value.release.id !== binding.release.id || value.release.fileManifestSha256 !== binding.release.fileManifestSha256 ||
    typeof value.release.id !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,100}$/.test(value.release.id) || !digest(value.release.fileManifestSha256))
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_CANDIDATE_MISMATCH");

  const phases = value.phases;
  if (!Array.isArray(phases) || phases.length !== requiredFullRegressionPhases.length ||
    phases.some(phase => !record(phase) || typeof phase.phase !== "string" || phase.passed !== true) ||
    new Set(phases.map(phase => record(phase) ? phase.phase : undefined)).size !== requiredFullRegressionPhases.length ||
    requiredFullRegressionPhases.some(phase => phases.filter(item => record(item) && item.phase === phase && item.passed === true).length !== 1))
    throw new Error("PRODUCT_MIGRATION_FULL_RUN_PHASES_REQUIRED");
  if (!validRollbackReceipt(value.rollback,binding)) throw new Error("PRODUCT_MIGRATION_FULL_RUN_ROLLBACK_REQUIRED");

  const cases = value.cases;
  if (!Array.isArray(cases) || !requiredCases.length) throw new Error("PRODUCT_MIGRATION_FULL_RUN_CASES_REQUIRED");
  const key = (test: RequiredCaseIdentity) => JSON.stringify([test.runner, test.file, test.title, test.project ?? ""]);
  const expectedKeys = new Set<string>();
  for (const requiredCase of requiredCases) {
    const identity = key(requiredCase);
    if (expectedKeys.has(identity) || !requiredCase.suite) throw new Error("PRODUCT_MIGRATION_FULL_RUN_CASES_REQUIRED");
    expectedKeys.add(identity);
  }
  const observedByIdentity = new Map<string, Record<string, unknown>[]>();
  for (const item of cases) {
    if (!record(item) || !["vitest", "playwright"].includes(String(item.runner)) || typeof item.file !== "string" ||
      typeof item.title !== "string" || typeof item.suite !== "string") continue;
    const identity = key(item as unknown as RequiredCaseIdentity);
    if (!expectedKeys.has(identity)) continue;
    const matches = observedByIdentity.get(identity) ?? [];
    matches.push(item);
    observedByIdentity.set(identity, matches);
  }
  return requiredCases.map(requiredCase => {
    const matched = observedByIdentity.get(key(requiredCase)) ?? [];
    if (matched.length !== 1 || matched[0].suite !== requiredCase.suite || matched[0].status !== "passed" || matched[0].required === false)
      throw new Error("PRODUCT_MIGRATION_EXACT_FULL_RUN_CASE_REQUIRED");
    return { ...requiredCase, status: "passed" as const };
  });
}

export function validateFullRegressionReceipt<T extends RequiredCaseIdentity & { suite: string }>(value: unknown, binding: FullRegressionReceiptBinding, requiredCase: T): T & { status: "passed" } {
  return validateFullRegressionCases(value, binding, [requiredCase])[0];
}
