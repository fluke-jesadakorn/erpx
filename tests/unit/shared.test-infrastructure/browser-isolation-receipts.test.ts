import { mkdir, mkdtemp, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { validateBrowserIsolation, type BrowserIsolationProof } from "../../../scripts/testing/browser-isolation-receipts";
import { browserEvidenceArtifactRootFromReceiptPath, verifyBrowserEvidenceArtifacts } from "../../../scripts/testing/browser-evidence-artifacts";
import { discoverCiReceiptFiles } from "../../../scripts/testing/ci-receipt-discovery";
import { browserIsolationReceiptsForLane, mergeBrowserAcceptanceReceipts, partitionBrowserIsolationSuites } from "../../../scripts/testing/browser-isolation-groups";
import type { BrowserCiReceipt } from "../../../scripts/testing/browser-sharding";
import type { Suite } from "../../../scripts/testing/catalogue";
import type { BrowserRunReceipt } from "../../../scripts/testing/browser-runner";
import type { BrowserInventoryCase } from "../../../scripts/testing/browser-sharding";
import { prepareFullAcceptanceRelease, verifyFullAcceptanceRelease } from "../../../scripts/testing/full-release";

const expected = { runtimeFingerprint: "a".repeat(64), verificationFingerprint: "b".repeat(64) };
function fixture(): BrowserIsolationProof {
  const inventory: BrowserInventoryCase[] = [
    { suite: "sales.e2e", file: "tests/e2e/sales.spec.ts", project: "desktop", title: "save quotation", required: true },
    { suite: "sales.e2e", file: "tests/e2e/sales.spec.ts", project: "mobile", title: "save quotation", required: true },
    { suite: "sales.e2e", file: "tests/e2e/sales.spec.ts", project: "mobile", title: "desktop rail", required: false, reason: "Desktop shell only." },
  ];
  const pass = (caseOrder: "forward" | "reverse", offset: number): BrowserRunReceipt => {
    const ordered = caseOrder === "forward" ? inventory : [...inventory].reverse();
    return { version: 1, passed: true, ...expected, shard: null, interrupted: null, suites: ["sales.e2e"], cleanupFailures: [], execution: { mode: "acceptance", caseOrder }, inventory: structuredClone(ordered), receipts: ordered.map((test, index) => test.required ? { ...test, required: undefined, passed: true, run: (offset + index).toString(16).padStart(12, "0"), cleanup: { completed: true, ownedDirectoryAbsent: true } } : { ...test, required: false }) };
  };
  const forward = pass("forward", 1), reverse = pass("reverse", 10), faultProbe = inventory[0];
  const probe: BrowserRunReceipt = { version: 1, passed: false, ...expected, shard: null, interrupted: null, suites: ["sales.e2e"], cleanupFailures: [], execution: { mode: "fault-probe", caseOrder: "forward", faultProbe }, inventory: [faultProbe], receipts: [{ ...faultProbe, required: undefined, passed: false, faultProbe: true, failure: "BROWSER_DECLARED_FAULT_PROBE", run: "000000000020", cleanup: { completed: true, ownedDirectoryAbsent: true } }] };
  return { forward, reverse, probe };
}

describe("independent browser repetition and failure restart acceptance", () => {
  it("counts business identities once while requiring a complete reverse pass after declared failure cleanup", () => {
    expect(validateBrowserIsolation(fixture(), expected)).toEqual({ cases: 2, rerunCases: 2, faultProbe: true, caseOrders: ["forward", "reverse"] });
  });
  it("rejects a missing repeated case even when forward acceptance passed", () => {
    const proof = fixture(); proof.reverse.receipts.pop();
    expect(() => validateBrowserIsolation(proof, expected)).toThrow("ISOLATION_FULL_REPEAT_COVERAGE_REQUIRED");
  });
  it("rejects reused case data across passes and an incompletely destroyed failure cohort", () => {
    const reused = fixture(); reused.reverse.receipts[1].run = reused.forward.receipts[0].run;
    expect(() => validateBrowserIsolation(reused, expected)).toThrow("ISOLATION_COHORT_REUSED");
    const unclean = fixture(); unclean.probe.receipts[0].cleanup!.ownedDirectoryAbsent = false;
    expect(() => validateBrowserIsolation(unclean, expected)).toThrow("ISOLATION_COHORT_CLEANUP_REQUIRED");
  });
  it("rejects candidate drift and retained cleanup failures in any experiment phase", () => {
    for (const stage of ["forward", "reverse", "probe"] as const) {
      const changed = fixture(); changed[stage].verificationFingerprint = "c".repeat(64);
      expect(() => validateBrowserIsolation(changed, expected)).toThrow("ISOLATION_CANDIDATE_MISMATCH");
      const dirty = fixture(); dirty[stage].cleanupFailures.push("database remains");
      expect(() => validateBrowserIsolation(dirty, expected)).toThrow("ISOLATION_CLEANUP_FAILED");
    }
  });
  it("rejects interrupted acceptance and shard provenance drift", () => {
    const interrupted = fixture(); interrupted.reverse.interrupted = "SIGTERM";
    expect(() => validateBrowserIsolation(interrupted, expected)).toThrow("ISOLATION_EXECUTION_METADATA_MISMATCH");
    const changed = fixture(); changed.forward.shard = { index: 1, total: 2 };
    expect(() => validateBrowserIsolation(changed, expected)).toThrow("ISOLATION_EXECUTION_METADATA_MISMATCH");
  });
  it("rejects a second forward run labelled as reverse and changed project exclusions", () => {
    const reordered = fixture(); reordered.reverse.inventory.reverse(); reordered.reverse.receipts.reverse();
    expect(() => validateBrowserIsolation(reordered, expected)).toThrow("ISOLATION_REVERSED_EXECUTION_REQUIRED");
    const excluded = fixture(); excluded.reverse.inventory[0].reason = "silently changed scope";
    expect(() => validateBrowserIsolation(excluded, expected)).toThrow("ISOLATION_INVENTORY_MISMATCH");
  });
  it("rejects an unexpected business failure, unexpected probe success, and an undeclared injected error", () => {
    const business = fixture(); business.reverse.receipts[1].passed = false;
    expect(() => validateBrowserIsolation(business, expected)).toThrow("ISOLATION_BUSINESS_PASS_REQUIRED");
    const success = fixture(); success.probe.passed = true;
    expect(() => validateBrowserIsolation(success, expected)).toThrow("ISOLATION_DECLARED_FAILURE_REQUIRED");
    const ordinaryFailure = fixture(); ordinaryFailure.probe.receipts[0].failure = "browser timeout";
    expect(() => validateBrowserIsolation(ordinaryFailure, expected)).toThrow("ISOLATION_EXACT_FAILURE_REQUIRED");
  });
  it("binds every browser cohort to its persisted receipt in the candidate artifact", async () => {
    const temporaryRoot = await mkdtemp(join(tmpdir(), "erpx-browser-artifacts-"));
    const artifactRoot = join(temporaryRoot, "test-suites");
    const browserRoot = join(artifactRoot, "browser");
    await mkdir(browserRoot, { recursive: true });
    expect(browserEvidenceArtifactRootFromReceiptPath(join(artifactRoot, "ci", "browser-1-1.json"))).toBe(artifactRoot);
    const downloadedRoot = join(temporaryRoot, "downloaded", "ci-browser-1-sha", "test-suites");
    expect(browserEvidenceArtifactRootFromReceiptPath(join(downloadedRoot, "ci", "browser-1-1.json"))).toBe(downloadedRoot);
    const strippedArtifactRoot = join(temporaryRoot, "downloaded", "ci-browser-1-sha");
    expect(browserEvidenceArtifactRootFromReceiptPath(join(strippedArtifactRoot, "ci", "browser-1-1.json"))).toBe(strippedArtifactRoot);
    const makeReceipt = (stamp: string, caseOrder: "forward" | "reverse", mode: "acceptance" | "fault-probe"): BrowserRunReceipt => ({
      version: 1,
      passed: mode === "acceptance",
      runtimeFingerprint: "a".repeat(64),
      verificationFingerprint: "b".repeat(64),
      shard: { index: 1, total: 1 },
      interrupted: null,
      suites: ["sales.e2e"],
      cleanupFailures: [],
      inventory: [],
      receipts: [],
      execution: { caseOrder, mode },
      evidencePath: `.scratch/dev/test-suites/browser/${stamp}`,
    });
    const forward = makeReceipt("2026-10-20T12-00-01-001Z", "forward", "acceptance");
    const reverse = makeReceipt("2026-10-20T12-00-02-002Z", "reverse", "acceptance");
    const probe = makeReceipt("2026-10-20T12-00-03-003Z", "forward", "fault-probe");
    const receipt: BrowserCiReceipt = {
      version: 1,
      kind: "browser",
      candidate: "a".repeat(40),
      runtimeFingerprint: "a".repeat(64),
      verificationFingerprint: "b".repeat(64),
      passed: true,
      shard: { index: 1, total: 1 },
      inventory: [],
      receipts: [],
      exclusions: [],
      isolation: null,
      cohorts: [{
        group: "standard",
        suiteIds: ["sales.e2e"],
        passed: true,
        forward,
        reverse,
        probe,
        forwardEvidenceDirectory: forward.evidencePath,
        reverseEvidenceDirectory: reverse.evidencePath,
        probeEvidenceDirectory: probe.evidencePath,
      }],
    };

    try {
      for (const run of [forward, reverse, probe]) {
        const directory = join(browserRoot, run.evidencePath!.split("/").at(-1)!);
        await mkdir(directory, { recursive: true });
        await writeFile(join(directory, "receipt.json"), JSON.stringify(run));
      }
      const archiveRoot = join(temporaryRoot, "ci", "attempts", "old");
      await mkdir(archiveRoot, { recursive: true });
      await writeFile(join(temporaryRoot, "ci", "native.json"), "{}");
      await writeFile(join(temporaryRoot, "ci", "browser-1-1.json"), "{}");
      await writeFile(join(archiveRoot, "native.json"), "{}");
      expect(await discoverCiReceiptFiles(join(temporaryRoot, "ci"))).toEqual([
        join(temporaryRoot, "ci", "browser-1-1.json"),
        join(temporaryRoot, "ci", "native.json"),
      ]);
      await expect(discoverCiReceiptFiles(join(temporaryRoot, "ci", "attempts"))).rejects.toThrow("CI_ARCHIVED_RECEIPT_DIRECTORY_FORBIDDEN");
      const archiveAlias = join(temporaryRoot, "ci", "receipt-input");
      await symlink(archiveRoot, archiveAlias, "dir");
      await expect(discoverCiReceiptFiles(archiveAlias)).rejects.toThrow("CI_ARCHIVED_RECEIPT_DIRECTORY_FORBIDDEN");
      await expect(verifyBrowserEvidenceArtifacts(receipt, artifactRoot)).resolves.toBeUndefined();

      const mismatchedPath = join(browserRoot, forward.evidencePath!.split("/").at(-1)!, "receipt.json");
      await writeFile(mismatchedPath, JSON.stringify({ ...forward, passed: false }));
      await expect(verifyBrowserEvidenceArtifacts(receipt, artifactRoot)).rejects.toThrow("CI_BROWSER_EVIDENCE_RECEIPT_MISMATCH");
    } finally {
      await rm(temporaryRoot, { recursive: true, force: true });
    }
  });
});

describe("full acceptance release binding", () => {
  const fingerprint = "d".repeat(64);
  let root: string;
  beforeEach(async () => { root = await realpath(await mkdtemp(join(tmpdir(), "erpx-full-release-"))); });
  afterEach(async () => { await rm(root, { recursive: true, force: true }); });

  async function createRelease(id = "2026-10-21T00-00-00-000Z", sourceFingerprint = fingerprint) {
    const directory = join(root, ".runtime/releases", id);
    await mkdir(directory, { recursive: true });
    const artifact = Buffer.from("compiled candidate\n");
    await writeFile(join(directory, "server.js"), artifact);
    const entries = { "server.js": createHash("sha256").update(artifact).digest("hex") };
    const fileManifest = Buffer.from(JSON.stringify(entries, null, 2) + "\n");
    const fileManifestSha256 = createHash("sha256").update(fileManifest).digest("hex");
    await writeFile(join(directory, "release-files.json"), fileManifest);
    await writeFile(join(directory, "release.json"), JSON.stringify({ id, sourceFingerprint, node: "26.11.1", fileManifestSha256 }));
    return directory;
  }

  it("accepts a canonical prebuilt release and the normal latest link only when every artifact matches", async () => {
    const directory = await createRelease();
    await symlink("2026-10-21T00-00-00-000Z", join(root, ".runtime/releases/latest"));
    const prebuilt = await verifyFullAcceptanceRelease(root, fingerprint, directory);
    const latest = await verifyFullAcceptanceRelease(root, fingerprint);
    expect(prebuilt).toMatchObject({ directory, id: "2026-10-21T00-00-00-000Z" });
    expect(latest).toEqual(prebuilt);
  });

  it("uses a prebuilt release without rebuilding and builds the default release before verification", async () => {
    const directory = await createRelease();
    const latest = join(root, ".runtime/releases/latest");
    await symlink("2026-10-21T00-00-00-000Z", latest);
    let prebuiltBuilds = 0;
    const prebuilt = await prepareFullAcceptanceRelease(root, fingerprint, directory, async () => { prebuiltBuilds += 1; });
    expect(prebuilt.id).toBe("2026-10-21T00-00-00-000Z");
    expect(prebuiltBuilds).toBe(0);

    let defaultBuilds = 0;
    const built = await prepareFullAcceptanceRelease(root, fingerprint, undefined, async () => { defaultBuilds += 1; });
    expect(built.id).toBe("2026-10-21T00-00-00-000Z");
    expect(defaultBuilds).toBe(1);
  });

  it("rejects latest aliases, release paths outside the isolated release root, and stale source bindings", async () => {
    const directory = await createRelease();
    await symlink("2026-10-21T00-00-00-000Z", join(root, ".runtime/releases/latest"));
    await expect(verifyFullAcceptanceRelease(root, fingerprint, join(root, ".runtime/releases/latest")))
      .rejects.toThrow("FULL_REGRESSION_RELEASE_PATH_NOT_CANONICAL");
    const outside = join(root, "outside");
    await mkdir(outside, { recursive: true });
    await expect(verifyFullAcceptanceRelease(root, fingerprint, outside)).rejects.toThrow("FULL_REGRESSION_RELEASE_DIRECTORY_REQUIRED");
    const stale = await createRelease("stale-release", "e".repeat(64));
    await expect(verifyFullAcceptanceRelease(root, fingerprint, stale)).rejects.toThrow("FULL_REGRESSION_CANDIDATE_MISMATCH");
    expect(directory).toContain(".runtime/releases/");
  });

  it("rejects release-manifest and compiled-file tampering", async () => {
    const directory = await createRelease();
    await writeFile(join(directory, "server.js"), "modified candidate\n");
    await expect(verifyFullAcceptanceRelease(root, fingerprint, directory)).rejects.toThrow("CANDIDATE_INVENTORY_FILE_CHANGED");

    const another = await createRelease("another-release");
    await writeFile(join(another, "release-files.json"), "{}\n");
    await expect(verifyFullAcceptanceRelease(root, fingerprint, another)).rejects.toThrow("FULL_REGRESSION_RELEASE_MANIFEST_MISMATCH");
  });

  it("rejects release IDs the full-receipt importer cannot accept before the full runner continues", async () => {
    const directory = await createRelease("release@invalid");
    let buildCalls = 0, rollbackReached = false;
    await expect((async () => {
      await prepareFullAcceptanceRelease(root, fingerprint, directory, async () => { buildCalls += 1; });
      rollbackReached = true;
    })()).rejects.toThrow("FULL_REGRESSION_RELEASE_ID_INVALID");
    expect(buildCalls).toBe(0);
    expect(rollbackReached).toBe(false);
  });
});

const browserSuite = (id: string, files: string[], fixture: Suite["fixture"], lane: Suite["lane"] = "e2e"): Suite => ({
  id,
  lane,
  owner: "shared.test-infrastructure",
  modules: ["shared.test-infrastructure"],
  files,
  fixture,
  requires: { jobs: false, scanner: false },
});

describe("full-regression browser isolation cohorts", () => {
  it("keeps product-host, Executive Work and Forms suites isolated and preserves every registered suite", () => {
    const sales = browserSuite("sales.e2e", ["tests/e2e/sales/sales.spec.ts"], "general-company");
    const work = browserSuite("work.journey", ["tests/journeys/shared.work/work.spec.ts"], "staff-work", "journeys");
    const product = browserSuite("identity_access.product-hosts.e2e", ["tests/e2e/identity_access/product-hosts.spec.ts"], "general-company");
    const executive = browserSuite("shared.executive.executive-work-mcp.e2e", ["tests/e2e/shared.executive/executive-work-mcp.spec.ts"], "executive-work");
    const forms = browserSuite("marketing.forms-host.e2e", ["tests/e2e/marketing/forms-host.spec.ts"], "engineering");

    const groups = partitionBrowserIsolationSuites([sales, executive, work, product, forms]);
    expect(groups.map(group => [group.id, group.suites.map(suite => suite.id)])).toEqual([
      ["standard", ["sales.e2e", "work.journey"]],
      ["executive-work", ["shared.executive.executive-work-mcp.e2e"]],
      ["product-host", ["identity_access.product-hosts.e2e"]],
      ["forms-host", ["marketing.forms-host.e2e"]],
    ]);
    expect(groups.flatMap(group => group.suites.map(suite => suite.id)).sort()).toEqual([sales, executive, work, product, forms].map(suite => suite.id).sort());
  });

  it("rejects host suites that cannot satisfy the exact browser-runner guards", () => {
    const sales = browserSuite("sales.e2e", ["tests/e2e/sales/sales.spec.ts"], "general-company");
    const executive = browserSuite("executive.e2e", ["tests/e2e/shared.executive/executive-work-mcp.spec.ts", "tests/e2e/extra.spec.ts"], "executive-work");
    const product = browserSuite("product.e2e", ["tests/e2e/identity_access/product-hosts.spec.ts", "tests/e2e/extra.spec.ts"], "general-company");
    const forms = browserSuite("forms.e2e", ["tests/e2e/marketing/forms-host.spec.ts", "tests/e2e/extra.spec.ts"], "engineering");
    const wrongFormsFixture = browserSuite("forms.e2e", ["tests/e2e/marketing/forms-host.spec.ts"], "general-company");

    expect(() => partitionBrowserIsolationSuites([sales, executive])).toThrow("EXACT_EXECUTIVE_WORK_HOST_SUITE_AND_RELEASE_REQUIRED");
    expect(() => partitionBrowserIsolationSuites([sales, product])).toThrow("EXACT_PRODUCT_HOST_SUITE_REQUIRED");
    expect(() => partitionBrowserIsolationSuites([sales, forms])).toThrow("EXACT_FORMS_HOST_SUITE_AND_RELEASE_REQUIRED");
    expect(() => partitionBrowserIsolationSuites([sales, wrongFormsFixture])).toThrow("EXACT_FORMS_HOST_SUITE_AND_RELEASE_REQUIRED");
  });

  it("reassembles lane receipts in catalogue order and refuses gaps or cross-group receipts", () => {
    const sales = browserSuite("sales.e2e", ["tests/e2e/sales/sales.spec.ts"], "general-company");
    const executive = browserSuite("executive.e2e", ["tests/e2e/shared.executive/executive-work-mcp.spec.ts"], "executive-work");
    const journey = browserSuite("work.journey", ["tests/journeys/shared.work/work.spec.ts"], "staff-work", "journeys");
    const product = browserSuite("product.e2e", ["tests/e2e/identity_access/product-hosts.spec.ts"], "general-company");
    const suites = [sales, executive, journey, product];
    const groups = [
      { suites: [sales, journey], receipts: [{ suite: "sales.e2e", title: "sale" }, { suite: "work.journey", title: "journey" }] },
      { suites: [product], receipts: [{ suite: "product.e2e", title: "host" }] },
      { suites: [executive], receipts: [{ suite: "executive.e2e", title: "executive" }] },
    ];

    expect(browserIsolationReceiptsForLane(suites, groups, "e2e").map(receipt => receipt.title)).toEqual(["sale", "executive", "host"]);
    expect(() => browserIsolationReceiptsForLane(suites, groups.slice(1), "e2e")).toThrow("BROWSER_ISOLATION_SUITE_RECEIPTS_REQUIRED");
    expect(() => browserIsolationReceiptsForLane(suites, [{ suites: [sales], receipts: [{ suite: "executive.e2e", title: "misbound" }] }], "e2e"))
      .toThrow("BROWSER_ISOLATION_RECEIPT_GROUP_MISMATCH");
  });

  it("merges isolated shard cohorts in global forward and reverse execution order", () => {
    const sales = browserSuite("sales.e2e", ["tests/e2e/sales/sales.spec.ts"], "general-company");
    const executive = browserSuite("executive.e2e", ["tests/e2e/shared.executive/executive-work-mcp.spec.ts"], "executive-work");
    const journey = browserSuite("work.journey", ["tests/journeys/shared.work/work.spec.ts"], "staff-work", "journeys");
    const product = browserSuite("product.e2e", ["tests/e2e/identity_access/product-hosts.spec.ts"], "general-company");
    const suites = [sales, executive, journey, product];
    const shard = { index: 2, total: 4 };
    const groupReceipt = (groupSuites: Suite[], caseOrder: "forward" | "reverse"): BrowserRunReceipt => {
      const orderedSuites = caseOrder === "forward" ? groupSuites : [...groupSuites].reverse();
      const projects = caseOrder === "forward" ? ["desktop", "mobile"] as const : ["mobile", "desktop"] as const;
      const inventory = orderedSuites.flatMap(suite => projects.map(project => ({ suite: suite.id, file: suite.files[0], title: `${suite.id} ${project}`, project, required: true })));
      return {
        version: 1, passed: true, runtimeFingerprint: "b".repeat(64), verificationFingerprint: "c".repeat(64), shard, interrupted: null,
        suites: orderedSuites.map(suite => suite.id), cleanupFailures: [], inventory,
        receipts: inventory.map(test => ({ suite: test.suite, file: test.file, project: test.project, title: test.title, passed: true })),
        execution: { caseOrder, mode: "acceptance" },
      };
    };
    const groups = [
      { suites: [sales, journey], receipt: groupReceipt([sales, journey], "forward") },
      { suites: [executive], receipt: groupReceipt([executive], "forward") },
      { suites: [product], receipt: groupReceipt([product], "forward") },
    ];
    const forward = mergeBrowserAcceptanceReceipts(suites, groups, "forward", shard);
    const reverse = mergeBrowserAcceptanceReceipts(suites, groups.map(group => ({ ...group, receipt: groupReceipt(group.suites, "reverse") })), "reverse", shard);

    expect(forward.inventory.map(test => test.title)).toEqual([
      "sales.e2e desktop", "sales.e2e mobile", "executive.e2e desktop", "executive.e2e mobile",
      "work.journey desktop", "work.journey mobile", "product.e2e desktop", "product.e2e mobile",
    ]);
    expect(reverse.inventory.map(test => test.title)).toEqual([
      "product.e2e mobile", "product.e2e desktop", "work.journey mobile", "work.journey desktop",
      "executive.e2e mobile", "executive.e2e desktop", "sales.e2e mobile", "sales.e2e desktop",
    ]);
    const emptyExecutive = groups.map(group => group.suites[0].id === executive.id
      ? { ...group, receipt: { ...group.receipt, receipts: [] } }
      : group);
    const sparse = mergeBrowserAcceptanceReceipts(suites, emptyExecutive, "forward", shard);
    expect(sparse.inventory.filter(test => test.suite === executive.id)).toHaveLength(2);
    expect(sparse.receipts.some(test => test.suite === executive.id)).toBe(false);
    expect(() => mergeBrowserAcceptanceReceipts(suites, groups.slice(0, 2), "forward", shard)).toThrow("BROWSER_ISOLATION_SUITE_RECEIPTS_REQUIRED");
  });
});
