import { cp,mkdir,readFile,writeFile,symlink,rm,readdir } from 'node:fs/promises';
import { constants } from 'node:fs';
import { join,resolve,relative } from 'node:path';
import { createHash } from 'node:crypto';
import { runtimeSourceFingerprint } from './source-fingerprint';

const root=process.cwd(), id=new Date().toISOString().replace(/[:.]/g,'-');
const sourceFingerprint=await runtimeSourceFingerprint(root);
const work=join(root,'.runtime/build',id),release=join(root,'.runtime/releases',id);
const node=resolve(process.env.ERPX_NODE_BIN??'.runtime/node/node-v24.20.0-darwin-arm64/bin/node');
if(Bun.spawnSync([node,'--version']).stdout.toString().trim()!=='v24.20.0')throw new Error('NODE_24_20_REQUIRED');
const acceptanceCheck=Bun.spawn([process.execPath,join(root,'scripts/generate-capability-matrix.ts'),'--check'],{cwd:root,stdout:'inherit',stderr:'inherit'});
if(await acceptanceCheck.exited!==0)throw new Error('BUSINESS_ACCEPTANCE_SNAPSHOT_STALE');
const hospitalityCheck=Bun.spawn([process.execPath,join(root,'scripts/generate-hospitality-evidence.ts'),'--check'],{cwd:root,stdout:'inherit',stderr:'inherit'});
if(await hospitalityCheck.exited!==0)throw new Error('HOSPITALITY_EVIDENCE_SNAPSHOT_STALE');
await mkdir(work,{recursive:true});await mkdir(release,{recursive:true});
for(const name of ['src','public','scripts','fixtures','tests/helpers','migrations/postgres','package.json','bun.lock','next.config.ts','tsconfig.json','tsconfig.engineering-tests.json','next-env.d.ts','postcss.config.mjs'])
  await cp(join(root,name),join(work,name),{recursive:true,mode:constants.COPYFILE_FICLONE});
