/** Prepare one private Service child bound to the already reviewed release. */
import {createHash,randomBytes} from 'node:crypto';
import {lstat,readFile,realpath,rm,writeFile} from 'node:fs/promises';
import {join,resolve} from 'node:path';
export async function prepareNativeServiceChild(directory:string,run:string){
 const root=resolve(directory),info=await lstat(root),release=join(root,'release');
 if(!/^[a-f0-9]{12}$/.test(run)||!root.endsWith(`/.runtime/browser/${run}`)||await realpath(root)!==root||!info.isDirectory()||info.isSymbolicLink()||(info.mode&0o077)!==0||info.uid!==process.getuid?.()||await realpath(release)!==release)throw Error('NATIVE_SERVICE_CHILD_DIRECTORY_INVALID');
 const releaseInfo=JSON.parse(await readFile(join(release,'release.json'),'utf8')),manifestBytes=await readFile(join(release,'release-files.json')),manifest=JSON.parse(manifestBytes.toString());
 const digest=createHash('sha256').update(manifestBytes).digest('hex'),entry=join(release,'apps/service/entry.mjs');
 if(releaseInfo.scope!=='isolated-engineering-service-only'||!/^[a-f0-9]{64}$/.test(releaseInfo.sourceFingerprint??'')||releaseInfo.softwareVersion!==`release-${releaseInfo.sourceFingerprint}`||releaseInfo.fileManifestSha256!==digest||await realpath(entry)!==entry||!/^[a-f0-9]{64}$/.test(manifest['apps/service/entry.mjs']??'')||createHash('sha256').update(await readFile(entry)).digest('hex')!==manifest['apps/service/entry.mjs'])throw Error('NATIVE_SERVICE_CHILD_RELEASE_INVALID');
 const nonce=randomBytes(16).toString('hex'),bytes=Buffer.from(JSON.stringify({version:1,appId:'service',port:3213,gatewayPort:3104,run,sourceFingerprint:releaseInfo.sourceFingerprint,fileManifestSha256:digest,nonce})),path=join(root,'native-service-child.json'),noncePath=join(root,'native-service-child-nonce');
 await writeFile(path,bytes,{mode:0o600,flag:'wx'});
 try{await writeFile(noncePath,nonce,{mode:0o600,flag:'wx'});}catch(error){await rm(path);throw error;}
 return {sha256:createHash('sha256').update(bytes).digest('hex')};
}
