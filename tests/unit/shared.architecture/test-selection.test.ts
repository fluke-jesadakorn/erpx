import { afterEach, describe, expect, it } from "vitest";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { spawnSync } from "node:child_process";
import { parseTestArguments } from "../../../scripts/testing/arguments";
import { exactNativeCasePattern, resolveRegisteredCase, suiteForRegisteredCase } from "../../../scripts/testing/exact-case-selection";
import { assertExplicitSelectionIncluded, importSpecifiers, loadCatalogue, reachesDatabaseFixture, repositoryRoot, requiresEngineeringBusinessFixture, resolveImport, selectSuites, validateBrowserFixtureContract, validateCatalogue, type Catalogue, type Suite } from "../../../scripts/testing/catalogue";
import { parseChanges, patternMatches, selectImpact, type ImpactResult, type SourceOwnership } from "../../../scripts/testing/impact";
import { selectCiSuites } from "../../../scripts/testing/ci-selection";
import { developmentSelection } from "../../../scripts/testing/development";
import { nativeCaseTitle, requiredCaseFailures } from "../../../scripts/testing/required-cases-reporter";
import { browserChildEnvironment, vitestChildEnvironment } from "../../../scripts/testing/runner-environment";
import { assertRequiredCaseInventory, assertUatRouteSources, browserRequiredCaseBaseline, currentRequiredCases, loadBrowserCaseBaseline, loadRequiredCases, requiredCaseKey } from "../../../scripts/testing/required-case-inventory";
import { loadProviderRehearsals, unselectedProviderRehearsals, validateProviderRehearsals } from "../../../scripts/testing/provider-rehearsals";
import { requiredUatMatrixCases } from "../../../scripts/testing/uat-required-identities";
import { assertAcmeBrowserSeedTarget } from "../../../fixtures/shared/builders/acme-browser-seed-target";
import { assertLocalBrowserRealmTarget } from "../../../fixtures/shared/builders/local-browser-erpx-realm";
import { pinnedBunBinary } from "../../../scripts/testing/runtime-tooling";
import { assertTestPlacement } from "../../../scripts/testing/layout";

const suite = (id: string, modules: string[], lane: Suite["lane"] = "unit"): Suite => ({ id, lane, owner: modules[0], modules, files: [`tests/${id}.test.ts`], fixture: "none", requires: { jobs: false, scanner: false }, ...(["unit", "integration"].includes(lane) ? { ci: { critical: true, reason: "Test fixture covers a protected business invariant." } } : {}) });
const catalogue: Catalogue = { version: 1, modules: ["sales", "finance", "people_payroll"], suites: [suite("sales.rules", ["sales"]), suite("finance.rules", ["finance"]), suite("people_payroll.expense", ["people_payroll"]), suite("sales.collection", ["sales", "finance"], "journeys")] };
const ownership: SourceOwnership = { owners: [
  { id: "sales", modules: ["sales"], source: ["src/server/sales/**"] },
  { id: "finance", modules: ["finance"], source: ["src/server/finance/**"] },
  { id: "people_payroll", modules: ["people_payroll"], source: ["src/server/people/**"] },
] };
const base = new Map([
  ["src/server/sales/pricing.ts", "export const price = 1"],
  ["src/server/finance/invoice.ts", "import { price } from '../sales/pricing'; export { price }"],
  ["src/server/people/expense.ts", "export const expense = 1"],
]);
const impacted = (changes: Parameters<typeof selectImpact>[0]["changed"], before = base, after = base, beforeOwnership = ownership, afterOwnership = ownership) => selectImpact({ changed: changes, base: before, head: after, baseCatalogue: catalogue, headCatalogue: catalogue, baseOwnership: beforeOwnership, headOwnership: afterOwnership });
function ciCatalogues() {
  const baseCritical = suite("finance.critical", ["finance"]);
  const headCritical = { ...baseCritical, ci: { critical: false, reason: "Deliberately weakened candidate classification for regression coverage." } };
  const nativeLow = { ...suite("sales.low-risk", ["sales"]), ci: { critical: false, reason: "Synthetic presentation-only coverage for selector tests." } };
  const nativeIntegration = { ...suite("sales.changed", ["sales"], "integration"), ci: { critical: false, reason: "Synthetic non-risk coverage for selector tests." } };
  const browser = [suite("identity_access.auth-smoke.e2e", ["identity_access"], "e2e"), suite("finance.demo-controlled-payment.e2e", ["finance"], "e2e"),
    suite("sales.change-flow.journeys", ["sales"], "journeys"), suite("identity_access.product-hosts.e2e", ["identity_access"], "e2e"), suite("finance.routes.uat", ["finance"], "uat")];
  const modules = ["finance", "sales", "identity_access"];
  const baseCatalogue: Catalogue = { version: 1, modules, suites: [baseCritical, nativeLow, nativeIntegration, ...browser] };
  const headCatalogue: Catalogue = { version: 1, modules, suites: [headCritical, nativeLow, nativeIntegration, ...browser] };
  return { baseCatalogue, headCatalogue, baseCritical, headCritical, nativeLow, nativeIntegration };
}
const ciImpact = (mode: ImpactResult["mode"], suites: Suite[] = [], changed: ImpactResult["changed"] = []): ImpactResult => ({ mode, suites, changed, modules: [], reasons: [mode === "full" ? "GLOBAL_DEPENDENCY" : mode === "affected" ? "OWNED_CHANGE" : "DOCUMENTATION_ONLY"] });

