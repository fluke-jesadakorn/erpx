import type { Suite } from "./catalogue";
import type { BrowserRunReceipt } from "./browser-runner";
import type { BrowserShard } from "./browser-sharding";
import { executiveWorkHostBrowserFile } from "./executive-work-host-cohort";
import { formsHostBrowserFile } from "./forms-host-cohort";
import { productHostBrowserFile } from "./product-host-cohort";

export type BrowserIsolationGroupId = "standard" | "product-host" | "executive-work" | "forms-host";
export type BrowserIsolationSuiteGroup = { id: BrowserIsolationGroupId; suites: Suite[] };
export type BrowserIsolationReceiptGroup<TReceipt extends { suite: string }> = {
  suites: readonly Pick<Suite, "id">[];
  receipts: readonly TReceipt[];
};

/** Keep browser suites that require an exact owned host in their own runner cohort. */
export function partitionBrowserIsolationSuites(suites: readonly Suite[]): BrowserIsolationSuiteGroup[] {
  if (!suites.length) throw new Error("BROWSER_SUITES_REQUIRED");

  const productHostSuites = suites.filter(suite => suite.files.includes(productHostBrowserFile));
  const executiveWorkSuites = suites.filter(suite => suite.files.includes(executiveWorkHostBrowserFile));
  const formsHostSuites = suites.filter(suite => suite.files.includes(formsHostBrowserFile));
  if (productHostSuites.length && (productHostSuites.length !== 1 || productHostSuites[0].files.length !== 1 || productHostSuites[0].files[0] !== productHostBrowserFile)) {
    throw new Error("EXACT_PRODUCT_HOST_SUITE_REQUIRED");
  }
  if (executiveWorkSuites.length && (executiveWorkSuites.length !== 1 || executiveWorkSuites[0].files.length !== 1 ||
    executiveWorkSuites[0].files[0] !== executiveWorkHostBrowserFile || executiveWorkSuites[0].fixture !== "executive-work")) {
    throw new Error("EXACT_EXECUTIVE_WORK_HOST_SUITE_AND_RELEASE_REQUIRED");
  }
  if (formsHostSuites.length && (formsHostSuites.length !== 1 || formsHostSuites[0].files.length !== 1 ||
    formsHostSuites[0].files[0] !== formsHostBrowserFile || formsHostSuites[0].fixture !== "engineering")) {
    throw new Error("EXACT_FORMS_HOST_SUITE_AND_RELEASE_REQUIRED");
  }

  const special = new Set([...productHostSuites, ...executiveWorkSuites, ...formsHostSuites].map(suite => suite.id));
  const standardSuites = suites.filter(suite => !special.has(suite.id));
  return [
    ...(standardSuites.length ? [{ id: "standard" as const, suites: standardSuites }] : []),
    ...(executiveWorkSuites.length ? [{ id: "executive-work" as const, suites: executiveWorkSuites }] : []),
    ...(productHostSuites.length ? [{ id: "product-host" as const, suites: productHostSuites }] : []),
    ...(formsHostSuites.length ? [{ id: "forms-host" as const, suites: formsHostSuites }] : []),
  ];
}

export type BrowserAcceptanceReceiptGroup = {
  suites: readonly Pick<Suite, "id">[];
  receipt: BrowserRunReceipt;
};

