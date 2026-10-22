import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdir, readFile, realpath, readdir, writeFile } from "node:fs/promises";
import { delimiter, dirname, join, resolve } from "node:path";
import { loadCatalogue, repositoryRoot, validateCatalogue, type Suite } from "./catalogue";
import { impactFromGit } from "./impact";
import { assertPrSelection, selectCiSuites, type CiSelection } from "./ci-selection";
import { runBrowserSuites, type BrowserRunReceipt } from "./browser-runner";
import { assignedBrowserShard, browserCaseKey, parseBrowserShard, type BrowserCaseIdentity, type BrowserInventoryCase, type BrowserShard } from "./browser-sharding";
import { internalRequiredCases, loadRequiredCases, requiredCaseKey, type CaseObservation, type RequiredCaseIdentity } from "./required-case-inventory";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { writeJsonOutput } from "./json-output";
import { assertPrCaseCoverage, assertPrReceiptBinding, summarizeNativeCaseRun } from "./pr-evidence";

const root = repositoryRoot;
const directoryFor = (candidate: string) => join(root, ".scratch/dev/test-suites/pr", candidate);
const requiredPrPhases = ["catalogue", "structure", "architecture-acceptance", "documentation", "products", "lint", "typecheck", "native-unit", "native-integration", "isolated-release"] as const;
type PrPhase = typeof requiredPrPhases[number];
type ReleaseIdentity = { id: string; fileManifestSha256: string };
type SelectionContext = { base: string; candidate: string; selection: CiSelection; selectionFingerprint: string; requiredCases: RequiredCaseIdentity[] };
type PrNativeReceipt = {
  version: 1; scope: "pr-regression"; kind: "native"; base: string; candidate: string; selectionFingerprint: string;
  runtimeFingerprint: string; verificationFingerprint: string; passed: boolean; phases: { phase: PrPhase; passed: boolean }[];
  suiteIds: string[]; files: string[]; cases: CaseObservation[];
  caseRuns: { lane: "unit" | "integration"; passed: boolean; files: string[]; caseCount: number; failureCodes: string[] }[];
  release?: ReleaseIdentity;
  deferredToFull: CiSelection["deferredToFull"]; counts: CiSelection["counts"];
};
type PrBrowserReceipt = {
  version: 1; scope: "pr-regression"; kind: "browser"; base: string; candidate: string; selectionFingerprint: string;
  runtimeFingerprint: string; verificationFingerprint: string; release?: ReleaseIdentity; passed: boolean; shard: BrowserShard;
  suiteIds: string[]; inventory: BrowserInventoryCase[]; cases: (BrowserCaseIdentity & { passed: boolean; cleanup?: { completed: boolean; ownedDirectoryAbsent: boolean } })[];
  caseResults: { suite: string; file?: string; project: "desktop" | "mobile"; title?: string; passed?: boolean; required?: false; reason?: string; cleanup?: { completed: boolean; ownedDirectoryAbsent: boolean }; failureCode?: string }[];
  exclusions: (BrowserCaseIdentity & { required: false; reason: string })[]; cleanupFailures: string[]; failureCodes: string[];
};
const fullBrowserSuite = (suite: Suite) => ["e2e", "journeys", "uat"].includes(suite.lane);

function git(args: string[]) { return execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim(); }
function checkedCommit(reference: string): string {
  if (!reference || reference.startsWith("-")) throw new Error("INVALID_CI_GIT_REFERENCE");
  return git(["rev-parse", "--verify", `${reference}^{commit}`]);
}
function gitJson<T>(commit: string, path: string): T | undefined {
  try { return JSON.parse(git(["show", `${commit}:${path}`])) as T; }
  catch { return undefined; }
}
function selectionFingerprint(context: Omit<SelectionContext, "selectionFingerprint" | "requiredCases">): string {
  const identity = {
    base: context.base,
    candidate: context.candidate,
    impact: context.selection.impact,
    baseCriticalFallback: context.selection.baseCriticalFallback,
    native: context.selection.native.map(suite => ({ id: suite.id, files: suite.files, ci: suite.ci, reasons: context.selection.reasons.find(item => item.id === suite.id)?.reasons ?? [] })),
    browser: context.selection.browser.map(suite => ({ id: suite.id, files: suite.files, reasons: context.selection.reasons.find(item => item.id === suite.id)?.reasons ?? [] })),
    deferredToFull: context.selection.deferredToFull,
  };
  return createHash("sha256").update(JSON.stringify(identity)).digest("hex");
}

