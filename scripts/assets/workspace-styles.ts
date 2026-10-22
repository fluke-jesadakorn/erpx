import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { appDefinitions } from '@erpx/contracts/apps';
import { loadWorkspaceGraph, dependencyClosure } from '../testing/workspace-graph';
const root = resolve(import.meta.dirname, '../..');
export function refreshWorkspaceStyles(check = false) {
// CSS scan owners come from executable imports, rather than package cycles.
// Resource-only @source inputs never cause JavaScript imports to execute.
const styleGraph = loadWorkspaceGraph(root);
for (const app of [...appDefinitions, { id: "compat", directory: "apps/compat" }]) {
  const entries = [...styleGraph.files.keys()].filter(file => file.startsWith(`${app.directory}/src/`));
  const reachable = dependencyClosure({ imports: styleGraph.runtimeImports }, entries);
  const sourcePackages = [...new Set([...reachable].flatMap(file => {
    const owner = /^packages\/([^/]+)\/src\/.*\.[cm]?[jt]sx?$/.exec(file)?.[1];
    return owner && !owner.startsWith("server-") && !["contracts", "catalogue"].includes(owner) ? [owner] : [];
  }))].sort();
  const path = resolve(root, app.directory, "src/app/globals.css"), contents = '@import "@erpx/workplace/routes/globals.css";\n@source "./";\n' + sourcePackages.map(name => `@source "../../../../packages/${name}/src";`).join("\n") + "\n";
  if (check) { if (readFileSync(path, "utf8") !== contents) throw new Error(`WORKSPACE_STYLE_INPUTS_STALE:${app.id}`); }
  else writeFileSync(path, contents);
}
}
if (import.meta.main) refreshWorkspaceStyles(process.argv.includes("--check"));
