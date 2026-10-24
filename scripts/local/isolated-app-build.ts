import { constants,existsSync } from 'node:fs';
import { cp,mkdir,readFile,readdir,rm,writeFile,open,readlink,realpath } from 'node:fs/promises';
import { join,relative,resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { appDefinition } from '@erpx/contracts/apps';
import { appRuntimeFingerprint,loadWorkspaceGraph } from '../testing/workspace-graph';
import { pinnedNode } from './app-task';
import { relocateStandaloneLinks, relocateCompilerDependencyAliases, artifactSymbolicLinks } from './standalone-links';
import { verifyAppArtifact } from './app-artifact-cache';
import { workspaceCopyInput } from './workspace-copy-input';
const root=resolve(import.meta.dirname,'../..');
const copyOptions={recursive:true,verbatimSymlinks:true,mode:constants.COPYFILE_FICLONE,filter:(path:string)=>workspaceCopyInput(root,path)};
async function files(directory:string):Promise<string[]>{
 const entries=await readdir(directory,{withFileTypes:true});
 return (await Promise.all(entries.map(async entry=>entry.isDirectory()?files(join(directory,entry.name)):entry.isFile()?[join(directory,entry.name)]:[]))).flat();
}
async function keyFor(id:string,bundler:string){
 const runtime=appRuntimeFingerprint(loadWorkspaceGraph(root),id),hash=createHash('sha256').update('isolated-app-build/v2').update(runtime).update(bundler).update(process.env.NEXT_DEPLOYMENT_ID??'default-runtime-deployment');
 for(const name of ['scripts/local/app-task.ts','scripts/local/isolated-app-build.ts','scripts/local/standalone-links.ts','scripts/local/app-artifact-cache.ts','scripts/local/workspace-copy-input.ts'])hash.update(name).update(await readFile(join(root,name)));
 return {runtime,key:hash.digest('hex')};
}
/** Build from a private source copy: Next may rewrite next-env and route types. */
export async function buildIsolatedApp(id:string,bundler='webpack'){
 if(!['webpack','turbopack'].includes(bundler))throw new Error('UNKNOWN_APP_BUNDLER');
 const app=appDefinition(id),identity=await keyFor(id,bundler),directory=join(root,'.scratch/dev/workspace-builds',id,identity.key),snapshot=join(directory,'source');
 await mkdir(directory,{recursive:true,mode:0o700});const lock=join(directory,'build.lock');let handle;
 try{handle=await open(lock,'wx',0o600);}catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST')throw error;const owner=JSON.parse(await readFile(lock,'utf8'));if(owner.root!==root||owner.id!==id||owner.key!==identity.key)throw new Error('APP_BUILD_LOCK_IDENTITY_CHANGED');try{process.kill(owner.pid,0);throw new Error('APP_BUILD_ALREADY_RUNNING');}catch(error){if((error as NodeJS.ErrnoException).code!=='ESRCH')throw error;}await rm(lock);handle=await open(lock,'wx',0o600);}
 await handle.writeFile(JSON.stringify({root,id,key:identity.key,pid:process.pid})+'\n');await handle.close();
 let child:ReturnType<typeof Bun.spawn>|undefined,interrupted=false;const stop=(signal:'SIGINT'|'SIGTERM')=>{interrupted=true;child?.kill(signal);};const onInt=()=>stop('SIGINT'),onTerm=()=>stop('SIGTERM');process.on('SIGINT',onInt);process.on('SIGTERM',onTerm);
 try{
  const output=join(root,app.directory,'.build'),receiptPath=join(output,'artifact.json');
  if(existsSync(receiptPath)){
   let receipt;try{receipt=JSON.parse(await readFile(receiptPath,'utf8'));}catch{/* A damaged receipt is a cache miss. */}
   if(receipt?.schema==='erpx-app-artifact/v2'&&receipt.app===id&&receipt.buildFingerprint===identity.key&&receipt.acceptance===false&&await verifyAppArtifact(output,receipt)){
    console.log(`APP_BUILD_CACHE_HIT:${id}:${identity.key.slice(0,12)}`);return;
   }
  }
  await rm(snapshot,{recursive:true,force:true});await mkdir(snapshot,{recursive:true});
  const inputs=[app.directory,'packages','src','public','scripts','fixtures','migrations','tests/support','tests/catalogue','tests/config','ops/macos','package.json','bun.lock','tsconfig.base.json','tsconfig.json','next.config.ts','next-env.d.ts','postcss.config.mjs','node_modules'];
  for(const name of inputs)if(existsSync(join(root,name)))await cp(join(root,name),join(snapshot,name),copyOptions);
  if(interrupted)throw new Error("APP_BUILD_INTERRUPTED");
  if((await keyFor(id,bundler)).key!==identity.key)throw new Error("APP_SOURCE_CHANGED_DURING_SNAPSHOT");
  const deploymentId=process.env.NEXT_DEPLOYMENT_ID??`app-${id}-${identity.runtime.slice(0,16)}`;
  await writeFile(join(snapshot,'packages/catalogue/src/release-artifact.json'),JSON.stringify({softwareVersion:deploymentId})+'\n');
  // Pass compiler/network trust inputs explicitly. Installed DB, auth and
  // provider credentials never become ambient inputs to an app build.
  const compilerKeys=new Set(['PATH','HOME','TMPDIR','TMP','TEMP','LANG','LC_ALL','TZ','CI','HTTP_PROXY','HTTPS_PROXY','NO_PROXY','http_proxy','https_proxy','no_proxy','NODE_EXTRA_CA_CERTS','SSL_CERT_FILE','SSL_CERT_DIR','TERM','TERM_PROGRAM','NEXT_TELEMETRY_DISABLED','CODEX_CI','CODEX_SHELL','CODEX_THREAD_ID']);
  const environment={...Object.fromEntries(Object.entries(process.env).filter(([name])=>compilerKeys.has(name)||name.startsWith('NEXT_PUBLIC_'))),NODE_ENV:'production',NODE_OPTIONS:'--max-old-space-size=8192',NEXT_DEPLOYMENT_ID:deploymentId,ERPX_BUILD_DIR:'.build',ERPX_APP_ID:id,ERPX_REPOSITORY_ROOT:snapshot,ERPX_RELEASE_NO_WEBPACK_CACHE:'1',ERPX_NODE_BIN:pinnedNode()};
  let attempts=0,passed=false;
  while(!passed&&attempts<2){
   attempts++;
   if(interrupted)throw new Error('APP_BUILD_INTERRUPTED');
   child=Bun.spawn([pinnedNode(),join(snapshot,'node_modules/next/dist/bin/next'),'build',join(snapshot,app.directory),`--${bundler}`],{cwd:snapshot,env:environment,stdin:'ignore',stdout:'inherit',stderr:'inherit'});
   passed=await child.exited===0;
   if(!passed&&attempts===1&&!interrupted)console.warn('Retrying the isolated compiler once; no artifact or application request has been published.');
  }
  if(!passed||interrupted)throw new Error(`ISOLATED_APP_BUILD_FAILED:${id}`);
  const current=await keyFor(id,bundler);if(current.key!==identity.key)throw new Error('APP_RUNTIME_SOURCE_CHANGED_DURING_BUILD');
  const compiled=join(snapshot,app.directory,'.build');if(!existsSync(join(compiled,'standalone',app.directory,'server.js')))throw new Error('APP_STANDALONE_ARTIFACT_REQUIRED');
  await rm(output,{recursive:true,force:true});await cp(compiled,output,{recursive:true,verbatimSymlinks:true,mode:constants.COPYFILE_FICLONE,filter:path=>{const name=relative(compiled,path);return name!=='cache'&&!name.startsWith('cache/');}});
  const standalone=join(output,'standalone'),standaloneApp=join(standalone,app.directory);
  await relocateStandaloneLinks(join(compiled,'standalone'),standalone);
  await relocateCompilerDependencyAliases(compiled,output,join(snapshot,'node_modules'));
  await cp(join(snapshot,'public'),join(standaloneApp,'public'),{recursive:true,mode:constants.COPYFILE_FICLONE});
  await cp(join(snapshot,'public'),join(standalone,'public'),{recursive:true,mode:constants.COPYFILE_FICLONE});
  await cp(join(snapshot,'migrations/postgres'),join(standalone,'migrations/postgres'),{recursive:true,mode:constants.COPYFILE_FICLONE});
  await cp(join(snapshot,'scripts/legal'),join(standalone,'scripts/legal'),{recursive:true,mode:constants.COPYFILE_FICLONE});
  const extractor=await Bun.build({entrypoints:[join(snapshot,'scripts/legal/extract_docx.mjs')],outdir:join(standalone,'scripts/legal'),target:'node',format:'esm',naming:'extract_docx.mjs',env:'disable',define:{'process.env.NODE_ENV':JSON.stringify('production')}});
  if(!extractor.success)throw new Error('APP_LEGAL_EXTRACTOR_BUILD_FAILED');
  await writeFile(join(standaloneApp,'entry.mjs'),`import {fileURLToPath} from 'node:url';\nimport {createServer} from 'node:net';\nconst port=Number(process.env.PORT);\nif(!Number.isInteger(port)||port<3200||port>3299)throw Error('EXPLICIT_OWNED_APP_PORT_REQUIRED');\nawait new Promise((done,reject)=>{const probe=createServer();probe.once('error',reject);probe.listen(port,'127.0.0.1',()=>probe.close(done));});\nprocess.env.HOSTNAME='127.0.0.1';\nprocess.env.ERPX_RESOURCE_ROOT??=fileURLToPath(new URL('../../',import.meta.url));\nprocess.env.ERPX_REPOSITORY_ROOT??=process.env.ERPX_RESOURCE_ROOT;\nprocess.env.ERPX_APP_ID=${JSON.stringify(id)};\nprocess.env.ERPX_APP_PORT=String(port);\nawait import('./server.js');\n`);
  await cp(join(compiled,'static'),join(standaloneApp,'.build/static'),{recursive:true,mode:constants.COPYFILE_FICLONE});
  const links:Record<string,string>={};
  for(const path of await artifactSymbolicLinks(output)){
   if(!(await realpath(path)).startsWith(output+'/'))throw new Error('APP_ARTIFACT_EXTERNAL_LINK');links[relative(output,path)]=await readlink(path);
  }
  const hashes:Record<string,string>={};for(const file of(await files(output)).sort())hashes[relative(output,file)]=createHash('sha256').update(await readFile(file)).digest('hex');
  await writeFile(receiptPath,JSON.stringify({schema:'erpx-app-artifact/v2',app:id,runtimeFingerprint:identity.runtime,buildFingerprint:identity.key,node:'26.11.1',bun:'1.4.3',bundler,attempts,acceptance:false,entry:`standalone/${app.directory}/entry.mjs`,softwareVersion:deploymentId,resources:"standalone",files:hashes,links},null,2)+'\n',{mode:0o600});
  console.log(`APP_BUILD_COMPLETE:${id}:${identity.key.slice(0,12)}`);
 }finally{process.off('SIGINT',onInt);process.off('SIGTERM',onTerm);await rm(lock,{force:true});}
}
