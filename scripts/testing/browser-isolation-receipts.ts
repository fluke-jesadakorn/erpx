import type { BrowserRunCaseReceipt, BrowserRunReceipt } from "./browser-runner";
import { assignedBrowserShard, browserCaseKey, type BrowserCaseIdentity, type BrowserShard } from "./browser-sharding";
import { canonicalCaseFile } from "./required-case-inventory";

export type BrowserIsolationProof = { forward: BrowserRunReceipt; reverse: BrowserRunReceipt; probe: BrowserRunReceipt };
export type FullRegressionBrowserCase = {
  runner: "playwright";
  suite: string;
  file: string;
  title: string;
  project: "desktop" | "mobile";
  status: "passed" | "skipped";
  required?: boolean;
};

export function browserRunCaseIdentity(test: BrowserRunCaseReceipt): BrowserCaseIdentity {
  if (!test.suite || !test.file || !test.title || !["desktop", "mobile"].includes(test.project)) throw new Error("ISOLATION_CASE_IDENTITY_REQUIRED");
  return { suite: test.suite, file: test.file, title: test.title, project: test.project };
}

export function serializeFullRegressionBrowserCases(
  receipts: readonly BrowserRunCaseReceipt[],
  suites: readonly { id: string; files: readonly string[] }[],
  lane: string,
): FullRegressionBrowserCase[] {
  return receipts.map(test => {
    const identity = browserRunCaseIdentity(test), owner = suites.find(suite => suite.id === identity.suite);
    if (!owner) throw new Error(`FULL_REGRESSION_BROWSER_CASE_SUITE_BINDING_REQUIRED:${lane}:${identity.suite}`);
    return {
      runner: "playwright",
      suite: identity.suite,
      file: canonicalCaseFile(identity.file, owner.files),
      title: identity.title,
      project: identity.project,
      status: test.passed ? "passed" : "skipped",
      required: test.required,
    };
  });
}

/** Repetition is separate evidence; it must never inflate unique acceptance coverage. */
export function validateBrowserIsolation(proof: BrowserIsolationProof, expected: { runtimeFingerprint: string; verificationFingerprint: string; shard?: BrowserShard }) {
  const runs = new Set<string>();
  const assigned = (test: BrowserRunReceipt["inventory"][number]) => !expected.shard || assignedBrowserShard(test.suite, test, expected.shard);
  const inventories = [proof.forward, proof.reverse].map(receipt => {
    if (!receipt || receipt.passed !== true || receipt.execution?.mode !== "acceptance") throw new Error("ISOLATION_ACCEPTANCE_PASS_REQUIRED");
    return receipt.inventory.map(test => JSON.stringify([browserCaseKey(test), test.required, test.reason ?? null])).sort();
  });
  if (JSON.stringify(inventories[0]) !== JSON.stringify(inventories[1])) throw new Error("ISOLATION_INVENTORY_MISMATCH");
  for (const [name, receipt] of Object.entries(proof)) {
    if (!receipt || receipt.runtimeFingerprint !== expected.runtimeFingerprint || receipt.verificationFingerprint !== expected.verificationFingerprint) throw new Error("ISOLATION_CANDIDATE_MISMATCH");
    if (receipt.version !== 1 || receipt.interrupted !== null || JSON.stringify(receipt.shard) !== JSON.stringify(name === "probe" ? null : expected.shard ?? null)) throw new Error("ISOLATION_EXECUTION_METADATA_MISMATCH");
    if (!Array.isArray(receipt.cleanupFailures) || receipt.cleanupFailures.length) throw new Error("ISOLATION_CLEANUP_FAILED");
    for (const test of receipt.receipts) {
      if (test.required === false) continue;
      if (!test.run || !/^[a-f0-9]{12}$/.test(test.run) || runs.has(test.run)) throw new Error("ISOLATION_COHORT_REUSED");
      runs.add(test.run);
      if (test.cleanup?.completed !== true || test.cleanup.ownedDirectoryAbsent !== true) throw new Error("ISOLATION_COHORT_CLEANUP_REQUIRED");
      if (name !== "probe" && (test.passed !== true || test.faultProbe)) throw new Error("ISOLATION_BUSINESS_PASS_REQUIRED");
    }
  }
  if (proof.forward.execution.caseOrder !== "forward" || proof.reverse.execution.caseOrder !== "reverse") throw new Error("ISOLATION_DISTINCT_CASE_ORDERS_REQUIRED");
  const forward = proof.forward.inventory.filter(test => test.required && assigned(test)).map(browserCaseKey);
  const reverse = proof.reverse.inventory.filter(test => test.required && assigned(test)).map(browserCaseKey);
  if (!forward.length || new Set(forward).size !== forward.length || JSON.stringify(reverse) !== JSON.stringify([...forward].reverse())) throw new Error("ISOLATION_REVERSED_EXECUTION_REQUIRED");
  for (const [receipt, identities] of [[proof.forward, forward], [proof.reverse, reverse]] as const) {
    const actual = receipt.receipts.filter(test => test.required !== false).map(test => browserCaseKey(browserRunCaseIdentity(test)));
    if (JSON.stringify(actual) !== JSON.stringify(identities)) throw new Error("ISOLATION_FULL_REPEAT_COVERAGE_REQUIRED");
  }
  const probe = proof.probe, declared = probe.execution?.faultProbe;
  if (probe.passed !== false || probe.execution?.mode !== "fault-probe" || !declared || !forward.includes(browserCaseKey(declared))) throw new Error("ISOLATION_DECLARED_FAILURE_REQUIRED");
  if (probe.receipts.length !== 1 || probe.receipts[0].passed !== false || probe.receipts[0].faultProbe !== true || probe.receipts[0].failure !== "BROWSER_DECLARED_FAULT_PROBE" || browserCaseKey(browserRunCaseIdentity(probe.receipts[0])) !== browserCaseKey(declared)) throw new Error("ISOLATION_EXACT_FAILURE_REQUIRED");
  return { cases: forward.length, rerunCases: reverse.length, faultProbe: true as const, caseOrders: ["forward", "reverse"] as const };
}
