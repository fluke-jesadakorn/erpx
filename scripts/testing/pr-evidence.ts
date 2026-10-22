import { canonicalCaseFile, requiredCaseKey, type CaseObservation, type NativeCaseReceipt, type RequiredCaseIdentity } from "./required-case-inventory";

export type PrReceiptBinding = {
  scope?: unknown;
  kind?: unknown;
  base?: unknown;
  candidate?: unknown;
  selectionFingerprint?: unknown;
  passed?: unknown;
};
export type PrCaseResult = { key: string; passed: boolean; cleanup?: { completed: boolean; ownedDirectoryAbsent: boolean } };
export type NativeCaseRunEvidence = {
  passed: boolean;
  cases: CaseObservation[];
  failureCodes: string[];
};

/** Keep safe per-case Vitest identities/statuses in the PR artifact even when a lane fails. */
export function summarizeNativeCaseRun(input: {
  commandPassed: boolean;
  selectedFiles: readonly string[];
  requiredCases: readonly RequiredCaseIdentity[];
  receipt: unknown;
}): NativeCaseRunEvidence {
  const failures = new Set<string>();
  const selectedFiles = [...input.selectedFiles];
  if (new Set(selectedFiles).size !== selectedFiles.length || !selectedFiles.length) failures.add("NATIVE_SELECTED_FILES_INVALID");
  if (!input.commandPassed) failures.add("NATIVE_COMMAND_FAILED");
  const value = input.receipt as Partial<NativeCaseReceipt> | undefined;
  if (!value || typeof value !== "object") {
    failures.add("NATIVE_CASE_RECEIPT_MISSING");
    return { passed: false, cases: [], failureCodes: [...failures].sort() };
  }
  if (value.version !== 1 || value.runner !== "vitest" || typeof value.passed !== "boolean" || !Array.isArray(value.files) || !Array.isArray(value.cases))
    failures.add("NATIVE_CASE_RECEIPT_INVALID");
  if (value.passed !== true) failures.add("NATIVE_REPORTER_REJECTED_CASES");
  const cases: CaseObservation[] = [];
  for (const item of Array.isArray(value.cases) ? value.cases : []) {
    if (!item || item.runner !== "vitest" || typeof item.file !== "string" || typeof item.title !== "string" || !item.title.trim() || item.project !== undefined ||
      (item.required !== undefined && typeof item.required !== "boolean")) {
      failures.add("NATIVE_CASE_IDENTITY_INVALID");
      continue;
    }
    const file = canonicalCaseFile(item.file, selectedFiles);
    if (!selectedFiles.includes(file)) { failures.add("NATIVE_CASE_OUTSIDE_SELECTION"); continue; }
    const status = typeof item.status === "string" && ["passed", "failed", "skipped", "pending", "todo", "cancelled"].includes(item.status) ? item.status : "failed";
    if (status !== item.status) failures.add("NATIVE_CASE_STATUS_INVALID");
    if (status !== "passed") failures.add("NATIVE_CASE_DID_NOT_PASS");
    cases.push({ runner: "vitest", file, title: item.title, status, ...(item.required === false ? { required: false } : {}) });
  }
  if (!Array.isArray(value.files) || JSON.stringify([...value.files].sort()) !== JSON.stringify([...selectedFiles].sort())) failures.add("NATIVE_CASE_FILE_SELECTION_MISMATCH");
  if (cases.length === 0 || selectedFiles.some(file => !cases.some(test => test.file === file))) failures.add("NATIVE_CASE_FILE_COVERAGE_INCOMPLETE");
  if (new Set(cases.map(requiredCaseKey)).size !== cases.length) failures.add("NATIVE_CASE_RESULT_DUPLICATE");
  const required = input.requiredCases.filter(test => test.runner === "vitest" && selectedFiles.includes(canonicalCaseFile(test.file)));
  for (const test of required) {
    const matches = cases.filter(actual => requiredCaseKey(actual) === requiredCaseKey(test));
    if (matches.length !== 1 || matches[0].required === false) failures.add("NATIVE_REQUIRED_CASE_MISSING");
  }
  return { passed: failures.size === 0, cases, failureCodes: [...failures].sort() };
}

/** A full-acceptance receipt or another candidate can never satisfy the PR profile. */
export function assertPrReceiptBinding(receipt: PrReceiptBinding, expected: { kind: "native" | "browser"; base: string; candidate: string; selectionFingerprint: string }): void {
  if (receipt.scope !== "pr-regression" || receipt.kind !== expected.kind) throw new Error("PR_RECEIPT_SCOPE_MISMATCH");
  if (receipt.base !== expected.base || receipt.candidate !== expected.candidate || receipt.selectionFingerprint !== expected.selectionFingerprint) throw new Error("PR_RECEIPT_CANDIDATE_MISMATCH");
  if (receipt.passed !== true) throw new Error("PR_RECEIPT_NOT_PASSED");
}

/** Every expected identity must appear once, pass without skipping, and clean up its own cohort. */
export function assertPrCaseCoverage(expected: readonly string[], observed: readonly PrCaseResult[], requireCleanup: boolean): void {
  if (new Set(expected).size !== expected.length || new Set(observed.map(item => item.key)).size !== observed.length) throw new Error("PR_CASE_RESULT_DUPLICATE");
  if (JSON.stringify([...expected].sort()) !== JSON.stringify(observed.map(item => item.key).sort())) throw new Error("PR_CASE_COVERAGE_INCOMPLETE");
  if (observed.some(item => !item.passed)) throw new Error("PR_CASE_DID_NOT_PASS");
  if (requireCleanup && observed.some(item => !item.cleanup?.completed || !item.cleanup.ownedDirectoryAbsent)) throw new Error("PR_CASE_CLEANUP_INCOMPLETE");
}
