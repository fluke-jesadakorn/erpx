import { createHash } from "node:crypto";
import { mkdir, readFile, realpath, writeFile } from "node:fs/promises";
import { basename, join, relative } from "node:path";
import { loadCatalogue, repositoryRoot, validateCatalogue } from "./catalogue";
import { runBrowserIsolation } from "./browser-isolation";
import { browserRunCaseIdentity } from "./browser-isolation-receipts";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { assertRequiredCaseResults, internalRequiredCases, loadRequiredCases, readNativeCaseReceipt } from "./required-case-inventory";
import { loadProviderRehearsals, unselectedProviderRehearsals } from "./provider-rehearsals";
import { type FullRegressionReceipt } from "./full-regression-receipt";
import { vitestObservations, syntheticRollbackEvidenceValid } from "./engineering/engineering-test-results";
import { writeJsonOutput } from "./json-output";

const root = repositoryRoot, catalogue = loadCatalogue();
validateCatalogue(catalogue);
const providerRehearsals = unselectedProviderRehearsals(loadProviderRehearsals(root, catalogue.modules));
for (const argument of process.argv.slice(2)) if (!["--list", "--json", "--"].includes(argument)) throw new Error(`UNKNOWN_FULL_ARGUMENT:${argument}`);
if (process.argv.slice(2).includes("--list")) {
  writeJsonOutput({ scope: "internal-regression", suites: catalogue.suites.filter(suite => suite.lane !== "provider"), requiredCases: internalRequiredCases(loadRequiredCases(root), catalogue.suites), browserIsolation: { caseOrders: ["forward", "reverse"], declaredFailureRestart: true, zeroRetries: true }, excludedProviderSuites: catalogue.suites.filter(suite => suite.lane === "provider").map(suite => suite.id), providerRehearsals });
} else {
  const runId = new Date().toISOString().replace(/[:.]/g, "-");
  const fullRoot = join(root, ".scratch/dev/test-suites/full"), directory = join(fullRoot, runId);
  await mkdir(fullRoot, { recursive: true, mode: 0o700 });
  await mkdir(directory, { mode: 0o700 });
  const candidate = Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root }).stdout.toString().trim();
  const runtimeFingerprint = await runtimeSourceFingerprint(root), verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  const phases: { phase: string; passed: boolean }[] = [];
  const requiredCases = internalRequiredCases(loadRequiredCases(root), catalogue.suites), cases: FullRegressionReceipt["cases"] = [];
  let release: FullRegressionReceipt["release"] = null;
  let rollback: FullRegressionReceipt["rollback"] = null;
  let passed = false;
  async function stable() {
    if (Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root }).stdout.toString().trim() !== candidate) throw new Error("FULL_REGRESSION_CANDIDATE_CHANGED");
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
    if (native) cases.push(...readNativeCaseReceipt(caseReceipt, catalogue.suites.filter(suite => suite.lane === name).flatMap(suite => suite.files), requiredCases).cases.map(test => {
      const owners = catalogue.suites.filter(suite => suite.lane === name && suite.files.includes(test.file));
      if (owners.length !== 1) throw new Error(`FULL_REGRESSION_CASE_SUITE_BINDING_REQUIRED:${name}:${test.file}`);
      return { ...test, suite: owners[0].id };
    }));
    await stable();
  }
  async function rollbackPhase(releaseDirectory:string) {
    await stable(); console.log("Full regression: rollback");
    const evidenceDirectory=join(directory,"rollback-evidence"),reportPath=join(directory,"rollback-vitest.json");
    await mkdir(evidenceDirectory,{mode:0o700});
    const child=Bun.spawn([process.execPath,"scripts/testing/test.ts","integration","tests/integration/shared.evidence/engineering-recovery.test.ts",
      "--reporter=json","--outputFile="+reportPath],{cwd:root,env:{...process.env,ERPX_ENGINEERING_RELEASE_DIR:releaseDirectory,
        ERPX_ENGINEERING_SOURCE_FINGERPRINT:runtimeFingerprint,ERPX_ENGINEERING_EVIDENCE_DIR:evidenceDirectory},
      stdin:"inherit",stdout:"inherit",stderr:"inherit"});
    const onInt=()=>child.kill("SIGINT"),onTerm=()=>child.kill("SIGTERM");
    process.on("SIGINT",onInt);process.on("SIGTERM",onTerm);
    let code:number;
    try { code=await child.exited; }
    finally { process.off("SIGINT",onInt);process.off("SIGTERM",onTerm); }
    if(code!==0)throw new Error("FULL_REGRESSION_PHASE_FAILED:rollback");
    const reportBytes=await readFile(reportPath),report=JSON.parse(reportBytes.toString());
    const observations=vitestObservations(root,report),target=observations.filter(test=>test.file==="tests/integration/shared.evidence/engineering-recovery.test.ts"&&
      test.title==="encrypts a complete isolated snapshot, restores immutable money and files, and rolls back only compatible releases");
    if(report.success!==true||observations.some(test=>test.status==="failed")||target.length!==1||target[0].status!=="passed")
      throw new Error("FULL_REGRESSION_EXACT_SYNTHETIC_ROLLBACK_CASE_REQUIRED");
    const evidenceBytes=await readFile(join(evidenceDirectory,"encrypted-recovery.json")),evidence=JSON.parse(evidenceBytes.toString());
    const releaseIdentity=release;
    if(!releaseIdentity||!syntheticRollbackEvidenceValid(evidence,runtimeFingerprint,releaseIdentity.fileManifestSha256))
      throw new Error("FULL_REGRESSION_SYNTHETIC_ROLLBACK_EVIDENCE_REQUIRED");
    const proof=evidence.rollback;
    rollback={scope:"isolated-synthetic-prior-release",candidateReleaseId:releaseIdentity.id,
      fixtureCandidateReleaseId:evidence.candidateReleaseId,previousReleaseId:proof.previousReleaseId,
      fileManifestSha256:releaseIdentity.fileManifestSha256,candidateArtifactSha256:proof.candidateArtifactSha256,
      previousArtifactSha256:proof.previousArtifactSha256,candidateServerSha256:proof.candidateServerSha256,
      previousServerSha256:proof.previousServerSha256,evidencePath:relative(root,join(evidenceDirectory,"encrypted-recovery.json")),
      evidenceSha256:createHash("sha256").update(evidenceBytes).digest("hex"),testReportPath:relative(root,reportPath),
      testReportSha256:createHash("sha256").update(reportBytes).digest("hex"),switchedToPrevious:true,databaseAndFilesUnchanged:true,
      databaseStateBeforeSha256:proof.databaseStateBeforeSha256,databaseStateAfterSha256:proof.databaseStateAfterSha256,
      privateFilesStateBeforeSha256:proof.privateFilesStateBeforeSha256,privateFilesStateAfterSha256:proof.privateFilesStateAfterSha256};
    phases.push({phase:"rollback",passed:true});
    await stable();
  }
  try {
    await phase("architecture", ["scripts/architecture/acceptance.ts"]);
    await phase("unit", ["scripts/testing/test.ts", "unit"]);
    await phase("integration", ["scripts/testing/test.ts", "integration"]);
    await phase("lint", ["run", "lint"]);
    await phase("typecheck", ["run", "typecheck"]);
    await phase("isolated-release", ["run", "deploy:build"]);
    const releaseDirectory = await realpath(join(root, ".runtime/releases/latest"));
    const releaseManifest = JSON.parse(await readFile(join(releaseDirectory, "release.json"), "utf8"));
    const releaseManifestBytes = await readFile(join(releaseDirectory, "release-files.json"));
    const releaseManifestSha256 = createHash("sha256").update(releaseManifestBytes).digest("hex");
    if (releaseManifest.sourceFingerprint !== runtimeFingerprint || releaseManifest.fileManifestSha256 !== releaseManifestSha256 ||
      typeof releaseManifest.id !== "string" || basename(releaseDirectory) !== releaseManifest.id) throw new Error("FULL_REGRESSION_CANDIDATE_MISMATCH");
    release = { id: releaseManifest.id, fileManifestSha256: releaseManifestSha256 };
    await rollbackPhase(releaseDirectory);
    const browserSuites = catalogue.suites.filter(suite => ["e2e", "journeys", "uat"].includes(suite.lane));
    await stable();
    const isolation = await runBrowserIsolation(browserSuites, { releaseDirectory, port: 3104, onEvidence: async (stage, evidenceDirectory) => {
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
    const receipt: FullRegressionReceipt = { version: 1, kind: "full-regression", scope: "internal-regression", runId, commit: candidate, passed, candidate,
      runtimeFingerprint, verificationFingerprint, release, rollback, phases, cases,
      providerRehearsals,
      providerSuites: catalogue.suites.filter(suite => suite.lane === "provider").map(suite => ({ suite: suite.id, selected: false, reason: "Separate opt-in provider verification; this receipt makes no live-provider acceptance claim." })) };
    await writeFile(join(directory, "receipt.json"), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 });
    console.log(JSON.stringify({ passed, receipt: join(directory, "receipt.json") }));
  }
}
