import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { appDefinitions } from '@erpx/contracts/apps';
import { dependencyClosure, loadWorkspaceGraph, workspaceInputFile } from './workspace-graph';
import { repositoryRoot } from './catalogue';
/** Explicit file inputs avoid treating an unrelated owner as a runtime input. */
export function refreshWorkspaceTaskInputs(check=false) {
 const root=repositoryRoot,graph=loadWorkspaceGraph(root);
 for(const app of appDefinitions){
  const sources=[...dependencyClosure(graph,[...graph.apps.get(app.id)!])].filter(file=>!file.startsWith('tests/')&&!file.startsWith('fixtures/')).map(workspaceInputFile).sort();
  const shared=['package.json','bun.lock','tsconfig.base.json','scripts/local/app-next-config.ts','scripts/local/app-task.ts','scripts/local/isolated-app-build.ts','scripts/local/standalone-links.ts','scripts/local/app-artifact-cache.ts','scripts/local/workspace-copy-input.ts','scripts/local/workspace-settings.json','scripts/testing/workspace-graph.ts','scripts/testing/scoped-lint.ts','scripts/testing/app-typecheck.ts'];
  const literal=(file:string)=>file.replace(/[\[\]{}?*!]/g,"\\$&");
  const inputs=["$TURBO_DEFAULT$",...[...new Set([...sources,...shared])].map(file=>`$TURBO_ROOT$/${literal(file)}`),"!.cache/**","!.turbo/**","!.next/**","!.build/**","!node_modules/**","!next-env.d.ts"];
  const buildInputs=[...new Set([...inputs,...[...graph.files.keys()].filter(file=>/^(?:public|migrations)\//.test(file)).map(file=>`$TURBO_ROOT$/${file}`)])];
  const value={extends:['//'],tasks:{typecheck:{inputs:[...inputs,'$TURBO_ROOT$/node_modules/typescript/package.json'],outputs:['.cache/typecheck.tsbuildinfo']},lint:{inputs:[...inputs,'$TURBO_ROOT$/eslint.config.mjs'],outputs:[]},build:{inputs:buildInputs,outputs:['.build/**'],env:['NEXT_PUBLIC_*','NEXT_DEPLOYMENT_ID'],passThroughEnv:['ERPX_NODE_BIN']}}};
  const path=resolve(root,app.directory,'turbo.json'),text=JSON.stringify(value,null,2)+'\n';
  if(check){if(readFileSync(path,'utf8')!==text)throw new Error(`WORKSPACE_TASK_INPUTS_STALE:${app.id}`);}else writeFileSync(path,text);
 }
}
if(import.meta.main)refreshWorkspaceTaskInputs(process.argv.includes('--check'));
