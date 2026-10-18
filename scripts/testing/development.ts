import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { type TestArguments } from "./arguments";
import { loadCatalogue, listSelection, repositoryRoot, validateCatalogue, type Catalogue } from "./catalogue";
import { impactFromGit, type ImpactResult } from "./impact";
import { assertTestPlacement } from "./layout";
import { writeJsonOutput } from "./json-output";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";

/** Development selection never grants internal-regression acceptance. */
export function developmentSelection(impact: ImpactResult, catalogue: Catalogue) {
  const suites = impact.mode === "none" ? [] : (impact.mode === "full" ? catalogue.suites : impact.suites)
    .filter(suite => suite.lane === "unit" || impact.mode === "affected" && suite.lane === "integration");
  const selected = new Set(suites.map(suite => suite.id));
  return {
    scope: "development" as const, acceptance: false as const,
    impact: { mode: impact.mode, reasons: impact.reasons, modules: impact.modules, changed: impact.changed },
    suites,
    deferredSuites: catalogue.suites.filter(suite => suite.lane !== "provider" && !selected.has(suite.id))
      .map(suite => ({ id: suite.id, lane: suite.lane, command: `bun run test:${suite.lane} --suite=${suite.id}` })),
    deferredChecks: [
      "bun scripts/architecture/acceptance.ts", "bun run lint", "bun run typecheck",
      "bun run deploy:build", "browser isolation (forward, declared-failure restart, reverse) via bun run test:full",
    ],
    excludedProviderSuites: catalogue.suites.filter(suite => suite.lane === "provider").map(suite => suite.id),
    requiredAcceptance: "bun run test:full",
  };
}

export async function runDevelopmentTests(options: TestArguments): Promise<number> {
  const root = repositoryRoot, catalogue = loadCatalogue();
  validateCatalogue(catalogue);
  assertTestPlacement(catalogue.suites);
  const plan = developmentSelection(impactFromGit(options.base!, root, options.head), catalogue);
  if (options.check) throw new Error("DEV_CHECK_NOT_SUPPORTED:use test:catalogue");
  if (options.json) writeJsonOutput(plan);
  else {
    console.log(`TEST_DEVELOPMENT:${plan.impact.mode}:${plan.impact.reasons.join(",")}`);
    if (options.list && plan.suites.length) console.log(listSelection(plan.suites));
    console.log(`DEVELOPMENT_ONLY: full acceptance still requires ${plan.requiredAcceptance}`);
    console.log(`DEFERRED_INTERNAL_SUITES:${plan.deferredSuites.length}`);
    for (const suite of plan.deferredSuites) console.log(`NOT_RUN\t${suite.id}\t${suite.command}`);
    for (const command of plan.deferredChecks) console.log(`NOT_RUN_CHECK\t${command}`);
    if (plan.excludedProviderSuites.length) console.log(`OPT_IN_PROVIDER_SUITES_NOT_RUN:${plan.excludedProviderSuites.join(",")}`);
  }
  if (options.list) return 0;

  const directory = join(root, ".scratch/dev/test-suites/development", new Date().toISOString().replace(/[:.]/g, "-"));
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const runtimeFingerprint = await runtimeSourceFingerprint(root);
  const verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  let exitCode = 1, sourceUnchanged = false;
  try {
    if (plan.suites.length) {
      const child = Bun.spawn([process.execPath, join(root, "scripts/testing/test.ts"),
        ...plan.suites.flatMap(suite => ["--suite", suite.id]), ...options.forwarded],
      { cwd: root, env: process.env, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
      const onInt = () => child.kill("SIGINT"), onTerm = () => child.kill("SIGTERM");
      process.on("SIGINT", onInt); process.on("SIGTERM", onTerm);
      try { exitCode = await child.exited; }
      finally { process.off("SIGINT", onInt); process.off("SIGTERM", onTerm); }
    } else exitCode = 0;
    sourceUnchanged = await runtimeSourceFingerprint(root) === runtimeFingerprint &&
      await verificationSourceFingerprint(root, runtimeFingerprint) === verificationFingerprint;
    if (!sourceUnchanged) throw new Error("DEVELOPMENT_TEST_SOURCE_CHANGED");
    return exitCode;
  } finally {
    const receipt = join(directory, "receipt.json");
    const passed = exitCode === 0 && sourceUnchanged;
    await writeFile(receipt, JSON.stringify({ version: 1, ...plan, passed, exitCode, sourceUnchanged,
      runtimeFingerprint, verificationFingerprint, forwarded: options.forwarded }, null, 2) + "\n", { mode: 0o600 });
    console.log(JSON.stringify({ scope: "development", acceptance: false, passed, receipt }));
  }
}
