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
import { oasisDemoApps, oasisNativeReference, releaseArtifactReference, verifyWorkspaceRelease, type WorkspaceReleaseManifest } from './workspace-release-manifest';
import { preparePdfAssets } from '../assets/prepare-pdf-assets';

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
export async function buildWorkspaceReleaseTool(snapshot:string,output:string,role:'gateway'|'jobs'|'stage',sourceFingerprint:string){
 if(typeof Bun==='undefined'||Bun.version!=='1.4.3')throw new Error('BUN_1_4_3_REQUIRED');
 await mkdir(dirname(output),{recursive:true,mode:0o700});await mkdir(output,{mode:0o700});
 const entry=role==='stage'?'oasis-demo-stage':`workspace-release-${role}`;
 const bundle=await Bun.build({entrypoints:[join(snapshot,`scripts/local/${entry}.ts`)],outdir:output,naming:'entry.mjs',target:'node',format:'esm',conditions:['react-server','production'],external:['pg'],env:'disable',define:{'process.env.NODE_ENV':JSON.stringify('production')},plugins:[{name:'next-server-navigation',setup(build){build.onResolve({filter:/^next\/navigation$/},()=>({path:join(snapshot,'node_modules/next/dist/client/components/navigation.react-server.js')}));}}]});
 if(!bundle.success)throw new Error(bundle.logs.map(log=>log.message).join('\n'));
 if(role==='jobs'){
  await vendorPackage(snapshot,output,'pg');
  for(const name of ['migrations/postgres','public/fonts','ops/macos/clamav'])await cp(join(snapshot,name),join(output,name),{recursive:true,mode:constants.COPYFILE_FICLONE});
  const extractor=await Bun.build({entrypoints:[join(snapshot,'scripts/legal/extract_docx.mjs')],outdir:join(output,'scripts/legal'),naming:'extract_docx.mjs',target:'node',format:'esm',env:'disable',define:{'process.env.NODE_ENV':JSON.stringify('production')}});
  if(!extractor.success)throw new Error('JOBS_LEGAL_EXTRACTOR_BUILD_FAILED');
 }
 await writeFile(join(output,'artifact.json'),JSON.stringify({schema:'erpx-tool-artifact/v1',role,runtimeFingerprint:sourceFingerprint,node:'26.11.1',bun:'1.4.3',acceptance:false,entry:'entry.mjs',...await inventory(output)},null,2)+'\n',{mode:0o600});
}