/** Merge isolated host-cohort receipts into the catalogue traversal order. */
export function mergeBrowserAcceptanceReceipts(
  suites: readonly Pick<Suite, "id">[],
  groups: readonly BrowserAcceptanceReceiptGroup[],
  caseOrder: "forward" | "reverse",
  shard: BrowserShard,
): BrowserRunReceipt {
  const receiptBySuite = new Map<string, BrowserRunReceipt>();
  for (const group of groups) {
    const ids = new Set(group.suites.map(suite => suite.id));
    if (ids.size !== group.suites.length) throw new Error("BROWSER_ISOLATION_SUITE_GROUP_DUPLICATE");
    if (group.receipt.passed !== true || group.receipt.execution?.mode !== "acceptance" || group.receipt.execution.caseOrder !== caseOrder ||
      JSON.stringify(group.receipt.shard) !== JSON.stringify(shard)) throw new Error("BROWSER_ISOLATION_GROUP_ACCEPTANCE_REQUIRED");
    for (const suite of group.suites) {
      if (receiptBySuite.has(suite.id) || !suites.some(candidate => candidate.id === suite.id)) throw new Error("BROWSER_ISOLATION_SUITE_GROUP_DUPLICATE");
      receiptBySuite.set(suite.id, group.receipt);
    }
    for (const test of [...group.receipt.inventory, ...group.receipt.receipts]) {
      if (!ids.has(test.suite)) throw new Error("BROWSER_ISOLATION_RECEIPT_GROUP_MISMATCH");
    }
  }
  if (receiptBySuite.size !== suites.length || suites.some(suite => !receiptBySuite.has(suite.id))) throw new Error("BROWSER_ISOLATION_SUITE_RECEIPTS_REQUIRED");

  const suiteOrder = caseOrder === "forward" ? [...suites] : [...suites].reverse();
  const projectOrder = caseOrder === "forward" ? ["desktop", "mobile"] as const : ["mobile", "desktop"] as const;
  const orderedCases = <T extends { suite: string; project: "desktop" | "mobile" }>(select: (receipt: BrowserRunReceipt) => readonly T[]) =>
    suiteOrder.flatMap(suite => projectOrder.flatMap(project => select(receiptBySuite.get(suite.id)!)
      .filter(test => test.suite === suite.id && test.project === project)));
  const first = groups[0]?.receipt;
  if (!first || groups.some(group => group.receipt.runtimeFingerprint !== first.runtimeFingerprint || group.receipt.verificationFingerprint !== first.verificationFingerprint ||
    group.receipt.interrupted !== null || group.receipt.cleanupFailures.length > 0)) throw new Error("BROWSER_ISOLATION_GROUP_CANDIDATE_MISMATCH");

  return {
    version: 1,
    passed: true,
    runtimeFingerprint: first.runtimeFingerprint,
    verificationFingerprint: first.verificationFingerprint,
    shard,
    interrupted: null,
    suites: suiteOrder.map(suite => suite.id),
    cleanupFailures: [],
    inventory: orderedCases(receipt => receipt.inventory),
    receipts: orderedCases(receipt => receipt.receipts),
    execution: { caseOrder, mode: "acceptance" },
  };
}

/** Reassemble lane receipts in catalogue order after isolated cohorts finish. */
export function browserIsolationReceiptsForLane<TReceipt extends { suite: string }>(
  suites: readonly Pick<Suite, "id" | "lane">[],
  groups: readonly BrowserIsolationReceiptGroup<TReceipt>[],
  lane: Suite["lane"],
): TReceipt[] {
  const receiptsBySuite = new Map<string, TReceipt[]>();
  for (const group of groups) {
    const groupSuiteIds = new Set(group.suites.map(suite => suite.id));
    if (groupSuiteIds.size !== group.suites.length || group.suites.some(suite => receiptsBySuite.has(suite.id))) {
      throw new Error("BROWSER_ISOLATION_SUITE_GROUP_DUPLICATE");
    }
    for (const receipt of group.receipts) {
      if (!groupSuiteIds.has(receipt.suite)) throw new Error("BROWSER_ISOLATION_RECEIPT_GROUP_MISMATCH");
    }
    for (const suite of group.suites) receiptsBySuite.set(suite.id, group.receipts.filter(receipt => receipt.suite === suite.id));
  }

  const laneSuites = suites.filter(suite => suite.lane === lane);
  for (const suite of laneSuites) if (!receiptsBySuite.has(suite.id)) throw new Error("BROWSER_ISOLATION_SUITE_RECEIPTS_REQUIRED");
  return laneSuites.flatMap(suite => receiptsBySuite.get(suite.id)!);
}
