import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { canonicalCaseFile, loadRequiredCases, requiredCaseKey, type RequiredCaseIdentity } from "./required-case-inventory";
import { repositoryRoot, type Catalogue, type Suite } from "./catalogue";

type ProductWorkflowRegister = { workflows?: { id?: unknown; cases?: unknown[] }[] };
type Project = "desktop" | "mobile";

function productWorkflowCases(root: string, registered: readonly RequiredCaseIdentity[]): Map<string, RequiredCaseIdentity[]> {
  const register = JSON.parse(readFileSync(resolve(root, "tests/catalogue/product-workflow-receipts.json"), "utf8")) as ProductWorkflowRegister;
  if (!Array.isArray(register.workflows)) throw new Error("INVALID_PRODUCT_WORKFLOW_CASE_REGISTER");
  const registeredByKey = new Map(registered.map(test => [requiredCaseKey(test), test]));
  const ids = new Map<string, RequiredCaseIdentity[]>();
  for (const workflow of register.workflows) {
    if (typeof workflow.id !== "string" || !workflow.id.trim() || !Array.isArray(workflow.cases)) throw new Error("INVALID_PRODUCT_WORKFLOW_CASE_REGISTER");
    const cases = workflow.cases.flatMap(value => {
      const identity = value as RequiredCaseIdentity;
      if (!identity || typeof identity !== "object" || typeof identity.runner !== "string" || typeof identity.file !== "string" || typeof identity.title !== "string") return [];
      const exact = registeredByKey.get(requiredCaseKey(identity));
      return exact ? [exact] : [];
    });
    ids.set(workflow.id, [...new Map(cases.map(test => [requiredCaseKey(test), test])).values()]);
  }
  return ids;
}

/** Resolve a unique exact registered case by workflow id, title, or file::title. */
export function resolveRegisteredCase(selector: string, options: { root?: string; project?: Project } = {}): RequiredCaseIdentity {
  const root = options.root ?? repositoryRoot;
  const value = selector.trim();
  if (!value) throw new Error("TEST_CASE_SELECTOR_REQUIRED");
  const registered = loadRequiredCases(root);
  const workflowCases = productWorkflowCases(root, registered);
  let matches: RequiredCaseIdentity[];

  if (value.startsWith("id:")) {
    matches = workflowCases.get(value.slice(3)) ?? [];
  } else if (value.startsWith("title:")) {
    matches = registered.filter(test => test.title === value.slice(6));
  } else {
    const separator = value.indexOf("::");
    if (separator > 0) {
      const files = [...new Set(registered.map(test => test.file))];
      const file = canonicalCaseFile(value.slice(0, separator), files), title = value.slice(separator + 2);
      matches = registered.filter(test => canonicalCaseFile(test.file) === file && test.title === title);
    } else {
      const byId = workflowCases.get(value) ?? [];
      const byTitle = registered.filter(test => test.title === value);
      matches = [...new Map([...byId, ...byTitle].map(test => [requiredCaseKey(test), test])).values()];
    }
  }
  if (options.project) matches = matches.filter(test => test.runner === "vitest" || test.project === options.project);
  if (!matches.length) throw new Error(`UNKNOWN_REGISTERED_TEST_CASE:${value}`);
  if (matches.length !== 1) throw new Error(`AMBIGUOUS_REGISTERED_TEST_CASE:${value}`);
  return matches[0];
}

export function suiteForRegisteredCase(catalogue: Catalogue, test: RequiredCaseIdentity): Suite {
  const matches = catalogue.suites.filter(suite => suite.files.includes(test.file));
  if (!matches.length) throw new Error(`REGISTERED_TEST_CASE_WITHOUT_SUITE:${test.file}`);
  if (matches.length !== 1) throw new Error(`AMBIGUOUS_REGISTERED_TEST_CASE_SUITE:${test.file}`);
  return matches[0];
}

export function exactNativeCasePattern(title: string): string {
  return `^${title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`;
}
