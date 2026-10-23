import {readFileSync,watch} from 'node:fs';
import {resolve} from 'node:path';
import {parseWorkspaceSource} from '../testing/workspace-imports';
import {refreshWorkspaceStyles} from '../assets/workspace-styles';

export function workspaceStyleWatchInput(file:string){
 return /^(?:apps|packages)\/[^/]+\/(?:src\/.+\.[cm]?[jt]sx?|package\.json)$/.test(file)&&!file.split('/').includes('node_modules');
}
export function workspaceStyleGraphChanged(file:string,previous:string|undefined,current:string|undefined){
 if(previous===current)return false;
 if(previous===undefined||current===undefined||file.endsWith('/package.json'))return true;
 const imports=(text:string)=>JSON.stringify(parseWorkspaceSource(file,text).modules.filter(module=>module.runtime).map(module=>module.specifier).sort());
 return imports(previous)!==imports(current);
}

/** One watcher belongs to the existing development supervisor. Ordinary text
 * and class edits leave the stylesheet untouched; new executable edges refresh
 * all derived lists without importing any business implementation. */
export function watchWorkspaceStyleSources(root:string,initial:ReadonlyMap<string,string>,options:{
 refresh?:()=>unknown; report?:(message:string)=>void; delayMs?:number;
}={}){
 const observed=new Map([...initial].filter(([file])=>workspaceStyleWatchInput(file))),pending=new Set<string>();
 const refresh=options.refresh??(()=>refreshWorkspaceStyles()),report=options.report??console.error;
 let timer:ReturnType<typeof setTimeout>|undefined,closed=false,failed=false;
 const flush=()=>{
  timer=undefined;if(closed)return;
  let changed=failed;
  for(const file of pending){
   let current:string|undefined;
   try{current=readFileSync(resolve(root,file),'utf8');}
   catch(error){if(!['ENOENT','ENOTDIR'].includes((error as NodeJS.ErrnoException).code??'')){failed=true;report(`WORKSPACE_STYLE_SOURCE_READ_FAILED:${file}`);continue;}}
   changed=workspaceStyleGraphChanged(file,observed.get(file),current)||changed;
   if(current===undefined)observed.delete(file);else observed.set(file,current);
  }
  pending.clear();
  if(changed)try{refresh();failed=false;}catch(error){failed=true;report(`WORKSPACE_STYLE_REFRESH_FAILED:${error instanceof Error?error.message:'unknown'}`);}
 };
 const watchers=['apps','packages'].map(directory=>watch(resolve(root,directory),{recursive:true},(_event,name)=>{
  if(closed||!name)return;
  const file=directory+'/'+name.toString().replaceAll('\\','/');
  if(!workspaceStyleWatchInput(file))return;
  pending.add(file);if(timer)clearTimeout(timer);timer=setTimeout(flush,options.delayMs??100);
 }));
 for(const watcher of watchers)watcher.on('error',()=>{failed=true;report('WORKSPACE_STYLE_WATCH_FAILED:restart the development supervisor to restore source tracking');});
 return{close(){if(closed)return;closed=true;if(timer)clearTimeout(timer);for(const watcher of watchers)watcher.close();pending.clear();}};
}
