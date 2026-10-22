import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { BrowserFaultProbeError, runBrowserSuites, type BrowserRunReceipt } from "./browser-runner";
import { repositoryRoot, type Suite } from "./catalogue";
import type { BrowserShard } from "./browser-sharding";
import { browserRunCaseIdentity, validateBrowserIsolation, type BrowserIsolationProof } from "./browser-isolation-receipts";
import type { ProductHostPlanPaths } from "./product-host-cohort";

async function retainedReceipt(evidenceDirectory: string): Promise<BrowserRunReceipt> {
  return JSON.parse(await readFile(join(evidenceDirectory, "receipt.json"), "utf8")) as BrowserRunReceipt;
}

export function assertCohortsDestroyed(receipt: BrowserRunReceipt) {
  if (receipt.cleanupFailures.length) throw new Error("ISOLATION_CLEANUP_FAILED");
  for (const test of receipt.receipts) {
    if (test.required === false) continue;
    if (!test.run || !/^[a-f0-9]{12}$/.test(test.run) || !test.cleanup?.completed || !test.cleanup.ownedDirectoryAbsent || existsSync(join(repositoryRoot, ".runtime/browser", test.run))) throw new Error("ISOLATION_COHORT_CLEANUP_REQUIRED");
  }
}

/** Prove every selected case twice and restart after a real, declared harness failure. */
export async function runBrowserIsolation(suites: Suite[], options: { releaseDirectory?: string; port?: 3102 | 3104; shard?: BrowserShard; productHostPlanPath?: string; productHostPlanPaths?: ProductHostPlanPaths; formsHostPlanPath?: string; onEvidence?: (stage: "forward" | "probe" | "reverse", directory: string) => Promise<void> } = {}) {
  console.log("Browser isolation: forward acceptance");
  const forwardResult = await runBrowserSuites(suites, { releaseDirectory: options.releaseDirectory, port: options.port, shard: options.shard, productHostPlanPath: options.productHostPlanPath, productHostPlanPaths: options.productHostPlanPaths, formsHostPlanPath: options.formsHostPlanPath, caseOrder: "forward" });
  const forward = { result: forwardResult, receipt: await retainedReceipt(forwardResult.evidenceDirectory) };
  await options.onEvidence?.("forward", forwardResult.evidenceDirectory);
  assertCohortsDestroyed(forward.receipt);
  const wanted = forward.receipt.receipts.find(test => test.required !== false && test.passed);
  if (!wanted) throw new Error("ISOLATION_EXECUTABLE_PROBE_CASE_REQUIRED");
  const suite = suites.find(candidate => candidate.id === wanted.suite);
  if (!suite) throw new Error("ISOLATION_PROBE_SUITE_REQUIRED");
  const identity = browserRunCaseIdentity(wanted);
  let probeDirectory: string;
  console.log("Browser isolation: declared failure and cleanup");
  try {
    await runBrowserSuites([suite], { releaseDirectory: options.releaseDirectory, port: options.port, productHostPlanPath: options.productHostPlanPath, productHostPlanPaths: options.productHostPlanPaths, formsHostPlanPath: options.formsHostPlanPath, projects: [wanted.project], faultProbe: identity });
    throw new Error("ISOLATION_EXPECTED_FAILURE_DID_NOT_OCCUR");
  } catch (error) {
    if (!(error instanceof BrowserFaultProbeError)) throw error;
    probeDirectory = error.evidenceDirectory;
  }
  const probe = { evidenceDirectory: probeDirectory, receipt: await retainedReceipt(probeDirectory) };
  await options.onEvidence?.("probe", probeDirectory);
  assertCohortsDestroyed(probe.receipt);
  console.log("Browser isolation: reverse acceptance after failure");
  const reverseResult = await runBrowserSuites(suites, { releaseDirectory: options.releaseDirectory, port: options.port, shard: options.shard, productHostPlanPath: options.productHostPlanPath, productHostPlanPaths: options.productHostPlanPaths, formsHostPlanPath: options.formsHostPlanPath, caseOrder: "reverse" });
  const reverse = { result: reverseResult, receipt: await retainedReceipt(reverseResult.evidenceDirectory) };
  await options.onEvidence?.("reverse", reverseResult.evidenceDirectory);
  assertCohortsDestroyed(reverse.receipt);
  const proof: BrowserIsolationProof = { forward: forward.receipt, reverse: reverse.receipt, probe: probe.receipt };
  const acceptance = validateBrowserIsolation(proof, { runtimeFingerprint: forwardResult.runtimeFingerprint, verificationFingerprint: forwardResult.verificationFingerprint, shard: options.shard });
  return { forward, reverse, probe, proof, acceptance };
}