describe("test selection contracts", () => {
  it("resolves unique exact case titles and file identities and rejects unknown or ambiguous selectors", () => {
    const id = "admin:company-brand-organization-iam";
    const title = "Company Admin exposes current Brand, Organization, and IAM owner reads without changing roles or package grants";
    const byTitle = resolveRegisteredCase(title);
    expect(byTitle).toMatchObject({ runner: "vitest", file: "tests/integration/identity_access/product-workspace.test.ts", title });
    expect(suiteForRegisteredCase(loadCatalogue(), byTitle).id).toBe("identity_access.product-workspace.integration");
    expect(() => resolveRegisteredCase(id)).toThrow("AMBIGUOUS_REGISTERED_TEST_CASE");
    expect(() => resolveRegisteredCase(`id:${id}`)).toThrow("AMBIGUOUS_REGISTERED_TEST_CASE");
    expect(resolveRegisteredCase(`title:${title}`)).toEqual(byTitle);
    expect(resolveRegisteredCase(`${byTitle.file}::${title}`)).toEqual(byTitle);
    expect(resolveRegisteredCase(`e2e/marketing/forms-host.spec.ts::isolated anonymous Forms host submits the published version and replays exact consent once`, { project: "mobile" }))
      .toMatchObject({ runner: "playwright", file: "tests/e2e/marketing/forms-host.spec.ts", project: "mobile" });
    expect(resolveRegisteredCase(`e2e/identity_access/product-hosts.spec.ts::isolated work Account Apps SSO reaches its actual owner landing and API`, { project: "desktop" }))
      .toMatchObject({ runner: "playwright", file: "tests/e2e/identity_access/product-hosts.spec.ts", project: "desktop" });
    expect(() => resolveRegisteredCase("unknown registered title")).toThrow("UNKNOWN_REGISTERED_TEST_CASE");
    const browserTitle = "paid guest booking recovers interrupted receipt and independently approved partial refund without duplicate money";
    expect(() => resolveRegisteredCase(browserTitle)).toThrow("AMBIGUOUS_REGISTERED_TEST_CASE");
    expect(resolveRegisteredCase(browserTitle, { project: "mobile" })).toMatchObject({ runner: "playwright", project: "mobile", title: browserTitle });
    expect(() => resolveRegisteredCase(`${byTitle.file}::unknown title`)).toThrow("UNKNOWN_REGISTERED_TEST_CASE");
    const pattern = exactNativeCasePattern("fee (USD) $1.00 + tax?");
    expect(new RegExp(pattern).test("fee (USD) $1.00 + tax?")).toBe(true);
    expect(new RegExp(pattern).test("prefix fee (USD) $1.00 + tax?")).toBe(false);
  });
  it("parses one exact case selector without changing the default lane, and rejects conflicting name filters", () => {
    const title = "Company Admin exposes current Brand, Organization, and IAM owner reads without changing roles or package grants";
    const parsed = parseTestArguments([`--case=${title}`]);
    expect(parsed).toMatchObject({ command: "tests", caseSelector: title, forwarded: [] });
    expect(parsed).not.toHaveProperty("lane");
    expect(parseTestArguments(["--case", title, "--list"]).list).toBe(true);
    expect(() => parseTestArguments(["--case=one", "--case=two"])).toThrow("DUPLICATE_EXACT_CASE_SELECTOR");
    expect(() => parseTestArguments(["--case="])).toThrow("TEST_OPTION_VALUE_REQUIRED:--case");
    expect(() => parseTestArguments(["--case=one", "--testNamePattern", "one"])).toThrow("EXACT_CASE_PATTERN_CONFLICT");
    expect(() => parseTestArguments(["--case=one", "--grep=one"])).toThrow("EXACT_CASE_PATTERN_CONFLICT");
    expect(parseTestArguments([]).lane).toBe("unit");
  });
  it("lists only the exact registered native or browser case without starting a runner", () => {
    const nativeTitle = "Company Admin exposes current Brand, Organization, and IAM owner reads without changing roles or package grants";
    const native = spawnSync(pinnedBunBinary(repositoryRoot), ["--env-file=/dev/null", "scripts/testing/test.ts", "integration", `--case=${nativeTitle}`, "--list", "--json"], { cwd: repositoryRoot, env: process.env, maxBuffer: 16 * 1024 * 1024 });
    expect(native.status, native.stderr.toString()).toBe(0);
    const nativeSelection = JSON.parse(native.stdout.toString());
    expect(nativeSelection.selectedCase).toMatchObject({ runner: "vitest", file: "tests/integration/identity_access/product-workspace.test.ts" });
    expect(nativeSelection.suites).toHaveLength(1);
    expect(nativeSelection.suites[0].files).toEqual(["tests/integration/identity_access/product-workspace.test.ts"]);

    const title = "paid guest booking recovers interrupted receipt and independently approved partial refund without duplicate money";
    const browser = spawnSync(pinnedBunBinary(repositoryRoot), ["--env-file=/dev/null", "scripts/testing/browser-runner.ts", "e2e", `--case=${title}`, "--project=mobile", "--list"], { cwd: repositoryRoot, env: process.env, maxBuffer: 16 * 1024 * 1024 });
    expect(browser.status, browser.stderr.toString()).toBe(0);
    const browserSelection = JSON.parse(browser.stdout.toString());
    expect(browserSelection.selectedCase).toMatchObject({ suite: "pms.booking-payment-recovery.e2e", project: "mobile", title });
    expect(browserSelection.suites).toHaveLength(1);
    expect(browserSelection.suites[0].files).toEqual(["tests/e2e/pms/pms-booking-payment-recovery.spec.ts"]);

    const conflicting = spawnSync(pinnedBunBinary(repositoryRoot), ["--env-file=/dev/null", "scripts/testing/browser-runner.ts", "e2e", `--case=${title}`, "--project=mobile", "--grep=booking", "--list"], { cwd: repositoryRoot, env: process.env, maxBuffer: 16 * 1024 * 1024 });
    expect(conflicting.status).not.toBe(0);
    expect(`${conflicting.stdout.toString()}${conflicting.stderr.toString()}`).toContain("EXACT_BROWSER_CASE_PATTERN_CONFLICT");
  });
  it("defaults development commands to unit while preserving explicit native selections and lane errors", () => {
    const native: Catalogue = { ...catalogue, suites: [...catalogue.suites, suite("finance.adapter", ["finance"], "integration")] };
    expect(selectSuites(native, parseTestArguments([])).every(item => item.lane === "unit")).toBe(true);
    expect(selectSuites(native, parseTestArguments(["--module=finance"])).map(item => item.id)).toEqual(["finance.rules"]);
    const files = ["tests/finance.rules.test.ts", "tests/finance.adapter.test.ts"];
    expect(selectSuites(native, parseTestArguments(files)).map(item => item.lane)).toEqual(["unit", "integration"]);
    expect(selectSuites(native, parseTestArguments(["--suite=finance.adapter"])).map(item => item.lane)).toEqual(["integration"]);
    expect(() => selectSuites(native, parseTestArguments(["tests/missing.test.ts"]))).toThrow("UNKNOWN_TEST_FILE");
    expect(() => selectSuites(native, parseTestArguments(["--suite=missing"]))).toThrow("UNKNOWN_TEST_SUITE");
    expect(() => selectSuites(native, parseTestArguments(["unit", ...files]))).toThrow("TEST_FILE_LANE_MISMATCH");
    expect(() => selectSuites(native, parseTestArguments(["integration", "--suite=finance.rules"]))).toThrow("TEST_SUITE_LANE_MISMATCH");
  });
  it("lists the real default and mixed native CLI selection without dropping integration files", () => {
    const run = (args: string[]) => {
      const child = spawnSync(pinnedBunBinary(repositoryRoot), ["--env-file=/dev/null", "scripts/testing/test.ts", ...args, "--list", "--json"], { cwd: repositoryRoot, env: process.env, maxBuffer: 16 * 1024 * 1024 });
      expect(child.status, child.stderr.toString()).toBe(0);
      return JSON.parse(child.stdout.toString()) as Suite[];
    };
    expect(run([])).toEqual(selectSuites(loadCatalogue(), { lane: "unit" }));
    expect(run(["--module=finance"])).toEqual(selectSuites(loadCatalogue(), { lane: "unit", modules: ["finance"] }));
    const files = ["tests/unit/shared.attachments/attachment-policy-inventory.test.ts", "tests/integration/shared.attachments/attachments-v2.test.ts"];
    expect(run(files).flatMap(item => item.files).sort()).toEqual(files.sort());
  });
  it("dispatches mixed native files to their lane configs without opening PostgreSQL", () => {
    const directory = mkdtempSync(join(tmpdir(), "erpx-mixed-native-dispatch-"));
    try {
      const node = join(directory, "node"), dispatchLog = join(directory, "dispatch.log");
      writeFileSync(node, [
        "#!/bin/sh",
        'if [ "$1" = "--version" ]; then',
        "  printf '%s\\n' 'v26.11.1'",
        "  exit 0",
        "fi",
        "{",
        "  printf '%s\\n' '---'",
        '  printf "argv=%s\\n" "$*"',
        '  printf "selected=%s\\n" "${ERPX_SELECTED_TEST_FILES-<unset>}"',
        '  if [ "${ERPX_TEST_ADMIN_URL+x}" = x ]; then',
        '    printf "admin=%s\\n" "$ERPX_TEST_ADMIN_URL"',
        "  else",
        '    printf "%s\\n" "admin=<unset>"',
        "  fi",
        '} >> "$ERPX_TEST_DISPATCH_LOG"',
        "exit 0",
      ].join("\n") + "\n", { mode: 0o700 });
      const files = [
        "tests/unit/shared.attachments/attachment-policy-inventory.test.ts",
        "tests/integration/shared.attachments/attachments-v2.test.ts",
      ];
      const child = spawnSync(pinnedBunBinary(repositoryRoot), ["--env-file=/dev/null", "scripts/testing/test.ts", ...files], {
        cwd: repositoryRoot,
        env: {
          ...process.env,
          ERPX_NODE_BIN: node,
          ERPX_TEST_ADMIN_URL: "dispatch-test-only",
          ERPX_TEST_DISPATCH_LOG: dispatchLog,
        },
        encoding: "utf8",
        maxBuffer: 16 * 1024 * 1024,
      });
      expect(child.error).toBeUndefined();
      expect(child.status, child.stderr).toBe(0);
      const invocations = readFileSync(dispatchLog, "utf8").split(/^---\n/m).slice(1).map(value => value.trim());
      expect(invocations).toHaveLength(2);
      expect(invocations[0]).toContain("--config tests/config/vitest.unit.config.mts");
      expect(invocations[0]).toContain(`selected=${JSON.stringify([files[0]])}`);
      expect(invocations[0]).toContain("admin=<unset>");
      expect(invocations[1]).toContain("--config tests/config/vitest.integration.config.mts");
      expect(invocations[1]).toContain(`selected=${JSON.stringify([files[1]])}`);
      expect(invocations[1]).toContain("admin=dispatch-test-only");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
  it("places suites under their declared lane and owner without allowing support or root cases", () => {
    const registered = { id: "finance.rules", lane: "unit" as const, owner: "finance", files: ["tests/unit/finance/rules.test.ts"] };
    expect(() => assertTestPlacement([registered])).not.toThrow();
    for (const file of ["tests/rules.test.ts", "tests/integration/finance/rules.test.ts", "tests/unit/sales/rules.test.ts", "tests/support/rules.test.ts", "tests/unit/finance/../rules.test.ts"])
      expect(() => assertTestPlacement([{ ...registered, files: [file] }])).toThrow("TEST_PLACEMENT_MISMATCH");
    expect(() => assertTestPlacement([{ ...registered, owner: "../finance" }])).toThrow("INVALID_TEST_OWNER_PATH");
  });
  it("retains every reviewed eligible browser identity when a nonempty suite loses or reclassifies a case", () => {
    const baseline = loadBrowserCaseBaseline();
    const actual = loadCatalogue();
    const files = actual.suites.filter(candidate => ["e2e", "journeys"].includes(candidate.lane)).flatMap(candidate => candidate.files);
    expect([...files].sort()).toEqual(expect.arrayContaining([...new Set(baseline.cases.map(test => test.file))].sort()));
    expect(baseline.cases).toHaveLength(501);
    expect(baseline.excludedCases).toHaveLength(9);
    expect(baseline.cases.filter(test => test.file === "tests/e2e/shared.ui/enterprise-ui.spec.ts")).toHaveLength(19);
    expect(baseline.cases.filter(test => test.file === "tests/e2e/finance/demo-controlled-payment.spec.ts")).toHaveLength(4);
    expect(baseline.cases.filter(test => test.file === "tests/journeys/sales/demo-sales-to-collection.spec.ts" && test.title.includes("isolated Sales cancels before stock movement"))).toHaveLength(2);
    expect(baseline.cases.filter(test => test.file === "tests/e2e/customer_authority/portal-invitation.spec.ts")).toHaveLength(4);
    expect(baseline.cases.filter(test => test.file === "tests/journeys/customer_authority/demo-retail-customer-authority.spec.ts" && test.title.includes("external customer portal invitation signup"))).toHaveLength(2);
    expect(baseline.excludedCases.filter(test => test.file === "tests/e2e/shared.ui/enterprise-ui.spec.ts")).toHaveLength(3);
    const registered = loadRequiredCases();
    const reviewedSupplemental = [
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file: "tests/e2e/pms/demo-hospitality-setup.spec.ts",
        title: "keeps hotel drafts scoped while refresh fences stale cash and an inspected higher room type upgrades through the drawer",
        project,
      })),
      ...([
        ["tests/e2e/pms/pms-owner-read-workflows.spec.ts", "pms/dashboard renders the selected hotel's live room status through its owner-scoped read"],
        ["tests/e2e/pms/pms-owner-read-workflows.spec.ts", "pms/inventory opens the mounted minibar route with inventory section identity and scoped records"],
        ["tests/e2e/pms/pms-owner-read-workflows.spec.ts", "pms/reports runs a date-range report for the selected hotel using read-only owner data"],
        ["tests/e2e/pms/pms-owner-read-workflows.spec.ts", "pms/settings shows configured rooms and rates from the selected hotel's scoped setup read"],
      ] as const).flatMap(([file, title]) => (["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file,
        title,
        project,
      }))),
      ...(["work", "pos", "pms", "finance", "people", "sales", "supply", "assets", "projects", "recruit", "learn", "support", "service", "legal", "manufacturing"] as const).flatMap(product => (["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file: "tests/e2e/identity_access/product-hosts.spec.ts",
        title: `isolated ${product} Account Apps SSO reaches its actual owner landing and API`,
        project,
      }))),
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file: "tests/e2e/identity_access/product-hosts.spec.ts",
        title: "isolated Work host creates, recovers and cancels a service request draft through Account and Apps",
        project,
      })),
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file: "tests/e2e/identity_access/product-hosts.spec.ts",
        title: "isolated Assets native drawer and dirty product switch observations across the appearance matrix",
        project,
      })),
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file: "tests/e2e/identity_access/product-hosts.spec.ts",
        title: "isolated Finance native invoice drawer keeps dirty work across the appearance matrix",
        project,
      })),
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file: "tests/e2e/identity_access/product-hosts.spec.ts",
        title: "isolated Finance host creates a completion-backed sales invoice through Account and Apps",
        project,
      })),
      {
        runner: "playwright" as const,
        file: "tests/e2e/shared.executive/executive-work-mcp.spec.ts",
        title: "isolated Work Executive UI reads a seeded connection through scoped MCP",
        project: "desktop" as const,
      },
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file: "tests/e2e/marketing/forms-host.spec.ts",
        title: "isolated anonymous Forms host submits the published version and replays exact consent once",
        project,
      })),
      ...(["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file: "tests/e2e/identity_access/product-hosts.spec.ts",
        title: "isolated Account My access and Company Admin show authorized products and current owner projections without changing H001 authority",
        project,
      })),
      ...([
        "Apps opens a pre-entitled Branch workspace for its scoped user",
        "Apps opens a pre-entitled Department workspace for its scoped user",
        "Branch user cannot open another Company through an Apps product context",
        "Account profile language and theme preferences persist after navigation and reload",
      ] as const).flatMap(title => (["desktop", "mobile"] as const).map(project => ({
        runner: "playwright" as const,
        file: "tests/e2e/identity_access/product-hosts.spec.ts",
        title,
        project,
      }))),
    ];
    for (const test of reviewedSupplemental) expect(registered.some(candidate => requiredCaseKey(candidate) === requiredCaseKey(test))).toBe(true);
    const reviewed = [...baseline.cases, ...reviewedSupplemental];
    const observed = [...reviewed.map(test => ({ ...test, required: true })), ...baseline.excludedCases.map(test => ({ ...test, required: false }))];
    expect(() => assertRequiredCaseInventory(registered, observed, { runner: "playwright", files })).not.toThrow();
    for (const test of reviewed) {
      const scope = { runner: "playwright" as const, files: [test.file] };
      const peers = observed.filter(candidate => candidate.file === test.file);
      expect(peers.some(candidate => requiredCaseKey(candidate) === requiredCaseKey(test))).toBe(true);
      const removed = peers.filter(candidate => requiredCaseKey(candidate) !== requiredCaseKey(test));
      expect(() => assertRequiredCaseInventory(registered, removed, scope), requiredCaseKey(test)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
      const reclassified = peers.map(candidate => requiredCaseKey(candidate) === requiredCaseKey(test) ? { ...candidate, required: false } : candidate);
      expect(() => assertRequiredCaseInventory(registered, reclassified, scope), requiredCaseKey(test)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
    }
    for (const excluded of baseline.excludedCases) expect(registered.some(test => requiredCaseKey(test) === requiredCaseKey(excluded))).toBe(false);
    expect(() => browserRequiredCaseBaseline({ ...baseline, cases: [] })).toThrow("INVALID_BROWSER_CASE_BASELINE");
    expect(() => browserRequiredCaseBaseline({ ...baseline, cases: [...baseline.cases, baseline.cases[0]] })).toThrow("INVALID_OR_DUPLICATE_BROWSER_CASE_BASELINE");
    expect(() => browserRequiredCaseBaseline({ ...baseline, excludedCases: [{ ...baseline.cases[0], reason: "silently excluded" }] })).toThrow("INVALID_OR_DUPLICATE_BROWSER_CASE_BASELINE");
  });
  it("validates browser case order without narrowing selection or forwarding it to native execution", () => {
    const id = "shared.ui.enterprise-ui.e2e";
    const selected = parseTestArguments(["e2e", "--suite", id, "--case-order=reverse", "--list", "--json"]);
    expect(selected).toMatchObject({ lane: "e2e", suiteIds: [id], caseOrder: "reverse", forwarded: [], list: true, json: true });
    expect(parseTestArguments(["journeys", "--case-order", "forward"]).caseOrder).toBe("forward");
    expect(parseTestArguments(["uat", "--case-order=reverse"]).caseOrder).toBe("reverse");
    expect(parseTestArguments(["e2e"]).caseOrder).toBeUndefined();
    expect(parseTestArguments(["impact", "--base=HEAD", "--case-order=reverse"]).forwarded).toEqual([]);
    for (const command of ["tests", "unit", "integration", "provider"]) expect(() => parseTestArguments([command, "--case-order=reverse"])).toThrow("BROWSER_CASE_ORDER_ONLY_FOR_BROWSER_LANES");
    for (const value of ["random", "Reverse", "forward,reverse", "0"]) expect(() => parseTestArguments(["e2e", `--case-order=${value}`])).toThrow("INVALID_BROWSER_CASE_ORDER");
    for (const args of [["--case-order"], ["--case-order="], ["--case-order", "--list"]]) expect(() => parseTestArguments(["e2e", ...args])).toThrow("TEST_OPTION_VALUE_REQUIRED");
    expect(() => parseTestArguments(["e2e", "--case-orders=reverse"])).toThrow("UNKNOWN_TEST_OPTION");
    expect(() => parseTestArguments(["e2e", "--case-order=forward", "--case-order=reverse"])).toThrow("DUPLICATE_BROWSER_CASE_ORDER");
    const result = spawnSync(pinnedBunBinary(repositoryRoot), ["--env-file=/dev/null", "scripts/testing/test.ts", "e2e", "--suite", id, "--case-order=reverse", "--list", "--json"], { cwd: repositoryRoot, env: process.env, maxBuffer: 16 * 1024 * 1024 });
    expect(result.error).toBeUndefined();
    expect(result.status, result.stderr.toString()).toBe(0);
    const listed = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(result.stdout));
    expect(listed.execution).toEqual({ caseOrder: "reverse", mode: "acceptance" });
    expect(listed.suites).toEqual(selectSuites(loadCatalogue(), { lane: "e2e", suiteIds: [id] }));
  });
  it("requires the seeded Engineering Business fixture for real helper calls before browser execution", () => {
    const actual = loadCatalogue();
    for (const id of ["shared.ui.demo-client-form-hydration.e2e", "customer_authority.demo-retail-customer-authority.journeys"]) {
      const registered = actual.suites.find(candidate => candidate.id === id)!;
      expect(registered.fixture, id).toBe("engineering");
      expect(() => validateCatalogue({ ...actual, suites: actual.suites.map(candidate => candidate.id === id ? { ...candidate, fixture: "general-company" } : candidate) })).toThrow(`ENGINEERING_BROWSER_FIXTURE_REQUIRED:${id}:${registered.files[0]}`);
      expect(() => validateBrowserFixtureContract(registered)).not.toThrow();
      expect(() => validateBrowserFixtureContract({ ...registered, lane: "uat", fixture: "uat" })).not.toThrow();
      expect(() => validateBrowserFixtureContract({ ...registered, fixture: "uat" })).toThrow("ENGINEERING_BROWSER_FIXTURE_REQUIRED");
    }
    const file = "tests/e2e/fixture-contract.spec.ts";
    for (const [source, required] of [
      ['import { businessFixture } from "../support/browser/engineering-business"; businessFixture(info);', true],
      ['import { businessFixture as loadFixture } from "../support/browser/engineering-business"; loadFixture(info);', true],
      ['import * as helpers from "../support/browser/engineering-business"; helpers.businessFixture(info);', true],
      ['import * as helpers from "../support/browser/engineering-business"; helpers["businessFixture"](info);', true],
      ['import { businessFixture as loadFixture } from "../support/browser/engineering-business"; function view(loadFixture: () => unknown) { return loadFixture(); }', false],
      ['import * as helpers from "../support/browser/engineering-business"; function view(helpers: {businessFixture(): unknown}) { return helpers.businessFixture(); }', false],
      ['import * as helpers from "../support/browser/engineering-business"; function view(helpers: {businessFixture(): unknown}) { return helpers.businessFixture(); } helpers.businessFixture(info);', true],
      ['import { businessFixture } from "../support/browser/engineering-business"; function view() { const businessFixture = () => ({}); return businessFixture(); }', false],
      ['import type { businessFixture } from "../support/browser/engineering-business"; type Fixture = ReturnType<typeof businessFixture>;', false],
      ['import { type businessFixture } from "../support/browser/engineering-business"; type Fixture = ReturnType<typeof businessFixture>;', false],
      ['import type * as helpers from "../support/browser/engineering-business"; type Fixture = ReturnType<typeof helpers.businessFixture>;', false],
      ['import { businessFixture } from "../support/browser/engineering-business"; const unrelated = "businessFixture(info)";', false],
      ['import { businessFixture } from "../support/browser/unrelated"; businessFixture(info);', false],
    ] as const) expect(requiresEngineeringBusinessFixture(source, file), source).toBe(required);
  });
  it("retains byte-exact oversized UTF-8 inventories through the pinned CLI stdout", () => {
    const value = { value: "x".repeat(655345) + "·ภาษาไทย🚀", count: 17, tail: ["required", "complete"] };
    const script = `import { writeJsonOutput } from ${JSON.stringify(join(repositoryRoot, "scripts/testing/json-output.ts"))}; writeJsonOutput({ value: "x".repeat(655345) + "·ภาษาไทย🚀", count: 17, tail: ["required", "complete"] });`;
    const stdout = spawnSync(pinnedBunBinary(repositoryRoot), ["--env-file=/dev/null", "--eval", script], { cwd: repositoryRoot, env: process.env, maxBuffer: 16 * 1024 * 1024 });
    expect(stdout.error).toBeUndefined();
    expect(stdout.status, stdout.stderr.toString()).toBe(0);
    expect(stdout.stdout).toEqual(Buffer.from(JSON.stringify(value, null, 2) + "\n", "utf8"));
    expect(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(stdout.stdout))).toEqual(value);

    const full = spawnSync(pinnedBunBinary(repositoryRoot), ["--env-file=/dev/null", "scripts/testing/full.ts", "--list", "--json"], { cwd: repositoryRoot, env: process.env, maxBuffer: 16 * 1024 * 1024 });
    expect(full.error).toBeUndefined();
    expect(full.status, full.stderr.toString()).toBe(0);
    const inventory = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(full.stdout));
    const actual = loadCatalogue();
    expect(full.stdout.byteLength).toBeGreaterThan(655360);
    expect(inventory.suites).toEqual(actual.suites.filter(candidate => candidate.lane !== "provider"));
    expect(inventory.requiredCases).toEqual(loadRequiredCases().filter(candidate => !actual.suites.some(suite => suite.lane === "provider" && suite.files.includes(candidate.file))));
    expect(inventory.requiredCases.some((candidate: { title: string }) => candidate.title === "signed-in as owner /holdings/[holdingSlug]/knowledge/handbook/published/[id] · owner · holding")).toBe(true);
    expect(inventory.excludedProviderSuites).toEqual(actual.suites.filter(candidate => candidate.lane === "provider").map(candidate => candidate.id));
    expect(inventory.providerRehearsals).toHaveLength(2);
  });
  it("selects module suites and their cross-module journeys", () => {
    expect(selectSuites(catalogue, { modules: ["sales"] }).map(suite => suite.id)).toEqual(["sales.rules", "sales.collection"]);
  });
  it("supports exact legacy file arguments without substring selection", () => {
    expect(selectSuites(catalogue, { files: ["tests/finance.rules.test.ts"] }).map(suite => suite.id)).toEqual(["finance.rules"]);
    expect(() => selectSuites(catalogue, { files: ["finance"] })).toThrow("UNKNOWN_TEST_FILE");
  });
  it("rejects unknown modules/suites and a valid but empty intersection", () => {
    expect(() => selectSuites(catalogue, { modules: ["expense"] })).toThrow("UNKNOWN_TEST_MODULE");
    expect(() => selectSuites(catalogue, { suiteIds: ["missing"] })).toThrow("UNKNOWN_TEST_SUITE");
    expect(() => selectSuites(catalogue, { lane: "unit", modules: ["missing-selection"] })).toThrow("UNKNOWN_TEST_MODULE");
    expect(() => selectSuites(catalogue, { lane: "journeys", modules: ["people_payroll"] })).toThrow("EMPTY_TEST_SELECTION");
  });
  it("rejects mixed explicit file and suite lanes before a subset can return green", () => {
    const native: Catalogue = { ...catalogue, suites: [...catalogue.suites, suite("finance.adapter", ["finance"], "integration")] };
    expect(() => selectSuites(native, { lane: "unit", files: ["tests/finance.rules.test.ts", "tests/finance.adapter.test.ts"] })).toThrow("TEST_FILE_LANE_MISMATCH:tests/finance.adapter.test.ts:integration:unit");
    expect(() => selectSuites(native, { lane: "unit", suiteIds: ["finance.rules", "finance.adapter"] })).toThrow("TEST_SUITE_LANE_MISMATCH:finance.adapter:integration:unit");
    expect(selectSuites(native, { files: ["tests/finance.rules.test.ts", "tests/finance.adapter.test.ts"] }).map(item => item.lane)).toEqual(["unit", "integration"]);
  });
  it("rejects explicitly requested suites lost to a module intersection or native runner filtering", () => {
    expect(() => selectSuites(catalogue, { modules: ["sales"], files: ["tests/sales.rules.test.ts", "tests/finance.rules.test.ts"] })).toThrow("EXCLUDED_EXPLICIT_TEST_FILE:tests/finance.rules.test.ts");
    expect(() => selectSuites(catalogue, { modules: ["sales"], suiteIds: ["sales.rules", "finance.rules"] })).toThrow("EXCLUDED_EXPLICIT_TEST_SUITE:finance.rules");
    const request = { files: ["tests/sales.rules.test.ts", "tests/sales.collection.test.ts"] };
    const mixed = selectSuites(catalogue, request);
    expect(() => assertExplicitSelectionIncluded(request, mixed.filter(item => item.lane === "unit" || item.lane === "integration"))).toThrow("EXCLUDED_EXPLICIT_TEST_FILE:tests/sales.collection.test.ts");
  });
  it("parses lane, repeated/comma selections, file args and forwarded framework options", () => {
    const args = parseTestArguments(["unit", "--", "--module=finance,sales", "--suite", "finance.rules", "tests/finance.rules.test.ts", "--testNamePattern", "currency"]);
    expect(args.lane).toBe("unit"); expect(args.modules).toEqual(["finance", "sales"]);
    expect(args.suiteIds).toEqual(["finance.rules"]); expect(args.files).toEqual(["tests/finance.rules.test.ts"]);
    expect(args.forwarded).toEqual(["--testNamePattern", "currency"]);
  });
  it("requires a base and prevents user narrowing of the computed impact", () => {
    expect(() => parseTestArguments(["impact"])).toThrow("IMPACT_BASE_REQUIRED");
    expect(() => parseTestArguments(["impact", "--base=HEAD", "--module=sales"])).toThrow("IMPACT_SELECTION_IS_COMPUTED");
    expect(() => parseTestArguments(["unit", "--base=HEAD"])).toThrow("GIT_REFERENCE_ONLY");
    expect(() => parseTestArguments(["unit", "--module="])).toThrow("TEST_OPTION_VALUE_REQUIRED");
  });
  it("discovers and validates the complete checked-in catalogue", () => {
    const actual = loadCatalogue();
    expect(actual.modules).toHaveLength(33);
    const malformed = {...actual,suites:[{...actual.suites[0],files:undefined}]} as unknown as Catalogue;
    expect(() => validateCatalogue(malformed)).toThrow(`INVALID_TEST_SUITE:${actual.suites[0].id}`);
    expect(actual.suites.filter(suite => suite.lane === "provider").every(suite => !!suite.provider?.required.length)).toBe(true);
  });
  it("confines fictional PMS provider recovery to booking E2E with real jobs and scanner", () => {
    const actual = loadCatalogue(), id = 'pms.booking-payment-recovery.e2e';
    expect(actual.suites.find(suite => suite.id === id)).toMatchObject({ fictionalProvider: 'pms-stripe', fixture: 'booking', lane: 'e2e', requires: { jobs: true, scanner: true } });
    for (const change of [{ fixture: 'hospitality' }, { lane: 'provider' }, { fictionalProvider: 'live-stripe' }, { requires: { jobs: false, scanner: true } }, { requires: { jobs: true, scanner: false } }]) {
      const changed = { ...actual, suites: actual.suites.map(suite => suite.id === id ? { ...suite, ...change } : suite) } as Catalogue;
      expect(() => validateCatalogue(changed)).toThrow(`INVALID_FICTIONAL_PROVIDER_SUITE:${id}`);
    }
  });
  it("selects the declared service fixture, scan workers and People dependency for existing browser proofs", () => {
    const actual = loadCatalogue();
    expect(actual.suites.find(suite => suite.id === "shared.ui.shared-workspace.e2e")).toMatchObject({ fixture: "services", requires: { jobs: true, scanner: true } });
    for (const id of ["people_payroll.self-service.e2e", "operations.staff-work.e2e", "shared.attachments.attachments-v2.e2e", "identity_access.member-profile.e2e", "collaboration.social-beta.e2e"])
      expect(actual.suites.find(suite => suite.id === id)?.requires, id).toEqual({ jobs: true, scanner: true });
    expect(selectSuites(actual, { lane: "e2e", modules: ["people_payroll"] }).some(suite => suite.id === "collaboration.home-feed-review.e2e")).toBe(true);
    expect(selectSuites(actual, { lane: "e2e", modules: ["collaboration"] }).some(suite => suite.id === "identity_access.member-profile.e2e")).toBe(true);
    expect(selectSuites(actual, { lane: "e2e", modules: ["operations"] }).some(suite => suite.id === "collaboration.social-beta.e2e")).toBe(true);
    expect(selectSuites(actual, { lane: "e2e", modules: ["projects"] }).some(suite => suite.id === "operations.staff-work.e2e")).toBe(true);
    for (const id of ["collaboration.social-beta.e2e", "operations.staff-work.e2e"])
      expect(actual.suites.find(suite => suite.id === id)?.modules, id).toContain("people_payroll");
    for (const capability of ["operations", "people_payroll"])
      expect(selectSuites(actual, { lane: "e2e", modules: [capability] }).some(suite => suite.id === "organization.acme-organization-data.e2e"), capability).toBe(true);
    for (const capability of ["operations", "organization", "people_payroll", "identity_access"])
      expect(selectSuites(actual, { lane: "e2e", modules: [capability] }).some(suite => suite.id === "platform_administration.administration.e2e"), capability).toBe(true);
    expect(actual.suites.find(suite => suite.id === "platform_administration.administration.e2e")).toMatchObject({ owner: "platform_administration", files: ["tests/e2e/platform_administration/administration.spec.ts"], fixture: "local-seed", requires: { jobs: true, scanner: false } });
    for (const capability of ["organization", "identity_access"])
      expect(selectSuites(actual, { lane: "journeys", modules: [capability] }).some(suite => suite.id === "people_payroll.thai-hotel-v1-hr-lifecycle.journeys"), capability).toBe(true);
    expect(actual.suites.find(suite => suite.id === "people_payroll.thai-hotel-v1-hr-lifecycle.journeys")).toMatchObject({ owner: "people_payroll", files: ["tests/journeys/people_payroll/thai-hotel-v1-hr-lifecycle.spec.ts"], fixture: "hospitality", requires: { jobs: false, scanner: false } });
    for (const capability of ["organization", "people_payroll"])
      expect(selectSuites(actual, { lane: "e2e", modules: [capability] }).some(suite => suite.id === "identity_access.demo-identity-activation-revocation.e2e"), capability).toBe(true);
    expect(actual.suites.find(suite => suite.id === "identity_access.demo-identity-activation-revocation.e2e")).toMatchObject({ owner: "identity_access", files: ["tests/e2e/identity_access/demo-identity-activation-revocation.spec.ts"], fixture: "general-company", requires: { jobs: false, scanner: false } });
  });
  it("strips ambient PostgreSQL credentials from units while retaining them for integration", () => {
    const environment = { ERPX_TEST_ADMIN_URL: "postgresql://administrator@localhost/postgres", PGPASSWORD: "isolated-password", PGDATABASE: "postgres", ERPX_NODE_BIN: "/pinned/node", ERPX_TEST_PG_BIN: "/postgresql/18/bin", PATH: `/usr/local/bin${delimiter}/usr/bin`, NODE_ENV: "test" };
    const childPath = `/postgresql/18/bin${delimiter}/usr/local/bin${delimiter}/usr/bin`;
    expect(vitestChildEnvironment(environment, false)).toEqual({ ERPX_NODE_BIN: "/pinned/node", ERPX_TEST_PG_BIN: "/postgresql/18/bin", PATH: childPath, NODE_ENV: "test" });
    expect(vitestChildEnvironment(environment, true)).toEqual({ ...environment, PATH: childPath });
    expect(vitestChildEnvironment({ ...environment, PATH: childPath }, true).PATH).toBe(childPath);
    expect(environment.PGDATABASE).toBe("postgres");
  });
  it("isolates purchase enforcement from ambient development settings in native and browser fixtures", () => {
    const ambient = { ERPX_SUPPORT_PACKAGE_ENFORCEMENT: "enforced", ERPX_NODE_BIN: "/pinned/node", NODE_ENV: "test" };
    expect(vitestChildEnvironment(ambient, false)).not.toHaveProperty("ERPX_SUPPORT_PACKAGE_ENFORCEMENT");
    expect(vitestChildEnvironment(ambient, true)).not.toHaveProperty("ERPX_SUPPORT_PACKAGE_ENFORCEMENT");
    expect(browserChildEnvironment(ambient)).not.toHaveProperty("ERPX_SUPPORT_PACKAGE_ENFORCEMENT");
    expect(browserChildEnvironment(ambient, { ERPX_SUPPORT_PACKAGE_ENFORCEMENT: "shadow" })).toHaveProperty("ERPX_SUPPORT_PACKAGE_ENFORCEMENT", "shadow");
    expect(ambient.ERPX_SUPPORT_PACKAGE_ENFORCEMENT).toBe("enforced");
  });
  it("selects the shared currency UI proof when any of its actual ERP or Front owners change", () => {
    const actual = loadCatalogue();
    for (const capability of ["finance", "sales", "people_payroll", "pos", "pms"])
      expect(selectSuites(actual, { lane: "e2e", modules: [capability] }).some(item => item.id === "finance.currency-selector.e2e"), capability).toBe(true);
    expect(actual.suites.find(item => item.id === "finance.currency-selector.e2e")).toMatchObject({
      owner: "finance", fixture: "local-seed", files: ["tests/e2e/finance/currency-selector.spec.ts"], requires: { jobs: false, scanner: false },
    });
  });
});

