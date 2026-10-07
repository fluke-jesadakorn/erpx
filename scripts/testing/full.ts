import { mkdir, readFile, realpath, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { loadCatalogue, repositoryRoot, validateCatalogue } from "./catalogue";
import { runBrowserIsolation } from "./browser-isolation";
import { browserRunCaseIdentity } from "./browser-isolation-receipts";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { assertRequiredCaseResults, internalRequiredCases, loadRequiredCases, readNativeCaseReceipt, type CaseObservation } from "./required-case-inventory";
import { loadProviderRehearsals, unselectedProviderRehearsals } from "./provider-rehearsals";
import { writeJsonOutput } from "./json-output";

const root = repositoryRoot, catalogue = loadCatalogue();
validateCatalogue(catalogue);
const providerRehearsals = unselectedProviderRehearsals(loadProviderRehearsals(root, catalogue.modules));
for (const argument of process.argv.slice(2)) if (!["--list", "--json", "--"].includes(argument)) throw new Error(`UNKNOWN_FULL_ARGUMENT:${argument}`);
if (process.argv.slice(2).includes("--list")) {
  writeJsonOutput({ scope: "internal-regression", suites: catalogue.suites.filter(suite => suite.lane !== "provider"), requiredCases: internalRequiredCases(loadRequiredCases(root), catalogue.suites), browserIsolation: { caseOrders: ["forward", "reverse"], declaredFailureRestart: true, zeroRetries: true }, excludedProviderSuites: catalogue.suites.filter(suite => suite.lane === "provider").map(suite => suite.id), providerRehearsals });
} else {
  const directory = join(root, ".scratch/dev/test-suites/full", new Date().toISOString().replace(/[:.]/g, "-"));
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const runtimeFingerprint = await runtimeSourceFingerprint(root), verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  const phases: { phase: string; passed: boolean }[] = [];
  const requiredCases = internalRequiredCases(loadRequiredCases(root), catalogue.suites), cases: CaseObservation[] = [];
  let passed = false;
  async function stable() {
    if (await runtimeSourceFingerprint(root) !== runtimeFingerprint || await verificationSourceFingerprint(root, runtimeFingerprint) !== verificationFingerprint) throw new Error("FULL_REGRESSION_SOURCE_CHANGED");
  }
  async function phase(name: string, args: string[]) {
    await stable(); console.log(`Full regression: ${name}`);
    const native = name === "unit" || name === "integration";
    const caseReceipt = join(directory, `${name}-cases.json`);
    const child = Bun.spawn([process.execPath, ...args], { cwd: root, env: { ...process.env, ...(native ? { ERPX_TEST_CASE_RECEIPT: caseReceipt } : {}) }, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
    const onInt = () => child.kill("SIGINT"), onTerm = () => child.kill("SIGTERM");
    process.on("SIGINT", onInt); process.on("SIGTERM", onTerm);
    let code: number;
    try { code = await child.exited; }
    finally { process.off("SIGINT", onInt); process.off("SIGTERM", onTerm); }
    phases.push({ phase: name, passed: code === 0 });
    if (code !== 0) throw new Error(`FULL_REGRESSION_PHASE_FAILED:${name}`);
    if (native) cases.push(...readNativeCaseReceipt(caseReceipt, catalogue.suites.filter(suite => suite.lane === name).flatMap(suite => suite.files), requiredCases).cases);
    await stable();
  }
  try {
    await phase("architecture", ["scripts/architecture/acceptance.ts"]);
    await phase("unit", ["scripts/local/test.ts", "unit"]);
    await phase("integration", ["scripts/local/test.ts", "integration"]);
    await phase("lint", ["run", "lint"]);
    await phase("typecheck", ["run", "typecheck"]);
    await phase("isolated-release", ["run", "deploy:build"]);
    const releaseDirectory = await realpath(join(root, ".runtime/releases/latest"));
    const release = JSON.parse(await readFile(join(releaseDirectory, "release.json"), "utf8"));
    if (release.sourceFingerprint !== runtimeFingerprint) throw new Error("FULL_REGRESSION_CANDIDATE_MISMATCH");
    const browserSuites = catalogue.suites.filter(suite => ["e2e", "journeys", "uat"].includes(suite.lane));
    await stable();
    const isolation = await runBrowserIsolation(browserSuites, { releaseDirectory, onEvidence: async (stage, evidenceDirectory) => {
      await writeFile(join(directory, `browser-${stage}.json`), JSON.stringify({ evidenceDirectory }, null, 2) + "\n", { mode: 0o600 });
      await stable();
    } });
    await writeFile(join(directory, "browser-isolation.json"), JSON.stringify({ acceptance: isolation.acceptance, proof: isolation.proof }, null, 2) + "\n", { mode: 0o600 });
    for (const lane of ["e2e", "journeys", "uat"] as const) {
      const suites = browserSuites.filter(suite => suite.lane === lane);
      if (!suites.length) throw new Error(`FULL_REGRESSION_MISSING_LANE:${lane}`);
      const ids = new Set(suites.map(suite => suite.id));
      const browser = { receipts: isolation.forward.receipt.receipts.filter(test => ids.has(test.suite)) };
      const observed = browser.receipts.map(test => ({ runner: "playwright" as const, ...browserRunCaseIdentity(test), status: test.passed ? "passed" : "skipped", required: test.required }));
      assertRequiredCaseResults(requiredCases, observed, { runner: "playwright", files: suites.flatMap(suite => suite.files), projects: ["desktop", "mobile"] });
      cases.push(...observed);
      await writeFile(join(directory, `${lane}.json`), JSON.stringify({ evidenceDirectory: isolation.forward.result.evidenceDirectory, cases: observed.filter(test => test.required !== false).length, runtimeFingerprint, verificationFingerprint, isolationEvidence: join(directory, "browser-isolation.json") }, null, 2) + "\n", { mode: 0o600 });
      phases.push({ phase: lane, passed: true }); await stable();
    }
    phases.push({ phase: "browser-isolation", passed: true });
    passed = true;
  } finally {
    const receipt = { version: 1, scope: "internal-regression", passed, runtimeFingerprint, verificationFingerprint,
      commit: Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root }).stdout.toString().trim(), phases, cases, providerRehearsals,
      providerSuites: catalogue.suites.filter(suite => suite.lane === "provider").map(suite => ({ suite: suite.id, selected: false, reason: "Separate opt-in provider verification; this receipt makes no live-provider acceptance claim." })) };
    await writeFile(join(directory, "receipt.json"), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 });
    console.log(JSON.stringify({ passed, receipt: join(directory, "receipt.json") }));
  }
}
