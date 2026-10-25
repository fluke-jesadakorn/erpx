import { chmod, copyFile, mkdir, readFile, realpath, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { randomBytes } from "node:crypto";
import { spawn, type ChildProcess } from "node:child_process";
import { createServer } from "node:net";
import { loadCatalogue, repositoryRoot, selectSuites, type Suite } from "./catalogue";
import { assertBrowserCasePassed, browserCaseProjectRequirement, browserCases, exactTitlePattern, type BrowserCase } from "./browser-results";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { requiredUatAuthProfiles } from "./uat-auth";
import { prepareBrowserCasePrerequisites } from "./browser-prerequisites";
import { localBrowserActorEnvironment } from "./browser-fixture-metadata";
import { prepareUatCaseEvidence, retainBrowserFixtureEvidence } from "./browser-evidence";
import { browserChildEnvironment } from "./runner-environment";
import { assignedBrowserShard, type BrowserShard, type BrowserInventoryCase, type BrowserCaseIdentity } from "./browser-sharding";
import { assertBrowserFaultProbeSelection, browserFaultProbeError, isBrowserFaultProbeCase, orderedBrowserExecutionCases, orderedBrowserExecutionGroups, parseBrowserCaseOrder, selectBrowserCaseOrder, triggerBrowserFaultProbe, type BrowserCaseOrder } from "./browser-execution-plan";
import { assertRequiredCaseInventory, canonicalCaseFile, loadRequiredCases } from "./required-case-inventory";
import { writeJsonOutput } from "./json-output";

const root = repositoryRoot;
const node = resolve(process.env.ERPX_NODE_BIN ?? join(root, ".runtime/node/node-v24.20.0-darwin-arm64/bin/node"));
const active = new Set<ChildProcess>();
let interrupted: string | undefined;
const wait = (ms: number) => new Promise<void>(done => setTimeout(done, ms));

async function freePort(port: number) {
  const server = createServer();
  await new Promise<void>((done, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", () => server.close(error => error ? reject(error) : done())); });
}

async function execute(args: string[], env: Record<string, string | undefined>, log: string) {
  if (interrupted) throw new Error(`BROWSER_RUN_INTERRUPTED:${interrupted}`);
  const cohort = Boolean(env.ERPX_BROWSER_SIMULATION_RUN_ID || env.ERPX_BROWSER_SIMULATION_RUN || env.ERPX_DEMO_RUN);
  const environment = cohort ? browserChildEnvironment(process.env, { ...env, ERPX_NODE_BIN: node }) : { ...process.env, ...env, ERPX_NODE_BIN: node };
  const bun = args[0] === process.execPath || args[0] === process.env.ERPX_BUN_BIN;
  const command = cohort && bun ? [args[0], "--env-file=/dev/null", ...args.slice(1)] : args;
  const handle = await import("node:fs/promises").then(fs => fs.open(log, "w", 0o600));
  const child = spawn(command[0], command.slice(1), { cwd: root, env: environment, stdio: ["ignore", handle.fd, handle.fd] });
  active.add(child);
  try {
    const code = await new Promise<number | null>((done, reject) => { child.once("error", reject); child.once("exit", done); });
    if (code !== 0) throw new Error(`BROWSER_COMMAND_FAILED:${code}:${log}`);
  } finally { active.delete(child); await handle.close(); }
}


export type BrowserRunCaseReceipt = { suite: string; file?: string; project: "desktop" | "mobile"; title?: string; run?: string; report?: string; passed?: boolean; required?: false; reason?: string; faultProbe?: true; failure?: string; cleanup?: { completed: boolean; ownedDirectoryAbsent: boolean } };
export type BrowserRunReceipt = { version: 1; passed: boolean; runtimeFingerprint: string; verificationFingerprint: string; shard: BrowserShard | null; interrupted: string | null; suites: string[]; cleanupFailures: string[]; inventory: BrowserInventoryCase[]; receipts: BrowserRunCaseReceipt[]; execution: { caseOrder: BrowserCaseOrder; mode: "acceptance" | "fault-probe"; faultProbe?: BrowserCaseIdentity } };
export class BrowserFaultProbeError extends Error {
  constructor(readonly evidenceDirectory: string) { super(browserFaultProbeError); this.name = "BrowserFaultProbeError"; }
}

export async function runBrowserSuites(suites: Suite[], options: { projects?: ("desktop" | "mobile")[]; grep?: string; headed?: boolean; releaseDirectory?: string; shard?: BrowserShard; caseOrder?: BrowserCaseOrder; faultProbe?: BrowserCaseIdentity; investorCapture?: boolean; port?: 3102 | 3104; caseSelection?: { suite: string; title: string; project: "desktop" | "mobile" }[]; onInvestorEvidence?: (directory: string) => void } = {}) {
  if (!suites.length || suites.some(suite => !["e2e", "journeys", "uat"].includes(suite.lane))) throw new Error("BROWSER_SUITES_REQUIRED");
  if (options.shard && (options.grep || options.projects)) throw new Error("SHARDED_BROWSER_FULL_INVENTORY_REQUIRED");
  const caseOrder = parseBrowserCaseOrder(options.caseOrder ?? "forward");
  const selection = options.caseSelection;
  if (selection && (!selection.length || new Set(selection.map(item => JSON.stringify(item))).size !== selection.length || selection.some(item => !suites.some(suite => suite.id === item.suite) || !["desktop", "mobile"].includes(item.project)))) throw new Error("INVALID_BROWSER_CASE_SELECTION");
  if (selection && (options.shard || options.grep || options.faultProbe)) throw new Error("EXPLICIT_BROWSER_SELECTION_CONFLICT");
  const groups = orderedBrowserExecutionGroups(suites, options.projects ?? ["desktop", "mobile"], caseOrder).filter(group => !selection || selection.some(item => item.suite === group.suite.id && item.project === group.project));
  assertBrowserFaultProbeSelection(options.faultProbe, groups);
  if (options.faultProbe && (options.shard || options.grep)) throw new Error("EXACT_BROWSER_FAULT_PROBE_SELECTION_REQUIRED");
  const execution: BrowserRunReceipt["execution"] = { caseOrder, mode: options.faultProbe ? "fault-probe" : "acceptance", ...(options.faultProbe ? { faultProbe: options.faultProbe } : {}) };
  if (Bun.version !== "1.2.17" || Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v24.20.0") throw new Error("PINNED_TOOLCHAIN_REQUIRED");
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const evidence = join(root, ".scratch/dev/test-suites/browser", stamp);
  await mkdir(evidence, { recursive: true, mode: 0o700 });
  options.onInvestorEvidence?.(evidence);
  const runtimeFingerprint = await runtimeSourceFingerprint(root), verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  const receipts: BrowserRunCaseReceipt[] = [];
  const inventory: BrowserInventoryCase[] = [];
  const registeredCases = loadRequiredCases(root);
  const templates = new Map<string, { run: string; env: Record<string, string>; output: string }>();
  const cleanupErrors: unknown[] = [];
  let releaseDirectory = options.releaseDirectory;
  let completed = false;
  const signal = (name: string) => { interrupted = name; for (const child of active) child.kill("SIGTERM"); };
  const onInt = () => signal("SIGINT"), onTerm = () => signal("SIGTERM");
  process.on("SIGINT", onInt); process.on("SIGTERM", onTerm);
  try {
    if (suites.some(suite => suite.lane === "uat") && !releaseDirectory) {
      await execute([process.execPath, "run", "deploy:build"], {}, join(evidence, "candidate-build.log"));
      releaseDirectory = await realpath(join(root, ".runtime/releases/latest"));
      const release = JSON.parse(await readFile(join(releaseDirectory, "release.json"), "utf8"));
      if (release.sourceFingerprint !== runtimeFingerprint) throw new Error("BROWSER_COMPILED_CANDIDATE_MISMATCH");
    }
    for (const { suite, project } of groups) {
      // Collection has its own cohort because legacy tests read fixture files at
      // module load time. Every executed case gets an entirely fresh cohort.
      const required = await cohort(suite, project, undefined);
      if (!required.length) throw new Error(`EMPTY_BROWSER_SUITE:${suite.id}:${project}`);
      assertRequiredCaseInventory(registeredCases, required.map(test => ({ ...test, runner: "playwright", ...browserCaseProjectRequirement(test) })), {
        runner: "playwright", files: suite.files, projects: [project], titleMatches: test => !options.grep || new RegExp(options.grep).test(`${project} ${required.find(actual => canonicalCaseFile(actual.file, suite.files) === test.file)?.fileTitle ?? test.file.replace(/^tests\/e2e\//, "")} ${test.title}`),
      });
      if (options.faultProbe) {
        const selected = required.filter(test => isBrowserFaultProbeCase(suite.id, test, options.faultProbe));
        if (selected.length !== 1 || !browserCaseProjectRequirement(selected[0]).required || selected[0].expectedStatus !== "passed") throw new Error("REQUIRED_BROWSER_FAULT_PROBE_CASE_NOT_FOUND");
      }
      const selected = selection?.filter(item => item.suite === suite.id && item.project === project);
      if (selected?.some(item => required.filter(test => test.title === item.title).length !== 1)) throw new Error("EXACT_BROWSER_SELECTED_CASE_REQUIRED");
      for (const test of orderedBrowserExecutionCases(required, caseOrder)) {
        if (selected && !selected.some(item => item.title === test.title)) continue;
        const requirement = browserCaseProjectRequirement(test);
        inventory.push({ suite: suite.id, file: test.file, project, title: test.title, ...requirement });
        if (options.faultProbe && !isBrowserFaultProbeCase(suite.id, test, options.faultProbe)) continue;
        if (options.shard && !assignedBrowserShard(suite.id, test, options.shard)) continue;
        if (requirement.required) {
          if (test.expectedStatus !== "passed") throw new Error(`REQUIRED_BROWSER_CASE_NOT_EXECUTABLE:${suite.id}:${project}:${test.title}`);
          await cohort(suite, project, test);
        }
        else receipts.push({ suite: suite.id, file: test.file, project, title: test.title, required: false, reason: requirement.reason });
      }
    }
    if (interrupted) throw new Error(`BROWSER_RUN_INTERRUPTED:${interrupted}`);
    completed = true;
    return { evidenceDirectory: evidence, cases: receipts.filter(receipt => "passed" in receipt && receipt.passed === true).length, runtimeFingerprint, verificationFingerprint, execution };
  } finally {
    process.off("SIGINT", onInt); process.off("SIGTERM", onTerm);
    const wasInterrupted = interrupted; interrupted = undefined;
    try {
      for (const template of templates.values()) {
        try { await cleanup(template.run, template.env, template.output); }
        catch (error) { cleanupErrors.push(error); }
      }
    } finally { interrupted = wasInterrupted; }
    const receipt: BrowserRunReceipt = { version: 1, passed: completed && !interrupted && !cleanupErrors.length, runtimeFingerprint, verificationFingerprint, shard: options.shard ?? null, interrupted: interrupted ?? null, suites: suites.map(suite => suite.id), cleanupFailures: cleanupErrors.map(String), inventory, receipts, execution };
    await writeFile(join(evidence, "receipt.json"), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 });
    if (cleanupErrors.length && completed) throw new AggregateError(cleanupErrors, "BROWSER_COHORT_CLEANUP_FAILED");
    if (interrupted && completed) throw new Error(`BROWSER_RUN_INTERRUPTED:${interrupted}`);
  }

  async function cleanup(run: string, env: Record<string, string>, output: string) {
    if (!existsSync(join(root, ".runtime/browser", run, "state.json"))) return;
    let evidenceFailure: unknown;
    try {
      await retainBrowserFixtureEvidence(root, run, output);
      for (const name of ["server.log", "jobs.log", "state.json", "attachment-baseline.json", "attachment-baseline-error.log", "jobs-enrollment.json", "jobs-pause.json", "jobs-resume.json"]) {
        const path = join(root, ".runtime/browser", run, name);
        if (existsSync(path)) { await copyFile(path, join(output, name)); await chmod(join(output, name), 0o600); }
      }
    }
    catch (error) { evidenceFailure = error; }
    for (let attempt = 0; attempt < 20 && existsSync(join(root, ".runtime/browser", run)); attempt++) {
      try { await execute([process.execPath, "scripts/local/run-server-script.ts", "scripts/local/browser-simulation.ts", "destroy", run], env, join(output, `cleanup-${attempt}.log`)); }
      catch (error) { if (attempt === 19) throw error; }
      if (existsSync(join(root, ".runtime/browser", run))) await wait(250);
    }
    if (existsSync(join(root, ".runtime/browser", run))) throw new Error("BROWSER_COHORT_CLEANUP_INCOMPLETE");
    if (evidenceFailure) throw new Error("BROWSER_FIXTURE_EVIDENCE_RETENTION_FAILED", { cause: evidenceFailure });
  }

  async function cohort(suite: Suite, project: "desktop" | "mobile", wanted?: BrowserCase): Promise<BrowserCase[]> {
    const run = randomBytes(6).toString("hex"), port = options.port ?? (suite.lane === "uat" || suite.fixture === "legal" || suite.fictionalProvider ? 3104 : 3102);
    const origin = `http://127.0.0.1:${port}`, output = join(evidence, run);
    await freePort(port); await mkdir(output, { mode: 0o700 });
    if (suite.fictionalProvider && (port !== 3104 || !releaseDirectory)) throw new Error('FICTIONAL_PROVIDER_COMPILED_3104_REQUIRED');
    const verifyRelease = (suite.lane === "uat" || Boolean(suite.fictionalProvider)) && Boolean(releaseDirectory);
    // Global health requires the real worker, scanner and reconciled storage.
    // Capability-specific requirements remain explicit in the catalogue.
    const requirements = { jobs: true, scanner: true };
    const env = {
      ERPX_BROWSER_SIMULATION_RUN_ID: run, ERPX_BROWSER_SIMULATION_PORT: String(port), ERPX_BROWSER_SIMULATION_PROFILE: suite.fixture,
      ERPX_BROWSER_SIMULATION_ATTACHMENTS: requirements.scanner ? "local" : "none", ERPX_BROWSER_SIMULATION_JOBS: requirements.jobs ? "1" : "0",
      ERPX_BROWSER_SIMULATION_LEGAL: suite.fixture === "legal" || suite.fixture === "uat" ? "1" : "0",
      ERPX_BROWSER_SIMULATION_MENU_IMAGES: options.investorCapture ? "1" : "0",
      ...(suite.fictionalProvider ? { ERPX_PMS_PROVIDER_FIXTURE: "1", ERPX_ENVIRONMENT: "local", ERPX_PUBLIC_ORIGIN: origin, ERPX_DATA_DIR: join(root, '.runtime/browser', run), POS_STRIPE_TEST_SECRET_KEY: 'sk_test_erpx_fictional_provider', POS_STRIPE_TEST_WEBHOOK_SECRET: 'whsec_erpx_fictional_provider' } : {}),
    };
    const simulation = (mode: string) => [process.execPath, "scripts/local/run-server-script.ts", "scripts/local/browser-simulation.ts", mode, run];
    const report = join(output, wanted ? "result.json" : "collection.json");
    let failed = false;
    let caseReceipt: BrowserRunCaseReceipt | undefined;
    let faultTriggered = false;
    try {
      if (await runtimeSourceFingerprint(root) !== runtimeFingerprint || await verificationSourceFingerprint(root, runtimeFingerprint) !== verificationFingerprint) throw new Error("BROWSER_SOURCE_CHANGED");
      const templateKey = JSON.stringify([suite.fixture, port, requirements, suite.fictionalProvider ?? null]);
      let template = templates.get(templateKey);
      if (!template) {
        const templateRun = randomBytes(6).toString("hex"), templateOutput = join(evidence, `template-${templateRun}`);
        await mkdir(templateOutput, { mode: 0o700 });
        template = { run: templateRun, env: { ...env, ERPX_BROWSER_SIMULATION_RUN_ID: templateRun, ...(suite.fictionalProvider ? { ERPX_DATA_DIR: join(root, '.runtime/browser', templateRun) } : {}) }, output: templateOutput };
        templates.set(templateKey, template);
        await execute([process.execPath, "scripts/local/run-server-script.ts", "scripts/local/browser-simulation.ts", "prepare", templateRun], template.env, join(templateOutput, "prepare.log"));
      }
      await execute(simulation("prepare"), { ...env, ERPX_BROWSER_SIMULATION_TEMPLATE_RUN: template.run }, join(output, "prepare.log"));
      const state = JSON.parse(await readFile(join(root, ".runtime/browser", run, "state.json"), "utf8"));
      if (state.sourceFingerprint !== runtimeFingerprint || state.origin !== origin || state.run !== run || !state.ready) throw new Error("BROWSER_COHORT_IDENTITY_CHANGED");
      const testEnv = { ...env, ERPX_INVESTOR_CAPTURE: options.investorCapture ? "1" : undefined, ...localBrowserActorEnvironment(suite.fixture, state.fixture), ERPX_DEMO_RUN: run, ERPX_BROWSER_SIMULATION_RUN: run, ERPX_E2E_BASE_URL: origin,
        ERPX_E2E_FIXTURE: join(root, ".runtime/browser", run, "fixture.json"), ERPX_SERVICES_FIXTURE: join(root, ".runtime/browser", run, "fixture.json"),
        ERPX_UAT_AUTH_DIR: suite.lane === "uat" ? join(root, ".runtime/browser", run, "uat-auth") : "",
        ERPX_UAT_EVIDENCE_FILE: suite.lane === "uat" && wanted ? await prepareUatCaseEvidence(root, run, output) : undefined,
        ERPX_REVIEW_EMAIL: suite.fixture === "local-seed" || suite.fixture === "staff-work" ? "jesadakorn.kirtnu@gmail.com" : undefined,
        ERPX_REVIEW_PASSWORD: suite.fixture === "local-seed" || suite.fixture === "staff-work" ? "12345678" : undefined,
        ERPX_E2E_NO_ROLE_EMAIL: state.fixture?.noRole?.email,
        ERPX_E2E_NO_ROLE_PASSWORD: state.fixture?.noRole?.password,
        ERPX_BOOKING_E2E: suite.fixture === "booking" ? "1" : "0",
        ERPX_TEST_BROWSER_FILES: JSON.stringify(suite.files), PLAYWRIGHT_JSON_OUTPUT_NAME: report };
      if (wanted) {
        await execute(simulation("start"), { ...env, ERPX_BROWSER_SIMULATION_RELEASE_DIR: verifyRelease ? releaseDirectory : undefined }, join(output, "start.log"));
        const started = JSON.parse(await readFile(join(root, ".runtime/browser", run, "state.json"), "utf8"));
        if (!Number.isInteger(started.pid) || started.pid < 1) throw new Error("BROWSER_COHORT_PROCESS_REQUIRED");
        await execute([node, "scripts/testing/browser-readiness.mts", origin, String(started.pid)], env, join(output, "readiness.log"));
        Object.assign(testEnv, await prepareBrowserCasePrerequisites({ suite, wanted, run, directory: join(root, ".runtime/browser", run), releaseDirectory, evidenceDirectory: output, environment: testEnv, execute }));
        await execute([node, "scripts/testing/browser-readiness.mts", origin, String(started.pid)], env, join(output, "prerequisites-readiness.log"));
        if (suite.lane === "uat") {
          const profiles = requiredUatAuthProfiles(suite, wanted);
          if (profiles.length) await execute([process.execPath, "scripts/local/prepare-uat-auth.ts"], { ...testEnv, ERPX_UAT_AUTH_PROFILES: profiles.join(",") }, join(output, "auth.log"));
        }
      }
      if (wanted && isBrowserFaultProbeCase(suite.id, wanted, options.faultProbe)) {
        faultTriggered = true;
        triggerBrowserFaultProbe(suite.id, wanted, options.faultProbe);
      }
      const args = [node, "node_modules/@playwright/test/cli.js", "test", ...suite.files, `--project=${project}`, "--workers=1", "--retries=0", "--reporter=json", `--output=${join(output, "artifacts")}`];
      if (!wanted) args.push("--list");
      if (wanted) {
        if (!wanted.fileTitle) throw new Error("EXACT_BROWSER_FILE_TITLE_REQUIRED");
        args.push("--grep", exactTitlePattern(wanted.title, wanted.project, wanted.fileTitle));
      }
      else if (options.grep) args.push("--grep", options.grep);
      if (wanted && options.headed) args.push("--headed");
      await execute(args, testEnv, join(output, "playwright.log"));
      await chmod(report, 0o600);
      const result = JSON.parse(await readFile(report, "utf8"));
      if (wanted) {
        assertBrowserCasePassed(result, wanted);
        if (await runtimeSourceFingerprint(root) !== runtimeFingerprint || await verificationSourceFingerprint(root, runtimeFingerprint) !== verificationFingerprint) throw new Error("BROWSER_SOURCE_CHANGED");
        caseReceipt = { suite: suite.id, file: wanted.file, project, title: wanted.title, run, report, passed: true };
        receipts.push(caseReceipt);
        console.log(JSON.stringify({ suite: suite.id, project, title: wanted.title, passed: true }));
        return [];
      }
      return browserCases(result);
    } catch (error) {
      failed = true;
      caseReceipt = { suite: suite.id, file: wanted?.file, project, title: wanted?.title, run, report, passed: false, ...(faultTriggered ? { faultProbe: true, failure: browserFaultProbeError } : {}) };
      receipts.push(caseReceipt);
      if (faultTriggered && error instanceof Error && error.message === browserFaultProbeError) throw new BrowserFaultProbeError(evidence);
      throw error;
    }
    finally {
      // A preparation failure can leave owned databases; the known run identity
      // permits exact cleanup even before the launcher emitted its final JSON.
      if (existsSync(join(root, ".runtime/browser", run, "state.json"))) {
        const wasInterrupted = interrupted; interrupted = undefined;
        try {
          await cleanup(run, env, output);
          if (caseReceipt) caseReceipt.cleanup = { completed: true, ownedDirectoryAbsent: !existsSync(join(root, ".runtime/browser", run)) };
        } catch (error) {
          if (caseReceipt) caseReceipt.cleanup = { completed: false, ownedDirectoryAbsent: !existsSync(join(root, ".runtime/browser", run)) };
          cleanupErrors.push(new Error(`BROWSER_COHORT_CLEANUP_FAILED:${run}`, { cause: error }));
          if (!failed) throw error;
          console.error(`Owned browser cohort cleanup failed; inspect ${output}`);
        }
        finally { interrupted = wasInterrupted; }
      }
    }
  }
}

if (import.meta.main) {
  const modules: string[] = [], suiteIds: string[] = [], files: string[] = [], options: Parameters<typeof runBrowserSuites>[1] = {};
  let lane: "e2e" | "journeys" | "uat" | undefined, list = false;
  const args = process.argv.slice(2);
  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    const value = (flag: string) => arg.startsWith(flag + "=") ? arg.slice(flag.length + 1) : args[++index];
    if (["e2e", "journeys", "uat"].includes(arg)) lane = arg as typeof lane;
    else if (arg === "--list") list = true;
    else if (arg === "--headed") options.headed = true;
    else if (arg === "--module" || arg.startsWith("--module=")) modules.push(...value("--module").split(","));
    else if (arg === "--suite" || arg.startsWith("--suite=")) suiteIds.push(...value("--suite").split(","));
    else if (arg === "--project" || arg.startsWith("--project=")) { const project = value("--project"); if (!["desktop", "mobile"].includes(project)) throw new Error("UNKNOWN_BROWSER_PROJECT"); options.projects = [project as "desktop" | "mobile"]; }
    else if (arg === "--grep" || arg.startsWith("--grep=")) options.grep = value("--grep");
    else if (arg === "--case-order" || arg.startsWith("--case-order=")) options.caseOrder = selectBrowserCaseOrder(options.caseOrder, value("--case-order"));
    else if (arg === "--release-dir" || arg.startsWith("--release-dir=")) options.releaseDirectory = resolve(value("--release-dir"));
    else if (arg.startsWith("tests/")) files.push(arg);
    else if (arg !== "--") throw new Error(`UNKNOWN_BROWSER_ARGUMENT:${arg}`);
  }
  const suites = selectSuites(loadCatalogue(), { lane: lane ?? (suiteIds.length ? undefined : "e2e"), modules, suiteIds, files });
  if (list) writeJsonOutput({ lane, suites, execution: { caseOrder: options.caseOrder ?? "forward", mode: "acceptance" } });
  else console.log(JSON.stringify(await runBrowserSuites(suites, options)));
}
