import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { requiredUatMatrixCases } from "./uat-required-identities";
export { assertUatRouteSources } from "./uat-required-identities";

export type RequiredCaseIdentity = { runner: "vitest" | "playwright"; file: string; title: string; project?: "desktop" | "mobile" };
export type CaseObservation = RequiredCaseIdentity & { status?: string; required?: boolean };
export type CaseScope = { runner: RequiredCaseIdentity["runner"]; files: readonly string[]; projects?: readonly ("desktop" | "mobile")[]; titleMatches?: (test: RequiredCaseIdentity) => boolean };
export type NativeCaseReceipt = { version: 1; runner: "vitest"; files: string[]; passed: boolean; cases: CaseObservation[] };
export type BrowserCaseBaseline = { version: 1; cases: RequiredCaseIdentity[]; excludedCases: (RequiredCaseIdentity & { reason: string })[] };
const rootDirectory = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

export function canonicalCaseFile(file: string, files: readonly string[] = []): string {
  const normalized = file.replaceAll("\\", "/");
  const index = normalized.indexOf("tests/");
  if (index >= 0 && (index === 0 || normalized[index - 1] === "/")) return normalized.slice(index);
  const matches = files.filter(candidate => candidate === normalized || candidate.endsWith(`/${normalized}`));
  return matches.length === 1 ? matches[0] : normalized;
}
export function requiredCaseKey(test: RequiredCaseIdentity): string {
  return JSON.stringify([test.runner, canonicalCaseFile(test.file), test.title, test.project ?? ""]);
}

function requiredCaseIdentity(value: unknown): RequiredCaseIdentity {
  const test = value as RequiredCaseIdentity;
  if (!test || !["vitest", "playwright"].includes(test.runner) || typeof test.file !== "string" || !/^tests\/(?!.*(?:^|\/)\.\.\/).+\.(?:test|spec)\.[cm]?[jt]sx?$/.test(test.file) || typeof test.title !== "string" || !test.title.trim() ||
    (test.runner === "playwright" ? !["desktop", "mobile"].includes(test.project ?? "") : test.project !== undefined)) throw new Error("INVALID_REQUIRED_CASE_REGISTER");
  return { runner: test.runner, file: test.file, title: test.title, ...(test.project ? { project: test.project } : {}) };
}

/** New candidate requirements are recorded separately from retained historical registers. */
export function currentRequiredCases(value: unknown): RequiredCaseIdentity[] {
  const register = value as { version?: unknown; cases?: unknown };
  if (!register || register.version !== 1 || !Array.isArray(register.cases) || !register.cases.length) throw new Error("INVALID_CURRENT_REQUIRED_CASE_REGISTER");
  const cases = register.cases.map(requiredCaseIdentity);
  if (new Set(cases.map(requiredCaseKey)).size !== cases.length) throw new Error("DUPLICATE_CURRENT_REQUIRED_CASE");
  return cases;
}
/** An explicit reviewed baseline, never regenerated from candidate discovery at runtime. */
export function browserRequiredCaseBaseline(value: unknown): BrowserCaseBaseline {
  const register = value as { version?: unknown; cases?: unknown; excludedCases?: unknown };
  if (!register || register.version !== 1 || !Array.isArray(register.cases) || !register.cases.length || !Array.isArray(register.excludedCases)) throw new Error("INVALID_BROWSER_CASE_BASELINE");
  const cases = register.cases.map(requiredCaseIdentity);
  const excludedCases = register.excludedCases.map(value => {
    const identity = requiredCaseIdentity(value);
    const reason = (value as { reason?: unknown }).reason;
    if (typeof reason !== "string" || !reason.trim()) throw new Error("INVALID_BROWSER_CASE_BASELINE_EXCLUSION");
    return { ...identity, reason };
  });
  const all = [...cases, ...excludedCases];
  if (all.some(test => test.runner !== "playwright") || new Set(all.map(requiredCaseKey)).size !== all.length) throw new Error("INVALID_OR_DUPLICATE_BROWSER_CASE_BASELINE");
  return { version: 1, cases, excludedCases };
}
export function loadBrowserCaseBaseline(root = rootDirectory): BrowserCaseBaseline {
  return browserRequiredCaseBaseline(JSON.parse(readFileSync(resolve(root, "tests/catalogue/required-browser-cases.json"), "utf8")));
}
// This retained case calls the actual Stripe Sandbox. Moving any other required
// case into the provider lane must not silently remove internal acceptance.
const providerExemptions = new Set([requiredCaseKey({ runner: "vitest", file: "tests/provider/commercial/billing-stripe-sandbox.test.ts", title: "runs reviewed manual and auto collection against actual Stripe Sandbox, with isolated accounts and an injected clock" })]);
export function internalRequiredCases(required: readonly RequiredCaseIdentity[], suites: readonly { files: readonly string[]; lane: string }[]): RequiredCaseIdentity[] {
  return required.filter(test => {
    const suite = suites.find(suite => suite.files.includes(test.file));
    if (!suite) throw new Error(`REGISTERED_REQUIRED_CASE_WITHOUT_SUITE:${test.file}`);
    if (suite.lane !== "provider") return true;
    if (!providerExemptions.has(requiredCaseKey(test))) throw new Error(`REGISTERED_REQUIRED_CASE_CANNOT_BECOME_PROVIDER:${requiredCaseKey(test)}`);
    return false;
  });
}

