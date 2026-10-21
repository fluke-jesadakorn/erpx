import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { assertExplicitSelectionIncluded, loadCatalogue, listSelection, reachesDatabaseFixture, repositoryRoot as root, selectSuites, validateCatalogue, type Suite } from "./catalogue";
import { parseTestArguments } from "./arguments";
import { exactNativeCasePattern, resolveRegisteredCase, suiteForRegisteredCase } from "./exact-case-selection";
import { impactFromGit } from "./impact";
import { vitestChildEnvironment } from "./runner-environment";
import { attachmentToolEnvironment } from "./runtime-tooling";
import { writeJsonOutput } from "./json-output";
import { assertTestPlacement } from "./layout";
import { runDevelopmentTests } from "./development";

if (Bun.version !== "1.4.3") throw new Error("Tests require Bun 1.4.3.");
const options = parseTestArguments(process.argv.slice(2));
if (options.caseSelector && ["impact", "dev"].includes(options.command)) throw new Error("EXACT_CASE_SELECTION_NOT_AVAILABLE_FOR_IMPACT");
if (options.command === "dev") process.exit(await runDevelopmentTests(options));
const catalogue = loadCatalogue();
validateCatalogue(catalogue);
assertTestPlacement(catalogue.suites);
if (options.check) { console.log(`TEST_CATALOGUE_VALID:${catalogue.modules.length} modules, ${catalogue.suites.length} suites`); process.exit(0); }
function requestedBrowserProject(args: readonly string[]): "desktop" | "mobile" | undefined {
  const values: string[] = [];
  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    if (arg.startsWith("--project=")) values.push(arg.slice("--project=".length));
    else if (arg === "--project") {
      const value = args[++index];
      if (!value || value.startsWith("-")) throw new Error("TEST_OPTION_VALUE_REQUIRED:--project");
      values.push(value);
    }
  }
  if (values.length > 1) throw new Error("DUPLICATE_TEST_PROJECT_SELECTION");
  if (!values.length) return undefined;
  if (values[0] !== "desktop" && values[0] !== "mobile") throw new Error("UNKNOWN_BROWSER_PROJECT");
  return values[0];
}
const requestedProject = options.caseSelector ? requestedBrowserProject(options.forwarded) : undefined;
const registeredCase = options.caseSelector ? resolveRegisteredCase(options.caseSelector, { project: requestedProject }) : undefined;
const caseSuite = registeredCase ? suiteForRegisteredCase(catalogue, registeredCase) : undefined;
if (caseSuite && options.lane && caseSuite.lane !== options.lane) throw new Error(`TEST_CASE_LANE_MISMATCH:${options.caseSelector}:${caseSuite.lane}:${options.lane}`);
if (registeredCase?.runner === "vitest" && requestedProject) throw new Error("BROWSER_PROJECT_ONLY_FOR_PLAYWRIGHT_CASE");
let suites: Suite[];
if (options.command === "impact") {
  const impact = impactFromGit(options.base!, root, options.head);
  if (options.json) writeJsonOutput({ ...impact, execution: impact.mode === "full"
    ? { caseOrders: ["forward", "reverse"], mode: "acceptance", declaredFailureRestart: true, zeroRetries: true }
    : { caseOrder: options.caseOrder ?? "forward", mode: "acceptance" } });
  else console.log(`TEST_IMPACT:${impact.mode}:${impact.reasons.join(",")}`);
  if (options.list) { if (!options.json && impact.suites.length) console.log(listSelection(impact.suites)); process.exit(0); }
  if (impact.mode === "none") process.exit(0);
  if (impact.mode === "full") {
    const full = Bun.spawn([process.execPath, resolve(root, "scripts/testing/full.ts"), ...options.forwarded], { cwd: root, env: process.env, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
    for (const signal of ["SIGINT", "SIGTERM"] as const) process.on(signal, () => full.kill(signal));
    process.exit(await full.exited);
  }
  suites = impact.suites;
} else {
  suites = selectSuites(catalogue, registeredCase ? { ...options, files: [...(options.files ?? []), registeredCase.file] } : options);
  if (caseSuite) suites = suites.filter(suite => suite.id === caseSuite.id);
  if (options.command === "tests" && !registeredCase) suites = suites.filter(suite => suite.lane === "unit" || suite.lane === "integration");
  if (!suites.length) throw new Error("EMPTY_TEST_SELECTION");
  assertExplicitSelectionIncluded(options, suites);
}
if (options.list) {
  const browserSelection = suites.some(suite => ["e2e", "journeys", "uat"].includes(suite.lane));
  if (options.json) writeJsonOutput(browserSelection ? { suites, ...(registeredCase ? { selectedCase: registeredCase } : {}), execution: { caseOrder: options.caseOrder ?? "forward", mode: "acceptance" } } : registeredCase ? { suites, selectedCase: registeredCase } : suites);
  else {
    console.log(listSelection(suites));
    if (registeredCase) console.log(`TEST_SELECTED_CASE:${registeredCase.runner}:${registeredCase.file}:${registeredCase.title}${registeredCase.project ? `:${registeredCase.project}` : ""}`);
    if (browserSelection) console.log(`TEST_BROWSER_EXECUTION_ORDER:${options.caseOrder ?? "forward"}`);
  }
  process.exit(0);
}

const candidates = [process.env.ERPX_NODE_BIN, resolve(root, ".runtime/node/node-v26.11.1-darwin-arm64/bin/node"), resolve(root, ".runtime/deployment/runtime/node/bin/node")].filter((path): path is string => !!path && existsSync(path));
const node = candidates[0];
if (!node) throw new Error("Pinned Node is missing. Set ERPX_NODE_BIN to Node 26.11.1.");
const version = Bun.spawnSync([node, "--version"]);
if (version.exitCode !== 0 || version.stdout.toString().trim() !== "v26.11.1") throw new Error("Tests require Node 26.11.1.");

const attachmentMode = process.env.ERPX_TEST_ATTACHMENTS_V2 ?? "false";
if (!["false", "true"].includes(attachmentMode)) throw new Error("INVALID_TEST_ATTACHMENT_MODE");
const env: Record<string, string | undefined> = { ...process.env, ERPX_NODE_BIN: node, NODE_ENV: "test", ERPX_ATTACHMENTS_V2: attachmentMode, ERPX_ATTACHMENT_UPLOAD_FREEZE: "false" };
function databaseCredentials(): void {
  if (!env.ERPX_TEST_ADMIN_URL) for (const file of [".env.test.local", ".env.test", ".env.local"]) {
    const path = resolve(root, file);
    if (!existsSync(path)) continue;
    const value = readFileSync(path, "utf8").match(/^\s*(?:export\s+)?ERPX_TEST_ADMIN_URL\s*=\s*(.*?)\s*$/m)?.[1];
    if (value) { env.ERPX_TEST_ADMIN_URL = value.replace(/^(["'])(.*)\1$/, "$2"); break; }
  }
  if (!env.ERPX_TEST_ADMIN_URL) throw new Error("Configure ERPX_TEST_ADMIN_URL for the isolated local test administrator.");
}
async function execute(argv: string[], childEnv: Record<string, string | undefined>): Promise<number> {
  const child = Bun.spawn(argv, { cwd: root, env: childEnv, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
  const interrupt = (signal: "SIGINT" | "SIGTERM") => child.kill(signal);
  const onInt = () => interrupt("SIGINT"), onTerm = () => interrupt("SIGTERM");
  process.on("SIGINT", onInt); process.on("SIGTERM", onTerm);
  try { return await child.exited; }
  finally { process.off("SIGINT", onInt); process.off("SIGTERM", onTerm); }
}

for (const kind of ["unit", "integration", "provider"] as const) {
  const selected = suites.filter(suite => suite.lane === kind);
  if (!selected.length) continue;
  if (kind === "integration") databaseCredentials();
  // Every external lane is a deliberate invocation. Missing dependencies fail before collecting skipped tests.
  if (kind === "provider") {
    for (const suite of selected) Object.assign(env, suite.provider!.environment);
    if (env.ERPX_TEST_STRIPE_SANDBOX === "1") {
      const binding = env.ERPX_TEST_STRIPE_KEY_BINDING ?? "STRIPE_SECRET_KEY";
      if (!["STRIPE_SECRET_KEY", "POS_STRIPE_TEST_SECRET_KEY"].includes(binding) || !env[binding]?.startsWith("sk_test_")) throw new Error("EXISTING_STRIPE_SANDBOX_TEST_KEY_REQUIRED");
      env.STRIPE_SECRET_KEY = env[binding];
    }
    for (const suite of selected) for (const variable of suite.provider!.required) if (!env[variable]) throw new Error(`PROVIDER_PREREQUISITE_REQUIRED:${suite.id}:${variable}`);
  }
  // Provider specs may mix PostgreSQL billing and non-database LDAP/cache checks.
  const groups = kind === "provider" ? [selected.filter(suite => !suite.files.some(file => reachesDatabaseFixture(file, root))), selected.filter(suite => suite.files.some(file => reachesDatabaseFixture(file, root)))] : [selected];
  for (const group of groups) {
    if (!group.length) continue;
    const postgres = kind === "integration" || group.some(suite => suite.files.some(file => reachesDatabaseFixture(file, root)));
    if (postgres) databaseCredentials();
    const selectedFiles = group.flatMap(suite => suite.files);
    const patternIndex = options.forwarded.findIndex(arg => ["-t", "--testNamePattern"].includes(arg) || arg.startsWith("--testNamePattern="));
    const namePattern = registeredCase?.runner === "vitest" ? exactNativeCasePattern(registeredCase.title) : patternIndex < 0 ? undefined : options.forwarded[patternIndex].split("=").slice(1).join("=") || options.forwarded[patternIndex + 1];
    const defaultReporter = options.forwarded.some(arg => arg === "--reporter" || arg.startsWith("--reporter=")) ? [] : ["--reporter", "default"];
    const integrationEnvironment = kind === "integration" ? { ...env, ...attachmentToolEnvironment(root) } : env;
    const childEnv = vitestChildEnvironment({ ...integrationEnvironment, ERPX_SELECTED_TEST_FILES: JSON.stringify(selectedFiles), ERPX_SELECTED_PROVIDER: kind === "provider" ? "1" : "0", ERPX_TEST_NAME_PATTERN: namePattern }, postgres);
    const exactPatternArgs = registeredCase?.runner === "vitest" ? ["--testNamePattern", exactNativeCasePattern(registeredCase.title)] : [];
    const exit = await execute([node, resolve(root, "node_modules/vitest/vitest.mjs"), "run", "--config", postgres ? "tests/config/vitest.integration.config.mts" : "tests/config/vitest.unit.config.mts", ...options.forwarded, ...exactPatternArgs, ...defaultReporter, "--reporter", resolve(root, "scripts/testing/required-cases-reporter.ts")], childEnv);
    if (exit !== 0) process.exit(exit);
  }
}
const browser = suites.filter(suite => ["e2e", "journeys", "uat"].includes(suite.lane));
if (browser.length) {
  const exit = await execute([process.execPath, resolve(root, "scripts/testing/browser-runner.ts"), ...browser.flatMap(suite => ["--suite", suite.id]), ...options.forwarded, ...(options.caseSelector ? ["--case", options.caseSelector] : []), ...(options.caseOrder ? ["--case-order", options.caseOrder] : [])], env);
  if (exit !== 0) process.exit(exit);
}
