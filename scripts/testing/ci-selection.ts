import type { Catalogue, Suite } from "./catalogue";
import type { ChangedFile, ImpactResult } from "./impact";
import { requiredCaseKey, type RequiredCaseIdentity } from "./required-case-inventory";

export const prBrowserBaseline = ["identity_access.auth-smoke.e2e", "finance.demo-controlled-payment.e2e"] as const;
const nativeLane = (suite: Suite) => suite.lane === "unit" || suite.lane === "integration";
const browserLane = (suite: Suite) => suite.lane === "e2e" || suite.lane === "journeys";
const fullBrowserSuite = (suite: Suite) => browserLane(suite) || suite.lane === "uat";
const productHostsSuite = (suite: Suite) => suite.id === "identity_access.product-hosts.e2e" || suite.files.includes("tests/e2e/identity_access/product-hosts.spec.ts");

export type CiSuiteReason = { id: string; lane: Suite["lane"]; reasons: string[] };
export type CiSelection = {
  version: 1;
  scope: "pr-regression";
  impact: Pick<ImpactResult, "mode" | "reasons" | "modules" | "changed">;
  baseCriticalFallback: boolean;
  native: Suite[];
  browser: Suite[];
  reasons: CiSuiteReason[];
  deferredToFull: CiSuiteReason[];
  providerExcluded: string[];
  counts: {
    candidateNativeSuites: number;
    selectedNativeSuites: number;
    candidatePrBrowserSuites: number;
    candidateFullBrowserSuites: number;
    selectedBrowserSuites: number;
    candidateRequiredNativeCases: number;
    selectedRequiredNativeCases: number;
    candidateRequiredBrowserCases: number;
    selectedRequiredBrowserCases: number;
  };
};

function assertNativeClassifications(catalogue: Catalogue): void {
  for (const suite of catalogue.suites.filter(nativeLane))
    if (!suite.ci || typeof suite.ci.critical !== "boolean" || !suite.ci.reason.trim()) throw new Error(`NATIVE_CI_CLASSIFICATION_REQUIRED:${suite.id}`);
}

/** Carry base criticality by file identity so a move or manifest reclassification cannot hide it. */
function baseCriticalCandidates(base: Catalogue, head: Catalogue, changed: ChangedFile[]) {
  const oldNative = base.suites.filter(nativeLane);
  const fallback = oldNative.some(suite => !suite.ci || typeof suite.ci.critical !== "boolean" || !suite.ci.reason.trim());
  if (fallback) return { fallback: true, suites: [] as Suite[] };

  const candidates = new Map<string, Suite>();
  for (const suite of oldNative.filter(item => item.ci!.critical)) {
    for (const file of suite.files) {
      const change = changed.find(item => item.previousPath === file || item.path === file);
      if (change?.status.startsWith("D") && change.path === file && !change.previousPath) throw new Error(`BASE_CRITICAL_TEST_REMOVED:${suite.id}:${file}`);
      const candidateFile = change?.previousPath === file ? change.path : file;
      const replacements = head.suites.filter(nativeLane).filter(item => item.files.includes(candidateFile));
      if (!replacements.length) {
        const moved = changed.some(item => item.previousPath === file && item.path !== file);
        if (!moved) throw new Error(`BASE_CRITICAL_TEST_UNMAPPED:${suite.id}:${candidateFile}`);
        const browserReplacement = head.suites.find(item => item.files.includes(candidateFile));
        if (!browserReplacement || !browserLane(browserReplacement) || productHostsSuite(browserReplacement))
          throw new Error(`BASE_CRITICAL_TEST_RECLASSIFIED_UNSUPPORTED:${suite.id}:${candidateFile}`);
        candidates.set(browserReplacement.id, browserReplacement);
        continue;
      }
      for (const replacement of replacements) candidates.set(replacement.id, replacement);
    }
  }
  return { fallback: false, suites: [...candidates.values()] };
}

function reasonsForCounts(suites: readonly Suite[], required: readonly RequiredCaseIdentity[]) {
  const files = new Set(suites.flatMap(suite => suite.files));
  return required.filter(test => files.has(test.file));
}