/** Retained reviewed registers, independent of whatever a changed test file discovers. */
export function loadRequiredCases(root = rootDirectory): RequiredCaseIdentity[] {
  const read = (file: string): unknown => JSON.parse(readFileSync(resolve(root, "tests/catalogue", file), "utf8"));
  const manifest = read("engineering-scenarios.json") as { capabilities: { cases: unknown[] }[] };
  const gates = read("engineering-gate-scenarios.json") as Record<string, unknown[]>;
  const attachments = read("engineering-attachment-owner-cases.json") as { cases: unknown[] }[];
  const legal = read("legal-uat-engineering-cases.json") as { file: string; title: string; projects: unknown[] }[];
  const matrix = read("uat-route-inventory.json") as { entries: Parameters<typeof requiredUatMatrixCases>[0] };
  const current = currentRequiredCases(JSON.parse(readFileSync(resolve(root, "tests/catalogue/required-cases.json"), "utf8")));
  const baseline = loadBrowserCaseBaseline(root);
  const values = [...manifest.capabilities.flatMap(capability => capability.cases), ...Object.values(gates).flat(), ...attachments.flatMap(owner => owner.cases),
    ...legal.flatMap(test => test.projects.map(project => ({ runner: "playwright", file: test.file, title: test.title, project }))), ...requiredUatMatrixCases(matrix.entries), ...current, ...baseline.cases];
  const cases = new Map<string, RequiredCaseIdentity>();
  for (const value of values) {
    const identity = requiredCaseIdentity(value);
    cases.set(requiredCaseKey(identity), identity);
  }
  if (!cases.size) throw new Error("EMPTY_REQUIRED_CASE_REGISTER");
  return [...cases.values()].sort((a, b) => requiredCaseKey(a).localeCompare(requiredCaseKey(b)));
}

export function requiredCasesForScope(required: readonly RequiredCaseIdentity[], scope: CaseScope): RequiredCaseIdentity[] {
  return required.filter(test => test.runner === scope.runner && scope.files.includes(test.file) && (!scope.projects || !!test.project && scope.projects.includes(test.project)) && (!scope.titleMatches || scope.titleMatches(test)));
}
export function assertRequiredCaseInventory(required: readonly RequiredCaseIdentity[], observed: readonly CaseObservation[], scope: CaseScope): void {
  const normalized = observed.map(test => ({ ...test, file: canonicalCaseFile(test.file, scope.files) }));
  for (const test of requiredCasesForScope(required, scope)) {
    const matches = normalized.filter(actual => requiredCaseKey(actual) === requiredCaseKey(test));
    if (matches.length !== 1 || matches[0].required === false) throw new Error(`REGISTERED_REQUIRED_CASE_MISSING:${requiredCaseKey(test)}`);
  }
}
export function assertRequiredCaseResults(required: readonly RequiredCaseIdentity[], observed: readonly CaseObservation[], scope: CaseScope): void {
  assertRequiredCaseInventory(required, observed, scope);
  const normalized = observed.map(test => ({ ...test, file: canonicalCaseFile(test.file, scope.files) }));
  for (const test of requiredCasesForScope(required, scope)) if (normalized.find(actual => requiredCaseKey(actual) === requiredCaseKey(test))?.status !== "passed")
    throw new Error(`REGISTERED_REQUIRED_CASE_DID_NOT_PASS:${requiredCaseKey(test)}`);
}

export function readNativeCaseReceipt(file: string, selectedFiles: readonly string[], required = loadRequiredCases()): NativeCaseReceipt {
  const receipt = JSON.parse(readFileSync(file, "utf8")) as NativeCaseReceipt;
  if (receipt.version !== 1 || receipt.runner !== "vitest" || !receipt.passed || !Array.isArray(receipt.files) || !Array.isArray(receipt.cases) || !receipt.cases.length || receipt.cases.some(test => test.runner !== "vitest" || test.project !== undefined || !selectedFiles.includes(canonicalCaseFile(test.file)) || typeof test.title !== "string" || !test.title || test.status !== "passed") || JSON.stringify([...receipt.files].sort()) !== JSON.stringify([...selectedFiles].sort())) throw new Error("NATIVE_CASE_RECEIPT_REQUIRED");
  assertRequiredCaseResults(required, receipt.cases, { runner: "vitest", files: selectedFiles });
  return receipt;
}
