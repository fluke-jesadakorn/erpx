/** Test-only Linux Service release, using the unchanged app compiler pipeline. */
import {createHash} from 'node:crypto';
import {createReadStream} from 'node:fs';
import {mkdir,readFile,readdir,realpath,rename,writeFile} from 'node:fs/promises';
import {join,relative,resolve,sep} from 'node:path';
import {buildIsolatedApp} from '../../local/isolated-app-build';
import {verifyAppArtifact} from '../../local/app-artifact-cache';
import {artifactSymbolicLinks} from '../../local/standalone-links';
import {runtimeSourceFingerprint} from '../../local/source-fingerprint';

const root=resolve('.'),caseRoot=process.env.ERPX_ENGINEERING_CASE_ROOT,nonce=process.env.ERPX_ENGINEERING_CASE_NONCE,run=process.env.ERPX_ENGINEERING_CASE_RUN;
if(!caseRoot||!nonce||!run||!/^[a-f0-9]{32}$/.test(nonce)||!/^[a-f0-9]{12}$/.test(run)||root!==join(resolve(caseRoot),'source'))throw Error('ENGINEERING_SERVICE_BUILD_CASE_REQUIRED');
const sourceFingerprint=await runtimeSourceFingerprint(root),softwareVersion=`release-${sourceFingerprint}`;
process.env.NEXT_DEPLOYMENT_ID=softwareVersion;
await buildIsolatedApp('service','webpack',{caseRoot,buildSource:join(resolve(caseRoot),'build'),nonce});
if(await runtimeSourceFingerprint(root)!==sourceFingerprint)throw Error('ENGINEERING_SERVICE_SOURCE_CHANGED');
const output=join(root,'apps/service/.build'),receipt=JSON.parse(await readFile(join(output,'artifact.json'),'utf8'));
if(receipt.softwareVersion!==softwareVersion||receipt.acceptance!==false||!await verifyAppArtifact(output,receipt))throw Error('ENGINEERING_SERVICE_ARTIFACT_INVALID');
const cohort=join(resolve(caseRoot),'.runtime/browser',run),release=join(cohort,'release');await mkdir(cohort,{recursive:true,mode:0o700});await rename(join(output,'standalone'),release);
for(const link of await artifactSymbolicLinks(release)){const actual=await realpath(link);if(!actual.startsWith(release+sep))throw Error('ENGINEERING_SERVICE_EXTERNAL_ARTIFACT_LINK');}
const files:Record<string,string>={},pending=[release];while(pending.length){const dir=pending.pop()!,entries=await readdir(dir,{withFileTypes:true});for(const entry of entries){const path=join(dir,entry.name);if(entry.isDirectory())pending.push(path);else if(entry.isFile()){const hash=createHash('sha256');for await(const chunk of createReadStream(path))hash.update(chunk as Buffer);files[relative(release,path)]=hash.digest('hex');}else if(!entry.isSymbolicLink())throw Error('ENGINEERING_SERVICE_ARTIFACT_ENTRY_INVALID');}}
const bytes=JSON.stringify(Object.fromEntries(Object.entries(files).sort(([a],[b])=>a.localeCompare(b,'en'))),null,2)+'\n',fileManifestSha256=createHash('sha256').update(bytes).digest('hex');
await writeFile(join(release,'release-files.json'),bytes,{mode:0o600,flag:'wx'});await writeFile(join(release,'release.json'),JSON.stringify({sourceFingerprint,softwareVersion,fileManifestSha256,scope:'isolated-engineering-service-only'},null,2)+'\n',{mode:0o600,flag:'wx'});
console.log(JSON.stringify({scope:'isolated-engineering-service-only',sourceFingerprint,softwareVersion,fileManifestSha256,acceptance:false}));
