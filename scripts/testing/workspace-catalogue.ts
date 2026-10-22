import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { products } from "@erpx/contracts/product-registry";
import { appDefinitions } from "@erpx/contracts/apps";
import { verificationSources, loadCatalogue, repositoryRoot, type Suite } from "./catalogue";
import { dependencyClosure, loadWorkspaceGraph } from "./workspace-graph";

const slots = ["CONTROL_DB", "CONTROL_READ_DB", "H001_DB", "H001_READ_DB", "H002_DB", "H002_READ_DB", "H003_DB", "H003_READ_DB"];
const sharedModules: Record<string, string[]> = {
  account: ["identity_access", "commercial"], apps: ["identity_access", "commercial", "organization"],
  admin: ["identity_access", "commercial", "brand", "organization", "platform_administration", "manage_erpx"],
  booking: ["pms"], portal: ["customer_authority", "customer_360", "support"], "legal-portal": ["legal"], forms: ["marketing"],
};
export function suiteApps(suite: Pick<Suite, "modules">): string[] {
  return appDefinitions.filter(app => {
    const modules = products.find(product => product.id === app.id)?.modules ?? sharedModules[app.id];
    return modules?.some(module => suite.modules.includes(module));
  }).map(app => app.id);
}
export function inferPostgresProfile(files: string[], texts: string[]) {
  const selected = new Set<string>();
  for (const source of texts) {
    for (const match of source.matchAll(/\b(?:env|localEnv)\s*(?:\.\s*(CONTROL(?:_READ)?_DB|H00[1-3](?:_READ)?_DB)|\[\s*["'](CONTROL(?:_READ)?_DB|H00[1-3](?:_READ)?_DB)["']\s*\])/g)) selected.add(match[1] ?? match[2]);
    for (const match of source.matchAll(/\{([^}]+)\}\s*=\s*(?:env|localEnv)\b/g)) for (const slot of slots) if (new RegExp(`\\b${slot}\\b`).test(match[1])) selected.add(slot);
  }
  if (!selected.size) for (const slot of slots) selected.add(slot);
  selected.add("CONTROL_DB");
  for (const holding of ["H001", "H002", "H003"]) if (selected.has(holding + "_DB") || selected.has(holding + "_READ_DB")) { selected.add(holding + "_DB"); selected.add(holding + "_READ_DB"); }
  const raw = files.some(file => /(?:migration|upgrade|recovery|backup|restore)/.test(file)) || texts.some(text => /\bmigrateDatabase\s*\(|["']migrations\/postgres\//.test(text));
  const staff = texts.some(text => text.includes("STAFF_TEST_MIGRATIONS")), control = texts.some(text => text.includes("CONTROL_TEST_MIGRATIONS"));
  const migratedSlots = raw ? [] : slots.filter(slot => selected.has(slot) && (slot === "CONTROL_DB" ? control : /^H00[1-3]_DB$/.test(slot) && staff));
  return { mode: raw || !migratedSlots.length ? "raw" as const : "template" as const, slots: slots.filter(slot => selected.has(slot)), migratedSlots };
}
if (import.meta.main) {
  const root = repositoryRoot, graph = loadWorkspaceGraph(root), catalogue = loadCatalogue(root);
  for (const suite of catalogue.suites) {
    const dependencies = dependencyClosure(graph, verificationSources([suite]));
    suite.apps = suiteApps(suite);
    suite.packages = [...new Set([...dependencies].flatMap(file => /^packages\/([^/]+)\//.exec(file)?.[1] ?? []))].map(name => `@erpx/${name}`).sort();
    if (suite.lane === "integration" || suite.fixture === "postgres") {
      const fixtureTexts = [...dependencies].filter(file => (file.startsWith("tests/") || file.startsWith("fixtures/")) &&
        !["tests/support/runtime-env.ts", "tests/support/runtime-setup.ts", "tests/support/postgres-templates.ts", "tests/support/postgres-global-setup.ts"].includes(file))
        .map(file => graph.files.get(file) ?? "");
      suite.postgres = inferPostgresProfile(suite.files, fixtureTexts);
    }
  }
  const path = resolve(root, "tests/catalogue/suite-manifest.json"), output = JSON.stringify(catalogue, null, 2) + "\n";
  if (process.argv.includes("--check")) { if (readFileSync(path, "utf8") !== output) throw new Error("WORKSPACE_CATALOGUE_STALE"); }
  else writeFileSync(path, output);
  console.log(JSON.stringify({ suites: catalogue.suites.length, postgres: catalogue.suites.filter(suite => suite.postgres).length,
    template: catalogue.suites.filter(suite => suite.postgres?.mode === "template").length,
    slots: catalogue.suites.filter(suite => suite.postgres).reduce((sum, suite) => sum + suite.postgres!.slots.length, 0) }));
}
