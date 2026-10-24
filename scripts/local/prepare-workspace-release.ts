import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { constants, existsSync } from 'node:fs';
import { cp, mkdir, readFile, readdir, realpath, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { appDefinitions } from '@erpx/contracts/apps';
import { pinnedNode } from './app-task';
import { verifyAppArtifact } from './app-artifact-cache';
import { runtimeSourceFingerprint } from './source-fingerprint';
import { appRuntimeFingerprint, loadWorkspaceGraph } from '../testing/workspace-graph';
import { copyWorkspaceSnapshot, workspaceCopyInput } from './workspace-copy-input';
import { releaseArtifactReference, verifyWorkspaceRelease, type WorkspaceReleaseManifest } from './workspace-release-manifest';

const root=resolve(import.meta.dirname,'../..');
const git=(...args:string[])=>execFileSync('git',args,{cwd:root,encoding:'utf8'}).trim();
const digest=(bytes:string|Buffer)=>createHash('sha256').update(bytes).digest('hex');
async function inventory(directory:string){
 const files:Record<string,string>={},links:Record<string,string>={};
 for(const entry of await readdir(directory,{recursive:true,withFileTypes:true})){
  const file=join(entry.parentPath,entry.name),name=relative(directory,file);
  if(entry.isDirectory())continue;
  if(!entry.isFile())throw new Error('TOOL_ARTIFACT_LINK_OR_SPECIAL_FILE');
  files[name]=digest(await readFile(file));
 }
 return{files,links};
}
async function vendorPackage(snapshot:string,output:string,name:string,seen=new Set<string>()){
 if(seen.has(name))return;seen.add(name);
 const manifestPath=createRequire(join(snapshot,'package.json')).resolve(name+'/package.json');
 const source=await realpath(dirname(manifestPath)),manifest=JSON.parse(await readFile(manifestPath,'utf8'));
 if(manifest.name!==name)throw new Error('TOOL_VENDOR_IDENTITY_MISMATCH');
 await cp(source,join(output,'node_modules',name),{recursive:true,verbatimSymlinks:true,mode:constants.COPYFILE_FICLONE});
 for(const dependency of Object.keys(manifest.dependencies??{}))await vendorPackage(snapshot,output,dependency,seen);
}
export async function buildWorkspaceReleaseTool(snapshot:string,output:string,role:'gateway'|'jobs',sourceFingerprint:string){
 if(typeof Bun==='undefined'||Bun.version!=='1.4.3')throw new Error('BUN_1_4_3_REQUIRED');
 await mkdir(dirname(output),{recursive:true,mode:0o700});await mkdir(output,{mode:0o700});
 const bundle=await Bun.build({entrypoints:[join(snapshot,`scripts/local/workspace-release-${role}.ts`)],outdir:output,naming:'entry.mjs',target:'node',format:'esm',conditions:['react-server','production'],external:['pg'],env:'disable',define:{'process.env.NODE_ENV':JSON.stringify('production')},plugins:[{name:'next-server-navigation',setup(build){build.onResolve({filter:/^next\/navigation$/},()=>({path:join(snapshot,'node_modules/next/dist/client/components/navigation.react-server.js')}));}}]});
 if(!bundle.success)throw new Error(bundle.logs.map(log=>log.message).join('\n'));
 if(role==='jobs'){
  await vendorPackage(snapshot,output,'pg');
  for(const name of ['migrations/postgres','scripts/legal','public/fonts','ops/macos/clamav'])await cp(join(snapshot,name),join(output,name),{recursive:true,mode:constants.COPYFILE_FICLONE});
  const extractor=await Bun.build({entrypoints:[join(snapshot,'scripts/legal/extract_docx.mjs')],outdir:join(output,'scripts/legal'),naming:'extract_docx.mjs',target:'node',format:'esm',env:'disable',define:{'process.env.NODE_ENV':JSON.stringify('production')}});
  if(!extractor.success)throw new Error('JOBS_LEGAL_EXTRACTOR_BUILD_FAILED');
 }
 await writeFile(join(output,'artifact.json'),JSON.stringify({schema:'erpx-tool-artifact/v1',role,runtimeFingerprint:sourceFingerprint,node:'26.11.1',bun:'1.4.3',acceptance:false,entry:'entry.mjs',...await inventory(output)},null,2)+'\n',{mode:0o600});
}

if(import.meta.main){
if(Bun.version!=='1.4.3')throw new Error('BUN_1_4_3_REQUIRED');
const args=process.argv.slice(2),verify=args.find(arg=>arg.startsWith('--verify='))?.slice(9);
if(verify){
 if(args.length!==1||!isAbsolute(verify))throw new Error('ABSOLUTE_RELEASE_VERIFICATION_PATH_REQUIRED');
 const manifest=await verifyWorkspaceRelease(verify);console.log(JSON.stringify({release:verify,candidate:manifest.candidate,productionCandidateFrozen:manifest.productionCandidateFrozen,apps:Object.keys(manifest.apps).length,acceptance:false,inventoryVerified:true}));
}else{
 const requested=args.find(arg=>arg.startsWith('--candidate='))?.slice(12),development=args.includes('--development');
 if(!requested||args.length!==(development?2:1))throw new Error('EXPLICIT_RELEASE_CANDIDATE_REQUIRED');
 const candidate=git('rev-parse','--verify',`${requested}^{commit}`);
 if(candidate!==git('rev-parse','HEAD'))throw new Error('RELEASE_CANDIDATE_MUST_BE_HEAD');
 if(!development&&git('status','--porcelain','--untracked-files=normal'))throw new Error('RELEASE_REQUIRES_CLEAN_FROZEN_CANDIDATE');
 pinnedNode();
 const sourceFingerprint=await runtimeSourceFingerprint(root),id=development?`development-${Date.now()}-${sourceFingerprint.slice(0,12)}`:candidate;
 const release=join(root,'.runtime/workspace-releases',id),snapshot=join(root,'.scratch/dev/workspace-release-source',id);
 if(existsSync(release)||existsSync(snapshot))throw new Error('RELEASE_DESTINATION_ALREADY_EXISTS');
 await mkdir(dirname(release),{recursive:true,mode:0o700});await mkdir(release,{mode:0o700});
 await mkdir(dirname(snapshot),{recursive:true,mode:0o700});await mkdir(snapshot,{mode:0o700});
 try{
  for(const name of ['src','apps','packages','public','scripts','fixtures','tests','docs','ops','migrations','.github','node_modules','package.json','bun.lock','next.config.ts','next-env.d.ts','tsconfig.base.json','tsconfig.json','postcss.config.mjs','eslint.config.mjs','README.md','AGENTS.md','CONTEXT.md','CONTRACT_POLICY.md','.bun-version','.node-version','bunfig.toml','turbo.json','.gitignore'])if(existsSync(join(root,name)))await copyWorkspaceSnapshot(join(root,name),join(snapshot,name),path=>workspaceCopyInput(root,path));
  if(await runtimeSourceFingerprint(snapshot)!==sourceFingerprint||await runtimeSourceFingerprint(root)!==sourceFingerprint)throw new Error('RELEASE_SOURCE_CHANGED_DURING_SNAPSHOT');
  const graph=loadWorkspaceGraph(snapshot),settings=JSON.parse(await readFile(join(snapshot,'scripts/local/workspace-settings.json'),'utf8'));
  const manifest:WorkspaceReleaseManifest={schema:'erpx-workspace-release/v2',candidate,sourceFingerprint,productionCandidateFrozen:!development,acceptance:false,acceptanceAdapter:null,node:'26.11.1',bun:'1.4.3',postgres:'18.6',createdAt:new Date().toISOString(),apps:{},gateway:null!,jobs:null!};
  for(const app of appDefinitions){
   const compiler=settings.bundlers[app.id]??settings.defaultBundler;
   const cached=join(root,app.directory,'.build');
   if(existsSync(join(cached,'artifact.json'))){
    let receipt;try{receipt=JSON.parse(await readFile(join(cached,'artifact.json'),'utf8'));}catch{/* Invalid cache metadata is a build miss. */}
    if(receipt?.schema==='erpx-app-artifact/v2'&&receipt.app===app.id&&receipt.acceptance===false&&receipt.bundler===compiler&&receipt.runtimeFingerprint===appRuntimeFingerprint(graph,app.id)&&await verifyAppArtifact(cached,receipt))
      await cp(cached,join(snapshot,app.directory,'.build'),{recursive:true,verbatimSymlinks:true,mode:constants.COPYFILE_FICLONE});
   }
   // The isolated builder checks the pipeline key and full relocated inventory
   // again; a source match alone never turns a damaged artifact into a cache hit.
   const build=Bun.spawn([process.execPath,'-e',`import {buildIsolatedApp} from './scripts/local/isolated-app-build.ts';await buildIsolatedApp(${JSON.stringify(app.id)},${JSON.stringify(compiler)});`],{cwd:snapshot,env:{...process.env,ERPX_NODE_BIN:pinnedNode()},stdout:'inherit',stderr:'inherit'});
   if(await build.exited!==0)throw new Error(`WORKSPACE_RELEASE_APP_BUILD_FAILED:${app.id}`);
   const path=`apps/${app.id}`;
   await cp(join(snapshot,app.directory,'.build'),join(release,path),{recursive:true,verbatimSymlinks:true,mode:constants.COPYFILE_FICLONE});
   manifest.apps[app.id]=await releaseArtifactReference(release,path,{app:app.id,runtimeFingerprint:appRuntimeFingerprint(graph,app.id)});
  }
  for(const role of ['gateway','jobs']as const){await buildWorkspaceReleaseTool(snapshot,join(release,role),role,sourceFingerprint);manifest[role]=await releaseArtifactReference(release,role,{role,runtimeFingerprint:sourceFingerprint});}
  if(await runtimeSourceFingerprint(root)!==sourceFingerprint||git('rev-parse','HEAD')!==candidate||!development&&git('status','--porcelain','--untracked-files=normal'))throw new Error('RELEASE_SOURCE_CHANGED_DURING_BUILD');
  await writeFile(join(release,'release.json'),JSON.stringify(manifest,null,2)+'\n',{mode:0o600,flag:'wx'});
  await verifyWorkspaceRelease(release,{candidate,frozen:!development});
  console.log(JSON.stringify({release,candidate,apps:appDefinitions.length,productionCandidateFrozen:!development,acceptance:false,inventoryVerified:true}));
 }catch(error){await writeFile(join(release,'BUILD_FAILED.json'),JSON.stringify({acceptance:false,complete:false,reason:error instanceof Error?error.message:'RELEASE_BUILD_FAILED'})+'\n',{mode:0o600});throw error;}
 finally{await rm(snapshot,{recursive:true,force:true});}
}

}
