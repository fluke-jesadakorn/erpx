import { mkdir, readFile, realpath, writeFile } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { createHash } from "node:crypto";
import { aggregateCiRegression, assignedBrowserShard, parseBrowserShard, requiredNativePhases, type BrowserCaseExclusion, type BrowserCaseReceipt, type BrowserCiReceipt, type NativeCiReceipt } from "./browser-sharding";
import { loadCatalogue, repositoryRoot, validateCatalogue } from "./catalogue";
import { assertCohortsDestroyed, runBrowserIsolation } from "./browser-isolation";
import { mergeBrowserAcceptanceReceipts, partitionBrowserIsolationSuites } from "./browser-isolation-groups";
import { validateBrowserIsolation } from "./browser-isolation-receipts";
import { runBrowserSuites, type BrowserRunReceipt } from "./browser-runner";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { internalRequiredCases, loadRequiredCases, readNativeCaseReceipt } from "./required-case-inventory";
import { preparePdfAssets } from "../assets/prepare-pdf-assets";
import { browserEvidenceArtifactRootFromReceiptPath, verifyBrowserEvidenceArtifacts } from "./browser-evidence-artifacts";
import { discoverCiReceiptFiles } from "./ci-receipt-discovery";
import { productHostSuiteId, validateProductHostCommercialCohorts } from "./product-host-commercial-evidence";
import { productHostPlanPathsFromEnvironment } from "./product-host-cohort";

const root = repositoryRoot, catalogue = loadCatalogue();
validateCatalogue(catalogue);
const args = process.argv.slice(2), mode = args.shift();
if (!["native", "browser", "aggregate"].includes(mode ?? "")) throw new Error("CI_REGRESSION_MODE_REQUIRED");
const option = (name: string) => args.find(argument => argument.startsWith(`${name}=`))?.slice(name.length + 1);
if (args.some(argument => !(mode === "browser" && argument === "--build") && !argument.startsWith(mode === "browser" ? "--shard=" : mode === "aggregate" ? "--directory=" : "<no-options>")) || args.length > (mode === "native" ? 0 : mode === "browser" ? 2 : 1) || new Set(args.map(argument => argument.split("=")[0])).size !== args.length) throw new Error("UNKNOWN_CI_REGRESSION_ARGUMENT");
const candidate = Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root }).stdout.toString().trim();
if (process.env.ERPX_TEST_CANDIDATE_SHA && process.env.ERPX_TEST_CANDIDATE_SHA !== candidate) throw new Error("CI_CANDIDATE_SHA_MISMATCH");
const browserSuites = catalogue.suites.filter(suite => ["e2e", "journeys", "uat"].includes(suite.lane));
const requiredCases = internalRequiredCases(loadRequiredCases(root), catalogue.suites);
const directory = join(root, ".scratch/dev/test-suites/ci");
await mkdir(directory, { recursive: true, mode: 0o700 });
const children = new Set<ReturnType<typeof Bun.spawn>>();
let interrupted: string | undefined;
const signal = (name: "SIGINT" | "SIGTERM") => { interrupted = name; for (const child of children) child.kill(name); };
process.on("SIGINT", () => signal("SIGINT")); process.on("SIGTERM", () => signal("SIGTERM"));

