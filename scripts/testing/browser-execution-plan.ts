import { browserCaseKey, type BrowserCaseIdentity } from "./browser-sharding";
import type { BrowserCase } from "./browser-results";

export type BrowserCaseOrder = "forward" | "reverse";
export const browserFaultProbeError = "BROWSER_DECLARED_FAULT_PROBE";

export function parseBrowserCaseOrder(value: string): BrowserCaseOrder {
  if (value !== "forward" && value !== "reverse") throw new Error("INVALID_BROWSER_CASE_ORDER");
  return value;
}

export function selectBrowserCaseOrder(previous: BrowserCaseOrder | undefined, value: string): BrowserCaseOrder {
  if (previous !== undefined) throw new Error("DUPLICATE_BROWSER_CASE_ORDER");
  return parseBrowserCaseOrder(value);
}

/** Copy, then reverse the complete traversal, without changing case identities. */
export function orderedBrowserExecutionGroups<S extends { id: string; files: string[] }>(suites: S[], projects: ("desktop" | "mobile")[], order: BrowserCaseOrder) {
  parseBrowserCaseOrder(order);
  if (!projects.length || projects.some(project => !["desktop", "mobile"].includes(project)) || new Set(projects).size !== projects.length || new Set(suites.map(suite => suite.id)).size !== suites.length) throw new Error("INVALID_BROWSER_EXECUTION_GROUPS");
  const groups = suites.flatMap(suite => projects.map(project => ({ suite, project })));
  return order === "reverse" ? groups.reverse() : groups;
}

export function orderedBrowserExecutionCases<T>(cases: T[], order: BrowserCaseOrder): T[] {
  parseBrowserCaseOrder(order);
  return order === "reverse" ? [...cases].reverse() : [...cases];
}

export function assertBrowserFaultProbeSelection(probe: BrowserCaseIdentity | undefined, groups: { suite: { id: string; files: string[] }; project: "desktop" | "mobile" }[]) {
  if (!probe) return;
  if (groups.length !== 1 || !probe.title?.trim() || groups[0].suite.id !== probe.suite || groups[0].project !== probe.project || !groups[0].suite.files.some(file => browserCaseKey({ ...probe, file }) === browserCaseKey(probe))) throw new Error("EXACT_BROWSER_FAULT_PROBE_SELECTION_REQUIRED");
}

export function isBrowserFaultProbeCase(suite: string, test: Pick<BrowserCase, "file" | "project" | "title">, probe: BrowserCaseIdentity | undefined): boolean {
  return Boolean(probe && browserCaseKey({ ...test, suite }) === browserCaseKey(probe));
}

/** This failure proves the harness cleanup path; it is never a business result. */
export function triggerBrowserFaultProbe(suite: string, test: Pick<BrowserCase, "file" | "project" | "title">, probe: BrowserCaseIdentity | undefined): void {
  if (isBrowserFaultProbeCase(suite, test, probe)) throw new Error(browserFaultProbeError);
}