describe("conservative base and head impact selection", () => {
  it("keeps development impact native and discloses deferred acceptance for full, affected and empty changes", () => {
    const native: Catalogue = { ...catalogue, suites: [...catalogue.suites, suite("finance.adapter", ["finance"], "integration"), suite("finance.provider", ["finance"], "provider")] };
    const impact = { mode: "affected" as const, reasons: ["owner"], modules: ["finance"], suites: native.suites.filter(item => item.modules.includes("finance")), changed: [] };
    const affected = developmentSelection(impact, native);
    expect(affected.suites.map(item => item.id)).toEqual(["finance.rules", "finance.adapter"]);
    expect(affected).toMatchObject({ scope: "development", acceptance: false, requiredAcceptance: "bun run test:full", excludedProviderSuites: ["finance.provider"] });
    expect(affected.deferredChecks).toEqual(expect.arrayContaining(["bun scripts/architecture/acceptance.ts", "bun run lint", "bun run typecheck", "bun run deploy:build"]));
    expect(affected.deferredSuites).toContainEqual({ id: "sales.collection", lane: "journeys", command: "bun run test:journeys --suite=sales.collection" });
    const full = developmentSelection({ ...impact, mode: "full", suites: native.suites }, native);
    expect(full.suites).toEqual(native.suites.filter(item => item.lane === "unit"));
    expect(full.deferredSuites.map(item => item.id)).toEqual(["sales.collection", "finance.adapter"]);
    const none = developmentSelection({ ...impact, mode: "none", suites: [] }, native);
    expect(none.suites).toEqual([]);
    expect(none.deferredSuites).toHaveLength(native.suites.length - 1);
    for (const args of [["dev"], ["dev", "--base=HEAD", "--suite=finance.rules"], ["dev", "--base=HEAD", "tests/finance.rules.test.ts"]])
      expect(() => parseTestArguments(args)).toThrow();
    expect(parseTestArguments(["dev", "--base=HEAD", "--head=HEAD", "--list", "--json"])).toMatchObject({ command: "dev", base: "HEAD", head: "HEAD", list: true, json: true, forwarded: [] });
  });
  it("requires complete semantic ownership in impact CI even after import boundaries pass", () => {
    const workflow = readFileSync(join(process.cwd(), ".github/workflows/nextjs.yml"), "utf8");
    const impactJob = workflow.match(/^  impact:\n([\s\S]*?)(?=^  native:)/m)?.[1];
    expect(impactJob).toContain("run: bun scripts/testing/pr-regression.ts native");
    const regressionRunner = readFileSync(join(process.cwd(), "scripts/testing/pr-regression.ts"), "utf8");
    for (const command of ['"scripts/architecture/acceptance.ts"', '"run", "structure:check"', '"run", "docs:check"', '"run", "products:check"', '"run", "lint"', '"run", "typecheck"', '"run", "deploy:build"']) expect(regressionRunner).toContain(command);
  });
  it("starts CI for operational runners, nested fixtures and workspace rules that force full impact", () => {
    const workflow=readFileSync(join(process.cwd(),".github/workflows/nextjs.yml"),"utf8");
    for(const event of ["push"]){
      const block=workflow.match(new RegExp(`^  ${event}:\\n([\\s\\S]*?)(?=^  (?:pull_request|merge_group|workflow_dispatch):)`,"m"))?.[1];
      if(!block)throw new Error(`Missing ${event} CI trigger`);
      expect(block).toContain("branches: [development]");
      expect(block).not.toMatch(/^    paths:/m);
      for(const path of ["AGENTS.md","ops/ucs/gpo_runner.py","ops/windows/PeopleDirectoryRunner.ps1","ops/samba/build-samba-apk.sh","ops/macos/erpxctl.py","fixtures/finance/expansion.ts","fixtures/pos/images/menu-americano.png"]){
        expect(impacted([{status:"M",path}]).mode).toBe("full");
      }
    }
  });
  it("starts the required development PR check even when only contracts or vocabulary documentation changes", () => {
    const workflow=readFileSync(join(process.cwd(),".github/workflows/nextjs.yml"),"utf8");
    // A path- or branch-filtered required workflow stays pending rather than
    // producing a candidate receipt. Keep the PR event entirely unconditional.
    expect(workflow).toMatch(/^  pull_request: \{\}$/m);
    const pushBlock=workflow.match(/^  push:\n([\s\S]*?)(?=^  pull_request:)/m)?.[1] ?? "";
    expect(pushBlock).not.toMatch(/^    paths:/m);
    for(const path of ["docs/contracts/next/people.md","CONTEXT.md","adr/026-owner-boundaries.md"]){
      expect(impacted([{status:"M",path}]).mode).toBe("none");
    }
    expect(workflow).toMatch(/^  merge_group:/m);
    expect(workflow).toMatch(/^  workflow_dispatch:/m);
    expect(workflow).toContain("branches: [development]");
    expect(workflow).toContain('if [ "$EVENT_NAME" = push ] && [ "$base_is_zero" = true ]; then');
    expect(workflow).toContain("Unable to resolve the event base; refusing an under-scoped PR profile.");
    expect(workflow).not.toContain("base.ref == 'development'");
    expect(workflow).toContain("cancel-in-progress: ${{ github.event_name != 'workflow_dispatch' }}");
    expect(workflow).toContain("if: needs.scope.outputs.profile == 'pr'");
    expect(workflow).toContain('name: Browser full regression ${{ matrix.shard }}/64');
    expect(workflow).toContain('contains(fromJSON(needs.scope.outputs.browser_shards), matrix.shard)');
    expect(workflow).toContain('pr-regression.ts browser --base=${{ needs.scope.outputs.base }} --head=${{ needs.scope.outputs.candidate }} --shard=${{ matrix.shard }}/4');
    expect(workflow).toContain('ci-regression.ts browser --shard=${{ matrix.shard }}/64');
    expect(workflow).not.toContain('browser-pr:');
    expect(workflow).toContain('test "$IMPACT_RESULT" = success');
    expect(workflow).toContain('test "$BROWSER_RESULT" = success');
    expect(workflow).toContain('pattern: pr-*-${{ needs.scope.outputs.candidate }}');
    expect(workflow).toContain('name: full-candidate-${{ needs.scope.outputs.candidate }}');
  });
  it("includes the owning module, reverse import consumers and journeys", () => {
    expect(impacted([{ status: "M", path: "src/server/sales/pricing.ts" }]).modules).toEqual(["finance", "sales"]);
    expect(impacted([{ status: "M", path: "src/server/sales/pricing.ts" }]).suites.map(suite => suite.id)).toEqual(["sales.rules", "finance.rules", "sales.collection"]);
  });
  it("selects a mapped new file even before anyone imports it", () => {
    expect(impacted([{ status: "A", path: "src/server/people/new-policy.ts" }]).suites.map(suite => suite.id)).toEqual(["people_payroll.expense"]);
  });
  it("retains reverse consumers from the base after deleting a file", () => {
    const head = new Map(base); head.delete("src/server/sales/pricing.ts"); head.set("src/server/finance/invoice.ts", "export const invoice = 1");
    expect(impacted([{ status: "D", path: "src/server/sales/pricing.ts" }], base, head).modules).toEqual(["finance", "sales"]);
  });
  it("includes both old and new ownership on a rename", () => {
    const head = new Map(base); head.delete("src/server/sales/pricing.ts"); head.set("src/server/people/pricing.ts", "export const price = 1");
    expect(impacted([{ status: "R100", previousPath: "src/server/sales/pricing.ts", path: "src/server/people/pricing.ts" }], base, head).modules).toEqual(["finance", "people_payroll", "sales"]);
  });
  it("uses changed membership from both ownership snapshots", () => {
    const headOwnership: SourceOwnership = { owners: [...ownership.owners, { id: "people_payroll", modules: ["people_payroll"], source: ["src/server/sales/pricing.ts"] }] };
    expect(impacted([{ status: "M", path: "src/server/sales/pricing.ts" }], base, base, ownership, headOwnership).modules).toEqual(["finance", "people_payroll", "sales"]);
  });
  it.each(["src/server/modules/identity_access/public/iam-service.ts", "src/server/modules/identity_access/domain/account-recovery.ts", "src/server/modules/identity_access/adapters/legacy/iam-service.ts", "src/server/modules/finance/adapters/legacy/finance-transactions.ts", "src/server/modules/finance/public/finance-transactions.ts", "src/server/modules/shared/application/command.ts", "src/server/modules/runtime/adapters/runtime/postgres.ts", "migrations/postgres/holding-write/0002.sql", "tests/support/helpers/expense-fixture.ts", "tests/config/vitest.unit.config.mts", "scripts/testing/catalogue.ts", "AGENTS.md", "tests/catalogue/engineering-scenarios.json"])("falls back to full for shared/global input %s", path => {
    expect(impacted([{ status: "M", path }]).mode).toBe("full");
  });
  it("forces a full run for relocated IAM and transaction code even with complete ownership and no consumers", () => {
    for (const path of ["src/server/modules/identity_access/domain/revocation.ts", "src/server/modules/finance/adapters/legacy/finance-transactions.ts", "src/server/modules/shared/domain/error.ts"]) {
      const mapped = { owners: [{ id: "finance", modules: ["finance"], source: [path] }] };
      const tree = new Map([[path, "export const rule = 1"]]);
      expect(impacted([{ status: "M", path }], tree, tree, mapped, mapped).reasons).toEqual([`GLOBAL_DEPENDENCY:${path}`]);
    }
  });
  it("fails closed on unknown new or removed files", () => {
    for (const status of ["A", "D"]) expect(impacted([{ status, path: "src/server/unmapped.ts" }]).reasons).toEqual(["UNMAPPED_SOURCE:src/server/unmapped.ts"]);
  });
  it("treats a shared owner as a full dependency even when it has one listed module", () => {
    const shared: SourceOwnership = { owners: [{ id: "shared.commands", modules: ["sales"], source: ["src/server/command-policy.ts"] }] };
    expect(impacted([{ status: "M", path: "src/server/command-policy.ts" }], base, base, shared, shared).mode).toBe("full");
  });
  it("selects a changed suite and its owning module", () => {
    expect(impacted([{ status: "M", path: "tests/people_payroll.expense.test.ts" }]).suites.map(suite => suite.id)).toEqual(["people_payroll.expense"]);
  });
  it("reports documentation-only changes as an empty applicable impact", () => {
    expect(impacted([{ status: "M", path: "docs/contract.md" }])).toMatchObject({ mode: "none", suites: [] });
  });
  it("parses Git rename/copy records without losing either path", () => {
    expect(parseChanges("R100\0old.ts\0new.ts\0D\0removed.ts\0M\0changed.ts\0")).toEqual([{ status: "R100", previousPath: "old.ts", path: "new.ts" }, { status: "D", path: "removed.ts" }, { status: "M", path: "changed.ts" }]);
    expect(() => parseChanges("R100\0old.ts\0")).toThrow("INVALID_GIT_RENAME");
  });
  it("walks cyclic dependencies without looping", () => {
    const tree = new Map(base); tree.set("src/server/sales/pricing.ts", "export { price } from '../finance/invoice'");
    expect(impacted([{ status: "M", path: "src/server/sales/pricing.ts" }], tree, tree).modules).toEqual(["finance", "sales"]);
  });
  it("recognizes literal dynamic imports, aliases and re-exports", () => {
    expect(importSpecifiers("export { a } from '@/server/a'; import('./b'); require('./c'); import type { X } from './d';")).toEqual(["@/server/a", "./b", "./c", "./d"]);
    expect(resolveImport("src/server/a.ts", "@/server/b", new Set(["src/server/b/index.ts"]))).toBe("src/server/b/index.ts");
    expect(resolveImport("src/server/a.ts", "./b.js", new Set(["src/server/b.ts"]))).toBe("src/server/b.ts");
    expect(patternMatches("src/server/a/deep/b.ts", "src/server/a/**")).toBe(true);
    expect(patternMatches("src/server/a/deep/b.ts", "src/server/a/*.ts")).toBe(false);
  });
});

