import type { Reporter, TestModule } from "vitest/node";
import { writeFileSync } from "node:fs";
import { assertRequiredCaseResults, canonicalCaseFile, loadRequiredCases, type CaseObservation, type RequiredCaseIdentity } from "./required-case-inventory";

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

/** A green acceptance run cannot be built from missing, skipped or todo cases. */
export default class RequiredCasesReporter implements Reporter {
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