if (mode === "aggregate") {
  const source = option("--directory");
  if (!source) throw new Error("CI_EVIDENCE_DIRECTORY_REQUIRED");
  const loaded = await Promise.all((await discoverCiReceiptFiles(resolve(root, source))).map(async path => ({ path, receipt: JSON.parse(await readFile(path, "utf8")) as NativeCiReceipt | BrowserCiReceipt })));
  const natives = loaded.filter(item => item.receipt.kind === "native").map(item => item.receipt) as NativeCiReceipt[];
  const browsers = loaded.filter((item): item is { path: string; receipt: BrowserCiReceipt } => item.receipt.kind === "browser");
  if (natives.length !== 1 || !browsers.length) throw new Error("FULL_CI_RECEIPTS_REQUIRED");
  for(const {path,receipt} of browsers){
    await verifyBrowserEvidenceArtifacts(receipt,browserEvidenceArtifactRootFromReceiptPath(path));
    if(!natives[0].release)throw new Error("NATIVE_CANDIDATE_RELEASE_REQUIRED");
    await validateProductHostCommercialCohorts(root,receipt.cohorts,
      {runtimeFingerprint:natives[0].runtimeFingerprint,verificationFingerprint:natives[0].verificationFingerprint,release:natives[0].release},
      browserSuites.some(suite=>suite.id===productHostSuiteId));
  }
  const receipt = aggregateCiRegression({ native: natives[0], browsers: browsers.map(item=>item.receipt), candidate, total: browsers[0].receipt.shard.total, requiredSuiteIds: browserSuites.map(suite => suite.id), requiredCases });
  await writeFile(join(directory, "aggregate.json"), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 });
  console.log(JSON.stringify(receipt));
} else {
  await preparePdfAssets();
  const runtimeFingerprint = await runtimeSourceFingerprint(root), verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  async function stable() {
    if (await runtimeSourceFingerprint(root) !== runtimeFingerprint || await verificationSourceFingerprint(root, runtimeFingerprint) !== verificationFingerprint) throw new Error("CI_REGRESSION_SOURCE_CHANGED");
  }
  async function execute(command: string[], environment: Record<string, string> = {}) {
    await stable();
    if (interrupted) throw new Error(`CI_REGRESSION_INTERRUPTED:${interrupted}`);
    const child = Bun.spawn([process.execPath, ...command], { cwd: root, env: { ...process.env, ...environment }, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
    children.add(child);
    try { if (await child.exited !== 0) throw new Error(`CI_REGRESSION_COMMAND_FAILED:${command.join(" ")}`); }
    finally { children.delete(child); }
    await stable();
  }
  async function verifiedRelease(path: string) {
    const releaseDirectory = await realpath(path), release = JSON.parse(await readFile(join(releaseDirectory, "release.json"), "utf8"));
    if (release.sourceFingerprint !== runtimeFingerprint) throw new Error("CI_RELEASE_CANDIDATE_MISMATCH");
    const bytes = await readFile(join(releaseDirectory, "release-files.json"));
    if (createHash("sha256").update(bytes).digest("hex") !== release.fileManifestSha256) throw new Error("CI_RELEASE_FILE_MANIFEST_MISMATCH");
    for (const [name, digest] of Object.entries(JSON.parse(bytes.toString()) as Record<string, string>)) {
      const file = resolve(releaseDirectory, name);
      if (!file.startsWith(`${releaseDirectory}/`) || createHash("sha256").update(await readFile(file)).digest("hex") !== digest) throw new Error(`CI_RELEASE_FILE_MISMATCH:${name}`);
    }
    return { releaseDirectory, identity: { id: String(release.id), fileManifestSha256: String(release.fileManifestSha256) } };
  }
  if (mode === "native") {
    const receipt: NativeCiReceipt = { version: 1, kind: "native", candidate, runtimeFingerprint, verificationFingerprint, passed: false, phases: [], cases: [] };
    const commands: Record<typeof requiredNativePhases[number], string[]> = { architecture: ["scripts/architecture/acceptance.ts"], unit: ["scripts/testing/test.ts", "unit"], integration: ["scripts/testing/test.ts", "integration"], lint: ["run", "lint"], typecheck: ["run", "typecheck"], "isolated-release": ["run", "deploy:build"] };
    try {
      for (const phase of requiredNativePhases) {
        const native = phase === "unit" || phase === "integration", caseReceipt = join(directory, `${phase}-cases.json`);
        await execute(commands[phase], native ? { ERPX_TEST_CASE_RECEIPT: caseReceipt } : {});
        if (native) receipt.cases.push(...readNativeCaseReceipt(caseReceipt, catalogue.suites.filter(suite => suite.lane === phase).flatMap(suite => suite.files), requiredCases).cases);
        receipt.phases.push({ phase, passed: true });
      }
      const release = await verifiedRelease(join(root, ".runtime/releases/latest")); receipt.release = release.identity;
      const archive = Bun.spawn(["tar", "-cf", join(directory, "candidate-release.tar"), "-C", release.releaseDirectory, "."], { cwd: root, stdout: "inherit", stderr: "inherit" });
      children.add(archive);
      try { if (await archive.exited !== 0) throw new Error("CI_RELEASE_ARCHIVE_FAILED"); } finally { children.delete(archive); }
      await stable(); receipt.passed = true;
    }
    finally { await writeFile(join(directory, "native.json"), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 }); }
  } else {
    const shard = parseBrowserShard(option("--shard") ?? "");
    const receipt: BrowserCiReceipt = { version: 1, kind: "browser", candidate, runtimeFingerprint, verificationFingerprint, passed: false, shard, inventory: [], receipts: [], exclusions: [], isolation: null, cohorts: [] };
    try {
      if (args.includes("--build")) await execute(["run", "deploy:build"]);
      if (!process.env.ERPX_TEST_RELEASE_DIR && !args.includes("--build")) throw new Error("CI_ATTESTED_RELEASE_DIRECTORY_REQUIRED");
      const { releaseDirectory, identity } = await verifiedRelease(process.env.ERPX_TEST_RELEASE_DIR ?? join(root, ".runtime/releases/latest"));
      receipt.release = identity;
      const groups = partitionBrowserIsolationSuites(browserSuites);
      const productHostPlanPaths = productHostPlanPathsFromEnvironment(process.env);
      const formsHostPlanPath = process.env.ERPX_TEST_FORMS_HOST_PLAN;
      const assignedRequiredSuites = new Set<string>();
      for (const test of requiredCases.filter(test => test.runner === "playwright" && test.project)) {
        const owners = browserSuites.filter(suite => suite.files.includes(test.file));
        if (owners.length !== 1) throw new Error(`CI_BROWSER_REQUIRED_CASE_SUITE_BINDING_REQUIRED:${test.file}`);
        if (assignedBrowserShard(owners[0].id, { file: test.file, title: test.title, project: test.project! }, shard)) assignedRequiredSuites.add(owners[0].id);
      }
      const forwardGroups: { suites: typeof browserSuites; receipt: BrowserRunReceipt }[] = [];
      const reverseGroups: { suites: typeof browserSuites; receipt: BrowserRunReceipt }[] = [];
      let probe: BrowserRunReceipt | undefined;
      async function readReceipt(evidenceDirectory: string) {
        return JSON.parse(await readFile(join(evidenceDirectory, "receipt.json"), "utf8")) as BrowserRunReceipt;
      }
      for (const group of groups) {
        const hasAssignedRequiredCases = group.suites.some(suite => assignedRequiredSuites.has(suite.id));
        const cohort = { group: group.id, suiteIds: group.suites.map(suite => suite.id), passed: false } as NonNullable<BrowserCiReceipt["cohorts"]>[number];
        receipt.cohorts!.push(cohort);
        try {
          if (group.id === "product-host" && (!productHostPlanPaths.default || !productHostPlanPaths.businessWorkflows?.["work.request-draft"] || !productHostPlanPaths.businessWorkflows?.["finance.sales-invoice"] || !productHostPlanPaths.businessWorkflows?.["legal.invitation-enrollment"])) {
            cohort.reason = "PRODUCT_HOST_SCOPED_PLANS_REQUIRED";
            receipt.blocker = { phase: "product-host", reason: "PRODUCT_HOST_SCOPED_PLANS_REQUIRED" };
            throw new Error("CI_PRODUCT_HOST_SCOPED_PLANS_REQUIRED");
          }
          if (group.id === "forms-host" && hasAssignedRequiredCases && !formsHostPlanPath) {
            cohort.reason = "FORMS_HOST_PLAN_REQUIRED";
            receipt.blocker = { phase: "forms-host", reason: "FORMS_HOST_PLAN_REQUIRED" };
            throw new Error("CI_FORMS_HOST_PLAN_REQUIRED");
          }
          if (hasAssignedRequiredCases) {
            const isolated = await runBrowserIsolation(group.suites, { releaseDirectory, shard, productHostPlanPaths, formsHostPlanPath });
            forwardGroups.push({ suites: group.suites, receipt: isolated.forward.receipt });
            reverseGroups.push({ suites: group.suites, receipt: isolated.reverse.receipt });
            cohort.forward = isolated.forward.receipt;
            cohort.reverse = isolated.reverse.receipt;
            cohort.forwardEvidenceDirectory = relative(root, isolated.forward.result.evidenceDirectory);
            cohort.reverseEvidenceDirectory = relative(root, isolated.reverse.result.evidenceDirectory);
            if (!probe) {
              probe = isolated.probe.receipt;
              cohort.probe = isolated.probe.receipt;
              cohort.probeEvidenceDirectory = relative(root, isolated.probe.evidenceDirectory);
            }
          } else {
            const forwardResult = await runBrowserSuites(group.suites, { releaseDirectory, shard, productHostPlanPaths, formsHostPlanPath, caseOrder: "forward" });
            const forward = await readReceipt(forwardResult.evidenceDirectory);
            assertCohortsDestroyed(forward);
            if (forward.receipts.some(test => test.required !== false)) throw new Error("CI_BROWSER_EMPTY_SHARD_GROUP_CONTRADICTS_REQUIRED_CASES");
            const reverseResult = await runBrowserSuites(group.suites, { releaseDirectory, shard, productHostPlanPaths, formsHostPlanPath, caseOrder: "reverse" });
            const reverse = await readReceipt(reverseResult.evidenceDirectory);
            assertCohortsDestroyed(reverse);
            if (reverse.receipts.some(test => test.required !== false)) throw new Error("CI_BROWSER_EMPTY_SHARD_GROUP_CONTRADICTS_REQUIRED_CASES");
            forwardGroups.push({ suites: group.suites, receipt: forward });
            reverseGroups.push({ suites: group.suites, receipt: reverse });
            cohort.forward = forward;
            cohort.reverse = reverse;
            cohort.forwardEvidenceDirectory = relative(root, forwardResult.evidenceDirectory);
            cohort.reverseEvidenceDirectory = relative(root, reverseResult.evidenceDirectory);
          }
          cohort.passed = true;
        } catch (error) {
          if (!cohort.reason) cohort.reason = error instanceof Error && /^[A-Z0-9_]+$/.test(error.message) ? error.message : "BROWSER_COHORT_FAILED";
          throw error;
        }
      }
      if (!probe) throw new Error("CI_BROWSER_SHARD_EXECUTABLE_PROBE_REQUIRED");
      const forward = mergeBrowserAcceptanceReceipts(browserSuites, forwardGroups, "forward", shard);
      const reverse = mergeBrowserAcceptanceReceipts(browserSuites, reverseGroups, "reverse", shard);
      const proof = { forward, reverse, probe };
      validateBrowserIsolation(proof, { runtimeFingerprint, verificationFingerprint, shard });
      const browser = forward;
      if (!browser.passed || browser.runtimeFingerprint !== runtimeFingerprint || browser.verificationFingerprint !== verificationFingerprint) throw new Error("CI_BROWSER_RECEIPT_MISMATCH");
      receipt.inventory = browser.inventory;
      receipt.receipts = browser.receipts.filter((test): test is BrowserCaseReceipt => test.required !== false && typeof test.file === "string" && typeof test.title === "string" && typeof test.passed === "boolean");
      receipt.exclusions = browser.receipts.filter((test): test is BrowserCaseExclusion => test.required === false && typeof test.file === "string" && typeof test.title === "string" && typeof test.reason === "string");
      receipt.isolation = proof;
      await stable(); receipt.passed = true;
    } finally { await writeFile(join(directory, `browser-${shard.index}-${shard.total}.json`), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 }); }
  }
}
