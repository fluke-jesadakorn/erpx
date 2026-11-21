import {lstatSync,readFileSync,readdirSync,realpathSync,watch} from 'node:fs';
import {isAbsolute,relative,resolve} from 'node:path';
import {parseWorkspaceSource} from '../testing/workspace-imports';
import {refreshWorkspaceStyles} from '../assets/workspace-styles';

const privatePart=(part:string)=>/^\.env(?:\.|$)/.test(part)||['node_modules','.next','.build','.cache','.turbo','.scratch','.runtime','.git'].includes(part);
export function workspaceStyleWatchInput(file:string){
 return /^(?:apps|packages)\/[^/]+\/(?:src\/.+\.[cm]?[jt]sx?|package\.json)$/.test(file)&&!file.split('/').some(privatePart);
}
export function workspaceStyleGraphChanged(file:string,previous:string|undefined,current:string|undefined){
 if(previous===current)return false;
 if(previous===undefined||current===undefined||file.endsWith('/package.json'))return true;
 const imports=(text:string)=>JSON.stringify(parseWorkspaceSource(file,text).modules.filter(module=>module.runtime).map(module=>module.specifier).sort());
 return imports(previous)!==imports(current);
}
export type WorkspaceStyleSubscription=(directory:string,changed:(name:string|null)=>void,failed:()=>void)=>{close():void};
/** The native adapter is also used by the real OS subscription smoke. */
export const subscribeWorkspaceStyleSources:WorkspaceStyleSubscription=(directory,changed,failed)=>{
 const watcher=watch(directory,{recursive:true},(_event,name)=>changed(name?.toString()??null));
 watcher.on('error',failed);return watcher;
};
const missing=(error:unknown)=>['ENOENT','ENOTDIR'].includes((error as NodeJS.ErrnoException).code??'');
function sourceDirectory(file:string){
 const parts=file.split('/');
 return !parts.some(privatePart)&&(['apps','packages'].includes(parts[0])&&parts.length<=2||/^(?:apps|packages)\/[^/]+\/src(?:\/|$)/.test(file));
}

/** One watcher belongs to the existing development supervisor. Reconcile only
 * event scopes (plus one startup handoff), never periodically poll the tree.
 * Sparse OS notifications must include newly added and deleted source paths. */
export function watchWorkspaceStyleSources(root:string,initial:ReadonlyMap<string,string>,options:{
 refresh?:()=>unknown; report?:(message:string)=>void; delayMs?:number; subscribe?:WorkspaceStyleSubscription;
}={}){
 const canonicalRoot=realpathSync(root),observed=new Map([...initial].filter(([file])=>workspaceStyleWatchInput(file)));
 const pending=new Set<string>(),scopes=new Set<string>(),settlers=new Set<()=>void>();
 const refresh=options.refresh??(()=>refreshWorkspaceStyles()),report=options.report??console.error;
 let timer:ReturnType<typeof setTimeout>|undefined,closed=false,failed=false;
 const settle=()=>{for(const done of settlers)done();settlers.clear();};
 const collect=(scope:string)=>{
  for(const file of observed.keys())if(file===scope||file.startsWith(scope+'/'))pending.add(file);
  const visit=(file:string)=>{
   if(!workspaceStyleWatchInput(file)&&!sourceDirectory(file))return;
   try{
    const entry=lstatSync(resolve(canonicalRoot,file));
    if(entry.isSymbolicLink())return;
    if(entry.isFile()){if(workspaceStyleWatchInput(file))pending.add(file);return;}
    if(!entry.isDirectory()||!sourceDirectory(file))return;
    for(const name of readdirSync(resolve(canonicalRoot,file)))visit(file+'/'+name);
   }catch(error){if(!missing(error)){failed=true;report(`WORKSPACE_STYLE_SOURCE_READ_FAILED:${file}`);}}
  };
  visit(scope);
 };
 const flush=()=>{
  timer=undefined;if(closed){settle();return;}
  for(const scope of scopes)collect(scope);scopes.clear();
  let changed=failed;
  for(const file of pending){
   let current:string|undefined;
   try{
    const entry=lstatSync(resolve(canonicalRoot,file));
    if(entry.isFile()&&!entry.isSymbolicLink())current=readFileSync(resolve(canonicalRoot,file),'utf8');
   }catch(error){if(!missing(error)){failed=true;report(`WORKSPACE_STYLE_SOURCE_READ_FAILED:${file}`);continue;}}
   changed=workspaceStyleGraphChanged(file,observed.get(file),current)||changed;
   if(current===undefined)observed.delete(file);else observed.set(file,current);
  }
  pending.clear();
  if(changed)try{
   const graph=refresh() as {files?:unknown}|undefined;
   if(graph?.files instanceof Map){observed.clear();for(const[file,text]of graph.files)if(typeof file==='string'&&workspaceStyleWatchInput(file)&&typeof text==='string')observed.set(file,text);}
   failed=false;
  }catch(error){failed=true;report(`WORKSPACE_STYLE_REFRESH_FAILED:${error instanceof Error?error.message:'unknown'}`);}
  settle();
 };
 const schedule=()=>{if(timer)clearTimeout(timer);timer=setTimeout(flush,options.delayMs??100);};
 const subscribe=options.subscribe??subscribeWorkspaceStyleSources;
 const watchers:ReturnType<WorkspaceStyleSubscription>[]=[];
 try{for(const directory of ['apps','packages'])watchers.push(subscribe(resolve(canonicalRoot,directory),name=>{
  if(closed)return;
  // macOS may report the watched basename, a subtree, or no filename at all.
  // Ambiguous/out-of-scope names reconcile this watched root without following
  // a supplied absolute path or traversing outside the owned repository.
  const normalized=name?.replaceAll('\\','/');
  const file=normalized?relative(canonicalRoot,resolve(canonicalRoot,directory,normalized)).replaceAll('\\','/') : directory;
  if(!normalized||isAbsolute(normalized)||normalized===directory||file!==directory&&!file.startsWith(directory+'/'))scopes.add(directory);
  else if(workspaceStyleWatchInput(file))pending.add(file);
  else if(sourceDirectory(file))scopes.add(file);
  else return;
  schedule();
 },()=>{if(closed)return;failed=true;report('WORKSPACE_STYLE_WATCH_FAILED:restart the development supervisor to restore source tracking');}));}
 catch(error){closed=true;for(const watcher of watchers)watcher.close();throw error;}
 // Close the gap between the caller's initial graph and native subscription.
 // This is a single event-independent startup reconciliation, not a poll loop.
 scopes.add('apps');scopes.add('packages');schedule();
 return{
  whenSettled(){return timer?new Promise<void>(done=>settlers.add(done)):Promise.resolve();},
  close(){if(closed)return;closed=true;if(timer)clearTimeout(timer);timer=undefined;for(const watcher of watchers)watcher.close();pending.clear();scopes.clear();settle();},
 };
}
