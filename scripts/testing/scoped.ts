import { refreshWorkspaceStyles } from '../assets/workspace-styles';
import { createHash } from 'node:crypto';
import { selectedTypecheckConfig, selectedTypecheckInputs } from './selected-typecheck';
import { vitestChildEnvironment } from './runner-environment';
import { existsSync, mkdirSync, readFileSync, writeFileSync, watch } from 'node:fs';
import { resolve } from 'node:path';
import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import { appDefinition } from '@erpx/contracts/apps';
import { verificationSources, loadCatalogue, validateCatalogue, repositoryRoot as root } from './catalogue';
import { dependencyClosure, workspaceInputFile, changedWorkspaceFiles, loadRevisionWorkspaceGraph, loadWorkspaceGraph, selectCandidateSuites, selectWorkspaceSuites, workspaceFingerprint } from './workspace-graph';
import { pinnedNode } from '../local/app-task';
import { refreshWorkspaceTaskInputs } from './workspace-tasks';

type Options={mode:'dev'|'ci'|'build';apps:string[];base?:string;head?:string;list:boolean;json:boolean;watch:boolean};
export function scopedArguments(input:string[]):Options {
 const args=input.filter(arg=>arg!=='--'),mode=args.shift()??'dev';if(!['dev','ci','build'].includes(mode))throw new Error('UNKNOWN_WORKSPACE_PROFILE');
 const options:Options={mode:mode as Options['mode'],apps:[],list:false,json:false,watch:false};
 for(const arg of args){if(['--list','--json','--watch'].includes(arg)){options[arg.slice(2) as 'list'|'json'|'watch']=true;continue;}
  const match=/^--(app|base|head)=(.+)$/.exec(arg);if(!match)throw new Error(`UNKNOWN_WORKSPACE_TEST_ARGUMENT:${arg}`);
  if(match[1]==='app')for(const id of match[2].split(',')){appDefinition(id);options.apps.push(id);}else options[match[1] as 'base'|'head']=match[2];
 }
 if(mode==='ci'&&(!options.base||!options.head))throw new Error('EXACT_CI_BASE_AND_HEAD_REQUIRED');
 if(options.watch&&(mode!=='dev'||options.list||options.json))throw new Error('WATCH_REQUIRES_EXECUTING_DEVELOPMENT_PROFILE');
 return options;
}
export function scopedPlan(options:Options){
 const catalogue=loadCatalogue();validateCatalogue(catalogue);
 for(const suite of catalogue.suites){if(!suite.apps || !suite.packages)throw new Error(`WORKSPACE_SUITE_OWNERSHIP_REQUIRED:${suite.id}`);if(suite.lane==='integration'&&!suite.postgres?.slots.length)throw new Error(`POSTGRES_SUITE_PROFILE_REQUIRED:${suite.id}`);}
 const current=loadWorkspaceGraph();
 for(const suite of catalogue.suites)for(const file of suite.verificationInputs??[])if(!current.files.has(file))throw new Error(`WORKSPACE_VERIFICATION_INPUT_UNMAPPED:${suite.id}:${file}`);
 if(options.head){const sha=execFileSync('git',['rev-parse','--verify',`${options.head}^{commit}`],{cwd:root,encoding:'utf8'}).trim();
  if(!options.list&&!options.json){const checked=execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim();if(sha!==checked||execFileSync('git',['status','--porcelain','--untracked-files=normal'],{cwd:root,encoding:'utf8'}).trim())throw new Error('CI_REQUIRES_CLEAN_CHECKED_OUT_CANDIDATE');}}
 const graph=options.head?loadRevisionWorkspaceGraph(options.head):current;
 const changed=options.apps.length?[]:changedWorkspaceFiles(root,options.base??'HEAD',options.head);
 const selection=options.apps.length?{...selectWorkspaceSuites(graph,catalogue.suites,{apps:options.apps}),changed,removed:[]}:selectCandidateSuites(graph,loadRevisionWorkspaceGraph(options.base??'HEAD'),catalogue.suites,changed);
 const native=selection.suites.filter(suite=>suite.lane==='unit'||suite.lane==='integration');
 const browser=selection.suites.filter(suite=>suite.lane==='e2e'||suite.lane==='journeys');
 return {graph,plan:{version:1,...selection,scope:options.mode==='ci'?'candidate-verification':'development',acceptance:false as const,base:options.base??'HEAD',candidate:options.head??null,native,browser,requiredAcceptance:'bun run release:acceptance --candidate=<clean-sha>'}};
}
let active:ChildProcess|undefined,stopping=false;
async function execute(args:string[]){
 if(stopping)throw new Error('DEVELOPMENT_VERIFICATION_CANCELLED');
 const child=spawn(args[0],args.slice(1),{cwd:root,env:process.env,stdio:'inherit',detached:process.platform!=='win32'});active=child;
 try{const code=await new Promise<number|null>((done,reject)=>{child.once('error',reject);child.once('exit',done);});if(code!==0)throw new Error(`SCOPED_VERIFICATION_FAILED:${args[1]}:${code}`);}finally{if(active===child)active=undefined;}
}
function cancel(){stopping=true;if(active?.pid){try{process.kill(process.platform==='win32'?active.pid:-active.pid,'SIGTERM');}catch{/* already exited */}}}
async function pureTask(key:string,run:()=>Promise<void>){
 const directory=resolve(root,'.scratch/dev/test-suites/task-cache');mkdirSync(directory,{recursive:true,mode:0o700});const path=resolve(directory,key+'.json');
 if(existsSync(path)){const receipt=JSON.parse(readFileSync(path,'utf8'));if(receipt.version===1&&receipt.key===key&&receipt.passed===true){console.log('PURE_TASK_CACHE_HIT:'+key.slice(0,12));return;}}
 await run();writeFileSync(path,JSON.stringify({version:1,key,passed:true,acceptance:false})+'\n',{mode:0o600});
}
export async function runScoped(options:Options){
 const {graph,plan}=scopedPlan(options);
 if(options.list||options.json){console.log(JSON.stringify(plan,null,options.json?0:2));return;}
 refreshWorkspaceStyles(true);
 const directory=resolve(root,'.scratch/dev/test-suites/workspaces',new Date().toISOString().replace(/[:.]/g,'-'));mkdirSync(directory,{recursive:true,mode:0o700});
 const entries=verificationSources(plan.suites).concat(plan.apps.flatMap(id=>[...graph.apps.get(id)!]));
 const fingerprint=workspaceFingerprint(graph,entries,['scripts/testing/scoped.ts','scripts/testing/catalogue.ts','tests/catalogue/suite-manifest.json','eslint.config.mjs']);
 let passed=false;const started=Date.now();
 try{
  if(options.mode==='build'){
   refreshWorkspaceTaskInputs();
   if(plan.apps.length)await execute([process.execPath,resolve(root,'node_modules/turbo/bin/turbo'),'run','build',...plan.apps.map(id=>`--filter=@erpx/app-${id}`),'--cache=local:rw','--concurrency=1']);
  }else{
   await execute([process.execPath,'scripts/testing/test.ts','--check']);
   await execute([process.execPath,'scripts/testing/layout.ts']);
   const tools=plan.changed.filter(file=>/^scripts\//.test(file)&&/\.[cm]?[jt]sx?$/.test(file)&&existsSync(resolve(root,file)));
   if(tools.length){
    await execute([pinnedNode(),'node_modules/typescript/bin/tsc','--noEmit','--project','tsconfig.json']);
    await execute([pinnedNode(),'node_modules/eslint/bin/eslint.js',...tools]);
   }
   if(plan.apps.length || plan.changed.some(file=>/^(?:packages|src|apps|scripts\/architecture)\//.test(file)))await execute([process.execPath,'scripts/architecture/check.ts']);
   if(plan.changed.some(file=>file.startsWith('docs/')||file.endsWith('.md')))await execute([process.execPath,'scripts/docs/check-documentation.ts']);
   refreshWorkspaceTaskInputs();
   if(plan.apps.length)await execute([process.execPath,resolve(root,'node_modules/turbo/bin/turbo'),'run','lint','typecheck',...plan.apps.map(id=>`--filter=@erpx/app-${id}`),'--cache=local:rw','--concurrency=2','--output-logs=errors-only']);
   const units=plan.native.filter(suite=>suite.lane==='unit'),integration=plan.native.filter(suite=>suite.lane==='integration');
   const testFiles=[...plan.native,...plan.browser].flatMap(suite=>suite.files);
   if(testFiles.length){
    const key=workspaceFingerprint(graph,[...selectedTypecheckInputs(testFiles),...verificationSources(plan.native)],['scripts/testing/selected-typecheck.ts','tests/config/tsconfig.json','tsconfig.base.json']);
    await pureTask('test-types-'+key,()=>execute([pinnedNode(),'--max-old-space-size=8192','node_modules/typescript/bin/tsc','--noEmit','--project',selectedTypecheckConfig(testFiles)]));
   }
   if(units.length){const sourceKey=workspaceFingerprint(graph,verificationSources(units),['scripts/testing/test.ts','scripts/testing/vitest-config.ts','tests/config/vitest.unit.config.mts','scripts/testing/required-cases-reporter.ts']);const key=createHash('sha256').update(sourceKey).update(JSON.stringify(Object.entries(vitestChildEnvironment(process.env,false)).sort(([a],[b])=>a.localeCompare(b)))).digest('hex');await pureTask(key,()=>execute([process.execPath,'scripts/testing/test.ts','unit',...units.map(suite=>`--suite=${suite.id}`)]));}
   // External state is always exercised. Never reuse a passing integration receipt.
   for(const group of [integration.filter(suite=>!suite.serial),...integration.filter(suite=>suite.serial).map(suite=>[suite])])if(group.length)await execute([process.execPath,'scripts/testing/test.ts','integration',...group.map(suite=>`--suite=${suite.id}`)]);
   if(plan.browser.length)await execute([process.execPath,'scripts/testing/development-smoke.ts',...plan.browser.map(suite=>`--suite=${suite.id}`)]);
  }
  if(stopping)throw new Error('DEVELOPMENT_VERIFICATION_CANCELLED');
  if(workspaceFingerprint(loadWorkspaceGraph(),entries,['scripts/testing/scoped.ts','scripts/testing/catalogue.ts','tests/catalogue/suite-manifest.json','eslint.config.mjs'])!==fingerprint)throw new Error('SCOPED_VERIFICATION_SOURCE_CHANGED');
  passed=true;
 }finally{writeFileSync(resolve(directory,'receipt.json'),JSON.stringify({...plan,passed,fingerprint,durationMs:Date.now()-started,acceptance:false},null,2)+'\n',{mode:0o600});console.log(JSON.stringify({scope:plan.scope,acceptance:false,passed,receipt:resolve(directory,'receipt.json')}));}
}
async function watchScoped(options:Options){
 let running=false,pending=true,terminated=false,timer:ReturnType<typeof setTimeout>|undefined;
 let watchedInputs:Set<string>|undefined;
 const refreshInputs=()=>{
  if(!options.apps.length)return;
  const {graph,plan}=scopedPlan(options);
  watchedInputs=new Set([...dependencyClosure(graph,[...plan.apps.flatMap(id=>[...graph.apps.get(id)!]),...verificationSources(plan.suites)])].map(workspaceInputFile));
 };
 const relevant=(path:string)=>!watchedInputs || watchedInputs.has(path) || options.apps.some(id=>path.startsWith(appDefinition(id).directory+'/')) || /^(?:tests\/catalogue|scripts\/(?:testing|architecture|local|assets))\//.test(path);
 const run=async()=>{if(terminated||running||!pending)return;running=true;pending=false;stopping=false;try{refreshInputs();await runScoped({...options,watch:false});}catch(error){console.error(error instanceof Error?error.message:String(error));}finally{running=false;if(pending&&!terminated)void run();}};
 const changed=()=>{pending=true;if(running)cancel();if(timer)clearTimeout(timer);timer=setTimeout(()=>void run(),300);};
 const watchers=['apps','packages','src','tests','scripts','fixtures','migrations','public','docs','ops','.github'].map(directory=>watch(resolve(root,directory),{recursive:true},(_,name)=>{
  if(!name||/(?:^|\/)(?:node_modules|\.next|\.build|\.cache|\.turbo)(?:\/|$)/.test(name)||/(?:next-env\.d\.ts|tsconfig\.tsbuildinfo|turbo\.json)$/.test(name))return;
  if(relevant(`${directory}/${name}`))changed();
 }));
 const rootInputs=new Set(['package.json','bun.lock','tsconfig.base.json','tsconfig.json','next.config.ts','eslint.config.mjs','postcss.config.mjs','turbo.json']);
 watchers.push(watch(root,(_,name)=>{if(name && rootInputs.has(name))changed();}));
 const finish=()=>{terminated=true;for(const watcher of watchers)watcher.close();if(timer)clearTimeout(timer);pending=false;cancel();};
 process.once('SIGINT',finish);process.once('SIGTERM',finish);
 await run();console.log('Watching workspace inputs. New edits cancel obsolete verification rounds.');
}
if(import.meta.main){
 const options=scopedArguments(process.argv.slice(2));pinnedNode();
 process.once('SIGTERM',cancel);if(!options.watch)process.once('SIGINT',cancel);
 if(options.watch)await watchScoped(options);else await runScoped(options);
}
