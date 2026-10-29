import type { Reporter, TestCase, TestModule } from "vitest/node";
import { writeFileSync } from "node:fs";
import { assertRequiredCaseResults, canonicalCaseFile, loadRequiredCases, type CaseObservation, type RequiredCaseIdentity } from "./required-case-inventory";
import { loadCatalogue } from "./catalogue";

type NamedTask = { name?: string; type?: string; parent?: NamedTask };
type RequiredTest = NamedTask & { fullName: string; result(): { state: string } };
type RequiredModule = { relativeModuleId: string; children: { allTests(): Iterable<RequiredTest> } };
/** Vitest 4's reporter fullName uses ` > `; retained JSON identities use actual ancestor names joined with spaces. */
export function nativeCaseTitle(test: RequiredTest): string {
  if (test.name === undefined) return test.fullName;
  const names = [test.name];
  for (let parent = test.parent; parent && parent.type !== "module"; parent = parent.parent) if (parent.name !== undefined) names.unshift(parent.name);
  return names.join(" ");
}
export function nativeCaseObservations(modules: ReadonlyArray<RequiredModule>, selected: string[], pattern?: RegExp): CaseObservation[] {
  return modules.flatMap(module => [...module.children.allTests()].filter(test => !pattern || pattern.test(nativeCaseTitle(test))).map(test =>
    ({ runner: "vitest" as const, file: canonicalCaseFile(module.relativeModuleId, selected), title: nativeCaseTitle(test), status: test.result().state })));
}
export function requiredCaseFailures(modules: ReadonlyArray<RequiredModule>, selected: string[], pattern?: RegExp, registered: readonly RequiredCaseIdentity[] = []) {
  const collected = new Set(modules.map(item => item.relativeModuleId.replaceAll("\\", "/")));
  const cases = modules.flatMap(item => [...item.children.allTests()]).filter(test => !pattern || pattern.test(nativeCaseTitle(test)));
  const missing = selected.filter(file => !collected.has(file));
  try { assertRequiredCaseResults(registered, nativeCaseObservations(modules, selected, pattern), { runner: "vitest", files: selected, titleMatches: test => !pattern || pattern.test(test.title) }); }
  catch (error) { missing.push(String(error)); }
  return { missing, skipped: cases.filter(test => ["skipped", "pending"].includes(test.result().state)).map(test => test.fullName), empty: !cases.length };
}

const diagnosticSqlStates = new Set(["23503", "23505", "23514", "40001", "40P01", "53200", "57014"]);
type DiagnosticError = { name?: unknown; message?: unknown; code?: unknown; stack?: unknown; stacks?: unknown };
/** Only reviewed identity and narrowly structured error facts may leave the worker. */
export function failedCaseDiagnostic(file: string, title: string, errors: readonly DiagnosticError[], selected: readonly string[], registered: readonly RequiredCaseIdentity[], knownFiles: readonly string[]) {
  if (!selected.includes(file) || !knownFiles.includes(file)) return undefined;
  const identity = registered.find(test => test.runner === "vitest" && test.file === file && test.title === title);
  const diagnostics = errors.slice(0, 4).map(error => {
    const timeout = typeof error.message === "string" ? /^Test timed out in (\d{1,8})ms\.(?:\n|$)/.exec(error.message) : null;
    const code = typeof error.code === "string" && diagnosticSqlStates.has(error.code) ? error.code : undefined;
    const category = timeout ? "timeout" : code ? "database" : error.name === "AssertionError" ? "assertion" : "error";
    const frames: { line: number; column: number }[] = [];
    const root = process.cwd().replaceAll("\\", "/").replace(/\/$/, "");
    const add = (path: unknown, line: unknown, column: unknown) => {
      if (typeof path !== "string" || path.replaceAll("\\", "/") !== `${root}/${file}` || !Number.isSafeInteger(line) || !Number.isSafeInteger(column) || (line as number) < 1 || (column as number) < 1 || frames.length >= 4) return;
      frames.push({line: line as number, column: column as number});
    };
    if (Array.isArray(error.stacks)) for (const frame of error.stacks.slice(0, 32)) if (frame && typeof frame === "object") add(frame.file, frame.line, frame.column);
    if (typeof error.stack === "string") for (const line of error.stack.slice(0, 16_384).split("\n").slice(0, 32)) {
      const match = /^\s*at (?:[^()\n]* \()?([^()\n]+):(\d+):(\d+)\)?$/.exec(line);
      if (match) add(match[1], Number(match[2]), Number(match[3]));
    }
    return { category, ...(code ? {code} : {}), ...(timeout ? {timeoutMs: Number(timeout[1])} : {}), ...(frames.length ? {frames} : {}) };
  });
  return {file, title: identity?.title ?? "[unregistered case]", errors: diagnostics};
}

/** A green acceptance run cannot be built from missing, skipped or todo cases. */
export default class RequiredCasesReporter implements Reporter {
  onTestCaseResult(test: TestCase): void {
    const result = test.result();
    if (result.state !== "failed") return;
    const selected = JSON.parse(process.env.ERPX_SELECTED_TEST_FILES ?? "[]") as string[];
    const file = canonicalCaseFile(test.module.relativeModuleId, selected);
    const knownFiles = loadCatalogue().suites.filter(suite => suite.framework !== "cargo" && ["unit", "integration", "provider"].includes(suite.lane)).flatMap(suite => suite.files);
    const diagnostic = failedCaseDiagnostic(file, nativeCaseTitle(test), result.errors, selected, loadRequiredCases(), knownFiles);
    if (diagnostic) console.error(`TEST_CASE_FAILURE_DIAGNOSTIC:${JSON.stringify(diagnostic)}`);
  }
  onTestRunEnd(modules: ReadonlyArray<TestModule>): void {
    const selected = JSON.parse(process.env.ERPX_SELECTED_TEST_FILES ?? "[]") as string[];
    const pattern = process.env.ERPX_TEST_NAME_PATTERN ? new RegExp(process.env.ERPX_TEST_NAME_PATTERN) : undefined;
    const files = selected.length ? selected : modules.map(module => canonicalCaseFile(module.relativeModuleId));
    const cases = nativeCaseObservations(modules, files, pattern);
    const { missing, skipped, empty } = requiredCaseFailures(modules, files, pattern, loadRequiredCases());
    const passed = !missing.length && !skipped.length && !empty && cases.every(test => test.status === "passed");
    if (process.env.ERPX_TEST_CASE_RECEIPT) writeFileSync(process.env.ERPX_TEST_CASE_RECEIPT, JSON.stringify({ version: 1, runner: "vitest", files, passed, cases }, null, 2) + "\n", { mode: 0o600 });
    if (missing.length || skipped.length || empty) {
      console.error(`REQUIRED_TEST_CASES_NOT_EXECUTED:missing=${missing.join(",")};skipped=${skipped.join(" | ")};empty=${empty}`);
      process.exitCode = 1;
    }
  }
}
