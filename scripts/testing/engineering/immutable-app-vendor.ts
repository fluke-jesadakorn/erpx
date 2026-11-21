/** Test-only physical vendor mount used for isolated compiled Service probes. */
import {createHash} from 'node:crypto';
import {createReadStream} from 'node:fs';
import {lstat,readFile,readdir,readlink,realpath,writeFile} from 'node:fs/promises';
import {join,relative,resolve,sep} from 'node:path';
import {pathToFileURL} from 'node:url';
import {runtimeSourceFingerprint} from '../../local/source-fingerprint';
import {workspaceCopyInput} from '../../local/workspace-copy-input';

async function shaFile(path:string,hash:ReturnType<typeof createHash>){for await(const chunk of createReadStream(path))hash.update(chunk as Buffer);}
export async function immutableAppVendorDigest(root:string){
 const hash=createHash('sha256'),pending=[root];
 while(pending.length){const dir=pending.pop()!,entries=await readdir(dir,{withFileTypes:true});entries.sort((a,b)=>a.name.localeCompare(b.name,'en'));
  for(const e of entries){const path=join(dir,e.name),name=relative(root,path);hash.update(name).update('\0');if(e.isSymbolicLink())hash.update('link\0').update(await readlink(path));else if(e.isDirectory()){hash.update('dir\0');pending.push(path);}else if(e.isFile()){hash.update('file\0');await shaFile(path,hash);}else throw Error('ENGINEERING_VENDOR_ENTRY_INVALID');hash.update('\0');}
 }
 return hash.digest('hex');
}
async function directReadOnlyMount(path:string){
 const lines=(await readFile('/proc/self/mountinfo','utf8')).split('\n');
 return lines.some(line=>{const [left]=line.split(' - '),parts=left?.split(' ')??[];return parts[4]===path&&parts[5]?.split(',').includes('ro');});
}
async function validateVendorClosure(modules:string,caseSource:string){
 const lines=(await readFile('/proc/self/mountinfo','utf8')).split('\n');
 for(const line of lines){const [left]=line.split(' - '),parts=left?.split(' ')??[],mount=parts[4];if(mount?.startsWith(modules+'/')&&!parts[5]?.split(',').includes('ro'))throw Error('ENGINEERING_VENDOR_WRITABLE_SUBMOUNT');}
 const pending=[modules];while(pending.length){const dir=pending.pop()!;for(const e of await readdir(dir,{withFileTypes:true})){const path=join(dir,e.name);if(e.isDirectory())pending.push(path);else if(e.isSymbolicLink()){
  const target=await realpath(path),insideVendor=target.startsWith(modules+sep),workspaceLink=path.startsWith(join(modules,'@erpx')+sep)&&[join(caseSource,'packages')+sep,join(caseSource,'apps')+sep].some(prefix=>target.startsWith(prefix));
  if(!insideVendor&&!workspaceLink)throw Error('ENGINEERING_VENDOR_LINK_OUTSIDE_CASE');
 }}}
}
async function validateSourceClosure(root:string){
 const links:string[]=[],pending=[root];while(pending.length){const dir=pending.pop()!;for(const e of await readdir(dir,{withFileTypes:true})){const path=join(dir,e.name);if(e.name==='node_modules'||!workspaceCopyInput(root,path))continue;if(e.isDirectory())pending.push(path);else if(e.isSymbolicLink()){const target=await realpath(path);if(!target.startsWith(root+sep)||!workspaceCopyInput(root,target)||relative(root,target).split(sep).includes('node_modules'))throw Error('ENGINEERING_SOURCE_LINK_OUTSIDE_CASE');links.push(JSON.stringify([relative(root,path),await readlink(path),relative(root,target)]));}}}
 return createHash('sha256').update(links.sort().join('\n')).digest('hex');
}
/** The host launcher must first establish that no RW vendor mount is active. */
export async function prepareImmutableAppVendor(caseRoot:string,nonce:string){
 const base=resolve(caseRoot),source=join(base,'source'),snapshot=join(base,'build'),info=await lstat(base);
 if(process.platform!=='linux'||process.arch!=='arm64'||Bun.version!=='1.4.3'||!/^[a-f0-9]{32}$/.test(nonce)||await realpath(base)!==base||!info.isDirectory()||info.isSymbolicLink()||(info.mode&0o077)!==0||info.uid!==process.getuid?.()||await realpath(source)!==source||await realpath(snapshot)!==snapshot)throw Error('ENGINEERING_BUILD_CASE_INVALID');
 const modules=join(snapshot,'node_modules'),sourceModules=join(source,'node_modules'),a=await lstat(modules),b=await lstat(sourceModules);
 if(!a.isDirectory()||a.isSymbolicLink()||!b.isDirectory()||b.isSymbolicLink()||await realpath(modules)!==modules||await realpath(sourceModules)!==sourceModules||!await directReadOnlyMount(modules)||!await directReadOnlyMount(sourceModules)||a.dev!==b.dev||a.ino!==b.ino)throw Error('ENGINEERING_VENDOR_READ_ONLY_MOUNT_REQUIRED');
 await validateVendorClosure(modules,snapshot);await validateVendorClosure(sourceModules,source);const sourceLinksSha256=await validateSourceClosure(source);if(await validateSourceClosure(snapshot)!==sourceLinksSha256)throw Error('ENGINEERING_SOURCE_LINKS_CHANGED');
 const authoredSourceFingerprint=await runtimeSourceFingerprint(source);if(await runtimeSourceFingerprint(snapshot)!==authoredSourceFingerprint)throw Error('ENGINEERING_BUILD_SOURCE_CHANGED');
 for(const tree of [source,snapshot]){const preparation=await import(pathToFileURL(join(tree,'scripts/assets/prepare-pdf-assets.ts')).href);await preparation.preparePdfAssets();}
 const sourceFingerprint=await runtimeSourceFingerprint(source);if(await runtimeSourceFingerprint(snapshot)!==sourceFingerprint)throw Error('ENGINEERING_BUILD_SOURCE_CHANGED');
 const pdfAssetsSha256=await immutableAppVendorDigest(join(source,'public/vendor/pdfjs-6.4.299'));if(await immutableAppVendorDigest(join(snapshot,'public/vendor/pdfjs-6.4.299'))!==pdfAssetsSha256)throw Error('ENGINEERING_PDF_ASSETS_CHANGED');
 const lockSha256=createHash('sha256').update(await readFile(join(snapshot,'bun.lock'))).digest('hex'),vendorSha256=await immutableAppVendorDigest(modules);
 const identity={version:1,nonce,authoredSourceFingerprint,sourceFingerprint,sourceLinksSha256,pdfAssetsSha256,lockSha256,vendorSha256};await writeFile(join(base,'build-identity.json'),JSON.stringify(identity),{mode:0o600,flag:'wx'});return identity;
}
export async function validateImmutableAppVendor(root:string,caseRoot:string,buildSource:string,nonce:string){
 if(process.platform!=='linux'||process.arch!=='arm64'||Bun.version!=='1.4.3')throw Error('ENGINEERING_BUILD_TOOLCHAIN_INVALID');
 const base=resolve(caseRoot),source=resolve(root),snapshot=resolve(buildSource),info=await lstat(base);
 if(await realpath(base)!==base||!info.isDirectory()||info.isSymbolicLink()||(info.mode&0o077)!==0||info.uid!==process.getuid?.()||source!==join(base,'source')||snapshot!==join(base,'build')||await realpath(source)!==source||await realpath(snapshot)!==snapshot)throw Error('ENGINEERING_BUILD_CASE_INVALID');
 const identityPath=join(base,'build-identity.json'),identityInfo=await lstat(identityPath);
 if(!identityInfo.isFile()||identityInfo.isSymbolicLink()||(identityInfo.mode&0o777)!==0o600||identityInfo.uid!==process.getuid?.()||identityInfo.size>4096||await realpath(identityPath)!==identityPath)throw Error('ENGINEERING_BUILD_IDENTITY_INVALID');
 const identity=JSON.parse(await readFile(identityPath,'utf8')) as {version:number;nonce:string;sourceFingerprint:string;sourceLinksSha256:string;lockSha256:string;vendorSha256:string};
 if(identity.version!==1||!/^[a-f0-9]{32}$/.test(nonce)||identity.nonce!==nonce||!/^[a-f0-9]{32}$/.test(identity.nonce)||!/^[a-f0-9]{64}$/.test(identity.sourceFingerprint)||!/^[a-f0-9]{64}$/.test(identity.lockSha256)||!/^[a-f0-9]{64}$/.test(identity.vendorSha256))throw Error('ENGINEERING_BUILD_IDENTITY_INVALID');
 const modules=join(snapshot,'node_modules'),sourceModules=join(source,'node_modules'),modulesInfo=await lstat(modules),sourceModulesInfo=await lstat(sourceModules);
 if(!modulesInfo.isDirectory()||modulesInfo.isSymbolicLink()||await realpath(modules)!==modules||!await directReadOnlyMount(modules)||!sourceModulesInfo.isDirectory()||sourceModulesInfo.isSymbolicLink()||await realpath(sourceModules)!==sourceModules||!await directReadOnlyMount(sourceModules)||modulesInfo.dev!==sourceModulesInfo.dev||modulesInfo.ino!==sourceModulesInfo.ino)throw Error('ENGINEERING_VENDOR_READ_ONLY_MOUNT_REQUIRED');
 await validateVendorClosure(modules,snapshot);await validateVendorClosure(sourceModules,source);if(await validateSourceClosure(source)!==identity.sourceLinksSha256||await validateSourceClosure(snapshot)!==identity.sourceLinksSha256)throw Error('ENGINEERING_SOURCE_LINKS_CHANGED');
 const lock=createHash('sha256').update(await readFile(join(snapshot,'bun.lock'))).digest('hex');
 if(lock!==identity.lockSha256||await runtimeSourceFingerprint(snapshot)!==identity.sourceFingerprint||await runtimeSourceFingerprint(source)!==identity.sourceFingerprint||await immutableAppVendorDigest(modules)!==identity.vendorSha256)throw Error('ENGINEERING_BUILD_SOURCE_CHANGED');
 const scope=join(modules,'@erpx');for(const entry of await readdir(scope,{withFileTypes:true})){if(!entry.isSymbolicLink())throw Error('ENGINEERING_WORKSPACE_LINK_REQUIRED');const target=await realpath(join(scope,entry.name));if(![join(snapshot,'packages')+sep,join(snapshot,'apps')+sep].some(prefix=>target.startsWith(prefix)))throw Error('ENGINEERING_WORKSPACE_LINK_OUTSIDE_CASE');}
 return {sourceFingerprint:identity.sourceFingerprint,vendorSha256:identity.vendorSha256};
}
