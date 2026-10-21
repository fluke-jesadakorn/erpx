import { describe, expect, it } from "vitest";
import { aggregateCiRegression, assertBrowserProbeCohortBinding, assignedBrowserShard, browserCaseKey, parseBrowserShard, requiredNativePhases, type BrowserCiReceipt, type BrowserInventoryCase, type NativeCiReceipt } from "../../../scripts/testing/browser-sharding";
import { internalRequiredCases, loadRequiredCases, requiredCasesForScope, type RequiredCaseIdentity } from "../../../scripts/testing/required-case-inventory";
import { requiredUatMatrixCases } from "../../../scripts/testing/uat-required-identities";
import {mkdtemp, mkdir, writeFile, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {dirname, join} from "node:path";
import {verificationSourceFingerprint} from "../../../scripts/local/source-fingerprint";
import type { BrowserRunReceipt } from "../../../scripts/testing/browser-runner";
import type { BrowserIsolationProof } from "../../../scripts/testing/browser-isolation-receipts";
import catalogue from "../../catalogue/suite-manifest.json";
import { mergeBrowserAcceptanceReceipts } from "../../../scripts/testing/browser-isolation-groups";

const candidate = "a".repeat(40), fingerprint = "b".repeat(64);
const standardSuite = "identity_access.auth-smoke.e2e";
const standardFile = "tests/e2e/identity_access/auth-smoke.spec.ts";
const cohortReceipt = (
  group: BrowserCiReceipt["cohorts"][number]["group"], suiteIds: string[], shard: number,
  evidence?: { forward: BrowserRunReceipt; reverse: BrowserRunReceipt; probe?: BrowserRunReceipt },
): BrowserCiReceipt["cohorts"][number] => ({
  group, suiteIds, passed: true, ...(evidence ? { forward: evidence.forward, reverse: evidence.reverse } : {}),
  forwardEvidenceDirectory: evidence?.forward.evidencePath ?? `shard-${shard}-${group}-forward`, reverseEvidenceDirectory: evidence?.reverse.evidencePath ?? `shard-${shard}-${group}-reverse`,
  ...(evidence?.probe ? { probe: evidence.probe, probeEvidenceDirectory: evidence.probe.evidencePath ?? `shard-${shard}-${group}-probe` } : {}),
});
function isolationFixture(shard: BrowserCiReceipt): { proof: BrowserIsolationProof; forward: BrowserRunReceipt; reverse: BrowserRunReceipt } {
  let run = shard.shard.index * 1000;
  const freshRun = () => (++run).toString(16).padStart(12, "0");
  const assigned = shard.inventory.filter(test => test.required && assignedBrowserShard(test.suite, test, shard.shard));
  if (!assigned.length) throw new Error("TEST_FIXTURE_ASSIGNED_CASE_REQUIRED");
  const suiteIds = [...new Set(shard.inventory.map(test => test.suite))];
  const suites = suiteIds.map(id => catalogue.suites.find(suite => suite.id === id)).filter((suite): suite is (typeof catalogue.suites)[number] => Boolean(suite));
  const executionInventory = (reverse: boolean) => {
    const orderedSuites = reverse ? [...suiteIds].reverse() : suiteIds;
    const projects = reverse ? ["mobile", "desktop"] as const : ["desktop", "mobile"] as const;
    return orderedSuites.flatMap(suite => projects.flatMap(project => {
      const cases = shard.inventory.filter(test => test.suite === suite && test.project === project);
      return reverse ? cases.reverse() : cases;
    }));
  };
  function acceptance(reverse: boolean): BrowserRunReceipt {
    const inventory = structuredClone(executionInventory(reverse));
    const receipts = inventory.filter(test => assignedBrowserShard(test.suite, test, shard.shard)).map(test => test.required
      ? { ...test, required: undefined, passed: true, run: freshRun(), cleanup: { completed: true, ownedDirectoryAbsent: true } }
      : { ...test, required: false as const, reason: test.reason! });
    const evidenceTime=(reverse?2:1)+(shard.shard.index-1)*3;
    return { version: 1, passed: true, runtimeFingerprint: fingerprint, verificationFingerprint: fingerprint, shard: shard.shard, interrupted: null, suites: suiteIds, cleanupFailures: [], inventory, receipts, execution: { mode: "acceptance", caseOrder: reverse ? "reverse" : "forward" }, evidencePath: `.scratch/dev/test-suites/browser/2026-10-20T12-00-${String(evidenceTime).padStart(2,"0")}-001Z` };
  }
  const rawForward = acceptance(false), rawReverse = acceptance(true);
  const forward = mergeBrowserAcceptanceReceipts(suites, [{ suites, receipt: rawForward }], "forward", shard.shard);
  const reverse = mergeBrowserAcceptanceReceipts(suites, [{ suites, receipt: rawReverse }], "reverse", shard.shard);
  const fault = assigned[0];
  const probe: BrowserRunReceipt = { version: 1, passed: false, runtimeFingerprint: fingerprint, verificationFingerprint: fingerprint, shard: null, interrupted: null, suites: [fault.suite], cleanupFailures: [], inventory: [fault], execution: { mode: "fault-probe", caseOrder: "forward", faultProbe: fault }, receipts: [{ ...fault, required: undefined, run: freshRun(), passed: false, faultProbe: true, failure: "BROWSER_DECLARED_FAULT_PROBE", cleanup: { completed: true, ownedDirectoryAbsent: true } }], evidencePath: `.scratch/dev/test-suites/browser/2026-10-20T12-00-${String(3+(shard.shard.index-1)*3).padStart(2,"0")}-001Z` };
  return { proof: { forward, reverse, probe }, forward: rawForward, reverse: rawReverse };
}
it("invalidates verification when lint rules or CI workflow/action bytes change without changing the runtime identity",async()=>{
  const root=await mkdtemp(join(tmpdir(),"erpx-verification-config-"));
  try{
    const files=["tests/case.test.ts","tests/catalogue/engineering-scenarios.json","tests/catalogue/legal-uat-engineering-cases.json","tests/catalogue/engineering-gate-scenarios.json","tests/catalogue/engineering-attachment-owner-cases.json","tests/catalogue/uat-route-inventory.json","tests/config/vitest.config.mts","tests/config/vitest.unit.config.mts","tests/config/vitest.integration.config.mts","tests/config/playwright.config.ts","eslint.config.mjs",".github/workflows/nextjs.yml",".github/actions/test-runtime/action.yml"];
    for(const name of files){await mkdir(dirname(join(root,name)),{recursive:true});await writeFile(join(root,name),"original");}
    const original=await verificationSourceFingerprint(root,fingerprint);
    for(const name of ["eslint.config.mjs",".github/workflows/nextjs.yml",".github/actions/test-runtime/action.yml"]){
      await writeFile(join(root,name),"changed acceptance policy");
      expect(await verificationSourceFingerprint(root,fingerprint)).not.toBe(original);
      await writeFile(join(root,name),"original");
      expect(await verificationSourceFingerprint(root,fingerprint)).toBe(original);
    }
    await writeFile(join(root,".github/workflows/new-required-check.yml"),"new acceptance policy");
    expect(await verificationSourceFingerprint(root,fingerprint)).not.toBe(original);
  }finally{await rm(root,{recursive:true,force:true});}
});
function fixture() {
  const native: NativeCiReceipt = { version: 1, kind: "native", candidate, runtimeFingerprint: fingerprint, verificationFingerprint: fingerprint, release: { id: "exact-native-build", fileManifestSha256: fingerprint }, passed: true, phases: requiredNativePhases.map(phase => ({ phase, passed: true })), cases: [{ runner: "vitest", file: "tests/sales.test.ts", title: "retains an issued quote", status: "passed" }] };
  const inventory: BrowserInventoryCase[] = [
    { suite: standardSuite, file: standardFile, title: "save a quotation", project: "desktop", required: true },
    { suite: standardSuite, file: standardFile, title: "save a quotation", project: "mobile", required: true },
    { suite: standardSuite, file: standardFile, title: "desktop rail", project: "desktop", required: true },
    { suite: standardSuite, file: standardFile, title: "case two", project: "desktop", required: true },
    { suite: standardSuite, file: standardFile, title: "desktop rail", project: "mobile", required: false, reason: "The rail exists only in the desktop shell." },
  ];
  const browsers: BrowserCiReceipt[] = [1, 2].map(index => {
    const shard = { index, total: 2 }, assigned = inventory.filter(test => assignedBrowserShard(test.suite, test, shard));
    const receipt: BrowserCiReceipt = { version: 1, kind: "browser", candidate, runtimeFingerprint: fingerprint, verificationFingerprint: fingerprint, release: { ...native.release! }, passed: true, shard, inventory: structuredClone(inventory), receipts: assigned.filter(test => test.required).map(test => ({ ...test, passed: true })), exclusions: assigned.filter(test => !test.required).map(test => ({ ...test, required: false as const, reason: test.reason! })), isolation: null, cohorts: [] };
    const isolation = isolationFixture(receipt);
    receipt.isolation = isolation.proof;
    receipt.cohorts = [cohortReceipt("standard", [standardSuite], index, { forward: isolation.forward, reverse: isolation.reverse, probe: isolation.proof.probe })];
    return receipt;
  });
  const requiredCases: RequiredCaseIdentity[] = [...native.cases.map(({ runner, file, title }) => ({ runner, file, title })), ...inventory.filter(test => test.required).map(({ file, title, project }) => ({ runner: "playwright" as const, file, title, project }))];
  return { native, browsers, candidate, total: 2, requiredSuiteIds: [standardSuite], requiredCases };
}

describe("stable independent browser shards", () => {
  it("assigns each case to exactly one shard, including file and project identity", () => {
    const cases = fixture().browsers[0].inventory;
    for (const test of cases) expect(Array.from({ length: 64 }, (_, index) => assignedBrowserShard(test.suite, test, { index: index + 1, total: 64 })).filter(Boolean)).toHaveLength(1);
    expect(browserCaseKey(cases[0])).not.toBe(browserCaseKey(cases[1]));
    expect(browserCaseKey(cases[0])).toBe(browserCaseKey({ ...cases[0], file: `/another/checkout/${cases[0].file}` }));
    expect(browserCaseKey(cases[0])).toBe(browserCaseKey({ ...cases[0], file: "auth-smoke.spec.ts" }));
    for (const different of [
      { ...cases[0], file: "another.spec.ts" },
      { ...cases[0], file: "/foreign/path/sales.spec.ts" },
      { ...cases[0], file: "nested/sales.spec.ts" },
      { ...cases[0], suite: "another.browser" },
      { ...cases[0], title: "another quotation" },
    ]) expect(browserCaseKey(cases[0])).not.toBe(browserCaseKey(different));
  });
  it("rejects invalid and out-of-range shard requests", () => {
    expect(parseBrowserShard("2/64")).toEqual({ index: 2, total: 64 });
    for (const value of ["0/64", "65/64", "1/0", "1/257", "1.5/64", "all", "1/2/3"]) expect(() => parseBrowserShard(value)).toThrow("INVALID_BROWSER_SHARD");
  });
});

describe("full candidate acceptance aggregation", () => {
  it("rejects a whole UAT actor loop removed from every shard's discovery and passed receipts", () => {
    const f = fixture(), suite = "shared.ui.uat-route-matrix.uat";
    const expected = requiredUatMatrixCases([{ template: "/holdings/[holdingSlug]/finance", area: "Holding workspace", signedOut: "login-or-explicit-denial", workspaceScopes: ["branch"], detailRoute: false }]);
    f.requiredCases.push(...expected); f.requiredSuiteIds.push(suite);
    for (const shard of f.browsers) {
      const inventory = expected.map(({ file, title, project }) => ({ suite, file, title, project: project!, required: true }));
      shard.inventory.push(...inventory);
      shard.receipts.push(...inventory.filter(test => assignedBrowserShard(suite, test, shard.shard)).map(test => ({ ...test, passed: true })));
      const isolation = isolationFixture(shard);
      shard.isolation = isolation.proof;
      shard.cohorts = [cohortReceipt("standard", [standardSuite, suite], shard.shard.index, { forward: isolation.forward, reverse: isolation.reverse, probe: isolation.proof.probe })];
    }
    expect(aggregateCiRegression(f).passed).toBe(true);
    for (const shard of f.browsers) {
      shard.inventory = shard.inventory.filter(test => !test.title.startsWith("signed-in as finance "));
      shard.receipts = shard.receipts.filter(test => !test.title.startsWith("signed-in as finance "));
    }
    expect(() => aggregateCiRegression(f)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
  });
  it("rejects deleting a registered native case while its file and every native phase remain green", () => {
    const f = fixture(); f.native.cases = [{ runner: "vitest", file: "tests/sales.test.ts", title: "another optional check", status: "passed" }];
    expect(() => aggregateCiRegression(f)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
  });
  it("rejects a required browser case removed consistently from discovery and all shard receipts", () => {
    const f = fixture();
    for (const shard of f.browsers) {
      shard.inventory = shard.inventory.filter(test => !(test.title === "save a quotation" && test.project === "mobile"));
      shard.receipts = shard.receipts.filter(test => !(test.title === "save a quotation" && test.project === "mobile"));
    }
    expect(() => aggregateCiRegression(f)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
  });
  it("rejects reclassifying a registered device case as an inspectable project exclusion", () => {
    const f = fixture(); for (const shard of f.browsers) {
      const test = shard.inventory.find(test => test.title === "save a quotation" && test.project === "mobile")!;
      test.required = false; test.reason = "Removed mobile coverage";
    }
    expect(() => aggregateCiRegression(f)).toThrow("REGISTERED_REQUIRED_CASE_MISSING");
  });
  it("loads all retained engineering registers and selects provider cases only by their explicit files", () => {
    const cases = loadRequiredCases();
    expect(cases.some(test => test.file === "tests/integration/shared.business-attachments/engineering-business-attachments.test.ts")).toBe(true);
    expect(cases.some(test => test.file === "tests/uat/legal/legal-uat.spec.ts" && test.project === "mobile")).toBe(true);
    expect(requiredCasesForScope(cases, { runner: "vitest", files: ["tests/sales.test.ts"] }).some(test => test.file === "tests/provider/commercial/billing-stripe-sandbox.test.ts")).toBe(false);
    expect(requiredCasesForScope(cases, { runner: "vitest", files: ["tests/provider/commercial/billing-stripe-sandbox.test.ts"] })).toHaveLength(1);
  });
  it("rejects hiding a registered internal required case by moving its file into the provider lane", () => {
    const required: RequiredCaseIdentity[] = [{ runner: "vitest", file: "tests/sales.test.ts", title: "required quote" }];
    expect(() => internalRequiredCases(required, [{ files: ["tests/sales.test.ts"], lane: "provider" }])).toThrow("REGISTERED_REQUIRED_CASE_CANNOT_BECOME_PROVIDER");
    const provider = loadRequiredCases().filter(test => test.file === "tests/provider/commercial/billing-stripe-sandbox.test.ts");
    expect(internalRequiredCases(provider, [{ files: ["tests/provider/commercial/billing-stripe-sandbox.test.ts"], lane: "provider" }])).toEqual([]);
  });
  it("accepts complete native phases and exact required browser coverage with inspectable exclusions", () => {
    expect(aggregateCiRegression(fixture())).toMatchObject({ passed: true, candidate, shards: 2, cases: 4, explicitProjectExclusions: 1 });
  });
  it("requires same-candidate forward and reverse business coverage plus an independently cleaned declared failure on every CI shard", () => {
    expect(aggregateCiRegression(fixture())).toMatchObject({ cases: 4, rerunCases: 4, faultProbes: 2, caseOrders: ["forward", "reverse"] });
    const absent = fixture(); absent.browsers[0].isolation = null;
    expect(() => aggregateCiRegression(absent)).toThrow("ISOLATION_PROOF_REQUIRED");
    const reverse = fixture(); reverse.browsers[0].isolation!.reverse.receipts.pop();
    expect(() => aggregateCiRegression(reverse)).toThrow("CI_ISOLATION_GROUP_RECEIPT_MISMATCH");
    const fault = fixture(); fault.browsers[0].isolation!.probe.receipts = [];
    expect(() => aggregateCiRegression(fault)).toThrow("ISOLATION_EXACT_FAILURE_REQUIRED");
    const drift = fixture(); drift.browsers[0].isolation!.reverse.verificationFingerprint = "c".repeat(64);
    expect(() => aggregateCiRegression(drift)).toThrow("CI_ISOLATION_GROUP_RECEIPT_MISMATCH");
    const reused = fixture(), proof = reused.browsers[0].isolation!;
    proof.probe.receipts[0].run = proof.forward.receipts.find(test => test.passed)!.run;
    expect(() => aggregateCiRegression(reused)).toThrow("ISOLATION_COHORT_REUSED");
    const cleanup = fixture(); cleanup.browsers[0].isolation!.probe.receipts[0].cleanup!.ownedDirectoryAbsent = false;
    expect(() => aggregateCiRegression(cleanup)).toThrow("ISOLATION_COHORT_CLEANUP_REQUIRED");
    const oldForward = fixture(); oldForward.browsers[0].isolation!.forward.inventory[0].reason = "invented metadata";
    oldForward.browsers[0].isolation!.reverse.inventory.find(test => browserCaseKey(test) === browserCaseKey(oldForward.browsers[0].isolation!.forward.inventory[0]))!.reason = "invented metadata";
    expect(() => aggregateCiRegression(oldForward)).toThrow("CI_ISOLATION_FORWARD_RECEIPT_MISMATCH");
  });
  it("keeps an explicit product-host prerequisite failure outside complete CI acceptance", () => {
    const f = fixture();
    f.browsers[0].passed = false;
    f.browsers[0].blocker = { phase: "product-host", reason: "PRODUCT_HOST_PLAN_REQUIRED" };
    f.browsers[0].cohorts = [
      cohortReceipt("standard", [standardSuite], 1, f.browsers[0].isolation!),
      { group: "executive-work", suiteIds: ["shared.executive.executive-work-mcp.e2e"], passed: true },
      { group: "product-host", suiteIds: ["identity_access.product-hosts.e2e"], passed: false, reason: "PRODUCT_HOST_PLAN_REQUIRED" },
    ];
    expect(() => aggregateCiRegression(f)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");
    f.browsers[0].passed = true;
    expect(() => aggregateCiRegression(f)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");
  });
  it("requires successful receipts for every isolation group in the current browser suite inventory", () => {
    const f = fixture();
    f.requiredSuiteIds.push("shared.executive.executive-work-mcp.e2e", "identity_access.product-hosts.e2e");
    expect(() => aggregateCiRegression(f)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");
    delete (f.browsers[0] as Partial<BrowserCiReceipt>).cohorts;
    expect(() => aggregateCiRegression(f)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");
    for (const shard of f.browsers) shard.cohorts = [
      cohortReceipt("standard", [standardSuite], shard.shard.index, shard.isolation!),
      { group: "executive-work", suiteIds: ["shared.executive.executive-work-mcp.e2e"], passed: true },
    ];
    expect(() => aggregateCiRegression(f)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");
    for (const shard of f.browsers) shard.cohorts = [
      cohortReceipt("standard", [standardSuite], shard.shard.index, shard.isolation!),
      { group: "product-host", suiteIds: ["identity_access.product-hosts.e2e"], passed: true },
      { group: "executive-work", suiteIds: ["shared.executive.executive-work-mcp.e2e"], passed: true },
    ];
    expect(() => aggregateCiRegression(f)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");
    for (const shard of f.browsers) shard.cohorts = [
      cohortReceipt("standard", [standardSuite], shard.shard.index, shard.isolation!),
      { group: "executive-work", suiteIds: ["shared.executive.executive-work-mcp.e2e"], passed: true },
      { group: "product-host", suiteIds: ["identity_access.product-hosts.e2e"], passed: true },
    ];
    expect(() => aggregateCiRegression(f)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");
  });
  it("binds each flattened proof to raw group receipts and exactly one group probe", () => {
    const swapped = fixture();
    swapped.browsers[0].cohorts[0].forward = structuredClone(swapped.browsers[0].cohorts[0].reverse!);
    swapped.browsers[0].cohorts[0].forward!.evidencePath=swapped.browsers[0].cohorts[0].forwardEvidenceDirectory;
    expect(() => aggregateCiRegression(swapped)).toThrow("CI_BROWSER_COHORT_ACCEPTANCE_METADATA_REQUIRED");

    const misbound = fixture();
    misbound.browsers[0].cohorts[0].suiteIds = ["shared.executive.executive-work-mcp.e2e"];
    expect(() => aggregateCiRegression(misbound)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");

    const missingProbe = fixture();
    delete (missingProbe.browsers[0].cohorts[0] as Partial<BrowserCiReceipt["cohorts"][number]>).probe;
    expect(() => aggregateCiRegression(missingProbe)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");

    const duplicateProbe = fixture();
    duplicateProbe.requiredSuiteIds.push("shared.executive.executive-work-mcp.e2e");
    for (const shard of duplicateProbe.browsers) {
      const original = shard.cohorts[0];
      shard.cohorts.push(cohortReceipt("executive-work", ["shared.executive.executive-work-mcp.e2e"], shard.shard.index, {
        forward: original.forward!, reverse: original.reverse!, probe: original.probe!,
      }));
    }
    expect(() => aggregateCiRegression(duplicateProbe)).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");

    const badGroupEvidence = fixture();
    badGroupEvidence.browsers[0].cohorts[0].forward = structuredClone(badGroupEvidence.browsers[0].cohorts[0].forward!);
    badGroupEvidence.browsers[0].cohorts[0].forward!.receipts[0].run = "deadbeefcafe";
    expect(() => aggregateCiRegression(badGroupEvidence)).toThrow("CI_ISOLATION_GROUP_RECEIPT_MISMATCH");

    const wrongPath=fixture();
    wrongPath.browsers[0].cohorts[0].forwardEvidenceDirectory=wrongPath.browsers[0].cohorts[0].reverseEvidenceDirectory;
    expect(()=>aggregateCiRegression(wrongPath)).toThrow("BROWSER_COHORT_EVIDENCE_BINDING_REQUIRED");
  });
  it("binds the shard fault probe to the cohort that owns its registered suite",()=>{
    const standard=fixture().browsers[0].cohorts[0],probe=standard.probe!;
    const misplaced=[
      {group:"standard" as const,suiteIds:[standardSuite]},
      {group:"executive-work" as const,suiteIds:["shared.executive.executive-work-mcp.e2e"],probe},
    ];
    const expected=[
      {group:"standard" as const,suiteIds:[standardSuite]},
      {group:"executive-work" as const,suiteIds:["shared.executive.executive-work-mcp.e2e"]},
    ];
    expect(()=>assertBrowserProbeCohortBinding(misplaced,expected)).toThrow("CI_ISOLATION_PROBE_COHORT_MISMATCH");
  });
  it("rejects missing, repeated or candidate-mismatched shards", () => {
    const f = fixture();
    expect(() => aggregateCiRegression({ ...f, browsers: f.browsers.slice(0, 1) })).toThrow("BROWSER_SHARDS_INCOMPLETE");
    expect(() => aggregateCiRegression({ ...f, browsers: [f.browsers[0], f.browsers[0]] })).toThrow("BROWSER_SHARDS_INCOMPLETE");
    f.browsers[1].candidate = "c".repeat(40);
    expect(() => aggregateCiRegression(f)).toThrow("BROWSER_SHARD_CANDIDATE_MISMATCH");
  });
  it("rejects a changed fingerprint or a differing discovered inventory", () => {
    const changed = fixture(); changed.browsers[1].verificationFingerprint = "d".repeat(64);
    expect(() => aggregateCiRegression(changed)).toThrow("BROWSER_SHARD_CANDIDATE_MISMATCH");
    const missing = fixture(); missing.browsers[1].inventory.pop();
    expect(() => aggregateCiRegression(missing)).toThrow("BROWSER_SHARD_INVENTORY_MISMATCH");
  });
  it("requires every shard to use the one attested native release artifact", () => {
    const changed = fixture(); changed.browsers[1].release!.fileManifestSha256 = "e".repeat(64);
    expect(() => aggregateCiRegression(changed)).toThrow("BROWSER_SHARD_CANDIDATE_MISMATCH");
    const missing = fixture(); delete missing.native.release;
    expect(() => aggregateCiRegression(missing)).toThrow("NATIVE_CANDIDATE_RELEASE_REQUIRED");
  });
  it("rejects absent required cases, failing cases and duplicate receipts", () => {
    const missing = fixture(); missing.browsers.find(shard => shard.receipts.length)!.receipts.pop();
    expect(() => aggregateCiRegression(missing)).toThrow("FULL_BROWSER_CASE_COVERAGE_REQUIRED");
    const failed = fixture(); failed.browsers.find(shard => shard.receipts.length)!.receipts[0].passed = false;
    expect(() => aggregateCiRegression(failed)).toThrow("UNEXPECTED_OR_FAILED_BROWSER_CASE");
    const duplicate = fixture(), shard = duplicate.browsers.find(shard => shard.receipts.length)!; shard.receipts.push(shard.receipts[0]);
    expect(() => aggregateCiRegression(duplicate)).toThrow("DUPLICATE_BROWSER_CASE_RECEIPT");
  });
  it("rejects an undeclared exclusion or an exclusion that was never receipted", () => {
    const missing = fixture(); missing.browsers.find(shard => shard.exclusions.length)!.exclusions = [];
    expect(() => aggregateCiRegression(missing)).toThrow("FULL_BROWSER_CASE_COVERAGE_REQUIRED");
    const changed = fixture(); changed.browsers.find(shard => shard.exclusions.length)!.exclusions[0].reason = "different justification";
    expect(() => aggregateCiRegression(changed)).toThrow("UNDECLARED_BROWSER_CASE_EXCLUSION");
  });
  it("rejects missing suite/project inventory even when all returned cases passed", () => {
    const f = fixture();
    expect(() => aggregateCiRegression({ ...f, requiredSuiteIds: [standardSuite, "identity_access.access-feedback.e2e"] })).toThrow("BROWSER_COHORT_PREREQUISITE_REQUIRED");
  });
  it("rejects native skips, absent phases and an unsuccessful native receipt", () => {
    const missing = fixture(); missing.native.phases.pop();
    expect(() => aggregateCiRegression(missing)).toThrow("NATIVE_REGRESSION_PHASE_MISSING");
    const failed = fixture(); failed.native.passed = false;
    expect(() => aggregateCiRegression(failed)).toThrow("NATIVE_CANDIDATE_REGRESSION_REQUIRED");
  });
});
