import { readFileSync, writeFileSync, renameSync, rmSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { resolve } from 'node:path';
import { appDefinitions } from '@erpx/contracts/apps';
import { loadWorkspaceGraph, dependencyClosure, type WorkspaceGraph } from '../testing/workspace-graph';
const root = resolve(import.meta.dirname, '../..');
export function workspaceStyleSheet(graph:Pick<WorkspaceGraph,'files'|'runtimeImports'>,directory:string){
  const entries=[...graph.files.keys()].filter(file=>file.startsWith(`${directory}/src/`));
  const reachable=dependencyClosure({imports:graph.runtimeImports},entries);
  // Scanning a package directory couples unrelated clients belonging to that
  // owner. Exact executable sources include class dictionaries and lazy imports,
  // while type-only and text-resource edges cannot pull in another UI graph.
  const sources=[...reachable].filter(file=>/^packages\/[^/]+\/src\/.+\.[cm]?[jt]sx?$/.test(file)).sort();
  return '@import "@erpx/workplace/routes/globals.css";\n@source "./";\n'+sources.map(file=>`@source ${JSON.stringify('../../../../'+file)};`).join('\n')+'\n';
}
export function refreshWorkspaceStyles(check = false) {
// CSS scan owners come from executable imports, rather than package cycles.
// Resource-only @source inputs never cause JavaScript imports to execute.
// The previous derived list can reference a file just deleted or renamed. It
// cannot be an input to deriving its replacement. Normal task graphs stay strict.
const styleGraph = loadWorkspaceGraph(root,{excludeStyleSources:true});
for (const app of [...appDefinitions, { id: "compat", directory: "apps/compat" }]) {
  const path = resolve(root, app.directory, "src/app/globals.css"), contents=workspaceStyleSheet(styleGraph,app.directory);
  if (check) { if (readFileSync(path, "utf8") !== contents) throw new Error(`WORKSPACE_STYLE_INPUTS_STALE:${app.id}`); }
  else if(readFileSync(path,'utf8')!==contents){
    const temporary=path+'.'+randomUUID()+'.tmp';
    writeFileSync(temporary,contents,{flag:'wx'});
    try{renameSync(temporary,path);}finally{rmSync(temporary,{force:true});}
  }
}
return styleGraph;
}
if (import.meta.main) refreshWorkspaceStyles(process.argv.includes("--check"));