describe("PR regression selection", () => {
  it("selects candidate-critical suites even when the base has no critical suite", () => {
    const lowRisk = { ...suite("sales.presentation", ["sales"]), ci: { critical: false, reason: "Presentation-only fixture used to verify the candidate classification path." } };
    const newCritical = { ...suite("sales.money-and-authority", ["sales"]), ci: { critical: true, reason: "The candidate assertions retain money and current-authority invariants." } };
    const browser = [suite("identity_access.auth-smoke.e2e", ["identity_access"], "e2e"), suite("finance.demo-controlled-payment.e2e", ["finance"], "e2e")];
    const modules = ["sales", "identity_access", "finance"];
    const baseCatalogue: Catalogue = { version: 1, modules, suites: [lowRisk, ...browser] };
    const headCatalogue: Catalogue = { version: 1, modules, suites: [lowRisk, newCritical, ...browser] };
    const plan = selectCiSuites({ impact: ciImpact("none"), baseCatalogue, headCatalogue });
    expect(plan.native.map(item => item.id)).toEqual([newCritical.id]);
  });

  it("keeps base-critical suites even when candidate metadata weakens their classification", () => {
    const { baseCatalogue, headCatalogue, headCritical } = ciCatalogues();
    const plan = selectCiSuites({ impact: ciImpact("none"), baseCatalogue, headCatalogue });
    expect(plan.native.map(item => item.id)).toContain(headCritical.id);
    expect(plan.reasons.find(item => item.id === headCritical.id)?.reasons).toContain("critical on base; carried across this change");
    expect(plan.browser.map(item => item.id)).toEqual(["identity_access.auth-smoke.e2e", "finance.demo-controlled-payment.e2e"]);
  });

  it("unions changed cross-module suites with every critical suite and the two-browser baseline", () => {
    const { baseCatalogue, headCatalogue, nativeLow, nativeIntegration } = ciCatalogues();
    const plan = selectCiSuites({ impact: ciImpact("affected", [nativeLow, nativeIntegration]), baseCatalogue, headCatalogue });
    expect(plan.native.map(item => item.id)).toEqual(["finance.critical", "sales.low-risk", "sales.changed"]);
    expect(plan.reasons.find(item => item.id === nativeIntegration.id)?.reasons).toContain("affected by base/head ownership or reverse dependencies");
    expect(plan.browser).toHaveLength(2);
  });

  it("fails closed when a base-critical test is deleted and follows a critical file move", () => {
    const initial = ciCatalogues();
    const renamedFile = "tests/unit/finance/critical-renamed.test.ts";
    const movedCatalogue: Catalogue = { ...initial.headCatalogue, suites: initial.headCatalogue.suites.map(item => item.id === initial.headCritical.id
      ? { ...item, id: "finance.critical-moved", files: [renamedFile], ci: { critical: false, reason: "Candidate classification changed during a move." } }
      : item) };
    const moved = selectCiSuites({ impact: ciImpact("affected", [], [{ status: "R100", previousPath: initial.baseCritical.files[0], path: renamedFile }]), baseCatalogue: initial.baseCatalogue, headCatalogue: movedCatalogue });
    expect(moved.native.map(item => item.id)).toContain("finance.critical-moved");
    expect(() => selectCiSuites({ impact: ciImpact("affected", [], [{ status: "D", path: initial.baseCritical.files[0] }]), baseCatalogue: initial.baseCatalogue, headCatalogue: initial.headCatalogue }))
      .toThrow("BASE_CRITICAL_TEST_REMOVED");
  });

  it("runs all PR-supported suites on global/missing-mapping fallback and defers UAT and product-host suites", () => {
    const { baseCatalogue, headCatalogue } = ciCatalogues();
    const plan = selectCiSuites({ impact: ciImpact("full"), baseCatalogue, headCatalogue });
    expect(plan.native).toHaveLength(3);
    expect(plan.browser.map(item => item.id)).toEqual(["identity_access.auth-smoke.e2e", "finance.demo-controlled-payment.e2e", "sales.change-flow.journeys"]);
    expect(plan.deferredToFull.map(item => item.id)).toEqual(expect.arrayContaining(["identity_access.product-hosts.e2e", "finance.routes.uat"]));
    const missingBase: Catalogue = { ...baseCatalogue, suites: baseCatalogue.suites.map(item => item.lane === "unit" || item.lane === "integration" ? { ...item, ci: undefined } : item) };
    const conservative = selectCiSuites({ impact: ciImpact("affected", []), baseCatalogue: missingBase, headCatalogue });
    expect(conservative.baseCriticalFallback).toBe(true);
    expect(conservative.native).toHaveLength(3);
    expect(conservative.browser).toHaveLength(3);
  });

  it("treats docs/testing Markdown as docs-only while still selecting critical native and browser baselines", () => {
    const { baseCatalogue, headCatalogue } = ciCatalogues();
    const input = { changed: [{ status: "M", path: "docs/testing/README.md" }], base, head: base, baseCatalogue, headCatalogue, baseOwnership: ownership, headOwnership: ownership };
    expect(selectImpact(input).mode).toBe("full");
    const impact = selectImpact(input, "pull-request");
    expect(impact).toMatchObject({ mode: "none", suites: [] });
    const plan = selectCiSuites({ impact, baseCatalogue, headCatalogue });
    expect(plan.native.map(item => item.id)).toEqual(["finance.critical"]);
    expect(plan.browser.map(item => item.id)).toEqual(["identity_access.auth-smoke.e2e", "finance.demo-controlled-payment.e2e"]);
  });

  it("requires all native classifications and independently pins the money, IAM, transaction, isolation, and selector anchors", () => {
    const actual = loadCatalogue();
    const critical = new Set(actual.suites.filter(item => item.lane === "unit" || item.lane === "integration").filter(item => item.ci?.critical).map(item => item.id));
    for (const id of ["finance.money.unit", "finance.company-money.unit", "finance.core-services.unit", "finance.books.integration", "people_payroll.accounting.unit",
      "people_payroll.payroll-math.unit", "identity_access.iam.integration", "identity_access.account-recovery.unit", "shared.architecture.architecture-transaction.unit",
      "shared.business-contracts.owner-isolation.unit", "shared.attachments.sensitive-attachment-access.integration", "files.application.unit", "shared.architecture.test-selection.unit",
      "shared.test-infrastructure.ci-regression.unit", "shared.test-infrastructure.browser-isolation-receipts.unit"])
      expect(critical.has(id), id).toBe(true);
    const target = actual.suites.find(item => item.id === "finance.money.unit")!;
    expect(() => validateCatalogue({ ...actual, suites: actual.suites.map(item => item.id === target.id ? { ...item, ci: undefined } : item) })).toThrow(`NATIVE_CI_CLASSIFICATION_REQUIRED:${target.id}`);
  });
});

