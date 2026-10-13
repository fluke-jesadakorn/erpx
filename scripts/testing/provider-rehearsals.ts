import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

export type ProviderRehearsal = {
  id: string;
  owner: string;
  modules: string[];
  source: string;
  packageScript: string;
  command: string;
  target: "disposable-postgres" | "guarded-development";
  prerequisites: { requiredEnvironment: string[]; conditions: string[] };
  interaction: { manual: true; steps: string[] };
};
export type ProviderRehearsalInventory = { version: 1; rehearsals: ProviderRehearsal[] };
const retainedRunners: Record<string, { source: string; command: string }> = {
  "test:pms:stripe:sandbox": { source: "tests/provider/pms/pms-stripe-sandbox-rehearsal.ts", command: "bun --conditions=react-server tests/provider/pms/pms-stripe-sandbox-rehearsal.ts" },
  "test:pos:stripe:sandbox": { source: "tests/provider/pos/verify-pos-stripe-sandbox.ts", command: "bun --conditions=react-server tests/provider/pos/verify-pos-stripe-sandbox.ts" },
};

/** Standalone interactive provider scripts are inventoried separately from framework cases. */
export function validateProviderRehearsals(inventory: ProviderRehearsalInventory, scripts: Readonly<Record<string, string>>, sourceExists: (source: string) => boolean, modules: readonly string[]): void {
  if (inventory.version !== 1 || !Array.isArray(inventory.rehearsals)) throw new Error("INVALID_PROVIDER_REHEARSAL_INVENTORY");
  const ids = new Set<string>(), runners = new Set<string>(), sources = new Set<string>();
  for (const rehearsal of inventory.rehearsals) {
    if (!rehearsal.id || ids.has(rehearsal.id) || !rehearsal.owner || !Array.isArray(rehearsal.modules) || !rehearsal.modules.length || rehearsal.modules.some(moduleId => !modules.includes(moduleId)) || !["disposable-postgres", "guarded-development"].includes(rehearsal.target) || rehearsal.interaction?.manual !== true || !rehearsal.interaction.steps?.length || !rehearsal.prerequisites?.requiredEnvironment?.length || !rehearsal.prerequisites.conditions?.length)
      throw new Error(`INVALID_PROVIDER_REHEARSAL:${rehearsal.id}`);
    if (runners.has(rehearsal.packageScript) || sources.has(rehearsal.source) || !/^(?:tests|scripts)\/(?!.*(?:^|\/)\.\.\/).+\.ts$/.test(rehearsal.source) || !sourceExists(rehearsal.source)) throw new Error(`PROVIDER_REHEARSAL_SOURCE_MISSING_OR_DUPLICATE:${rehearsal.source}`);
    const retained = retainedRunners[rehearsal.packageScript];
    if (!retained || retained.source !== rehearsal.source || retained.command !== rehearsal.command || scripts[rehearsal.packageScript] !== rehearsal.command) throw new Error(`PROVIDER_REHEARSAL_RUNNER_MISMATCH:${rehearsal.packageScript}`);
    ids.add(rehearsal.id); runners.add(rehearsal.packageScript); sources.add(rehearsal.source);
  }
  for (const runner of Object.keys(retainedRunners)) if (!runners.has(runner)) throw new Error(`PROVIDER_REHEARSAL_NOT_REGISTERED:${runner}`);
  // A new top-level tests/*.ts executable cannot disappear merely because it
  // does not use the framework .test/.spec naming convention.
  for (const [runner, command] of Object.entries(scripts)) {
    if (!runner.startsWith("test:") || !/^bun(?:\s+--\S+)*\s+tests\//.test(command)) continue;
    const source = command.match(/\b(tests\/[^\s]+\.ts)(?:\s|$)/)?.[1];
    if (source && !/\.(?:test|spec)\.ts$/.test(source) && !sources.has(source)) throw new Error(`EXECUTABLE_TEST_NOT_REGISTERED:${runner}:${source}`);
  }
}

export function loadProviderRehearsals(root: string, modules: readonly string[]): ProviderRehearsal[] {
  const inventory = JSON.parse(readFileSync(resolve(root, "tests/catalogue/provider-rehearsals.json"), "utf8")) as ProviderRehearsalInventory;
  const scripts = (JSON.parse(readFileSync(resolve(root, "package.json"), "utf8")) as { scripts: Record<string, string> }).scripts;
  validateProviderRehearsals(inventory, scripts, source => existsSync(resolve(root, source)), modules);
  return inventory.rehearsals;
}

export function unselectedProviderRehearsals(rehearsals: readonly ProviderRehearsal[]) {
  return rehearsals.map(rehearsal => ({ ...rehearsal, selected: false, executed: false, reason: "Separate opt-in interactive provider rehearsal; internal regression does not attest to provider acceptance." }));
}