export function selectCiSuites(input: {
  impact: ImpactResult;
  baseCatalogue: Catalogue;
  headCatalogue: Catalogue;
  requiredCases?: readonly RequiredCaseIdentity[];
}): CiSelection {
  const { impact, baseCatalogue, headCatalogue } = input;
  const requiredCases = input.requiredCases ?? [];
  assertNativeClassifications(headCatalogue);
  const headNative = headCatalogue.suites.filter(nativeLane);
  const allBrowser = headCatalogue.suites.filter(browserLane);
  const allFullBrowser = headCatalogue.suites.filter(suite => fullBrowserSuite(suite));
  const browser = allBrowser.filter(suite => !productHostsSuite(suite));
  for (const id of prBrowserBaseline) {
    const suite = headCatalogue.suites.find(item => item.id === id);
    if (!suite || !browserLane(suite) || productHostsSuite(suite)) throw new Error(`PR_BROWSER_BASELINE_REQUIRED:${id}`);
  }

  const baseCritical = baseCriticalCandidates(baseCatalogue, headCatalogue, impact.changed);
  const nativeReasons = new Map<string, Set<string>>(), browserReasons = new Map<string, Set<string>>();
  const add = (map: Map<string, Set<string>>, suite: Suite, reason: string) => {
    const reasons = map.get(suite.id) ?? new Set<string>();
    reasons.add(reason); map.set(suite.id, reasons);
  };

  if (impact.mode === "full" || baseCritical.fallback) {
    const reason = baseCritical.fallback ? "base criticality is absent or incomplete; run all native suites for this transition" : `conservative impact fallback: ${impact.reasons.join(",")}`;
    for (const suite of headNative) add(nativeReasons, suite, reason);
    for (const suite of browser) add(browserReasons, suite, reason);
  } else {
    for (const suite of headNative.filter(item => item.ci!.critical)) add(nativeReasons, suite, `critical on candidate: ${suite.ci!.reason}`);
    for (const suite of baseCritical.suites) {
      if (nativeLane(suite)) add(nativeReasons, suite, "critical on base; carried across this change");
      else if (browserLane(suite)) add(browserReasons, suite, "critical native case moved to an executable PR browser suite");
    }
    for (const suite of impact.suites) {
      if (nativeLane(suite)) add(nativeReasons, suite, "affected by base/head ownership or reverse dependencies");
      else if (browserLane(suite) && !productHostsSuite(suite)) add(browserReasons, suite, "affected by base/head ownership or reverse dependencies");
    }
  }
  for (const id of prBrowserBaseline) add(browserReasons, headCatalogue.suites.find(item => item.id === id)!, "required PR browser baseline");

  const native = headNative.filter(suite => nativeReasons.has(suite.id));
  const selectedBrowser = browser.filter(suite => browserReasons.has(suite.id));
  const reasons: CiSuiteReason[] = [...native, ...selectedBrowser].map(suite => ({ id: suite.id, lane: suite.lane, reasons: [...(nativeReasons.get(suite.id) ?? browserReasons.get(suite.id) ?? [])].sort() }));
  const selectedIds = new Set([...native, ...selectedBrowser].map(suite => suite.id));
  const deferredToFull = headCatalogue.suites.filter(suite => !selectedIds.has(suite.id) && suite.lane !== "provider").map(suite => ({
    id: suite.id,
    lane: suite.lane,
    reasons: [suite.lane === "uat" ? "full profile only: UAT environment plan" : productHostsSuite(suite) ? "full profile only: product-host environment plan" : "not critical and unaffected by this candidate"],
  }));

  const candidateRequiredNative = reasonsForCounts(headNative, requiredCases.filter(test => test.runner === "vitest"));
  const selectedRequiredNative = reasonsForCounts(native, requiredCases.filter(test => test.runner === "vitest"));
  const candidateRequiredBrowser = reasonsForCounts(allFullBrowser, requiredCases.filter(test => test.runner === "playwright"));
  const selectedRequiredBrowser = reasonsForCounts(selectedBrowser, requiredCases.filter(test => test.runner === "playwright"));
  const uniqueCount = (cases: readonly RequiredCaseIdentity[]) => new Set(cases.map(requiredCaseKey)).size;

  return {
    version: 1,
    scope: "pr-regression",
    impact: { mode: impact.mode, reasons: impact.reasons, modules: impact.modules, changed: impact.changed },
    baseCriticalFallback: baseCritical.fallback,
    native,
    browser: selectedBrowser,
    reasons,
    deferredToFull,
    providerExcluded: headCatalogue.suites.filter(suite => suite.lane === "provider").map(suite => suite.id),
    counts: {
      candidateNativeSuites: headNative.length,
      selectedNativeSuites: native.length,
      candidatePrBrowserSuites: browser.length,
      candidateFullBrowserSuites: allFullBrowser.length,
      selectedBrowserSuites: selectedBrowser.length,
      candidateRequiredNativeCases: uniqueCount(candidateRequiredNative),
      selectedRequiredNativeCases: uniqueCount(selectedRequiredNative),
      candidateRequiredBrowserCases: uniqueCount(candidateRequiredBrowser),
      selectedRequiredBrowserCases: uniqueCount(selectedRequiredBrowser),
    },
  };
}

export function assertPrSelection(selection: CiSelection, headCatalogue: Catalogue): void {
  const expectedNative = headCatalogue.suites.filter(suite => nativeLane(suite) && selection.native.some(selected => selected.id === suite.id));
  const expectedBrowser = headCatalogue.suites.filter(suite => browserLane(suite) && !productHostsSuite(suite) && selection.browser.some(selected => selected.id === suite.id));
  if (expectedNative.length !== selection.native.length || expectedBrowser.length !== selection.browser.length) throw new Error("PR_SELECTION_CONTAINS_UNSUPPORTED_OR_DUPLICATE_SUITE");
  for (const id of prBrowserBaseline) if (!selection.browser.some(suite => suite.id === id)) throw new Error(`PR_BROWSER_BASELINE_DROPPED:${id}`);
  if (selection.browser.some(suite => suite.lane === "uat" || productHostsSuite(suite))) throw new Error("PR_SELECTION_CONTAINS_FULL_ONLY_BROWSER_SUITE");
  const selectedIds = [...selection.native, ...selection.browser].map(suite => suite.id);
  if (new Set(selectedIds).size !== selectedIds.length || !selection.native.length) throw new Error("INVALID_OR_EMPTY_PR_SELECTION");
}
