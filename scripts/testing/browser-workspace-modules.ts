import { existsSync } from 'node:fs';
import { mkdir, readFile, readdir, realpath, symlink } from 'node:fs/promises';
import { copyWorkspaceSnapshot, workspaceCopyInput } from '../local/workspace-copy-input';
import { basename, dirname, isAbsolute, join, relative, sep } from 'node:path';

async function workspaces(root: string) {
  const found=new Map<string,string>();
  for(const category of ['packages','apps']){
    if(!existsSync(join(root,category)))continue;
    for(const entry of await readdir(join(root,category),{withFileTypes:true})){
      if(!entry.isDirectory())continue;
      const directory=join(root,category,entry.name),manifest=join(directory,'package.json');
      if(!existsSync(manifest))continue;
      const name=JSON.parse(await readFile(manifest,'utf8')).name;
      if(typeof name!=='string'||!/^@erpx\/[^/]+$/.test(name)||found.has(name)||await realpath(directory)!==directory)throw new Error('EXACT_BROWSER_WORKSPACE_PACKAGE_REQUIRED');
      found.set(name,directory);
    }
  }
  if(!found.size)throw new Error('BROWSER_WORKSPACE_PACKAGES_REQUIRED');
  return found;
}
export async function assertBrowserWorkspaceModules(root: string) {
  for(const[name,directory]of await workspaces(root)){
    if(await realpath(join(root,'node_modules',name))!==directory)throw new Error('CASE_LOCAL_WORKSPACE_MODULE_REQUIRED');
  }
}

async function assertCompilerDependencies(root:string) {
  for (const name of ['next','react','react-dom']) {
    const dependency=join(root,'node_modules',name);
    if (existsSync(dependency) && !(await realpath(dependency)).startsWith(root+sep)) throw new Error('CASE_LOCAL_COMPILER_MODULE_REQUIRED');
  }
}

/** Workspace aliases always bind to the case. Turbopack also needs vendor
 * modules inside its root; clone their bytes without broadening that root. */
export async function linkBrowserWorkspaceModules(template: string, target: string, bundler:'webpack'|'turbopack'='webpack') {
  if (!['webpack','turbopack'].includes(bundler)) throw new Error('EXPLICIT_BROWSER_WORKSPACE_COMPILER_REQUIRED');
  const parent=dirname(dirname(template));
  const ownedRoots=[template,target].every(path=>/^[a-f0-9]{12}$/.test(basename(dirname(path)))&&existsSync(join(path,'package.json'))&&existsSync(join(path,'packages')));
  if(!isAbsolute(template)||!isAbsolute(target)||template===target||basename(template)!=='app'||basename(target)!=='app'||dirname(dirname(target))!==parent||basename(parent)!=='browser'||basename(dirname(parent))!=='.runtime'||!ownedRoots)throw new Error('OWNED_BROWSER_WORKSPACE_ROOTS_REQUIRED');
  for(const root of [template,target])if(await realpath(root)!==root||JSON.parse(await readFile(join(root,'package.json'),'utf8')).name!=='erpx')throw new Error('OWNED_BROWSER_WORKSPACE_ROOTS_REQUIRED');
  const source=join(template,'node_modules'),destination=join(target,'node_modules');
  if(await realpath(source)!==source||existsSync(destination))throw new Error('FRESH_BROWSER_NODE_MODULES_REQUIRED');
  const aliases=await readdir(join(source,'@erpx'));
  const original=await workspaces(template),cloned=await workspaces(target);
  if(original.size!==aliases.length||cloned.size!==aliases.length)throw new Error('EXACT_BROWSER_WORKSPACE_ALIAS_REQUIRED');
  for(const name of aliases){
    if(!original.has(`@erpx/${name}`)||!cloned.has(`@erpx/${name}`)||await realpath(join(source,'@erpx',name))!==original.get(`@erpx/${name}`))throw new Error('EXACT_BROWSER_WORKSPACE_ALIAS_REQUIRED');
  }
  if (bundler==='turbopack') await assertCompilerDependencies(template);
  await mkdir(destination,{mode:0o700});
  if (bundler==='turbopack') {
    await copyWorkspaceSnapshot(source,destination,path=>{
      const name=relative(source,path);
      return name!=='@erpx'&&!name.startsWith('@erpx'+sep)&&workspaceCopyInput(template,path);
    });
  } else for(const entry of await readdir(source))if(entry!=='@erpx')await symlink(join(source,entry),join(destination,entry));
  await mkdir(join(destination,'@erpx'),{mode:0o700});
  for(const name of aliases)await symlink(cloned.get(`@erpx/${name}`)!,join(destination,'@erpx',name),'dir');
  await assertBrowserWorkspaceModules(target);
  if (bundler==='turbopack') await assertCompilerDependencies(target);
}