export async function createPrSelection(baseReference: string, headReference?: string): Promise<SelectionContext> {
  const base = checkedCommit(baseReference), candidate = headReference ? checkedCommit(headReference) : checkedCommit("HEAD");
  const candidateIsWorkingTree = !headReference;
  const headCatalogue = candidateIsWorkingTree ? loadCatalogue(root) : gitJson<ReturnType<typeof loadCatalogue>>(candidate, "tests/catalogue/suite-manifest.json");
  if (!headCatalogue) throw new Error("HEAD_TEST_CATALOGUE_MISSING");
  if (candidateIsWorkingTree) validateCatalogue(headCatalogue, root);
  const baseCatalogue = gitJson<ReturnType<typeof loadCatalogue>>(base, "tests/catalogue/suite-manifest.json") ?? {
    ...headCatalogue,
    suites: headCatalogue.suites.map(suite => suite.lane === "unit" || suite.lane === "integration" ? { ...suite, ci: undefined } : suite),
  };
  const impact = impactFromGit(base, root, headReference ? candidate : undefined, "pull-request");
  const requiredCases = internalRequiredCases(loadRequiredCases(root), headCatalogue.suites);
  const selection = selectCiSuites({ impact, baseCatalogue, headCatalogue, requiredCases });
  assertPrSelection(selection, headCatalogue);
  const fingerprint = selectionFingerprint({ base, candidate, selection });
  return { base, candidate, selection, selectionFingerprint: fingerprint, requiredCases };
}

function planOutput(context: SelectionContext) {
  const { selection } = context;
  return {
    ...selection,
    base: context.base,
    candidate: context.candidate,
    selectionFingerprint: context.selectionFingerprint,
    selected: selection.reasons,
    fullProfile: {
      browserSuites: loadCatalogue(root).suites.filter(fullBrowserSuite).map(suite => suite.id),
      uatSuites: loadCatalogue(root).suites.filter(suite => suite.lane === "uat").map(suite => suite.id),
      productHostSuites: loadCatalogue(root).suites.filter(suite => suite.id === "identity_access.product-hosts.e2e").map(suite => suite.id),
    },
    requiredCaseCounts: {
      candidateNative: selection.counts.candidateRequiredNativeCases,
      selectedNative: selection.counts.selectedRequiredNativeCases,
      candidateBrowser: selection.counts.candidateRequiredBrowserCases,
      selectedBrowser: selection.counts.selectedRequiredBrowserCases,
    },
  };
}