await mkdir(join(work,'ops/macos'),{recursive:true});
await cp(join(root,'ops/macos/clamav'),join(work,'ops/macos/clamav'),{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(root,'ops/macos/install-attachment-runtime.py'),join(work,'ops/macos/install-attachment-runtime.py'),{mode:constants.COPYFILE_FICLONE});
await cp(join(root,'ops/macos/attachment-rollout.md'),join(work,'ops/macos/attachment-rollout.md'),{mode:constants.COPYFILE_FICLONE});
for(const name of ['erpxctl.py','offsite.py','recovery_status.py'])await cp(join(root,'ops/macos',name),join(work,'ops/macos',name),{mode:constants.COPYFILE_FICLONE});
await cp(join(root,'node_modules'),join(work,'node_modules'),{recursive:true,mode:constants.COPYFILE_FICLONE});
// Identify the actual compiled source, including uncommitted changes. Never use
// a branch label or package version as evidence that two releases are identical.
if(await runtimeSourceFingerprint(work)!==sourceFingerprint||await runtimeSourceFingerprint(root)!==sourceFingerprint)
  throw new Error('RELEASE_SOURCE_CHANGED_DURING_COPY');
const softwareVersion=`release-${sourceFingerprint}`;
await writeFile(join(work,'src/data/release-artifact.json'),JSON.stringify({softwareVersion})+'\n');
// No development .env, database, fixture state or secrets enter the build tree.
const child=Bun.spawn([node,join(work,'node_modules/next/dist/bin/next'),'build'],{cwd:work,stdout:'inherit',stderr:'inherit',env:{...process.env,NODE_ENV:'production',NEXT_DEPLOYMENT_ID:softwareVersion,ERPX_BUILD_DIR:'.next'}});
if(await child.exited!==0)throw new Error('NEXT_BUILD_FAILED');
await cp(join(work,'.next/standalone'),release,{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(work,'.next/static'),join(release,'.next/static'),{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(work,'public'),join(release,'public'),{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(work,'migrations/postgres'),join(release,'migrations/postgres'),{recursive:true});
await mkdir(join(release,'ops/macos'),{recursive:true});
await cp(join(work,'ops/macos/clamav'),join(release,'ops/macos/clamav'),{recursive:true});
await cp(join(work,'ops/macos/install-attachment-runtime.py'),join(release,'ops/macos/install-attachment-runtime.py'));
await cp(join(work,'ops/macos/attachment-rollout.md'),join(release,'ops/macos/attachment-rollout.md'));
for(const name of ['erpxctl.py','offsite.py','recovery_status.py'])await cp(join(work,'ops/macos',name),join(release,'ops/macos',name));
await cp(join(work,'scripts/legal'),join(release,'scripts/legal'),{recursive:true});
// Disabled unless the isolated booking launcher attests an exact fictional cohort.
await mkdir(join(release,'tools'),{recursive:true});
await cp(join(work,'scripts/local/pms-provider-fixture.mjs'),join(release,'tools/pms-provider-fixture.mjs'));
const productionDefines={'process.env.NODE_ENV':JSON.stringify('production')};
const extraction=await Bun.build({entrypoints:[join(work,'scripts/legal/extract_docx.mjs')],outdir:join(release,'scripts/legal'),target:'node',format:'esm',naming:'extract_docx.mjs',env:'disable',define:productionDefines});
if(!extraction.success)throw new Error('LEGAL_EXTRACTOR_BUILD_FAILED');
const tools=['jobs','launch','provision','bootstrap-admin','bootstrap-staging-admin','doctor','minio-snapshot','migrate-files-to-minio','reconcile-attachments'];
const bundle=await Bun.build({entrypoints:tools.map(name=>join(work,'scripts/local',`${name}.ts`)),outdir:join(release,'tools'),target:'node',format:'esm',conditions:['react-server','production'],external:['pg'],naming:'[name].mjs',env:'disable',define:productionDefines,
  // Bun defaults NODE_ENV to development without minification. Deployed tools
  // must compile the same production gates as Next's standalone web runtime.
  // Mirror Next's server-layer resolution for permission helpers used by jobs.
  plugins:[{name:'next-server-navigation',setup(build){build.onResolve({filter:/^next\/navigation$/},()=>({path:join(work,'node_modules/next/dist/client/components/navigation.react-server.js')}));}}],
});
if(!bundle.success)throw new Error(bundle.logs.map(log=>log.message).join('\n'));
const schemaHash=createHash('sha256');
for(const track of (await readdir(join(work,'migrations/postgres'))).sort()) for(const file of (await readdir(join(work,'migrations/postgres',track))).filter(f=>f.endsWith('.sql')).sort())
  schemaHash.update(track+'/'+file).update(await readFile(join(work,'migrations/postgres',track,file)));
const attachmentRuntime=JSON.parse(await readFile(join(work,'ops/macos/clamav/runtime-versions.json'),'utf8'));
const artifacts:Record<string,string>={};
for(const file of (await readdir(release,{recursive:true,withFileTypes:true})).filter(entry=>entry.isFile()).map(entry=>join(entry.parentPath,entry.name)).sort())
  artifacts[relative(release,file)]=createHash('sha256').update(await readFile(file)).digest('hex');
const artifactManifest=JSON.stringify(artifacts,null,2)+'\n';
await writeFile(join(release,'release-files.json'),artifactManifest);
const manifest={id,softwareVersion,sourceFingerprint,deploymentId:softwareVersion,node:'24.20.0',postgres:'18.6',attachmentRuntime,createdAt:new Date().toISOString(),schemaSha256:schemaHash.digest('hex'),lockfileSha256:createHash('sha256').update(await readFile(join(work,'bun.lock'))).digest('hex'),fileManifestSha256:createHash('sha256').update(artifactManifest).digest('hex')};
await writeFile(join(release,'release.json'),JSON.stringify(manifest,null,2));
await rm(join(root,'.runtime/releases/latest'),{force:true});await symlink(release,join(root,'.runtime/releases/latest'));
// Remove only this build's completed workspace. Releases may still be referenced
// by running verification, retained evidence or rollback requirements.
await rm(work,{recursive:true,force:true});
console.log(`Release: ${release}`);
