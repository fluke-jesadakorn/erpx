import { expect, it } from "vitest";
import { assertBrowserCasePassed, browserCaseProjectRequirement, browserProjects, browserCases, exactTitlePattern } from "../../scripts/testing/browser-results";
import { parseUatAuthProfiles, requiredUatAuthProfiles, uatAuthProfiles } from "../../scripts/testing/uat-auth";
import type { Suite } from "../../scripts/testing/catalogue";

function report(status: string, expectedStatus = "passed", results = [{ status }]) {
  return { suites: [{ title: "example.spec.ts", specs: [{ file: "/repo/tests/e2e/example.spec.ts", title: "save a [claim] (once)", tests: [{ projectName: "desktop", expectedStatus, status: status === "passed" ? "expected" : "skipped", results }] }] }] };
}
it("requires exactly the selected browser case to pass without retries", () => {
  const wanted = browserCases(report("passed"))[0];
  expect(() => assertBrowserCasePassed(report("passed"), wanted)).not.toThrow();
  expect(() => assertBrowserCasePassed(report("skipped"), wanted)).toThrow("REQUIRED_BROWSER_CASE_DID_NOT_PASS");
  expect(() => assertBrowserCasePassed(report("passed", "skipped"), wanted)).toThrow("REQUIRED_BROWSER_CASE_DID_NOT_PASS");
  expect(() => assertBrowserCasePassed(report("passed", "passed", [{ status: "failed" }, { status: "passed" }]), wanted)).toThrow("REQUIRED_BROWSER_CASE_DID_NOT_PASS");
  expect(() => assertBrowserCasePassed({ suites: [] }, wanted)).toThrow("EXACT_BROWSER_CASE_EXECUTION_REQUIRED");
});
it("keeps nested scenario identity and escapes literal title punctuation", () => {
  const nested = { suites: [{ title: "example.spec.ts", suites: [{ ...report("passed").suites[0], title: "Expense" }] }] };
  expect(browserCases(nested)[0].title).toBe("Expense save a [claim] (once)");
  expect(new RegExp(exactTitlePattern("save a [claim] (once)")).test("save a [claim] (once)")).toBe(true);
  expect(new RegExp(exactTitlePattern("save a [claim] (once)")).test("save a c once")).toBe(false);
  const selected = new RegExp(exactTitlePattern("Expense save a [claim] (once)", "desktop", "example.spec.ts"));
  expect(selected.test(" desktop example.spec.ts Expense save a [claim] (once)")).toBe(true);
  expect(selected.test(" mobile example.spec.ts Expense save a [claim] (once)")).toBe(false);
  expect(selected.test(" desktop other.spec.ts Expense save a [claim] (once)")).toBe(false);
  expect(selected.test(" desktop example.spec.ts Other Expense save a [claim] (once)")).toBe(false);
});

it("retains collected UAT auth annotations without changing the case identity", () => {
  const value = report("passed");
  const annotation = uatAuthProfiles("finance").annotation;
  const annotated = { suites: [{ ...value.suites[0], specs: value.suites[0].specs.map(spec => ({ ...spec, tests: spec.tests.map(test => ({ ...test, annotations: [annotation] })) })) }] };
  expect(browserCases(annotated)[0]).toEqual({ ...browserCases(value)[0], annotations: [annotation] });
});

const uatSuite: Suite = { id: "routes.uat", owner: "shared.ui", lane: "uat", modules: ["finance"], files: ["tests/e2e/example.spec.ts"], fixture: "uat", requires: { jobs: false, scanner: false }, uatAuth: { mode: "case-annotations" } };
it("prepares only the declared UAT profile and none for anonymous or self-authenticated cases", () => {
  const wanted = browserCases(report("passed"))[0];
  expect(requiredUatAuthProfiles(uatSuite, { ...wanted, annotations: [uatAuthProfiles("owner-branch").annotation] })).toEqual(["owner-branch"]);
  expect(requiredUatAuthProfiles(uatSuite, { ...wanted, annotations: [uatAuthProfiles().annotation] })).toEqual([]);
  expect(requiredUatAuthProfiles({ ...uatSuite, uatAuth: { mode: "self-authenticated" } }, wanted)).toEqual([]);
});
it("fails closed when case auth requirements are missing, malformed or refer to unknown profiles", () => {
  const wanted = browserCases(report("passed"))[0];
  expect(() => requiredUatAuthProfiles(uatSuite, wanted)).toThrow("UAT_CASE_AUTH_MAPPING_REQUIRED");
  expect(() => requiredUatAuthProfiles(uatSuite, { ...wanted, annotations: [uatAuthProfiles("owner").annotation, uatAuthProfiles("finance").annotation] })).toThrow("UAT_CASE_AUTH_MAPPING_REQUIRED");
  expect(() => requiredUatAuthProfiles(uatSuite, { ...wanted, annotations: [{ type: "erpx-uat-auth", description: "[" }] })).toThrow("INVALID_UAT_CASE_AUTH_MAPPING");
  expect(() => requiredUatAuthProfiles(uatSuite, { ...wanted, annotations: [{ type: "erpx-uat-auth", description: '["unknown-profile"]' }] })).toThrow("INVALID_UAT_AUTH_PROFILES");
});
it("validates explicit auth preparation selections and distinguishes empty from standalone full preparation", () => {
  expect(parseUatAuthProfiles(undefined)).toBeUndefined(); expect(parseUatAuthProfiles("")).toEqual([]);
  expect(parseUatAuthProfiles("finance, owner,finance")).toEqual(["finance", "owner"]);
  expect(() => parseUatAuthProfiles("finance,,owner")).toThrow("INVALID_UAT_AUTH_PROFILES");
  expect(() => parseUatAuthProfiles("typo")).toThrow("INVALID_UAT_AUTH_PROFILES");
});
it("requires both projects by default and records an explicit desktop-only coverage reason", () => {
  const wanted = browserCases(report("passed"))[0];
  expect(browserCaseProjectRequirement(wanted)).toEqual({ required: true });
  const annotations = [browserProjects(["desktop"], "The sidebar rail exists only in the desktop layout.").annotation];
  expect(browserCaseProjectRequirement({ ...wanted, annotations })).toEqual({ required: true, reason: "The sidebar rail exists only in the desktop layout." });
  expect(browserCaseProjectRequirement({ ...wanted, project: "mobile", annotations })).toEqual({ required: false, reason: "The sidebar rail exists only in the desktop layout." });
  expect(() => assertBrowserCasePassed(report("skipped"), wanted)).toThrow("REQUIRED_BROWSER_CASE_DID_NOT_PASS");
});
it("rejects malformed, unknown or unexplained browser project restrictions", () => {
  const wanted = browserCases(report("passed"))[0];
  for (const description of ["{", 'null', '{"projects":[] ,"reason":"none"}', '{"projects":["tablet"],"reason":"other"}', '{"projects":["desktop"],"reason":""}']) expect(() => browserCaseProjectRequirement({ ...wanted, annotations: [{ type: "erpx-browser-projects", description }] })).toThrow("INVALID_BROWSER_PROJECT_REQUIREMENT");
  expect(() => browserCaseProjectRequirement({ ...wanted, annotations: [browserProjects(["desktop"], "rail").annotation, browserProjects(["mobile"], "drawer").annotation] })).toThrow("INVALID_BROWSER_PROJECT_REQUIREMENT");
});
