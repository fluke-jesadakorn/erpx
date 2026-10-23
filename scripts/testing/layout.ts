import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { loadCatalogue, repositoryRoot, validateCatalogue, type Suite } from "./catalogue";
import { appDefinitions } from "@erpx/contracts/apps";
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
    if (!existsSync(resolve(root, directory))) continue;
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
  if (existsSync(resolve(root, "src/server"))) checkServer("src/server");
  if (existsSync(resolve(root, "apps"))) {
    const registered = new Map([...appDefinitions.map(app => [app.directory, app.package] as const), ["apps/compat", "@erpx/app-compat"]]);
    for (const entry of readdirSync(resolve(root, "apps"), { withFileTypes: true })) {
      if (!entry.isDirectory()) throw new Error(`UNCATEGORIZED_APP_ROOT:apps/${entry.name}`);
      if (!registered.has(`apps/${entry.name}`)) throw new Error(`UNREGISTERED_APP:apps/${entry.name}`);
    }
    for (const [directory, name] of registered) {
      for (const file of ["package.json", "next.config.mjs", "tsconfig.json", "src/app/layout.tsx", "src/app/page.tsx", "src/proxy.ts"])
        if (!existsSync(resolve(root, directory, file))) throw new Error(`APP_COMPOSITION_FILE_REQUIRED:${directory}:${file}`);
      const manifest = JSON.parse(readFileSync(resolve(root, directory, "package.json"), "utf8"));
      if (manifest.name !== name || manifest.private !== true) throw new Error(`APP_PACKAGE_IDENTITY_CHANGED:${directory}`);
    }
    for (const entry of readdirSync(resolve(root, "packages"), { withFileTypes: true }).filter(entry => entry.isDirectory())) {
      const directory = resolve(root, "packages", entry.name);
      const manifest = JSON.parse(readFileSync(resolve(directory, "package.json"), "utf8"));
      if (manifest.name !== `@erpx/${entry.name}` || manifest.private !== true || !manifest.exports)
        throw new Error(`WORKSPACE_PACKAGE_IDENTITY_CHANGED:${entry.name}`);
      if (entry.name.startsWith("server-")) for (const folder of readdirSync(resolve(directory, "src"), { withFileTypes: true })) {
        if (folder.isDirectory() && !["domain", "application", "adapters", "public"].includes(folder.name))
          throw new Error(`SERVER_OWNER_PLACEMENT:${entry.name}/${folder.name}`);
      }
    }
  }
}

if (import.meta.main) {
  checkRepositoryLayout();
  console.log("Repository layout passed: test lanes/owners, providers and support categories.");
}