if(import.meta.main){
if(Bun.version!=='1.4.3')throw new Error('BUN_1_4_3_REQUIRED');
const args=process.argv.slice(2),verify=args.find(arg=>arg.startsWith('--verify='))?.slice(9);
const oasisDemo=args.includes('--oasis-demo');
if(verify){
 if(args.length!==Number(oasisDemo)+1||!isAbsolute(verify))throw new Error('ABSOLUTE_RELEASE_VERIFICATION_PATH_REQUIRED');
 const manifest=await verifyWorkspaceRelease(verify,{oasisDemo});console.log(JSON.stringify({release:verify,candidate:manifest.candidate,productionCandidateFrozen:manifest.productionCandidateFrozen,apps:Object.keys(manifest.apps).length,acceptance:false,inventoryVerified:true}));
}else{
 const requested=args.find(arg=>arg.startsWith('--candidate='))?.slice(12),development=args.includes('--development');
 if(!requested||args.length!==Number(development)+Number(oasisDemo)+1)throw new Error('EXPLICIT_RELEASE_CANDIDATE_REQUIRED');
 const candidate=git('rev-parse','--verify',`${requested}^{commit}`);
 if(candidate!==git('rev-parse','HEAD'))throw new Error('RELEASE_CANDIDATE_MUST_BE_HEAD');
 if(!development&&git('status','--porcelain','--untracked-files=normal'))throw new Error('RELEASE_REQUIRES_CLEAN_FROZEN_CANDIDATE');
 if(oasisDemo){
  process.env.ERPX_HOLDING_ROUTING='enabled';
  process.env.ERPX_ENVIRONMENT='staging';
  process.env.ERPX_PRODUCT_ENVIRONMENT='staging';
  process.env.NEXT_PUBLIC_ERPX_OASIS_DEMO_ASSETS='1';
 }
 pinnedNode();
 await preparePdfAssets();
 const sourceFingerprint=await runtimeSourceFingerprint(root),id=development?`development-${Date.now()}-${sourceFingerprint.slice(0,12)}`:candidate;
 const name=oasisDemo?`oasis-${id}`:id;
 const release=join(root,'.runtime/workspace-releases',name),snapshot=join(root,'.scratch/dev/workspace-release-source',name);
 if(existsSync(release)||existsSync(snapshot))throw new Error('RELEASE_DESTINATION_ALREADY_EXISTS');
 await mkdir(dirname(release),{recursive:true,mode:0o700});await mkdir(release,{mode:0o700});
 await mkdir(dirname(snapshot),{recursive:true,mode:0o700});await mkdir(snapshot,{mode:0o700});
 try{
  for(const name of ['src','apps','packages','public','scripts','fixtures','tests','contracts','docs','ops','migrations','.github','node_modules','package.json','bun.lock','next.config.ts','next-env.d.ts','tsconfig.base.json','tsconfig.json','postcss.config.mjs','eslint.config.mjs','README.md','AGENTS.md','CONTEXT.md','Cargo.toml','Cargo.lock','rust-toolchain.toml','LICENSE','NOTICE','CONTRIBUTING.md','.bun-version','.node-version','bunfig.toml','turbo.json','.gitignore'])if(existsSync(join(root,name)))await copyWorkspaceSnapshot(join(root,name),join(snapshot,name),path=>workspaceCopyInput(root,path));
  if(await runtimeSourceFingerprint(snapshot)!==sourceFingerprint||await runtimeSourceFingerprint(root)!==sourceFingerprint)throw new Error('RELEASE_SOURCE_CHANGED_DURING_SNAPSHOT');
  if(oasisDemo){
   // Regenerate derived Tailwind inputs in the frozen copy so removed routes
   // neither leak into the asset graph nor require changing the live checkout.
   const styles=Bun.spawn([process.execPath,'--no-env-file','scripts/assets/workspace-styles.ts'],{cwd:snapshot,stdin:'ignore',stdout:'inherit',stderr:'inherit'});
   if(await styles.exited!==0)throw new Error('OASIS_WORKSPACE_STYLE_REGENERATION_FAILED');
  }
  const graph=loadWorkspaceGraph(snapshot),settings=JSON.parse(await readFile(join(snapshot,'scripts/local/workspace-settings.json'),'utf8'));
  const common={candidate,sourceFingerprint,productionCandidateFrozen:!development,acceptance:false,acceptanceAdapter:null,node:'26.11.1',bun:'1.4.3',postgres:'18.6',createdAt:new Date().toISOString(),apps:{} as WorkspaceReleaseManifest['apps'],gateway:null!} as const;
  const manifest:WorkspaceReleaseManifest=oasisDemo?{...common,schema:'erpx-oasis-demo-release/v1',stage:null!,native:null!}:{...common,schema:'erpx-workspace-release/v2',jobs:null!};
  const selectedApps=appDefinitions.filter(app=>!oasisDemo||oasisDemoApps.some(id=>id===app.id));
  for(const app of selectedApps){
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
  await buildWorkspaceReleaseTool(snapshot,join(release,'gateway'),'gateway',sourceFingerprint);
  manifest.gateway=await releaseArtifactReference(release,'gateway',{role:'gateway',runtimeFingerprint:sourceFingerprint});
  if(manifest.schema==='erpx-oasis-demo-release/v1'){
   await buildWorkspaceReleaseTool(snapshot,join(release,'stage'),'stage',sourceFingerprint);
   manifest.stage=await releaseArtifactReference(release,'stage',{role:'stage',runtimeFingerprint:sourceFingerprint});
   // Cargo checks the same frozen checkout and shares its ignored cache across
   // application rebuilds. The final source and Git checks still gate packaging.
   const target=join(root,'.runtime/workspace-release-rust/oasis');
   const native=Bun.spawn(['cargo','build','--release','--locked','--no-default-features','--features','private-development-api','-p','erpx-service','--bin','erpx-server'],{cwd:root,env:{...process.env,CARGO_TARGET_DIR:target},stdin:'ignore',stdout:'inherit',stderr:'inherit'});
   if(await native.exited!==0)throw new Error('OASIS_NATIVE_RELEASE_BUILD_FAILED');
   await mkdir(join(release,'native'),{mode:0o700});
   await cp(join(target,'release/erpx-server'),join(release,'native/erpx-server'),{mode:constants.COPYFILE_FICLONE});
   manifest.native=await oasisNativeReference(release);
  }else{
   await buildWorkspaceReleaseTool(snapshot,join(release,'jobs'),'jobs',sourceFingerprint);
   manifest.jobs=await releaseArtifactReference(release,'jobs',{role:'jobs',runtimeFingerprint:sourceFingerprint});
  }
  if(await runtimeSourceFingerprint(root)!==sourceFingerprint||git('rev-parse','HEAD')!==candidate||!development&&git('status','--porcelain','--untracked-files=normal'))throw new Error('RELEASE_SOURCE_CHANGED_DURING_BUILD');
  await writeFile(join(release,'release.json'),JSON.stringify(manifest,null,2)+'\n',{mode:0o600,flag:'wx'});
  await verifyWorkspaceRelease(release,{candidate,frozen:!development,oasisDemo});
  console.log(JSON.stringify({release,candidate,apps:selectedApps.length,productionCandidateFrozen:!development,acceptance:false,inventoryVerified:true}));
 }catch(error){await writeFile(join(release,'BUILD_FAILED.json'),JSON.stringify({acceptance:false,complete:false,reason:error instanceof Error?error.message:'RELEASE_BUILD_FAILED'})+'\n',{mode:0o600});throw error;}
 finally{await rm(snapshot,{recursive:true,force:true});}
}

}