describe("required native cases", () => {
  const testModule = (names: [string, string][]) => ({ relativeModuleId: "tests/case.test.ts", children: { allTests: () => names.map(([fullName, state]) => ({ fullName, result: () => ({ state }) })) } });
  it("rejects a selected file disappearing from framework collection", () => {
    expect(requiredCaseFailures([testModule([["rules", "passed"]])], ["tests/case.test.ts", "tests/missing.test.ts"]).missing).toEqual(["tests/missing.test.ts"]);
  });
  it("requires current screenshot guard identities independently of a shortened or renamed test collection", () => {
    const required = loadRequiredCases().filter(test => test.file === "tests/unit/shared.test-infrastructure/case-screenshot.test.ts");
    expect(required).toHaveLength(7);
    const scope = { runner: "vitest" as const, files: ["tests/unit/shared.test-infrastructure/case-screenshot.test.ts"] };
    expect(() => assertRequiredCaseInventory(required, required.slice(1), scope)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
    expect(() => assertRequiredCaseInventory(required, [{ ...required[0], title: "new optional screenshot check" }, ...required.slice(1)], scope)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
    expect(() => assertRequiredCaseInventory(required, [...required, { ...required[0], title: "new optional screenshot check" }], scope)).not.toThrow();
    expect(() => currentRequiredCases({ version: 1, cases: [] })).toThrow("INVALID_CURRENT_REQUIRED_CASE_REGISTER");
    expect(() => currentRequiredCases({ version: 1, cases: [required[0], required[0]] })).toThrow("DUPLICATE_CURRENT_REQUIRED_CASE");
  });
  it("requires WorkCards business identities on both devices and retains only the explicit hardware sweep exclusion", () => {
    const file = "tests/e2e/shared.work/work-cards.spec.ts", hardware = "cards fit 390, 768 and 1440px in both themes and retain keyboard focus";
    const required = loadRequiredCases().filter(test => test.file === file);
    expect(required).toHaveLength(13);
    expect(required.filter(test => test.project === "desktop")).toHaveLength(7);
    expect(required.filter(test => test.project === "mobile")).toHaveLength(6);
    expect(required.filter(test => test.title === hardware)).toEqual([{ runner: "playwright", file, title: hardware, project: "desktop" }]);
    const scope = { runner: "playwright" as const, files: [file], projects: ["desktop", "mobile"] as ("desktop" | "mobile")[] };
    const synced = required.find(test => test.project === "mobile" && test.title === "pin order syncs between devices and conflicts preserve the proposed arrangement");
    expect(synced).toBeDefined();
    expect(() => assertRequiredCaseInventory(required, required.filter(test => test !== synced), scope)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
    expect(() => assertRequiredCaseInventory(required, required.map(test => test === synced ? { ...test, title: "optional local-only pin check" } : test), scope)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
    expect(() => assertRequiredCaseInventory(required, [...required, { runner: "playwright", file, title: hardware, project: "mobile", required: false }], scope)).not.toThrow();
  });
  it("rejects skipped, pending and empty required cases", () => {
    expect(requiredCaseFailures([testModule([["skip", "skipped"], ["todo", "pending"]])], []).skipped).toEqual(["skip", "todo"]);
    expect(requiredCaseFailures([testModule([])], []).empty).toBe(true);
  });
  it("permits an explicit case filter only when its matching cases execute", () => {
    const cases = [testModule([["currency rule", "passed"], ["other rule", "skipped"]])];
    expect(requiredCaseFailures(cases, [], /currency/)).toEqual({ missing: [], skipped: [], empty: false });
    expect(requiredCaseFailures(cases, [], /absent/).empty).toBe(true);
  });
  it("rejects a registered case disappearing from a collected file and accepts additional optional cases", () => {
    const registered = [{ runner: "vitest" as const, file: "tests/case.test.ts", title: "approved currency rule" }];
    expect(requiredCaseFailures([testModule([["other check", "passed"]])], ["tests/case.test.ts"], undefined, registered).missing[0]).toContain("REGISTERED_REQUIRED_CASE_MISSING");
    expect(requiredCaseFailures([testModule([["approved currency rule", "passed"], ["new optional check", "passed"]])], ["tests/case.test.ts"], undefined, registered)).toEqual({ missing: [], skipped: [], empty: false });
    expect(requiredCaseFailures([testModule([["new optional check", "passed"]])], ["tests/case.test.ts"], /new optional/, registered).missing).toEqual([]);
    expect(requiredCaseFailures([testModule([["other currency check", "passed"]])], ["tests/case.test.ts"], /currency/, registered).missing[0]).toContain("REGISTERED_REQUIRED_CASE_MISSING");
  });
  it("retains original JSON case identities from Vitest ancestry without rewriting literal greater-than signs", () => {
    const parent = { type: "suite", name: "money > threshold", parent: { type: "suite", name: "Commerce", parent: { type: "module", name: "ignored-file.test.ts" } } };
    const test = { name: "rounds tax > zero", parent, fullName: "Commerce > money > threshold > rounds tax > zero", result: () => ({ state: "passed" }) };
    expect(nativeCaseTitle(test)).toBe("Commerce money > threshold rounds tax > zero");
    const capabilityModule = { relativeModuleId: "tests/case.test.ts", children: { allTests: () => [test] } };
    expect(requiredCaseFailures([capabilityModule], ["tests/case.test.ts"], /^Commerce money > threshold rounds tax > zero$/, [{ runner: "vitest", file: "tests/case.test.ts", title: "Commerce money > threshold rounds tax > zero" }])).toEqual({ missing: [], skipped: [], empty: false });
  });
  it("fails when a registered file has no discovery even if another required file passes", () => {
    const registered = [{ runner: "playwright" as const, file: "tests/e2e/required.spec.ts", title: "required consent", project: "mobile" as const }];
    expect(() => assertRequiredCaseInventory(registered, [{ runner: "playwright", file: "tests/e2e/optional.spec.ts", title: "optional consent", project: "mobile" }], { runner: "playwright", files: ["tests/e2e/required.spec.ts", "tests/e2e/optional.spec.ts"], projects: ["mobile"] })).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
  });
});

describe("complete UAT source route inventory", () => {
  const pages = ["src/app/page.tsx", "src/app/holdings/[holdingSlug]/people/page.tsx"];
  const entries = [{ source: pages[0], template: "/" }, { source: pages[1], template: "/holdings/[holdingSlug]/people" }];
  it("rejects a missing required page replaced by an unrelated route despite an unchanged count", () => {
    expect(() => assertUatRouteSources({ pageRouteCount: 2, entries: [entries[0], { source: "src/app/optional/page.tsx", template: "/optional" }] }, pages)).toThrow("UAT_ROUTE_SOURCE_INVENTORY_MISMATCH");
    expect(() => assertUatRouteSources({ pageRouteCount: 2, entries: [...entries].reverse() }, pages)).not.toThrow();
    expect(() => assertUatRouteSources({ pageRouteCount: 2, entries: [entries[0], { ...entries[1], template: "/optional" }] }, pages)).toThrow("UAT_ROUTE_SOURCE_INVENTORY_MISMATCH");
  });
  it("rejects a duplicated page hiding another required page despite an unchanged count", () => {
    expect(() => assertUatRouteSources({ pageRouteCount: 2, entries: [entries[0], entries[0]] }, pages)).toThrow("UAT_ROUTE_SOURCE_INVENTORY_MISMATCH");
  });
  const route = { template: "/holdings/[holdingSlug]/finance", area: "Holding workspace", signedOut: "login-or-explicit-denial" as const, workspaceScopes: ["holding", "branch", "department", "branch-department"], detailRoute: false };
  it("rejects removal of an entire signed-in actor category despite passing anonymous discovery", () => {
    const required = requiredUatMatrixCases([route]);
    const scope = { runner: "playwright" as const, files: ["tests/uat/shared.ui/uat-route-matrix.spec.ts"], projects: ["desktop", "mobile"] as ("desktop" | "mobile")[] };
    expect(() => assertRequiredCaseInventory(required, required, scope)).not.toThrow();
    expect(() => assertRequiredCaseInventory(required, required.filter(test => !test.title.startsWith("signed-in as finance ")), scope)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
  });
  it("rejects removal of a whole scope loop and preserves exact names on both projects", () => {
    const required = requiredUatMatrixCases([route]);
    const title = "holding, branch, department, and combined scope routes · branch-department /holdings/[holdingSlug]/finance · scope:branch-department";
    expect(required.filter(test => test.title === title).map(test => test.project)).toEqual(["desktop", "mobile"]);
    expect(() => assertRequiredCaseInventory(required, required.filter(test => !test.title.includes("scope:branch-department")), { runner: "playwright", files: ["tests/uat/shared.ui/uat-route-matrix.spec.ts"], projects: ["desktop", "mobile"] })).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
  });
});

describe("disposable Acme seed target", () => {
  it("accepts only all eight exact unstarted browser databases and rejects mixed development targets", () => {
    const run="abcdef012345",state={run,prefix:`erpx_browser_${run}`,profile:"local-seed"};
    const databases=Object.fromEntries(["CONTROL_DB","CONTROL_READ_DB","H001_DB","H001_READ_DB","H002_DB","H002_READ_DB","H003_DB","H003_READ_DB"].map(binding=>[binding,`postgresql://erpx_test_admin@127.0.0.1:5432/erpx_browser_${run}_${binding.toLowerCase()}`]));
    expect(()=>assertAcmeBrowserSeedTarget({databases},state,run)).not.toThrow();
    expect(()=>assertAcmeBrowserSeedTarget({databases:{...databases,H001_DB:"postgresql://erpx_test_admin@127.0.0.1:5432/erpx_dev_h001"}},state,run)).toThrow("EXACT_ACME_BROWSER_SEED_DATABASE_REQUIRED");
    expect(()=>assertAcmeBrowserSeedTarget({databases:{...databases,H002_DB:databases.H002_DB.replace(run,"aaaaaaaaaaaa")}},state,run)).toThrow("EXACT_ACME_BROWSER_SEED_DATABASE_REQUIRED");
    expect(()=>assertAcmeBrowserSeedTarget({databases}, {...state,ready:true},run)).toThrow("EXACT_ACME_BROWSER_SEED_COHORT_REQUIRED");
    const {CONTROL_READ_DB,...incomplete}=databases;void CONTROL_READ_DB;
    expect(()=>assertAcmeBrowserSeedTarget({databases:incomplete},state,run)).toThrow("EXACT_ACME_BROWSER_SEED_COHORT_REQUIRED");
  });
});

describe("standalone provider rehearsal inventory", () => {
  const modules = loadCatalogue().modules;
  const rehearsals = loadProviderRehearsals(process.cwd(), modules);
  const scripts = Object.fromEntries(rehearsals.map(rehearsal => [rehearsal.packageScript, rehearsal.command]));
  it("discloses both existing manual sandbox runners as unselected and unexecuted", () => {
    expect(rehearsals.map(rehearsal => rehearsal.packageScript)).toEqual(["test:pms:stripe:sandbox", "test:pos:stripe:sandbox"]);
    expect(() => validateProviderRehearsals({ version: 1, rehearsals }, scripts, () => true, modules)).not.toThrow();
    expect(unselectedProviderRehearsals(rehearsals).every(rehearsal => rehearsal.interaction.manual && !rehearsal.selected && !rehearsal.executed)).toBe(true);
  });
  it("rejects a missing retained registration or executable source", () => {
    expect(() => validateProviderRehearsals({ version: 1, rehearsals: rehearsals.slice(1) }, scripts, () => true, modules)).toThrow("PROVIDER_REHEARSAL_NOT_REGISTERED:test:pms:stripe:sandbox");
    expect(() => validateProviderRehearsals({ version: 1, rehearsals }, scripts, source => source !== rehearsals[0].source, modules)).toThrow("PROVIDER_REHEARSAL_SOURCE_MISSING_OR_DUPLICATE");
  });
  it("rejects package command drift and a register that changes the executable", () => {
    expect(() => validateProviderRehearsals({ version: 1, rehearsals }, { ...scripts, "test:pms:stripe:sandbox": "bun tests/provider/pms/pms-stripe-sandbox-rehearsal.ts" }, () => true, modules)).toThrow("PROVIDER_REHEARSAL_RUNNER_MISMATCH");
    expect(() => validateProviderRehearsals({ version: 1, rehearsals: [{ ...rehearsals[0], source: "tests/unrelated.ts" }, rehearsals[1]] }, scripts, () => true, modules)).toThrow("PROVIDER_REHEARSAL_RUNNER_MISMATCH");
  });
  it("rejects an executable non-framework test added without a standalone registration", () => {
    expect(() => validateProviderRehearsals({ version: 1, rehearsals }, { ...scripts, "test:new:provider": "bun --conditions=react-server tests/new-provider-rehearsal.ts" }, () => true, modules)).toThrow("EXECUTABLE_TEST_NOT_REGISTERED");
  });
});

describe("database-free unit fixture imports", () => {
  const directories: string[] = [];
  afterEach(() => { for (const directory of directories.splice(0)) rmSync(directory, { recursive: true, force: true }); });
  it("rejects transitive and dynamic fixture imports while accepting a pure helper", () => {
    const root = mkdtempSync(join(tmpdir(), "erpx-test-catalogue-")); directories.push(root);
    mkdirSync(join(root, "tests/support/helpers"), { recursive: true });
    writeFileSync(join(root, "tests/support/runtime-env.ts"), "export const env = {};");
    writeFileSync(join(root, "tests/support/helpers/a.ts"), "export { env } from '../runtime-env';");
    writeFileSync(join(root, "tests/case.test.ts"), "import('./support/helpers/a');");
    writeFileSync(join(root, "tests/pure.test.ts"), "export const number = 1;");
    expect(reachesDatabaseFixture("tests/case.test.ts", root)).toBe(true);
    expect(reachesDatabaseFixture("tests/pure.test.ts", root)).toBe(false);
  });
  it("detects actual PostgreSQL value imports without interpreting source-text fixtures or type imports as connections", () => {
    const root = mkdtempSync(join(tmpdir(), "erpx-postgres-import-")); directories.push(root);
    mkdirSync(join(root, "tests"), { recursive: true });
    for (const [name, source, database] of [
      ["pool", 'import { Pool } from "pg"; const pool = new Pool();', true],
      ["require", 'const {Pool} = require("pg");', true],
      ["dynamic", 'async function connect(){const pg = await import("pg");return new pg.Pool();}', true],
      ["type", 'import type { Pool } from "pg"; let connection:Pool;', false],
      ["specifier-type", 'import { type Pool } from "pg"; let connection:Pool;', false],
      ["text-fixture", 'const sourceFixture = \'import {Pool} from "pg"; new Pool()\';', false],
    ] as const) {
      writeFileSync(join(root, `tests/${name}.test.ts`), source);
      expect(reachesDatabaseFixture(`tests/${name}.test.ts`, root), name).toBe(database);
    }
  });
});

it("rejects mixed run, non-owned and started ERPX realm browser targets before prerequisite writes", () => {
  const run = "abcdef012345", state = { run, prefix: `erpx_browser_${run}`, profile: "local-seed" };
  const bindings = ["CONTROL_DB", "CONTROL_READ_DB", "H001_DB", "H001_READ_DB", "H002_DB", "H002_READ_DB", "H003_DB", "H003_READ_DB"];
  const databases = Object.fromEntries(bindings.map(binding => [binding, `postgresql://erpx_test_admin@127.0.0.1:5432/erpx_browser_${run}_${binding.toLowerCase()}`]));
  const targets = Object.fromEntries(bindings.map(binding => [binding, { name: `erpx_browser_${run}_${binding.toLowerCase()}`, owner: "erpx_test_admin" }]));
  expect(() => assertLocalBrowserRealmTarget({ databases }, state, run, targets)).not.toThrow();
  expect(() => assertLocalBrowserRealmTarget({ databases }, state, "aaaaaaaaaaaa", targets)).toThrow("EXACT_ACME_BROWSER_SEED_COHORT_REQUIRED");
  expect(() => assertLocalBrowserRealmTarget({ databases }, state, run, { ...targets, H003_DB: { ...targets.H003_DB, owner: "postgres" } })).toThrow("EXACT_LOCAL_BROWSER_REALM_DATABASE_REQUIRED");
  expect(() => assertLocalBrowserRealmTarget({ databases }, state, run, { ...targets, H003_DB: { ...targets.H003_DB, name: "erpx_dev_h003" } })).toThrow("EXACT_LOCAL_BROWSER_REALM_DATABASE_REQUIRED");
  expect(() => assertLocalBrowserRealmTarget({ databases }, { ...state, ready: true }, run, targets)).toThrow("EXACT_ACME_BROWSER_SEED_COHORT_REQUIRED");
  expect(() => assertLocalBrowserRealmTarget({ databases }, { ...state, profile: "staff-work" }, run, targets)).toThrow("EXACT_LOCAL_BROWSER_REALM_DATABASE_REQUIRED");
});

it("isolates the Service enforcement flag while retaining only explicit browser fixture overrides", () => {
  const ambient={ERPX_SERVICE_PACKAGE_ENFORCEMENT:"enforced",ERPX_NODE_BIN:"/pinned/node"};
  expect(vitestChildEnvironment(ambient,false)).not.toHaveProperty("ERPX_SERVICE_PACKAGE_ENFORCEMENT");
  expect(vitestChildEnvironment(ambient,true)).not.toHaveProperty("ERPX_SERVICE_PACKAGE_ENFORCEMENT");
  expect(browserChildEnvironment(ambient)).not.toHaveProperty("ERPX_SERVICE_PACKAGE_ENFORCEMENT");
  expect(browserChildEnvironment(ambient,{ERPX_SERVICE_PACKAGE_ENFORCEMENT:"shadow"})).toHaveProperty("ERPX_SERVICE_PACKAGE_ENFORCEMENT","shadow");
  expect(ambient.ERPX_SERVICE_PACKAGE_ENFORCEMENT).toBe("enforced");
});
