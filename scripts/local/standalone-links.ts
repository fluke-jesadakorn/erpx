import { readlink, readFile, readdir, realpath, unlink, symlink, lstat } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";

/** Never enumerate through an alias, including an alias to an unrelated tree. */
export async function artifactSymbolicLinks(directory: string): Promise<string[]> {
  const found: string[]=[];
  for(const entry of await readdir(directory,{withFileTypes:true})){
    const path=join(directory,entry.name);
    if(entry.isSymbolicLink())found.push(path);
    else if(entry.isDirectory())found.push(...await artifactSymbolicLinks(path));
  }
  return found;
}

/** Traced dependency aliases must survive moving and deleting the build tree. */
export async function relocateStandaloneLinks(standalone: string, release: string): Promise<number> {
  const sourceRoot = resolve(standalone), releaseRoot = await realpath(release);
  const canonicalSource = await realpath(standalone).catch((error: NodeJS.ErrnoException) => {
    if (error.code !== "ENOENT") throw error;
    return sourceRoot;
  });
  const inside = (file: string, root: string) => file === root || file.startsWith(root + sep);
  const entries = await artifactSymbolicLinks(releaseRoot);
  let count = 0;
  for (const file of entries) {
    const target = resolve(dirname(file), await readlink(file));
    const source = [sourceRoot, canonicalSource].find(root => inside(target, root));
    const relocated = source ? join(releaseRoot, relative(source, target)) : target;
    if (!inside(relocated, releaseRoot)) throw new Error(`STANDALONE_EXTERNAL_LINK:${relative(releaseRoot, file)}`);
    const replacement = relative(dirname(file), relocated);
    await unlink(file);
    await symlink(replacement, file);
    const actual = await realpath(file);
    if (!inside(actual, releaseRoot)) throw new Error(`STANDALONE_EXTERNAL_LINK:${relative(releaseRoot, file)}`);
    count++;
  }
  return count;
}

/** Turbopack's intermediate aliases must use the traced runtime copy as well. */
export async function relocateCompilerDependencyAliases(compiled: string, output: string, sourceModules: string) {
  const sourceRoot=await realpath(sourceModules),outputRoot=await realpath(output),compilerRoot=await realpath(compiled);
  const aliases=join(outputRoot,'node_modules'),standalone=join(outputRoot,'standalone');
  try{await lstat(aliases);}catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return 0;throw error;}
  if(await realpath(aliases)!==aliases||await realpath(standalone)!==standalone)throw new Error('COMPILER_ALIAS_ROOT_REQUIRED');
  const plan:{file:string;target:string}[]=[];
  for(const file of await artifactSymbolicLinks(aliases)){
    const original=join(compilerRoot,relative(outputRoot,file));
    if(!(await lstat(original)).isSymbolicLink())throw new Error('ORIGINAL_COMPILER_ALIAS_REQUIRED');
    const source=resolve(dirname(original),await readlink(original));
    const logicalRoot=[resolve(sourceModules),sourceRoot].find(root=>source.startsWith(root+sep));
    if(!logicalRoot||!(await realpath(source)).startsWith(sourceRoot+sep))throw new Error('COMPILER_ALIAS_SOURCE_OUTSIDE_SNAPSHOT');
    const target=join(standalone,'node_modules',relative(logicalRoot,source));
    if(!(await realpath(target)).startsWith(standalone+sep))throw new Error('COMPILER_ALIAS_RUNTIME_REQUIRED');
    const before=JSON.parse(await readFile(join(source,'package.json'),'utf8')),after=JSON.parse(await readFile(join(target,'package.json'),'utf8'));
    if(!before.name||!before.version||before.name!==after.name||before.version!==after.version)throw new Error('COMPILER_ALIAS_PACKAGE_IDENTITY_CHANGED');
    plan.push({file,target});
  }
  for(const{file,target}of plan){await unlink(file);await symlink(relative(dirname(file),target),file);}
  return plan.length;
}
