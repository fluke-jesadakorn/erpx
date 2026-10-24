import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { appDefinitions, type AppDefinition } from '@erpx/contracts/apps';
import { dependencyClosure, loadWorkspaceGraph, workspaceInputFile, type WorkspaceGraph } from './workspace-graph';
import { repositoryRoot } from './catalogue';

export function workspaceTaskDefinition(graph: WorkspaceGraph, app: AppDefinition) {
 const sourceFiles=(files:Iterable<string>)=>[...files].filter(file=>!file.startsWith('tests/')&&!file.startsWith('fixtures/')).map(workspaceInputFile).sort();
 const sources=sourceFiles(dependencyClosure(graph,graph.apps.get(app.id)!));
 const entries=[...graph.files.keys()].filter(file=>file.startsWith(`${app.directory}/src/`)||file===`${app.directory}/next.config.mjs`||file===`${app.directory}/package.json`);
 const runtimeSources=sourceFiles(dependencyClosure({imports:graph.runtimeImports},entries));
 const common=['package.json','bun.lock','tsconfig.base.json','next.config.ts','postcss.config.mjs','scripts/local/app-next-config.ts'];
 // The launcher reads compiler selection from settings. Hash that file for the
 // outer task; the inner artifact cache still keys only the selected compiler.
 const compiler=['scripts/local/app-task.ts','scripts/local/workspace-settings.json','scripts/local/isolated-app-build.ts','scripts/local/standalone-links.ts','scripts/local/app-artifact-cache.ts','scripts/local/workspace-copy-input.ts','scripts/assets/prepare-pdf-assets.ts'];
 const verification=['scripts/testing/workspace-graph.ts','scripts/testing/workspace-imports.ts','scripts/testing/workspace-source-cache.ts','scripts/testing/scoped-lint.ts','scripts/testing/app-typecheck.ts'];
 const literal=(file:string)=>file.replace(/[\[\]{}?*!]/g,'\\$&');
 const inputs=(files:string[])=>['$TURBO_DEFAULT$',...[...new Set(files)].map(file=>`$TURBO_ROOT$/${literal(file)}`),'!.cache/**','!.turbo/**','!.next/**','!.build/**','!node_modules/**','!next-env.d.ts','!turbo.json','!AGENTS.md'];
 const verificationInputs=inputs([...sources,...common,...verification]);
 const runtimeInputs=inputs([...runtimeSources,...common,...compiler,...[...graph.files.keys()].filter(file=>/^(?:public|migrations)\//.test(file))]);
 // A new type result may be required while the inner artifact cache still reuses
 // an identical runtime. Verification helpers are not runtime compiler inputs.
 return {extends:['//'],tasks:{typecheck:{inputs:[...verificationInputs,'$TURBO_ROOT$/node_modules/typescript/package.json'],outputs:['.cache/typecheck.tsbuildinfo']},lint:{inputs:[...verificationInputs,'$TURBO_ROOT$/eslint.config.mjs'],outputs:[]},build:{dependsOn:['typecheck'],inputs:runtimeInputs,outputs:['.build/**'],env:['NEXT_PUBLIC_*','NEXT_DEPLOYMENT_ID'],passThroughEnv:['ERPX_NODE_BIN']}}};
}
/** Explicit file inputs avoid treating unrelated owners or verifiers as runtime. */
export function refreshWorkspaceTaskInputs(check=false) {
 const root=repositoryRoot,graph=loadWorkspaceGraph(root);
 for(const app of appDefinitions){
  const path=resolve(root,app.directory,'turbo.json'),text=JSON.stringify(workspaceTaskDefinition(graph,app),null,2)+'\n';
  if(check){if(readFileSync(path,'utf8')!==text)throw new Error(`WORKSPACE_TASK_INPUTS_STALE:${app.id}`);}else writeFileSync(path,text);
 }
}
if(import.meta.main)refreshWorkspaceTaskInputs(process.argv.includes('--check'));
