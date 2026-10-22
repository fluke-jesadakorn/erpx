import type { TestProject } from "vitest/node";
import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { loadCatalogue, repositoryRoot } from "../../scripts/testing/catalogue";
import { databaseTracks, ensureSchemaTemplate, recoverAbandonedSuiteLeases, type NativeFixturePlan, type DatabaseSlot } from "./postgres-templates";

async function fixtureFingerprint(root: string) {
  const hash = createHash("sha256").update("schema-only-native-fixtures/v1");
  for (const directory of ["fixtures", "tests/support"]) {
    const files = (await readdir(join(root, directory), { recursive: true, withFileTypes: true })).filter(entry => entry.isFile()).map(entry => join(entry.parentPath, entry.name)).filter(file => /\.[cm]?[jt]sx?$/.test(file)).sort();
    for (const file of files) hash.update(file.slice(root.length)).update("\0").update(await readFile(file)).update("\0");
  }
  return hash.digest("hex");
}
export default async function setup(project: TestProject) {
  const root = repositoryRoot, selected = JSON.parse(process.env.ERPX_SELECTED_TEST_FILES ?? "[]") as string[];
  const suites = loadCatalogue(root).suites.filter(suite => suite.files.some(file => selected.includes(file)));
  const plan: NativeFixturePlan = { version: 1, root, pid: process.pid, templates: {}, files: {} };
  for (const suite of suites) for (const file of suite.files.filter(file => selected.includes(file))) {
    if (!suite.postgres?.slots.length || suite.postgres.slots.some(slot => !(slot in databaseTracks)) || suite.postgres.migratedSlots.some(slot => !suite.postgres!.slots.includes(slot)) || suite.postgres.mode === "raw" && suite.postgres.migratedSlots.length > 0) throw new Error(`POSTGRES_SUITE_PROFILE_REQUIRED:${suite.id}`);
    plan.files[file] = { mode: suite.postgres.mode, slots: suite.postgres.slots as DatabaseSlot[], migratedSlots: suite.postgres.migratedSlots as DatabaseSlot[] };
  }
  const benchmark = process.env.ERPX_FIXTURE_BENCHMARK_MODE;
  if (benchmark) {
    if (process.env.ERPX_PERFORMANCE_BENCHMARK !== 'workspace-fixtures-v1' || !['legacy-raw','template'].includes(benchmark)) throw new Error('EXPLICIT_FIXTURE_BENCHMARK_REQUIRED');
    const directory=process.env.ERPX_FIXTURE_BENCHMARK_OUTPUT;
    if (!directory?.startsWith(join(root,'.scratch/dev/performance/'))) throw new Error('OWNED_FIXTURE_BENCHMARK_OUTPUT_REQUIRED');
    await mkdir(directory,{recursive:true,mode:0o700});
    plan.benchmark={mode:benchmark as 'legacy-raw'|'template',directory};
    if (benchmark === 'legacy-raw') for (const file of Object.values(plan.files)) { file.mode='raw'; file.slots=Object.keys(databaseTracks) as DatabaseSlot[]; file.migratedSlots=[]; }
  }
  await recoverAbandonedSuiteLeases(root);
  const fingerprint = await fixtureFingerprint(root);
  for (const track of new Set(Object.values(plan.files).filter(file => file.mode === "template").flatMap(file => file.migratedSlots.map(slot => databaseTracks[slot]))))
    plan.templates[track] = await ensureSchemaTemplate(root, track, fingerprint);
  const directory = join(root, ".scratch/dev/test-suites/native"); await mkdir(directory, { recursive: true, mode: 0o700 });
  const manifest = join(directory, `fixtures-${process.pid}-${Date.now()}.json`);
  await writeFile(manifest, JSON.stringify(plan, null, 2) + "\n", { mode: 0o600 });
  project.provide("erpxFixtures", manifest);
}
