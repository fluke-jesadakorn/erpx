import { constants } from 'node:fs';
import { copyFile, lstat, mkdir, readdir, readlink, symlink } from 'node:fs/promises';
import { join } from 'node:path';
import { relative, resolve, sep } from 'node:path';
const privateDirectories = new Set(['.next', '.build', '.cache', '.turbo', '.scratch', '.runtime', '.git']);
/** Source snapshots never inherit local configuration, state or generated output. */
export function workspaceCopyInput(root: string, path: string) {
  const base = resolve(root), target = resolve(path);
  if (target !== base && !target.startsWith(base + sep)) return false;
  return !relative(base, target).split(sep).some(part => privateDirectories.has(part) || /^\.env(?:\.|$)/.test(part) || part === '.DS_Store');
}

/** Copy fresh source snapshots with bounded filesystem work, retaining literal
 * links and pruning rejected directories before their contents are inspected. */
export async function copyWorkspaceSnapshot(source:string,target:string,filter:(path:string)=>boolean){
 const pending=[{source,target}],files:Array<{source:string;target:string;link:boolean}>=[];
 for(let index=0;index<pending.length;index++){
  const current=pending[index];if(!filter(current.source))continue;
  const stat=await lstat(current.source);
  if(stat.isSymbolicLink()){files.push({...current,link:true});continue;}
  if(!stat.isDirectory()){if(!stat.isFile())throw new Error('UNSUPPORTED_SNAPSHOT_FILE');files.push({...current,link:false});continue;}
  await mkdir(current.target,{recursive:true,mode:stat.mode&0o777});
  for(const entry of await readdir(current.source))pending.push({source:join(current.source,entry),target:join(current.target,entry)});
 }
 let next=0;
 const copied=await Promise.allSettled(Array.from({length:Math.min(32,files.length)},async()=>{
  for(let index=next++;index<files.length;index=next++){
   const file=files[index];
   if(file.link)await symlink(await readlink(file.source),file.target);
   else await copyFile(file.source,file.target,constants.COPYFILE_FICLONE|constants.COPYFILE_EXCL);
  }
 }));
 const failed=copied.find(result=>result.status==='rejected');if(failed?.status==='rejected')throw failed.reason;
}
