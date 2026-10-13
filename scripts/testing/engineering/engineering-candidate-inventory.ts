/** Verify the complete retained candidate before placing it in a shareable archive. */
import {createHash} from 'node:crypto';
import {lstat,readFile,readdir,readlink,realpath} from 'node:fs/promises';
import {isAbsolute,join,resolve,sep} from 'node:path';

const sha=(bytes:Buffer)=>createHash('sha256').update(bytes).digest('hex');
const privatePath=(path:string)=>path.split('/').some(part=>/^(?:\.env(?:\..*)?|\.git|\.runtime|\.scratch|\.cache)$/.test(part)&&part!=='.env.example')
 ||/(?:^|\/)\.next\/cache(?:\/|$)/.test(path)||/(?:^|\/)(?:databases\.json|state\.json|[^/]*-credentials\.json)$/.test(path);

export async function verifyEngineeringCandidateInventory(requested:string,fileManifestSha256:string){
 const directory=await realpath(requested),manifestBytes=await readFile(join(directory,'release-files.json'));
 if(!/^[a-f0-9]{64}$/.test(fileManifestSha256)||sha(manifestBytes)!==fileManifestSha256)throw new Error('CANDIDATE_INVENTORY_MANIFEST_MISMATCH');
 const manifest:unknown=JSON.parse(manifestBytes.toString());
 if(!manifest||typeof manifest!=='object'||Array.isArray(manifest))throw new Error('CANDIDATE_INVENTORY_MANIFEST_REQUIRED');
 const entries=Object.entries(manifest);
 for(const [path,digest] of entries)if(isAbsolute(path)||path.split('/').some(part=>!part||part==='.'||part==='..')||privatePath(path)||typeof digest!=='string'||!/^[a-f0-9]{64}$/.test(digest))throw new Error('CANDIDATE_INVENTORY_UNSAFE_MANIFEST');
 const expected=new Map(entries),files:Array<{path:string;sha256:string;bytes:number}>=[],symlinks:Array<{path:string;target:string}>=[],physicalFiles=new Set<string>();
 async function walk(path:string,ancestors:Set<string>){
  const absolute=join(directory,path),actual=await realpath(absolute);
  if(actual!==directory&&!actual.startsWith(directory+sep))throw new Error('CANDIDATE_INVENTORY_EXTERNAL_LINK');
  if(privatePath(path))throw new Error('CANDIDATE_INVENTORY_PRIVATE_PATH');
  const stat=await lstat(absolute);
  if(stat.isSymbolicLink()){
   const target=await readlink(absolute);if(isAbsolute(target)||!resolve(absolute,'..',target).startsWith(directory+sep))throw new Error('CANDIDATE_INVENTORY_EXTERNAL_LINK');
   symlinks.push({path,target});
  }
  const targetStat=stat.isSymbolicLink()?await lstat(actual):stat;
  if(targetStat.isDirectory()){
   if(ancestors.has(actual))throw new Error('CANDIDATE_INVENTORY_CYCLIC_LINK');
   const next=new Set(ancestors).add(actual);
   for(const name of (await readdir(absolute)).sort())await walk(path?`${path}/${name}`:name,next);
  }else if(targetStat.isFile()){
   if(!expected.has(path)&&path!=='release.json'&&path!=='release-files.json')throw new Error('CANDIDATE_INVENTORY_UNVERIFIED_FILE');
   const bytes=await readFile(absolute),digest=sha(bytes);if(expected.has(path)&&digest!==expected.get(path))throw new Error('CANDIDATE_INVENTORY_FILE_CHANGED');
   files.push({path,sha256:digest,bytes:bytes.length});physicalFiles.add(actual);
  }else throw new Error('CANDIDATE_INVENTORY_UNSUPPORTED_ENTRY');
 }
 await walk('',new Set());
 const found=new Set(files.map(file=>file.path));
 if(found.size!==entries.length+2||entries.some(([path])=>!found.has(path))||!found.has('release.json')||!found.has('release-files.json'))throw new Error('CANDIDATE_INVENTORY_INCOMPLETE');
 return {schemaVersion:1,fileManifestSha256,manifestEntries:entries.length,verifiedFilePaths:files.length,physicalFiles:physicalFiles.size,files:files.sort((a,b)=>a.path.localeCompare(b.path)),symlinks:symlinks.sort((a,b)=>a.path.localeCompare(b.path))};
}
