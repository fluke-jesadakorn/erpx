import { existsSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { loadCatalogue, repositoryRoot, validateCatalogue, type Suite } from "./catalogue";
import { loadProviderRehearsals } from "./provider-rehearsals";

/** File placement follows the declared lane and owner, including shared owners. */
export function assertTestPlacement(suites: readonly Pick<Suite, "id" | "lane" | "owner" | "files">[]): void {
  for (const suite of suites) {
    if (!/^[a-z][a-z0-9_.-]*$/.test(suite.owner)) throw new Error(`INVALID_TEST_OWNER_PATH:${suite.id}`);
    const prefix = `tests/${suite.lane}/${suite.owner}/`;
    for (const file of suite.files) {
      if (!file.startsWith(prefix) || file.slice(prefix.length).includes("/") || file.split("/").includes(".."))
        throw new Error(`TEST_PLACEMENT_MISMATCH:${suite.id}:${file}:${prefix}`);
    }
  }
}

export function checkRepositoryLayout(root = repositoryRoot): void {
  const catalogue = loadCatalogue(root);
  validateCatalogue(catalogue, root);
  assertTestPlacement(catalogue.suites);
  for (const rehearsal of loadProviderRehearsals(root, catalogue.modules)) {
    if (!rehearsal.source.startsWith(`tests/provider/${rehearsal.owner}/`))
      throw new Error(`PROVIDER_REHEARSAL_PLACEMENT_MISMATCH:${rehearsal.id}`);
  }
  for (const directory of ["tests", "scripts", "src/lib"]) {
    for (const entry of readdirSync(resolve(root, directory), { withFileTypes: true })) {
      if (entry.isFile() && /\.(?:[cm]?[jt]sx?|json)$/.test(entry.name))
        throw new Error(`UNCATEGORIZED_REPOSITORY_FILE:${directory}/${entry.name}`);
    }
  }
  const mixedComponents = resolve(root, "src/components/modules");
  if (existsSync(mixedComponents) && readdirSync(mixedComponents).length)
    throw new Error("MIXED_COMPONENT_OWNER_DIRECTORY");
  function checkServer(directory: string): void {
    for (const entry of readdirSync(resolve(root, directory), { withFileTypes: true })) {
      const file = `${directory}/${entry.name}`;
      if (file === "src/server/modules") continue;
      if (entry.isDirectory()) checkServer(file);
      else if (/\.tsx?$/.test(file) && file !== "src/server/runtime/env.ts")
        throw new Error(`LEGACY_SERVER_LOCATION:${file}`);
    }
  }
  checkServer("src/server");
}

if (import.meta.main) {
  checkRepositoryLayout();
  console.log("Repository layout passed: test lanes/owners, providers and support categories.");
}
