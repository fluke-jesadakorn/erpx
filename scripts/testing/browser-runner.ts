import { chmod, copyFile, mkdir, readFile, realpath, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { createHash, randomBytes } from "node:crypto";
import { spawn, type ChildProcess } from "node:child_process";
import { createServer } from "node:net";
import { loadCatalogue, repositoryRoot, selectSuites, type Suite } from "./catalogue";
import { assertBrowserCasePassed, browserCaseProjectRequirement, browserCases, exactTitlePattern, type BrowserCase } from "./browser-results";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { requiredUatAuthProfiles } from "./uat-auth";
import { prepareBrowserCasePrerequisites } from "./browser-prerequisites";
import { localBrowserActorEnvironment } from "./browser-fixture-metadata";
import { prepareUatCaseEvidence, retainBrowserFixtureEvidence, retainWorkspaceApplicationLogs } from "./browser-evidence";
import { browserChildEnvironment } from "./runner-environment";
import { assignedBrowserShard, type BrowserShard, type BrowserInventoryCase, type BrowserCaseIdentity } from "./browser-sharding";
import { assertBrowserFaultProbeSelection, browserFaultProbeError, isBrowserFaultProbeCase, orderedBrowserExecutionCases, orderedBrowserExecutionGroups, parseBrowserCaseOrder, selectBrowserCaseOrder, triggerBrowserFaultProbe, type BrowserCaseOrder } from "./browser-execution-plan";
import { assertRequiredCaseInventory, canonicalCaseFile, loadRequiredCases } from "./required-case-inventory";
import { writeJsonOutput } from "./json-output";
import {productIds} from "@erpx/contracts/product-registry";
import { assertProductHostPlanWorkflow, productHostBrowserFile, productHostProducts, productHostPlan, productHostBusinessWorkflowForCase, productHostPlanPathForWorkflow, productHostPlanPathsFromEnvironment, productHostReadOnlyWorkflowForCase, startProductHostEdge, type ProductHostBusinessWorkflow, type ProductHostPlanPaths } from "./product-host-cohort";
import { executiveWorkHostBrowserFile, startExecutiveWorkHostEdge } from "./executive-work-host-cohort";
import { formsHostBrowserFile, formsHostCaseTitle, formsHostOrigin, formsHostPlan, startFormsHostEdge } from "./forms-host-cohort";
import {captureUiBrowserEvidence,uiLocalFrontAppForCase,verifyUiBrowserEvidence,type UiBrowserEvidenceWitness} from "./ui-evidence-witness";
import {captureProductHostReceiptBindings} from "./product-host-commercial-evidence";
import { developmentHostProfile, developmentHostOrigin, type DevelopmentHostProfile } from './development-host-profile';
import { startDevelopmentHostProcess } from './development-host-process';
import { preparePdfAssets } from "../assets/prepare-pdf-assets";
import { resolveRegisteredCase, suiteForRegisteredCase } from "./exact-case-selection";

const root = repositoryRoot;
const node = resolve(process.env.ERPX_NODE_BIN ?? join(root, ".runtime/node/node-v26.11.1-darwin-arm64/bin/node"));
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
  if(cohort)for(const key of ["ERPX_PRODUCT_HOST_LEASE","ERPX_PRODUCT_HOST_PLAN","ERPX_PRODUCT_HOST_PLAN_SHA256","ERPX_FORMS_HOST_LEASE","ERPX_FORMS_HOST_PLAN","ERPX_FORMS_HOST_PLAN_SHA256"])if(env[key]===undefined)delete environment[key];
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


export type BrowserRunCaseReceipt = { suite: string; file?: string; project: "desktop" | "mobile"; title?: string; run?: string; report?: string; reportSha256?: string; passed?: boolean; required?: false; reason?: string; faultProbe?: true; failure?: string; productHostLease?:{path:string;sha256:string}; productHostCommercial?:{path:string;sha256:string;bytes:number}; productHostBusiness?:{proof:{path:string;sha256:string;bytes:number};fixtureState:{path:string;sha256:string;bytes:number};screenshot:{path:string;sha256:string;bytes:number}};productHostReadOnly?:{path:string;sha256:string;bytes:number}; executiveWorkHostLease?:{path:string;sha256:string}; formsHostLease?:{path:string;sha256:string}; developmentHost?:{path:string;sha256:string;apps:string[]}; cleanup?: { completed: boolean; ownedDirectoryAbsent: boolean }; uiEvidence?:UiBrowserEvidenceWitness };
export type BrowserRunReceipt = { version: 1; acceptance?: false; passed: boolean; runtimeFingerprint: string; verificationFingerprint: string; shard: BrowserShard | null; interrupted: string | null; suites: string[]; cleanupFailures: string[]; inventory: BrowserInventoryCase[]; receipts: BrowserRunCaseReceipt[]; execution: { caseOrder: BrowserCaseOrder; mode: "acceptance" | "fault-probe" | "development-smoke"; acceptance?: false; faultProbe?: BrowserCaseIdentity }; evidencePath?: string };
export class BrowserFaultProbeError extends Error {
  constructor(readonly evidenceDirectory: string) { super(browserFaultProbeError); this.name = "BrowserFaultProbeError"; }
}

export async function runBrowserSuites(suites: Suite[], options: { mode?: "development-smoke"; workspace?: DevelopmentHostProfile; projects?: ("desktop" | "mobile")[]; grep?: string; headed?: boolean; releaseDirectory?: string; shard?: BrowserShard; caseOrder?: BrowserCaseOrder; faultProbe?: BrowserCaseIdentity; investorCapture?: boolean; port?: 3102 | 3104; caseSelection?: { suite: string; file?: string; title: string; project: "desktop" | "mobile" }[]; onInvestorEvidence?: (directory: string) => void; productHostPlanPath?:string; productHostPlanPaths?:ProductHostPlanPaths; formsHostPlanPath?:string } = {}) {
  if (!suites.length || suites.some(suite => !["e2e", "journeys", "uat"].includes(suite.lane))) throw new Error("BROWSER_SUITES_REQUIRED");
  if (options.shard && (options.grep || options.projects)) throw new Error("SHARDED_BROWSER_FULL_INVENTORY_REQUIRED");
  const development = options.mode === "development-smoke";
  const workspace = options.workspace ? developmentHostProfile(options.workspace) : undefined;
  if(workspace&&!development)throw new Error('WORKSPACE_BROWSER_DEVELOPMENT_MODE_REQUIRED');
  if (development && (!options.caseSelection?.length || options.faultProbe || options.caseOrder && options.caseOrder !== "forward" || options.releaseDirectory || options.projects?.some(project => project !== "desktop"))) throw new Error("DEVELOPMENT_SMOKE_FORWARD_SELECTION_REQUIRED");
  const caseOrder = parseBrowserCaseOrder(options.caseOrder ?? "forward");
  const selection = options.caseSelection;
  const registeredCases = loadRequiredCases(root);
  if (selection && (!selection.length || new Set(selection.map(item => JSON.stringify(item))).size !== selection.length || selection.some(item => !suites.some(suite => suite.id === item.suite) || !["desktop", "mobile"].includes(item.project)))) throw new Error("INVALID_BROWSER_CASE_SELECTION");
  if (selection) for (const item of selection) {
    const suite = suites.find(candidate => candidate.id === item.suite)!;
    const candidates = registeredCases.filter(test => test.runner === "playwright" && test.project === item.project && test.title === item.title &&
      suite.files.includes(canonicalCaseFile(test.file, suite.files)) && (!item.file || canonicalCaseFile(item.file, suite.files) === canonicalCaseFile(test.file, suite.files)));
    if (!candidates.length) throw new Error(`UNKNOWN_REGISTERED_BROWSER_CASE:${item.suite}:${item.project}:${item.title}`);
    if (candidates.length !== 1) throw new Error(`AMBIGUOUS_REGISTERED_BROWSER_CASE:${item.suite}:${item.project}:${item.title}`);
  }
  if (selection && (options.shard || options.grep || options.faultProbe)) throw new Error("EXPLICIT_BROWSER_SELECTION_CONFLICT");
  const groups = orderedBrowserExecutionGroups(suites, options.projects ?? ["desktop", "mobile"], caseOrder).filter(group => !selection || selection.some(item => item.suite === group.suite.id && item.project === group.project));
  assertBrowserFaultProbeSelection(options.faultProbe, groups);
  if (options.faultProbe && (options.shard || options.grep)) throw new Error("EXACT_BROWSER_FAULT_PROBE_SELECTION_REQUIRED");
  await preparePdfAssets();
  const execution: BrowserRunReceipt["execution"] = { caseOrder, mode: development ? "development-smoke" : options.faultProbe ? "fault-probe" : "acceptance", ...(development ? { acceptance: false as const } : {}), ...(options.faultProbe ? { faultProbe: options.faultProbe } : {}) };
  if (Bun.version !== "1.4.3" || Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v26.11.1") throw new Error("PINNED_TOOLCHAIN_REQUIRED");
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const evidence = join(root, ".scratch/dev/test-suites/browser", stamp);
  await mkdir(evidence, { recursive: true, mode: 0o700 });
  options.onInvestorEvidence?.(evidence);
  const runtimeFingerprint = await runtimeSourceFingerprint(root), verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  const receipts: BrowserRunCaseReceipt[] = [];
  const inventory: BrowserInventoryCase[] = [];
  const templates = new Map<string, { run: string; env: Record<string, string>; output: string }>();
  const cleanupErrors: unknown[] = [];
  let releaseDirectory = options.releaseDirectory;
  const productHostCases=suites.some(suite=>suite.files.includes(productHostBrowserFile));
  const executiveWorkCases=suites.some(suite=>suite.files.includes(executiveWorkHostBrowserFile));
  const formsHostCases=suites.some(suite=>suite.files.includes(formsHostBrowserFile));
  const formsHostSuite=suites.find(suite=>suite.files.includes(formsHostBrowserFile));
  const formsHostRequiredOnShard=Boolean(formsHostSuite&&registeredCases.some(test=>test.runner==="playwright"&&test.project&&
    canonicalCaseFile(test.file,formsHostSuite.files)===formsHostBrowserFile&&
    (!options.shard||assignedBrowserShard(formsHostSuite.id,{file:test.file,title:test.title,project:test.project},options.shard))&&
    (!selection||selection.some(item=>item.suite===formsHostSuite.id&&item.title===test.title&&item.project===test.project))));
  if(executiveWorkCases&&(suites.length!==1||suites[0].files.length!==1||suites[0].files[0]!==executiveWorkHostBrowserFile||suites[0].fixture!=="executive-work"||!releaseDirectory||productHostCases))throw Error("EXACT_EXECUTIVE_WORK_HOST_SUITE_AND_RELEASE_REQUIRED");
  if(formsHostCases&&(suites.length!==1||suites[0].files.length!==1||suites[0].files[0]!==formsHostBrowserFile||suites[0].fixture!=="engineering"||suites[0].lane!=="e2e"||!releaseDirectory||productHostCases||executiveWorkCases))throw Error("EXACT_FORMS_HOST_SUITE_AND_RELEASE_REQUIRED");
  const productPlanPaths=options.productHostPlanPaths??productHostPlanPathsFromEnvironment(process.env,options.productHostPlanPath);
  const productPlanInputs=new Map<string,Awaited<ReturnType<typeof productHostPlan>>>();
  const loadProductPlan=async(path:string)=>{
    const absolute=resolve(root,path),existing=productPlanInputs.get(absolute);if(existing)return existing;
    const input=await productHostPlan(root,absolute,runtimeFingerprint,verificationFingerprint,releaseDirectory!);
    if(JSON.stringify(productHostProducts)!==JSON.stringify(productIds))throw Error("PRODUCT_HOST_REGISTRY_CHANGED");
    if(resolve(root,input.plan.tools.node.path)!==node)throw Error("PRODUCT_HOST_PINNED_NODE_BINDING_REQUIRED");
    if(resolve(root,input.plan.tools.chromium.path)!==(await import("playwright")).chromium.executablePath())throw Error("PRODUCT_HOST_ORIGINAL_PLAYWRIGHT_EXECUTABLE_REQUIRED");
    productPlanInputs.set(absolute,input);return input;
  };
  const productHostSuite=productHostCases?suites.find(suite=>suite.files.includes(productHostBrowserFile)):undefined;
  const selectedProductHostCases=productHostSuite?registeredCases.filter(test=>test.runner==="playwright"&&test.project&&
    canonicalCaseFile(test.file,productHostSuite.files)===productHostBrowserFile&&
    (!options.shard||assignedBrowserShard(productHostSuite.id,{file:test.file,title:test.title,project:test.project},options.shard))&&
    (!selection||selection.some(item=>item.suite===productHostSuite.id&&item.title===test.title&&item.project===test.project))&&
    (!options.grep||new RegExp(options.grep).test(`${test.project} ${test.file.replace(/^tests\/e2e\//," ")} ${test.title}`))):[];
  const neededProductHostWorkflows=new Set<ProductHostBusinessWorkflow>();
  for(const test of selectedProductHostCases){const workflow=productHostBusinessWorkflowForCase(test.file,test.title);if(workflow)neededProductHostWorkflows.add(workflow);}
  const defaultProductPlanPath=productHostPlanPathForWorkflow(productPlanPaths,null);
  if(productHostCases&&(!defaultProductPlanPath||!releaseDirectory))throw Error("PRODUCT_HOST_MODE_REQUIRES_EXPLICIT_PLAN_AND_CURRENT_RELEASE");
  if(productHostCases)await loadProductPlan(defaultProductPlanPath!);
  for(const workflow of neededProductHostWorkflows){
    const path=productHostPlanPathForWorkflow(productPlanPaths,workflow);
    if(!path)throw Error(workflow==="work.request-draft"?"PRODUCT_HOST_WORK_PLAN_REQUIRED":workflow==="finance.sales-invoice"?"PRODUCT_HOST_FINANCE_PLAN_REQUIRED":"PRODUCT_HOST_BUSINESS_PLAN_REQUIRED");
    const input=await loadProductPlan(path);
    assertProductHostPlanWorkflow(input.plan,workflow);
  }
  const productPlanForCase=(wanted:BrowserCase|undefined)=>{
    if(!wanted)return defaultProductPlanPath?productPlanInputs.get(resolve(root,defaultProductPlanPath)):undefined;
    const workflow=productHostBusinessWorkflowForCase(wanted.file,wanted.title),path=productHostPlanPathForWorkflow(productPlanPaths,workflow);
    if(!path)throw Error(workflow==="work.request-draft"?"PRODUCT_HOST_WORK_PLAN_REQUIRED":workflow==="finance.sales-invoice"?"PRODUCT_HOST_FINANCE_PLAN_REQUIRED":"PRODUCT_HOST_MODE_REQUIRES_EXPLICIT_PLAN_AND_CURRENT_RELEASE");
    const input=productPlanInputs.get(resolve(root,path));if(!input)throw Error("PRODUCT_HOST_PLAN_NOT_VALIDATED");return input;
  };
  const formsArm=options.formsHostPlanPath??process.env.ERPX_TEST_FORMS_HOST_PLAN;
  if(formsHostRequiredOnShard&&(!formsArm||!releaseDirectory))throw Error("FORMS_HOST_MODE_REQUIRES_EXPLICIT_PLAN_AND_CURRENT_RELEASE");
  const formsPlan=formsHostRequiredOnShard?await formsHostPlan(root,formsArm!,runtimeFingerprint,verificationFingerprint,releaseDirectory!):null;
  if(formsPlan&&resolve(root,formsPlan.plan.tools.node.path)!==node)throw Error("FORMS_HOST_PINNED_NODE_BINDING_REQUIRED");
  if(formsPlan&&resolve(root,formsPlan.plan.tools.chromium.path)!==(await import("playwright")).chromium.executablePath())throw Error("FORMS_HOST_ORIGINAL_PLAYWRIGHT_EXECUTABLE_REQUIRED");
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
      const required: BrowserCase[] = development ? selection!.filter(item => item.suite === suite.id && item.project === project).map(item => {
        if (!item.file) throw new Error("DEVELOPMENT_SMOKE_FILE_IDENTITY_REQUIRED");
        // Registered identities are collectable without opening PostgreSQL.
        const fileTitle = relative(join(root,"tests"),join(root,item.file)).replaceAll("\\","/");
        return { file: item.file, fileTitle, title: item.title, project, expectedStatus: "passed" };
      }) : await cohort(suite, project, undefined);
      if (!required.length) throw new Error(`EMPTY_BROWSER_SUITE:${suite.id}:${project}`);
      assertRequiredCaseInventory(registeredCases, required.map(test => ({ ...test, runner: "playwright", ...browserCaseProjectRequirement(test) })), {
        runner: "playwright", files: development ? [...new Set(required.map(test=>test.file))] : suite.files, projects: [project], titleMatches: test => development ? required.some(item=>item.title===test.title && item.file===canonicalCaseFile(test.file,suite.files)) : !options.grep || new RegExp(options.grep).test(`${project} ${required.find(actual => canonicalCaseFile(actual.file, suite.files) === test.file)?.fileTitle ?? test.file.replace(/^tests\/e2e\//, "")} ${test.title}`),
      });
      if (options.faultProbe) {
        const selected = required.filter(test => isBrowserFaultProbeCase(suite.id, test, options.faultProbe));
        if (selected.length !== 1 || !browserCaseProjectRequirement(selected[0]).required || selected[0].expectedStatus !== "passed") throw new Error("REQUIRED_BROWSER_FAULT_PROBE_CASE_NOT_FOUND");
      }
      const selected = selection?.filter(item => item.suite === suite.id && item.project === project);
      if (selected?.some(item => required.filter(test => test.title === item.title && (!item.file || canonicalCaseFile(test.file, suite.files) === canonicalCaseFile(item.file, suite.files))).length !== 1)) throw new Error("EXACT_BROWSER_SELECTED_CASE_REQUIRED");
      for (const test of orderedBrowserExecutionCases(required, caseOrder)) {
        if (selected && !selected.some(item => item.title === test.title && (!item.file || canonicalCaseFile(test.file, suite.files) === canonicalCaseFile(item.file, suite.files)))) continue;
        const requirement = browserCaseProjectRequirement(test);
        if (selected && !requirement.required) throw new Error(`EXACT_BROWSER_SELECTED_CASE_NOT_REQUIRED:${suite.id}:${project}:${test.title}`);
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
    const receipt: BrowserRunReceipt = { version: 1, ...(development ? { acceptance: false as const } : {}), passed: completed && !interrupted && !cleanupErrors.length, runtimeFingerprint, verificationFingerprint, shard: options.shard ?? null, interrupted: interrupted ?? null, suites: suites.map(suite => suite.id), cleanupFailures: cleanupErrors.map(String), inventory, receipts, execution, evidencePath: relative(root, evidence) };
    await writeFile(join(evidence, "receipt.json"), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 });
    if (cleanupErrors.length && completed) throw new AggregateError(cleanupErrors, "BROWSER_COHORT_CLEANUP_FAILED");
    if (interrupted && completed) throw new Error(`BROWSER_RUN_INTERRUPTED:${interrupted}`);
  }

  async function cleanup(run: string, env: Record<string, string>, output: string, uiEvidence?:UiBrowserEvidenceWitness) {
    if (!existsSync(join(root, ".runtime/browser", run, "state.json"))) return;
    let evidenceFailure: unknown;
    try {
      if(uiEvidence)await verifyUiBrowserEvidence(root,uiEvidence,true);
      await retainBrowserFixtureEvidence(root, run, output);
      if(development&&workspace)await retainWorkspaceApplicationLogs(root,run,output,workspace.apps);
      for (const name of ["server.log", "jobs.log", "state.json", "attachment-baseline.json", "attachment-baseline-error.log", "jobs-enrollment.json", "jobs-pause.json", "jobs-resume.json"]) {
        const path = join(root, ".runtime/browser", run, name);
        if(name==="state.json"&&uiEvidence)continue; // Exact original state was retained and checked before teardown.
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
    if(uiEvidence)await verifyUiBrowserEvidence(root,uiEvidence);
  }

  async function cohort(suite: Suite, project: "desktop" | "mobile", wanted?: BrowserCase): Promise<BrowserCase[]> {
    const productMode=suite.files.includes(productHostBrowserFile);
    if(workspace&&(productMode||suite.files.includes(executiveWorkHostBrowserFile)||suite.files.includes(formsHostBrowserFile)||suite.fictionalProvider||suite.lane==='uat'||releaseDirectory))throw new Error('WORKSPACE_BROWSER_PROFILE_IS_EXCLUSIVE');
    const productPlan=productMode?productPlanForCase(wanted):undefined;
    const executiveWorkMode=suite.files.includes(executiveWorkHostBrowserFile);
    const formsHostSuiteMode=suite.files.includes(formsHostBrowserFile),formsMode=formsHostSuiteMode&&Boolean(wanted);
    if(productMode&&(suite.files.length!==1||!productPlan))throw Error("EXACT_PRODUCT_HOST_SUITE_AND_ARM_REQUIRED");
    if(executiveWorkMode&&(suite.files.length!==1||suite.fixture!=="executive-work"||!releaseDirectory||productMode))throw Error("EXACT_EXECUTIVE_WORK_HOST_CASE_REQUIRED");
    if(formsHostSuiteMode&&(suite.files.length!==1||suite.fixture!=="engineering"||!releaseDirectory||productMode||executiveWorkMode))throw Error("EXACT_FORMS_HOST_CASE_AND_ARM_REQUIRED");
    if(formsMode&&!formsPlan)throw Error("EXACT_FORMS_HOST_CASE_AND_ARM_REQUIRED");
    if(formsMode&&wanted&&(canonicalCaseFile(wanted.file,suite.files)!==formsHostBrowserFile||wanted.title!==formsHostCaseTitle))throw Error("EXACT_FORMS_HOST_CASE_AND_ARM_REQUIRED");
    const run = randomBytes(6).toString("hex"), port = workspace||productMode||executiveWorkMode||formsHostSuiteMode?3104:options.port ?? (suite.lane === "uat" || suite.fixture === "legal" || suite.fictionalProvider ? 3104 : 3102);
    const origin = `http://127.0.0.1:${port}`, output = join(evidence, run);
    await freePort(port); await mkdir(output, { mode: 0o700 });
    if (suite.fictionalProvider && (port !== 3104 || !releaseDirectory)) throw new Error('FICTIONAL_PROVIDER_COMPILED_3104_REQUIRED');
    const verifyRelease = (port === 3104 || productMode || suite.lane === "uat" || Boolean(suite.fictionalProvider)) && Boolean(releaseDirectory);
    // Global health requires the real worker, scanner and reconciled storage.
    // Capability-specific requirements remain explicit in the catalogue.
    const requirements = development ? { jobs: suite.requires.jobs, scanner: suite.requires.scanner } : { jobs: true, scanner: true };
    const env = {
      ERPX_BROWSER_SIMULATION_RUN_ID: run, ERPX_BROWSER_SIMULATION_PORT: String(port), ERPX_BROWSER_SIMULATION_PROFILE: suite.fixture,
      ERPX_BROWSER_EXECUTION_MODE: development ? "development-smoke" : "acceptance",
      ...(workspace?{ERPX_BROWSER_WORKSPACE_APPS:JSON.stringify(workspace.apps),ERPX_BROWSER_WORKSPACE_BUNDLER:workspace.bundler??'webpack',...(workspace.consoleRetired?{ERPX_BROWSER_WORKSPACE_CONSOLE_RETIRED:'1'}:{})}:{}),
      ERPX_BROWSER_SIMULATION_ATTACHMENTS: requirements.scanner ? "local" : "none", ERPX_BROWSER_SIMULATION_JOBS: requirements.jobs ? "1" : "0",
      ERPX_BROWSER_SIMULATION_LEGAL: suite.fixture === "legal" || suite.fixture === "uat" ? "1" : "0",
      ERPX_BROWSER_SIMULATION_MENU_IMAGES: options.investorCapture ? "1" : "0",
      ...(suite.id === "customer_authority.demo-retail-customer-authority.journeys" ? {
        ERPX_BROWSER_SIMULATION_CUSTOMER_PORTAL_INVITATIONS: "1", ERPX_ENVIRONMENT: "local", ERPX_PUBLIC_ORIGIN: origin,
        ERPX_EMAIL_ADAPTER: "sandbox",
        ERPX_DATA_DIR: join(root, ".runtime/browser", run), RESEND_API_KEY: "isolated-sandbox-no-provider-key",
        RESEND_FROM: "Fictional <no-reply@example.test>",
      } : {}),
      ...(productMode?{ERPX_BROWSER_SIMULATION_PRODUCT_HOSTS:"1",ERPX_PRODUCT_HOST_PLAN:productPlan!.binding.path,ERPX_PRODUCT_HOST_PLAN_SHA256:productPlan!.binding.sha256}:{}),
      ...(executiveWorkMode?{ERPX_BROWSER_SIMULATION_EXECUTIVE_WORK_HOSTS:"1"}:{}),
      ...(formsMode?{ERPX_BROWSER_SIMULATION_FORMS_HOSTS:"1",ERPX_FORMS_HOST_PLAN:formsPlan!.binding.path,ERPX_FORMS_HOST_PLAN_SHA256:formsPlan!.binding.sha256}:{}),
      ...(suite.fictionalProvider ? { ERPX_PMS_PROVIDER_FIXTURE: "1", ERPX_ENVIRONMENT: "local", ERPX_PUBLIC_ORIGIN: origin, ERPX_DATA_DIR: join(root, '.runtime/browser', run), POS_STRIPE_TEST_SECRET_KEY: 'sk_test_erpx_fictional_provider', POS_STRIPE_TEST_WEBHOOK_SECRET: 'whsec_erpx_fictional_provider' } : {}),
    };
    const simulation = (mode: string) => [process.execPath, "scripts/local/run-server-script.ts", "scripts/local/browser-simulation.ts", mode, run];
    const report = join(output, wanted ? "result.json" : "collection.json");
    let failed = false;
    let caseReceipt: BrowserRunCaseReceipt | undefined;
    let faultTriggered = false;
    let developmentEdge:Awaited<ReturnType<typeof startDevelopmentHostProcess>>|undefined;
    let productEdge:Awaited<ReturnType<typeof startProductHostEdge>>|undefined;
    let executiveWorkEdge:Awaited<ReturnType<typeof startExecutiveWorkHostEdge>>|undefined;
    let formsEdge:Awaited<ReturnType<typeof startFormsHostEdge>>|undefined;
    try {
      if (await runtimeSourceFingerprint(root) !== runtimeFingerprint || await verificationSourceFingerprint(root, runtimeFingerprint) !== verificationFingerprint) throw new Error("BROWSER_SOURCE_CHANGED");
      const templateKey = JSON.stringify([suite.fixture, port, requirements, suite.fictionalProvider ?? null, workspace ?? null, ...(productMode?["product-hosts-v1"]:[]), ...(executiveWorkMode?["executive-work-hosts-v1"]:[]), ...(formsMode?["forms-host-v1"]:[])]);
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
      const browserEnvironment = { ...env };
      delete browserEnvironment.ERPX_FORMS_HOST_PLAN;
      delete browserEnvironment.ERPX_FORMS_HOST_PLAN_SHA256;
      const testEnv = { ...browserEnvironment, ERPX_INVESTOR_CAPTURE: options.investorCapture ? "1" : undefined, ...localBrowserActorEnvironment(suite.fixture, state.fixture), ERPX_DEMO_RUN: run, ERPX_BROWSER_SIMULATION_RUN: run, ERPX_E2E_BASE_URL: workspace ? developmentHostOrigin(workspace.entryApp) : formsMode ? formsHostOrigin : origin,
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
        if(workspace){
          developmentEdge=await startDevelopmentHostProcess({root,run,output,node,sourceFingerprint:runtimeFingerprint,profile:workspace});
          Object.assign(testEnv,{ERPX_DEVELOPMENT_HOST_LEASE:developmentEdge.path,NODE_EXTRA_CA_CERTS:developmentEdge.certificatePath});
        }else{
        await execute([node, "scripts/testing/browser-readiness.mts", origin, String(started.pid), ...(development ? ["--development", ...(requirements.jobs ? ["--jobs"] : []), ...(requirements.scanner ? ["--scanner"] : [])] : [])], env, join(output, "readiness.log"));
        Object.assign(testEnv, await prepareBrowserCasePrerequisites({ suite, wanted, run, directory: join(root, ".runtime/browser", run), releaseDirectory, evidenceDirectory: output, environment: testEnv, execute }));
        await execute([node, "scripts/testing/browser-readiness.mts", origin, String(started.pid), ...(development ? ["--development", ...(requirements.jobs ? ["--jobs"] : []), ...(requirements.scanner ? ["--scanner"] : [])] : [])], env, join(output, "prerequisites-readiness.log"));
        }
        if(productMode){
          productEdge=await startProductHostEdge(root,productPlan!,run,output,productHostBusinessWorkflowForCase(wanted.file,wanted.title),productHostReadOnlyWorkflowForCase(wanted.file,wanted.title));
          Object.assign(testEnv,{ERPX_PRODUCT_HOST_LEASE:productEdge.path,NODE_EXTRA_CA_CERTS:productEdge.lease.ca.path});
        }
        if(executiveWorkMode){
          executiveWorkEdge=await startExecutiveWorkHostEdge(root,{run,output,runtimeFingerprint,verificationFingerprint,releaseDirectory:releaseDirectory!,nodePath:node,chromiumPath:(await import("playwright")).chromium.executablePath()});
          Object.assign(testEnv,{ERPX_EXECUTIVE_WORK_HOST_LEASE:executiveWorkEdge.path});
        }
        if(formsMode){
          formsEdge=await startFormsHostEdge(root,formsPlan!,run,output);
          Object.assign(testEnv,{ERPX_FORMS_HOST_LEASE:formsEdge.path,NODE_EXTRA_CA_CERTS:formsEdge.lease.ca.path});
        }
        if (suite.lane === "uat") {
          const profiles = requiredUatAuthProfiles(suite, wanted);
          if (profiles.length) await execute([process.execPath, "scripts/testing/prepare-uat-auth.ts"], { ...testEnv, ERPX_UAT_AUTH_PROFILES: profiles.join(",") }, join(output, "auth.log"));
        }
      }
      if (wanted && isBrowserFaultProbeCase(suite.id, wanted, options.faultProbe)) {
        faultTriggered = true;
        triggerBrowserFaultProbe(suite.id, wanted, options.faultProbe);
      }
      const args = [node, "node_modules/@playwright/test/cli.js", "test", "--config", "tests/config/playwright.config.ts", ...suite.files, `--project=${project}`, "--workers=1", "--retries=0", "--reporter=json", `--output=${join(output, "artifacts")}`];
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
        if(productEdge)await productEdge.stable();
        if(executiveWorkEdge)await executiveWorkEdge.stable();
        if(formsEdge)await formsEdge.stable();
        if (await runtimeSourceFingerprint(root) !== runtimeFingerprint || await verificationSourceFingerprint(root, runtimeFingerprint) !== verificationFingerprint) throw new Error("BROWSER_SOURCE_CHANGED");
        const localFrontApp=uiLocalFrontAppForCase(suite.id,wanted.file,wanted.title);
        const uiEvidence=await captureUiBrowserEvidence({root,run,reportPath:report,report:result,runtimeFingerprint,verificationFingerprint,...(localFrontApp?{localFrontApp}:{}),...(productEdge?{productHostLeasePath:productEdge.path}:{}),...(executiveWorkEdge?{executiveWorkHostLeasePath:executiveWorkEdge.path}:{}),...(formsEdge?{formsHostLeasePath:formsEdge.path}:{})});
        if(formsEdge&&!uiEvidence)throw Error("FORMS_HOST_UI_OBSERVATION_REQUIRED");
        const reportSha256=createHash("sha256").update(await readFile(report)).digest("hex");
        const productBindings=productEdge?await captureProductHostReceiptBindings(root,evidence,run,report,result):undefined;
        if(productBindings&&productBindings.leaseSha256!==createHash("sha256").update(await readFile(productEdge!.path)).digest("hex"))throw Error("PRODUCT_HOST_LEASE_CHANGED_DURING_RECEIPT_CAPTURE");
        const productHostEvidence=productEdge?("productHostReadOnly" in productBindings! ? {productHostReadOnly:productBindings!.productHostReadOnly} : {productHostCommercial:productBindings!.productHostCommercial,...("productHostBusiness" in productBindings! ? {productHostBusiness:productBindings!.productHostBusiness} : {})}):{};
        if(developmentEdge){
          const path=join(output,'development-host-lease.json');await copyFile(developmentEdge.path,path);await chmod(path,0o600);
          await copyFile(join(root,'.runtime/browser',run,'workspace-runtime.json'),join(output,'workspace-runtime.json'));
        }
        caseReceipt = { suite: suite.id, file: wanted.file, project, title: wanted.title, run, report, reportSha256, passed: true, ...(developmentEdge?{developmentHost:{path:join(output,'development-host-lease.json'),sha256:createHash('sha256').update(await readFile(developmentEdge.path)).digest('hex'),apps:workspace!.apps}}:{}), ...(productEdge?{productHostLease:uiEvidence?.productHostLease??{path:productEdge.path,sha256:productBindings!.leaseSha256}}:{}),...productHostEvidence,...(executiveWorkEdge?{executiveWorkHostLease:uiEvidence?.executiveWorkHostLease??{path:executiveWorkEdge.path,sha256:createHash("sha256").update(await readFile(executiveWorkEdge.path)).digest("hex")}}:{}),...(formsEdge?{formsHostLease:uiEvidence?.formsHostLease??{path:formsEdge.path,sha256:createHash("sha256").update(await readFile(formsEdge.path)).digest("hex")}}:{}),...(uiEvidence?{uiEvidence}:{}) };
        receipts.push(caseReceipt);
        console.log(JSON.stringify({ suite: suite.id, project, title: wanted.title, passed: true }));
        return [];
      }
      return browserCases(result).map(test => ({ ...test, file: canonicalCaseFile(test.file, suite.files) }));
    } catch (error) {
      failed = true;
      caseReceipt = { suite: suite.id, file: wanted?.file, project, title: wanted?.title, run, report, passed: false, ...(faultTriggered ? { faultProbe: true, failure: browserFaultProbeError } : {}) };
      receipts.push(caseReceipt);
      if (faultTriggered && error instanceof Error && error.message === browserFaultProbeError) throw new BrowserFaultProbeError(evidence);
      throw error;
    }
    finally {
      let edgeFailed=false,edgeFailure:unknown;
      if(developmentEdge){try{await developmentEdge.close();await copyFile(join(root,'.runtime/browser',run,'workspace-edge-result.json'),join(output,'development-host-result.json'));}catch(error){cleanupErrors.push(error);}}
      if(productEdge){try{await productEdge.stable();}catch(error){edgeFailed=true;edgeFailure=error;}try{await productEdge.close();}catch(error){if(!edgeFailed){edgeFailed=true;edgeFailure=error;}}if(edgeFailed)cleanupErrors.push(edgeFailure);}
      if(executiveWorkEdge){try{await executiveWorkEdge.stable();}catch(error){edgeFailed=true;edgeFailure=error;}try{await executiveWorkEdge.close();}catch(error){if(!edgeFailed){edgeFailed=true;edgeFailure=error;}}if(edgeFailed)cleanupErrors.push(edgeFailure);}
      if(formsEdge){try{await formsEdge.stable();}catch(error){edgeFailed=true;edgeFailure=error;}try{await formsEdge.close();}catch(error){if(!edgeFailed){edgeFailed=true;edgeFailure=error;}}if(edgeFailed)cleanupErrors.push(edgeFailure);}
      // A preparation failure can leave owned databases; the known run identity
      // permits exact cleanup even before the launcher emitted its final JSON.
      if (existsSync(join(root, ".runtime/browser", run, "state.json"))) {
        const wasInterrupted = interrupted; interrupted = undefined;
        try {
          await cleanup(run, env, output,caseReceipt?.uiEvidence);
          if (caseReceipt) caseReceipt.cleanup = { completed: true, ownedDirectoryAbsent: !existsSync(join(root, ".runtime/browser", run)) };
        } catch (error) {
          if (caseReceipt) caseReceipt.cleanup = { completed: false, ownedDirectoryAbsent: !existsSync(join(root, ".runtime/browser", run)) };
          cleanupErrors.push(new Error(`BROWSER_COHORT_CLEANUP_FAILED:${run}`, { cause: error }));
          if (!failed) throw error;
          console.error(`Owned browser cohort cleanup failed; inspect ${output}`);
        }
        finally { interrupted = wasInterrupted; }
      }
      if(edgeFailed&&!failed)throw edgeFailure;
    }
  }
}

if (import.meta.main) {
  const modules: string[] = [], suiteIds: string[] = [], files: string[] = [], options: Parameters<typeof runBrowserSuites>[1] = {};
  let caseSelector: string | undefined;
  let productHostWorkPlanPath: string | undefined, productHostFinancePlanPath: string | undefined;
  let projectSelected = false;
  let portSelected = false;
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
    else if (arg === "--project" || arg.startsWith("--project=")) { if (projectSelected) throw new Error("DUPLICATE_TEST_PROJECT_SELECTION"); projectSelected = true; const project = value("--project"); if (!["desktop", "mobile"].includes(project)) throw new Error("UNKNOWN_BROWSER_PROJECT"); options.projects = [project as "desktop" | "mobile"]; }
    else if (arg === "--port" || arg.startsWith("--port=")) { if (portSelected) throw new Error("DUPLICATE_BROWSER_PORT_SELECTION"); portSelected = true; const port = value("--port"); if (port !== "3102" && port !== "3104") throw new Error("UNKNOWN_BROWSER_PORT"); options.port = Number(port) as 3102 | 3104; }
    else if (arg === "--grep" || arg.startsWith("--grep=")) options.grep = value("--grep");
    else if (arg === "--case-order" || arg.startsWith("--case-order=")) options.caseOrder = selectBrowserCaseOrder(options.caseOrder, value("--case-order"));
    else if (arg === "--case" || arg.startsWith("--case=")) {
      if (caseSelector) throw new Error("DUPLICATE_EXACT_CASE_SELECTOR");
      caseSelector = value("--case");
    }
    else if(arg==="--product-host-plan"||arg.startsWith("--product-host-plan="))options.productHostPlanPath=resolve(value("--product-host-plan"));
    else if(arg==="--product-host-work-plan"||arg.startsWith("--product-host-work-plan=")){if(productHostWorkPlanPath)throw Error("DUPLICATE_PRODUCT_HOST_WORK_PLAN");productHostWorkPlanPath=resolve(value("--product-host-work-plan"));}
    else if(arg==="--product-host-finance-plan"||arg.startsWith("--product-host-finance-plan=")){if(productHostFinancePlanPath)throw Error("DUPLICATE_PRODUCT_HOST_FINANCE_PLAN");productHostFinancePlanPath=resolve(value("--product-host-finance-plan"));}
    else if(arg==="--forms-host-plan"||arg.startsWith("--forms-host-plan="))options.formsHostPlanPath=resolve(value("--forms-host-plan"));
    else if (arg === "--release-dir" || arg.startsWith("--release-dir=")) options.releaseDirectory = resolve(value("--release-dir"));
    else if (arg.startsWith("tests/")) files.push(arg);
    else if (arg !== "--") throw new Error(`UNKNOWN_BROWSER_ARGUMENT:${arg}`);
  }
  if(productHostWorkPlanPath||productHostFinancePlanPath){
    const defaultPath=options.productHostPlanPath??productHostWorkPlanPath??productHostFinancePlanPath;
    options.productHostPlanPaths={default:defaultPath,businessWorkflows:{...(productHostWorkPlanPath?{"work.request-draft":productHostWorkPlanPath}:{}),...(productHostFinancePlanPath?{"finance.sales-invoice":productHostFinancePlanPath}:{})}};
  }
  if (caseSelector && options.grep) throw new Error("EXACT_BROWSER_CASE_PATTERN_CONFLICT");
  const catalogue = loadCatalogue();
  let suites = selectSuites(catalogue, { lane: lane ?? (suiteIds.length || caseSelector ? undefined : "e2e"), modules, suiteIds, files });
  if (caseSelector) {
    const identity = resolveRegisteredCase(caseSelector, { project: options.projects?.length === 1 ? options.projects[0] : undefined });
    if (identity.runner !== "playwright" || !identity.project) throw new Error(`REGISTERED_CASE_RUNNER_MISMATCH:${caseSelector}`);
    const suite = suiteForRegisteredCase(catalogue, identity);
    if (!suites.some(candidate => candidate.id === suite.id)) throw new Error(`REGISTERED_CASE_OUTSIDE_BROWSER_SELECTION:${caseSelector}`);
    suites = [suite];
    options.caseSelection = [{ suite: suite.id, file: identity.file, title: identity.title, project: identity.project }];
  }
  if (list) writeJsonOutput({ lane, suites, ...(caseSelector ? { selectedCase: options.caseSelection?.[0] } : {}), execution: { caseOrder: options.caseOrder ?? "forward", mode: "acceptance" } });
  else console.log(JSON.stringify(await runBrowserSuites(suites, options)));
}
