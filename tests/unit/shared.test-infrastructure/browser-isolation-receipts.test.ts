import { describe, expect, it } from "vitest";
import { validateBrowserIsolation, type BrowserIsolationProof } from "../../../scripts/testing/browser-isolation-receipts";
import type { BrowserRunReceipt } from "../../../scripts/testing/browser-runner";
import type { BrowserInventoryCase } from "../../../scripts/testing/browser-sharding";

const expected = { runtimeFingerprint: "a".repeat(64), verificationFingerprint: "b".repeat(64) };
function fixture(): BrowserIsolationProof {
  const inventory: BrowserInventoryCase[] = [
    { suite: "sales.e2e", file: "tests/e2e/sales.spec.ts", project: "desktop", title: "save quotation", required: true },
    { suite: "sales.e2e", file: "tests/e2e/sales.spec.ts", project: "mobile", title: "save quotation", required: true },
    { suite: "sales.e2e", file: "tests/e2e/sales.spec.ts", project: "mobile", title: "desktop rail", required: false, reason: "Desktop shell only." },
  ];
  const pass = (caseOrder: "forward" | "reverse", offset: number): BrowserRunReceipt => {
    const ordered = caseOrder === "forward" ? inventory : [...inventory].reverse();
    return { version: 1, passed: true, ...expected, shard: null, interrupted: null, suites: ["sales.e2e"], cleanupFailures: [], execution: { mode: "acceptance", caseOrder }, inventory: structuredClone(ordered), receipts: ordered.map((test, index) => test.required ? { ...test, required: undefined, passed: true, run: (offset + index).toString(16).padStart(12, "0"), cleanup: { completed: true, ownedDirectoryAbsent: true } } : { ...test, required: false }) };
  };
  const forward = pass("forward", 1), reverse = pass("reverse", 10), faultProbe = inventory[0];
  const probe: BrowserRunReceipt = { version: 1, passed: false, ...expected, shard: null, interrupted: null, suites: ["sales.e2e"], cleanupFailures: [], execution: { mode: "fault-probe", caseOrder: "forward", faultProbe }, inventory: [faultProbe], receipts: [{ ...faultProbe, required: undefined, passed: false, faultProbe: true, failure: "BROWSER_DECLARED_FAULT_PROBE", run: "000000000020", cleanup: { completed: true, ownedDirectoryAbsent: true } }] };
  return { forward, reverse, probe };
}

describe("independent browser repetition and failure restart acceptance", () => {
  it("counts business identities once while requiring a complete reverse pass after declared failure cleanup", () => {
    expect(validateBrowserIsolation(fixture(), expected)).toEqual({ cases: 2, rerunCases: 2, faultProbe: true, caseOrders: ["forward", "reverse"] });
  });
  it("rejects a missing repeated case even when forward acceptance passed", () => {
    const proof = fixture(); proof.reverse.receipts.pop();
    expect(() => validateBrowserIsolation(proof, expected)).toThrow("ISOLATION_FULL_REPEAT_COVERAGE_REQUIRED");
  });
  it("rejects reused case data across passes and an incompletely destroyed failure cohort", () => {
    const reused = fixture(); reused.reverse.receipts[1].run = reused.forward.receipts[0].run;
    expect(() => validateBrowserIsolation(reused, expected)).toThrow("ISOLATION_COHORT_REUSED");
    const unclean = fixture(); unclean.probe.receipts[0].cleanup!.ownedDirectoryAbsent = false;
    expect(() => validateBrowserIsolation(unclean, expected)).toThrow("ISOLATION_COHORT_CLEANUP_REQUIRED");
  });
  it("rejects candidate drift and retained cleanup failures in any experiment phase", () => {
    for (const stage of ["forward", "reverse", "probe"] as const) {
      const changed = fixture(); changed[stage].verificationFingerprint = "c".repeat(64);
      expect(() => validateBrowserIsolation(changed, expected)).toThrow("ISOLATION_CANDIDATE_MISMATCH");
      const dirty = fixture(); dirty[stage].cleanupFailures.push("database remains");
      expect(() => validateBrowserIsolation(dirty, expected)).toThrow("ISOLATION_CLEANUP_FAILED");
    }
  });
  it("rejects interrupted acceptance and shard provenance drift", () => {
    const interrupted = fixture(); interrupted.reverse.interrupted = "SIGTERM";
    expect(() => validateBrowserIsolation(interrupted, expected)).toThrow("ISOLATION_EXECUTION_METADATA_MISMATCH");
    const changed = fixture(); changed.forward.shard = { index: 1, total: 2 };
    expect(() => validateBrowserIsolation(changed, expected)).toThrow("ISOLATION_EXECUTION_METADATA_MISMATCH");
  });
  it("rejects a second forward run labelled as reverse and changed project exclusions", () => {
    const reordered = fixture(); reordered.reverse.inventory.reverse(); reordered.reverse.receipts.reverse();
    expect(() => validateBrowserIsolation(reordered, expected)).toThrow("ISOLATION_REVERSED_EXECUTION_REQUIRED");
    const excluded = fixture(); excluded.reverse.inventory[0].reason = "silently changed scope";
    expect(() => validateBrowserIsolation(excluded, expected)).toThrow("ISOLATION_INVENTORY_MISMATCH");
  });
  it("rejects an unexpected business failure, unexpected probe success, and an undeclared injected error", () => {
    const business = fixture(); business.reverse.receipts[1].passed = false;
    expect(() => validateBrowserIsolation(business, expected)).toThrow("ISOLATION_BUSINESS_PASS_REQUIRED");
    const success = fixture(); success.probe.passed = true;
    expect(() => validateBrowserIsolation(success, expected)).toThrow("ISOLATION_DECLARED_FAILURE_REQUIRED");
    const ordinaryFailure = fixture(); ordinaryFailure.probe.receipts[0].failure = "browser timeout";
    expect(() => validateBrowserIsolation(ordinaryFailure, expected)).toThrow("ISOLATION_EXACT_FAILURE_REQUIRED");
  });
});