async function privateWrite(path: string, value: unknown) {
  await mkdir(join(path, ".."), { recursive: true, mode: 0o700 });
  await writeFile(path, JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
}

async function verifiedRelease(path: string, runtimeFingerprint: string) {
  const releaseDirectory = await realpath(path), release = JSON.parse(await readFile(join(releaseDirectory, "release.json"), "utf8"));
  if (release.sourceFingerprint !== runtimeFingerprint) throw new Error("PR_RELEASE_CANDIDATE_MISMATCH");
  const bytes = await readFile(join(releaseDirectory, "release-files.json"));
  if (createHash("sha256").update(bytes).digest("hex") !== release.fileManifestSha256) throw new Error("PR_RELEASE_FILE_MANIFEST_MISMATCH");
  for (const [name, digest] of Object.entries(JSON.parse(bytes.toString()) as Record<string, string>)) {
    const file = resolve(releaseDirectory, name);
    if (!file.startsWith(`${releaseDirectory}/`) || createHash("sha256").update(await readFile(file)).digest("hex") !== digest) throw new Error(`PR_RELEASE_FILE_MISMATCH:${name}`);
  }
  return { releaseDirectory, identity: { id: String(release.id), fileManifestSha256: String(release.fileManifestSha256) } };
}

async function execute(command: string[], environment: Record<string, string> = {}) {
  const executablePath = process.env.PATH ?? "";
  const path = [dirname(process.execPath), executablePath].filter(Boolean).join(delimiter);
  const child = Bun.spawn([process.execPath, ...command], { cwd: root, env: { ...process.env, PATH: path, ...environment }, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
  const exitCode = await child.exited;
  if (exitCode !== 0) throw new Error(`PR_REGRESSION_COMMAND_FAILED:${command.join(" ")}:${exitCode}`);
}

async function runNative(context: SelectionContext): Promise<PrNativeReceipt> {
  const out = directoryFor(context.candidate);
  await mkdir(out, { recursive: true, mode: 0o700 });
  const runtimeFingerprint = await runtimeSourceFingerprint(root), verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  const phases: PrNativeReceipt["phases"] = [];
  const receipt: PrNativeReceipt = { version: 1, scope: "pr-regression", kind: "native", base: context.base, candidate: context.candidate,
    selectionFingerprint: context.selectionFingerprint, runtimeFingerprint, verificationFingerprint, passed: false, phases, suiteIds: context.selection.native.map(suite => suite.id),
    files: context.selection.native.flatMap(suite => suite.files), cases: [], caseRuns: [], deferredToFull: context.selection.deferredToFull, counts: context.selection.counts };
  await privateWrite(join(out, "native.json"), receipt);
  const stable = async () => {
    if (await runtimeSourceFingerprint(root) !== runtimeFingerprint || await verificationSourceFingerprint(root, runtimeFingerprint) !== verificationFingerprint) throw new Error("PR_SOURCE_CHANGED_DURING_RUN");
  };
  const phase = async (name: PrPhase, command: string[], env: Record<string, string> = {}) => {
    await stable();
    await execute(command, env);
    await stable();
    phases.push({ phase: name, passed: true });
  };
  try {
    await phase("catalogue", ["scripts/testing/test.ts", "--check"]);
    await phase("structure", ["run", "structure:check"]);
    await phase("architecture-acceptance", ["scripts/architecture/acceptance.ts"]);
    await phase("documentation", ["run", "docs:check"]);
    await phase("products", ["run", "products:check"]);
    await phase("lint", ["run", "lint"]);
    await phase("typecheck", ["run", "typecheck"]);
    for (const lane of ["unit", "integration"] as const) {
      const suites = context.selection.native.filter(suite => suite.lane === lane);
      const casePath = join(out, `${lane}-cases.json`);
      if (suites.length) {
        const selectedFiles = suites.flatMap(suite => suite.files);
        const phaseName = lane === "unit" ? "native-unit" : "native-integration";
        let phaseError: unknown;
        try {
          await stable();
          await execute(["scripts/testing/test.ts", lane, ...suites.flatMap(suite => ["--suite", suite.id])], { ERPX_TEST_CASE_RECEIPT: casePath });
          await stable();
        } catch (error) { phaseError = error; }
        let observed: unknown;
        try { observed = JSON.parse(await readFile(casePath, "utf8")); } catch { /* The private receipt still reports a missing case artifact. */ }
        const evidence = summarizeNativeCaseRun({ commandPassed: phaseError === undefined, selectedFiles, requiredCases: context.requiredCases, receipt: observed });
        receipt.cases.push(...evidence.cases);
        receipt.caseRuns.push({ lane, passed: evidence.passed, files: selectedFiles, caseCount: evidence.cases.length, failureCodes: evidence.failureCodes });
        phases.push({ phase: phaseName, passed: evidence.passed });
        if (phaseError) throw phaseError;
        if (!evidence.passed) throw new Error(`PR_NATIVE_CASE_EVIDENCE_FAILED:${evidence.failureCodes.join(",")}`);
      } else phases.push({ phase: lane === "unit" ? "native-unit" : "native-integration", passed: true });
    }
    await phase("isolated-release", ["run", "deploy:build"]);
    const release = await verifiedRelease(join(root, ".runtime/releases/latest"), runtimeFingerprint);
    receipt.release = release.identity;
    const archive = Bun.spawn(["tar", "-cf", join(out, "candidate-release.tar"), "-C", release.releaseDirectory, "."], { cwd: root, stdin: "ignore", stdout: "inherit", stderr: "inherit" });
    if (await archive.exited !== 0) throw new Error("PR_RELEASE_ARCHIVE_FAILED");
    await stable();
    if (phases.length !== requiredPrPhases.length || requiredPrPhases.some(required => phases.filter(item => item.phase === required && item.passed).length !== 1)) throw new Error("PR_NATIVE_PHASES_INCOMPLETE");
    receipt.passed = true;
    return receipt;
  } finally {
    await privateWrite(join(out, "native.json"), receipt);
  }
}

function browserIdentity(value: Record<string, unknown>): BrowserCaseIdentity {
  if (typeof value.suite !== "string" || typeof value.file !== "string" || typeof value.title !== "string" || !["desktop", "mobile"].includes(String(value.project))) throw new Error("INVALID_PR_BROWSER_CASE_IDENTITY");
  return { suite: value.suite, file: value.file, title: value.title, project: value.project as "desktop" | "mobile" };
}

async function runBrowser(context: SelectionContext, shard: BrowserShard): Promise<PrBrowserReceipt> {
  const runtimeFingerprint = await runtimeSourceFingerprint(root), verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  const receipt: PrBrowserReceipt = { version: 1, scope: "pr-regression", kind: "browser", base: context.base, candidate: context.candidate,
    selectionFingerprint: context.selectionFingerprint, runtimeFingerprint, verificationFingerprint, passed: false, shard,
    suiteIds: context.selection.browser.map(suite => suite.id), inventory: [], cases: [], caseResults: [], exclusions: [], cleanupFailures: [], failureCodes: [] };
  let evidenceDirectory: string | undefined;
  const fail = (error: unknown) => {
    const message = error instanceof Error ? error.message : "";
    const code = /^[A-Z][A-Z0-9_]{2,}(?::[A-Z0-9_]+)*$/.test(message) ? message.split(":")[0] : "PR_BROWSER_RUN_FAILED";
    receipt.failureCodes.push(code);
  };
  try {
    const releaseDir = process.env.ERPX_TEST_RELEASE_DIR;
    if (!releaseDir) throw new Error("PR_ATTESTED_RELEASE_DIRECTORY_REQUIRED");
    const release = await verifiedRelease(releaseDir, runtimeFingerprint);
    receipt.release = release.identity;
    let result: Awaited<ReturnType<typeof runBrowserSuites>> | undefined;
    let runError: unknown;
    try {
      result = await runBrowserSuites(context.selection.browser, { releaseDirectory: release.releaseDirectory, shard, caseOrder: "forward", onInvestorEvidence: path => { evidenceDirectory = path; } });
      evidenceDirectory = result.evidenceDirectory;
    } catch (error) { runError = error; fail(error); }
    const raw = evidenceDirectory ? JSON.parse(await readFile(join(evidenceDirectory, "receipt.json"), "utf8")) as BrowserRunReceipt : undefined;
    if (!raw) throw runError ?? new Error("PR_BROWSER_RUN_RECEIPT_MISSING");
    receipt.inventory = raw.inventory;
    receipt.cleanupFailures = raw.cleanupFailures.map(value => {
      const message = String(value).split(":")[0];
      return /^[A-Z][A-Z0-9_]{2,}$/.test(message) ? message : "BROWSER_CLEANUP_FAILED";
    });
    if (raw.version !== 1 || !raw.passed || raw.interrupted || raw.cleanupFailures.length || raw.execution.caseOrder !== "forward" || raw.execution.mode !== "acceptance" ||
        raw.shard?.index !== shard.index || raw.shard.total !== shard.total || raw.runtimeFingerprint !== runtimeFingerprint || raw.verificationFingerprint !== verificationFingerprint ||
        JSON.stringify(raw.suites) !== JSON.stringify(receipt.suiteIds)) fail(new Error("PR_BROWSER_RUN_RECEIPT_MISMATCH"));
    for (const observed of raw.receipts) {
      const safeProject = observed.project === "mobile" ? "mobile" : "desktop";
      const fullIdentity = typeof observed.file === "string" && typeof observed.title === "string" ? {
        suite: observed.suite, file: observed.file, project: safeProject, title: observed.title,
      } satisfies BrowserCaseIdentity : undefined;
      const failureCode = observed.failure ? (/^[A-Z][A-Z0-9_]{2,}(?::[A-Z0-9_]+)*$/.test(observed.failure) ? observed.failure.split(":")[0] : "BROWSER_CASE_FAILED") : undefined;
      receipt.caseResults.push({ suite: observed.suite, ...(observed.file ? { file: observed.file } : {}), project: safeProject,
        ...(observed.title ? { title: observed.title } : {}), ...(typeof observed.passed === "boolean" ? { passed: observed.passed } : {}),
        ...(observed.required === false ? { required: false as const, ...(observed.reason ? { reason: observed.reason } : {}) } : {}),
        ...(observed.cleanup ? { cleanup: observed.cleanup } : {}), ...(failureCode ? { failureCode } : {}) });
      if (!fullIdentity) { fail(new Error("PR_BROWSER_CASE_IDENTITY_MISSING")); continue; }
      if (!assignedBrowserShard(fullIdentity.suite, fullIdentity, shard)) { fail(new Error("PR_BROWSER_CASE_ON_WRONG_SHARD")); continue; }
      if (observed.required === false) {
        if (!observed.reason?.trim()) fail(new Error("PR_BROWSER_EXCLUSION_REASON_REQUIRED"));
        else receipt.exclusions.push({ ...fullIdentity, required: false, reason: observed.reason });
      } else {
        receipt.cases.push({ ...fullIdentity, passed: observed.passed === true, ...(observed.cleanup ? { cleanup: observed.cleanup } : {}) });
        if (observed.passed !== true || !observed.cleanup?.completed || !observed.cleanup.ownedDirectoryAbsent) fail(new Error("PR_BROWSER_CASE_OR_CLEANUP_FAILED"));
      }
    }
    if (new Set(receipt.cases.map(browserCaseKey)).size !== receipt.cases.length || new Set(receipt.exclusions.map(browserCaseKey)).size !== receipt.exclusions.length) fail(new Error("PR_BROWSER_DUPLICATE_CASE_RESULT"));
    receipt.passed = !!result && raw.passed && !runError && receipt.failureCodes.length === 0 && receipt.cleanupFailures.length === 0;
  } catch (error) {
    fail(error);
    if (evidenceDirectory) {
      try {
        const raw = JSON.parse(await readFile(join(evidenceDirectory, "receipt.json"), "utf8")) as BrowserRunReceipt;
        receipt.inventory = raw.inventory;
        receipt.cleanupFailures = raw.cleanupFailures.map(() => "BROWSER_CLEANUP_FAILED");
        receipt.caseResults = raw.receipts.map(observed => ({ suite: observed.suite, ...(observed.file ? { file: observed.file } : {}), project: observed.project,
          ...(observed.title ? { title: observed.title } : {}), ...(typeof observed.passed === "boolean" ? { passed: observed.passed } : {}),
          ...(observed.required === false ? { required: false as const, ...(observed.reason ? { reason: observed.reason } : {}) } : {}),
          ...(observed.cleanup ? { cleanup: observed.cleanup } : {}), ...(observed.failure ? { failureCode: "BROWSER_CASE_FAILED" } : {}) }));
      } catch { /* Keep the minimal failure receipt below. */ }
    }
  }
  await privateWrite(join(directoryFor(context.candidate), `browser-${shard.index}-${shard.total}.json`), receipt);
  if (!receipt.passed) {
    console.error(`PR_BROWSER_PROFILE_FAILED:${receipt.failureCodes.join(",") || "BROWSER_RECEIPT_INCOMPLETE"};cleanup=${receipt.cleanupFailures.length}`);
    throw new Error("PR_BROWSER_PROFILE_FAILED");
  }
  return receipt;
}

async function collectJsonFiles(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  return (await Promise.all(entries.map(entry => entry.isDirectory() ? collectJsonFiles(join(directory, entry.name)) : entry.name.endsWith(".json") ? [join(directory, entry.name)] : []))).flat();
}

async function aggregate(context: SelectionContext, evidenceDirectory: string) {
  const files = await collectJsonFiles(resolve(root, evidenceDirectory));
  const all = await Promise.all(files.map(async path => ({ path, value: JSON.parse(await readFile(path, "utf8")) as Record<string, unknown> })));
  const natives = all.filter(item => item.value.kind === "native").map(item => item.value as unknown as PrNativeReceipt);
  const browsers = all.filter(item => item.value.kind === "browser").map(item => item.value as unknown as PrBrowserReceipt);
  if (natives.length !== 1 || browsers.length !== 4 || new Set(browsers.map(item => item.shard?.index)).size !== 4) throw new Error("PR_CI_RECEIPTS_INCOMPLETE");
  const native = natives[0], candidate = checkedCommit("HEAD");
  assertPrReceiptBinding(native, { kind: "native", base: context.base, candidate, selectionFingerprint: context.selectionFingerprint });
  if (candidate !== context.candidate || native.scope !== "pr-regression" || native.version !== 1 || native.kind !== "native" || !native.passed || native.base !== context.base ||
      native.candidate !== candidate || native.selectionFingerprint !== context.selectionFingerprint || native.phases.length !== requiredPrPhases.length ||
      requiredPrPhases.some(phase => native.phases.filter(item => item.phase === phase && item.passed).length !== 1)) throw new Error("PR_NATIVE_CANDIDATE_REGRESSION_REQUIRED");
  if (!native.release?.id || !/^[a-f0-9]{64}$/.test(native.release.fileManifestSha256) || !/^[a-f0-9]{64}$/.test(native.runtimeFingerprint) || !/^[a-f0-9]{64}$/.test(native.verificationFingerprint)) throw new Error("PR_CANDIDATE_RELEASE_AND_FINGERPRINT_REQUIRED");
  const selectedFiles = context.selection.native.flatMap(suite => suite.files);
  if (new Set(native.files).size !== selectedFiles.length || JSON.stringify([...native.files].sort()) !== JSON.stringify([...selectedFiles].sort()) ||
      JSON.stringify([...native.suiteIds].sort()) !== JSON.stringify(context.selection.native.map(suite => suite.id).sort()) || !Array.isArray(native.cases) || !native.cases.length ||
      native.cases.some(test => test.runner !== "vitest" || test.project !== undefined || !selectedFiles.includes(test.file) || test.status !== "passed" || !test.title.trim())) throw new Error("PR_NATIVE_CASE_INVENTORY_REQUIRED");
  const expectedCaseRuns = (["unit", "integration"] as const).map(lane => ({ lane, files: context.selection.native.filter(suite => suite.lane === lane).flatMap(suite => suite.files) })).filter(run => run.files.length > 0);
  if (!Array.isArray(native.caseRuns) || native.caseRuns.length !== expectedCaseRuns.length || expectedCaseRuns.some((expected, index) => {
    const actual = native.caseRuns[index];
    return !actual || actual.lane !== expected.lane || !actual.passed || JSON.stringify([...actual.files].sort()) !== JSON.stringify([...expected.files].sort()) ||
      actual.caseCount !== native.cases.filter(test => expected.files.includes(test.file)).length || !Array.isArray(actual.failureCodes) || actual.failureCodes.length > 0;
  })) throw new Error("PR_NATIVE_LANE_RECEIPT_INCOMPLETE");
  const nativeFilesWithCases = new Set(native.cases.map(test => test.file));
  if (selectedFiles.some(file => !nativeFilesWithCases.has(file))) throw new Error("PR_NATIVE_FILE_CASES_MISSING");
  const nativeRequired = context.requiredCases.filter(test => test.runner === "vitest" && selectedFiles.includes(test.file));
  for (const expected of nativeRequired) if (native.cases.filter(actual => requiredCaseKey(actual) === requiredCaseKey(expected)).length !== 1) throw new Error(`PR_REQUIRED_NATIVE_CASE_MISSING:${requiredCaseKey(expected)}`);
  const nativeRequiredKeys = new Set(nativeRequired.map(requiredCaseKey));
  assertPrCaseCoverage(nativeRequired.map(requiredCaseKey), native.cases.filter(test => nativeRequiredKeys.has(requiredCaseKey(test))).map(test => ({ key: requiredCaseKey(test), passed: test.status === "passed" })), false);
  if ((await runtimeSourceFingerprint(root)) !== native.runtimeFingerprint || (await verificationSourceFingerprint(root, native.runtimeFingerprint)) !== native.verificationFingerprint) throw new Error("PR_CANDIDATE_SOURCE_FINGERPRINT_MISMATCH");

  const expectedSuiteIds = context.selection.browser.map(suite => suite.id);
  const expectedSuiteProjects = new Set(expectedSuiteIds.flatMap(id => ["desktop", "mobile"].map(project => JSON.stringify([id, project]))));
  let inventorySignature: string | undefined;
  const requiredInventory = new Map<string, BrowserInventoryCase>(), exclusionsInventory = new Map<string, string>();
  const passedCases = new Map<string, BrowserCaseIdentity>(), excludedCases = new Map<string, BrowserCaseIdentity & { required: false; reason: string }>();
  for (const receipt of browsers) {
    assertPrReceiptBinding(receipt, { kind: "browser", base: context.base, candidate, selectionFingerprint: context.selectionFingerprint });
    if (receipt.scope !== "pr-regression" || receipt.version !== 1 || receipt.kind !== "browser" || !receipt.passed || receipt.base !== context.base || receipt.candidate !== candidate ||
        receipt.selectionFingerprint !== context.selectionFingerprint || receipt.runtimeFingerprint !== native.runtimeFingerprint || receipt.verificationFingerprint !== native.verificationFingerprint ||
        !receipt.release?.id || receipt.release.id !== native.release.id || receipt.release.fileManifestSha256 !== native.release.fileManifestSha256 || receipt.shard.total !== 4 ||
        !Array.isArray(receipt.inventory) || !Array.isArray(receipt.cases) || !Array.isArray(receipt.caseResults) || !Array.isArray(receipt.exclusions) || receipt.cleanupFailures.length || receipt.failureCodes.length) throw new Error("PR_BROWSER_CANDIDATE_MISMATCH");
    if (receipt.suiteIds.some(id => !expectedSuiteIds.includes(id) || id === "identity_access.product-hosts.e2e") || JSON.stringify([...receipt.suiteIds].sort()) !== JSON.stringify([...expectedSuiteIds].sort())) throw new Error("PR_BROWSER_SUITE_SELECTION_MISMATCH");
    const signatureRows = receipt.inventory.map(item => {
      const identity = browserIdentity(item as unknown as Record<string, unknown>), key = browserCaseKey(identity);
      if (!expectedSuiteIds.includes(identity.suite) || typeof item.required !== "boolean" || (!item.required && !item.reason?.trim())) throw new Error("INVALID_PR_BROWSER_INVENTORY");
      if (item.required) requiredInventory.set(key, item); else exclusionsInventory.set(key, item.reason!);
      return [key, item.required, item.reason ?? null] as const;
    }).sort((left, right) => left[0].localeCompare(right[0]));
    if (new Set(signatureRows.map(row => row[0])).size !== signatureRows.length) throw new Error("DUPLICATE_PR_BROWSER_INVENTORY");
    const signature = JSON.stringify(signatureRows);
    if (inventorySignature !== undefined && inventorySignature !== signature) throw new Error("PR_BROWSER_SHARD_INVENTORY_MISMATCH");
    inventorySignature = signature;
    const suiteProjects = new Set(receipt.inventory.map(item => JSON.stringify([item.suite, item.project])));
    if (JSON.stringify([...suiteProjects].sort()) !== JSON.stringify([...expectedSuiteProjects].sort())) throw new Error("PR_BROWSER_PROJECT_INVENTORY_MISMATCH");
  }
  if (!inventorySignature) throw new Error("PR_BROWSER_INVENTORY_REQUIRED");
  for (const receipt of browsers) {
    const shard = parseBrowserShard(`${receipt.shard.index}/${receipt.shard.total}`);
    const expectedRequired = [...requiredInventory.values()].filter(test => assignedBrowserShard(test.suite, test, shard)).map(browserCaseKey).sort();
    const actualRequired = receipt.cases.map(test => {
      const key = browserCaseKey(test);
      if (!test.passed || !test.cleanup?.completed || !test.cleanup?.ownedDirectoryAbsent || !requiredInventory.has(key) || !assignedBrowserShard(test.suite, test, shard)) throw new Error(`UNEXPECTED_OR_UNCLEAN_PR_BROWSER_CASE:${key}`);
      return key;
    }).sort();
    if (new Set(actualRequired).size !== actualRequired.length || JSON.stringify(actualRequired) !== JSON.stringify(expectedRequired)) throw new Error("PR_BROWSER_CASE_COVERAGE_OR_CLEANUP_INCOMPLETE");
    assertPrCaseCoverage(expectedRequired, receipt.cases.map(test => ({ key: browserCaseKey(test), passed: test.passed, cleanup: test.cleanup })), true);
    const resultKeys = receipt.caseResults.map(result => {
      if (typeof result.suite !== "string" || typeof result.file !== "string" || typeof result.title !== "string" || !["desktop", "mobile"].includes(result.project)) throw new Error("PR_BROWSER_CASE_RESULT_IDENTITY_MISSING");
      const identity = browserIdentity(result as unknown as Record<string, unknown>), key = browserCaseKey(identity);
      if (result.required === false) {
        if (result.reason !== exclusionsInventory.get(key) || !receipt.exclusions.some(test => browserCaseKey(test) === key && test.reason === result.reason)) throw new Error("PR_BROWSER_CASE_RESULT_EXCLUSION_MISMATCH");
      } else if (result.passed !== true || !result.cleanup?.completed || !result.cleanup.ownedDirectoryAbsent || !receipt.cases.some(test => browserCaseKey(test) === key)) throw new Error("PR_BROWSER_CASE_RESULT_CLEANUP_MISMATCH");
      return key;
    }).sort();
    if (new Set(resultKeys).size !== resultKeys.length || resultKeys.length !== receipt.cases.length + receipt.exclusions.length) throw new Error("PR_BROWSER_CASE_RESULT_INVENTORY_MISMATCH");
    assertPrCaseCoverage(expectedRequired, receipt.caseResults.filter(result => result.required !== false).map(result => ({
      key: browserCaseKey(browserIdentity(result as unknown as Record<string, unknown>)), passed: result.passed === true, cleanup: result.cleanup,
    })), true);
    for (const test of receipt.cases) {
      const key = browserCaseKey(test);
      if (passedCases.has(key)) throw new Error("DUPLICATE_PR_BROWSER_CASE_RESULT");
      passedCases.set(key, test);
    }
    const expectedExcluded = [...exclusionsInventory.entries()].filter(([key]) => {
      const test = receipt.inventory.find(item => browserCaseKey(item) === key)!;
      return assignedBrowserShard(test.suite, test, shard);
    });
    const actualExcluded = receipt.exclusions.map(test => {
      const key = browserCaseKey(test);
      if (test.required !== false || !test.reason.trim() || exclusionsInventory.get(key) !== test.reason || !assignedBrowserShard(test.suite, test, shard)) throw new Error("UNEXPECTED_PR_BROWSER_EXCLUSION");
      return [key, test.reason] as const;
    }).sort((left, right) => left[0].localeCompare(right[0]));
    if (new Set(actualExcluded.map(item => item[0])).size !== actualExcluded.length || JSON.stringify(actualExcluded) !== JSON.stringify(expectedExcluded.sort((left, right) => left[0].localeCompare(right[0])))) throw new Error("PR_BROWSER_EXCLUSION_COVERAGE_INCOMPLETE");
    for (const test of receipt.exclusions) {
      const key = browserCaseKey(test);
      if (excludedCases.has(key)) throw new Error("DUPLICATE_PR_BROWSER_EXCLUSION");
      excludedCases.set(key, test);
    }
  }
  if (requiredInventory.size !== passedCases.size || exclusionsInventory.size !== excludedCases.size) throw new Error("PR_BROWSER_CASE_COVERAGE_INCOMPLETE");
  const browserRequired = context.requiredCases.filter(test => test.runner === "playwright" && expectedSuiteIds.some(id => context.selection.browser.find(suite => suite.id === id)?.files.includes(test.file)));
  for (const expected of browserRequired) {
    const key = browserCaseKey({ suite: expectedSuiteIds.find(id => context.selection.browser.find(suite => suite.id === id)?.files.includes(expected.file))!, file: expected.file, title: expected.title, project: expected.project! });
    if (!requiredInventory.has(key)) throw new Error(`PR_REQUIRED_BROWSER_CASE_MISSING:${key}`);
  }

  const receipt = {
    version: 1 as const,
    scope: "pr-regression" as const,
    passed: true as const,
    base: context.base,
    candidate,
    selectionFingerprint: context.selectionFingerprint,
    runtimeFingerprint: native.runtimeFingerprint,
    verificationFingerprint: native.verificationFingerprint,
    release: native.release,
    selectedSuites: context.selection.reasons,
    deferredToFull: context.selection.deferredToFull,
    providerExcluded: context.selection.providerExcluded,
    counts: {
      ...context.selection.counts,
      candidatePrSupportedSuiteCount: context.selection.counts.candidateNativeSuites + context.selection.counts.candidatePrBrowserSuites,
      candidateFullProfileSuiteCount: context.selection.counts.candidateNativeSuites + context.selection.counts.candidateFullBrowserSuites,
      prSelectedSuiteCount: context.selection.counts.selectedNativeSuites + context.selection.counts.selectedBrowserSuites,
      nativeCasesRun: native.cases.length,
      browserCasesRun: passedCases.size,
      browserCasesExcluded: excludedCases.size,
    },
    native: { phases: native.phases, cases: native.cases },
    browser: { shards: 4, cases: [...passedCases.values()], exclusions: [...excludedCases.values()], cleanupFailures: [] as string[] },
  };
  const output = join(directoryFor(candidate), "aggregate.json");
  await privateWrite(output, receipt);
  return receipt;
}

type ParsedArgs = { mode: "run" | "native" | "browser" | "aggregate" | "plan"; base?: string; head?: string; shard?: string; directory?: string; list: boolean; json: boolean };
function parseArgs(input: string[]): ParsedArgs {
  let mode: ParsedArgs["mode"] = "run";
  const args = [...input];
  if (["run", "native", "browser", "aggregate", "plan"].includes(args[0] ?? "")) mode = args.shift() as ParsedArgs["mode"];
  const result: ParsedArgs = { mode, list: false, json: false };
  for (const arg of args) {
    if (arg === "--list") { if (result.list) throw new Error("DUPLICATE_CI_LIST_OPTION"); result.list = true; continue; }
    if (arg === "--json") { if (result.json) throw new Error("DUPLICATE_CI_JSON_OPTION"); result.json = true; continue; }
    const match = /^(--base|--head|--shard|--directory)=(.+)$/.exec(arg);
    if (!match) throw new Error(`UNKNOWN_CI_REGRESSION_ARGUMENT:${arg}`);
    const key = match[1].slice(2) as "base" | "head" | "shard" | "directory";
    if (result[key]) throw new Error(`DUPLICATE_CI_REGRESSION_OPTION:${match[1]}`);
    result[key] = match[2];
  }
  if (!result.base) throw new Error("CI_BASE_SHA_REQUIRED:use --base=<sha>");
  if (result.list && !["run", "plan"].includes(mode)) throw new Error("CI_LIST_ONLY_SUPPORTED_FOR_PLAN");
  if (result.mode === "browser" && !result.shard) throw new Error("CI_BROWSER_SHARD_REQUIRED");
  if (result.mode !== "browser" && result.shard) throw new Error("CI_BROWSER_SHARD_MODE_ONLY");
  if (result.mode === "aggregate" && !result.directory) throw new Error("CI_AGGREGATE_DIRECTORY_REQUIRED");
  if (result.mode !== "aggregate" && result.directory) throw new Error("CI_AGGREGATE_DIRECTORY_MODE_ONLY");
  return result;
}

export async function runPrRegression(input: string[]) {
  const options = parseArgs(input);
  const context = await createPrSelection(options.base!, options.head);
  const expectedCandidate = process.env.ERPX_TEST_CANDIDATE_SHA;
  if (expectedCandidate && expectedCandidate !== context.candidate) throw new Error("CI_CANDIDATE_SHA_MISMATCH");
  if (options.json || options.list || options.mode === "plan") {
    if (!options.list && options.mode !== "plan") throw new Error("CI_PLAN_REQUIRES_LIST_OR_PLAN_MODE");
    writeJsonOutput(planOutput(context));
    return;
  }
  if (["native", "browser", "aggregate"].includes(options.mode) && context.candidate !== checkedCommit("HEAD")) throw new Error("CI_EXECUTION_HEAD_MUST_BE_CHECKED_OUT");
  if (options.mode === "native") { await runNative(context); return; }
  if (options.mode === "browser") { await runBrowser(context, parseBrowserShard(options.shard!)); return; }
  if (options.mode === "aggregate") { await aggregate(context, options.directory!); return; }
  const native = await runNative(context);
  if (!native.passed) throw new Error("PR_NATIVE_PROFILE_REQUIRED");
  const release = join(directoryFor(context.candidate), "candidate-release-extracted");
  await mkdir(release, { recursive: true, mode: 0o700 });
  const archive = Bun.spawn(["tar", "-xf", join(directoryFor(context.candidate), "candidate-release.tar"), "-C", release], { cwd: root, stdin: "ignore", stdout: "inherit", stderr: "inherit" });
  if (await archive.exited !== 0) throw new Error("PR_RELEASE_EXTRACT_FAILED");
  process.env.ERPX_TEST_RELEASE_DIR = release;
  for (let index = 1; index <= 4; index++) await runBrowser(context, { index, total: 4 });
  await aggregate(context, join(directoryFor(context.candidate)));
}

if (import.meta.main) await runPrRegression(process.argv.slice(2));
