import { cp,mkdir,readFile,writeFile,symlink,rm,readdir } from 'node:fs/promises';
import { constants } from 'node:fs';
import { join,resolve } from 'node:path';
import { createHash } from 'node:crypto';

const root=process.cwd(), id=new Date().toISOString().replace(/[:.]/g,'-');
const work=join(root,'.runtime/build',id),release=join(root,'.runtime/releases',id);
const node=resolve(process.env.ERPX_NODE_BIN??'.runtime/node/node-v24.20.0-darwin-arm64/bin/node');
if(Bun.spawnSync([node,'--version']).stdout.toString().trim()!=='v24.20.0')throw new Error('NODE_24_20_REQUIRED');
const acceptanceCheck=Bun.spawn([process.execPath,join(root,'scripts/generate-capability-matrix.ts'),'--check'],{cwd:root,stdout:'inherit',stderr:'inherit'});
if(await acceptanceCheck.exited!==0)throw new Error('BUSINESS_ACCEPTANCE_SNAPSHOT_STALE');
const hospitalityCheck=Bun.spawn([process.execPath,join(root,'scripts/generate-hospitality-evidence.ts'),'--check'],{cwd:root,stdout:'inherit',stderr:'inherit'});
if(await hospitalityCheck.exited!==0)throw new Error('HOSPITALITY_EVIDENCE_SNAPSHOT_STALE');
await mkdir(work,{recursive:true});await mkdir(release,{recursive:true});
for(const name of ['src','public','scripts','fixtures','tests/helpers','migrations/postgres','package.json','bun.lock','next.config.ts','tsconfig.json','next-env.d.ts','postcss.config.mjs'])
  await cp(join(root,name),join(work,name),{recursive:true,mode:constants.COPYFILE_FICLONE});
await mkdir(join(work,'ops/macos'),{recursive:true});
await cp(join(root,'ops/macos/clamav'),join(work,'ops/macos/clamav'),{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(root,'ops/macos/install-attachment-runtime.py'),join(work,'ops/macos/install-attachment-runtime.py'),{mode:constants.COPYFILE_FICLONE});
await cp(join(root,'ops/macos/attachment-rollout.md'),join(work,'ops/macos/attachment-rollout.md'),{mode:constants.COPYFILE_FICLONE});
await cp(join(root,'node_modules'),join(work,'node_modules'),{recursive:true,mode:constants.COPYFILE_FICLONE});
// Identify the actual compiled source, including uncommitted changes. Never use
// a branch label or package version as evidence that two releases are identical.
const sourceHash=createHash('sha256').update('node24.20.0:bun1.2.17');
for(const directory of ['src','public','scripts','fixtures','tests/helpers','migrations/postgres']) {
  const files=(await readdir(join(work,directory),{recursive:true,withFileTypes:true}))
    .filter(entry=>entry.isFile()).map(entry=>join(entry.parentPath,entry.name)).sort();
  for(const file of files) {
    const relative=file.slice(work.length+1);
    if(relative==='src/data/release-artifact.json')continue;
    sourceHash.update(relative).update('\0').update(await readFile(file)).update('\0');
  }
}
for(const name of ['bun.lock','package.json','next.config.ts','tsconfig.json','postcss.config.mjs'])
  sourceHash.update(name).update('\0').update(await readFile(join(work,name))).update('\0');
const attachmentOps=[
  'ops/macos/install-attachment-runtime.py',
  'ops/macos/attachment-rollout.md',
  ...(await readdir(join(work,'ops/macos/clamav'),{withFileTypes:true})).filter(entry=>entry.isFile()).map(entry=>entry.name).sort().map(name=>`ops/macos/clamav/${name}`),
];
for(const name of attachmentOps)sourceHash.update(name).update('\0').update(await readFile(join(work,name))).update('\0');
for(const key of Object.keys(process.env).filter(key=>key.startsWith('NEXT_PUBLIC_')).sort())
  sourceHash.update(key).update('\0').update(process.env[key]??'').update('\0');
const softwareVersion=`release-${sourceHash.digest('hex')}`;
await writeFile(join(work,'src/data/release-artifact.json'),JSON.stringify({softwareVersion})+'\n');
// No development .env, database, fixture state or secrets enter the build tree.
const child=Bun.spawn([node,join(work,'node_modules/next/dist/bin/next'),'build'],{cwd:work,stdout:'inherit',stderr:'inherit',env:{...process.env,NODE_ENV:'production',ERPX_BUILD_DIR:'.next'}});
if(await child.exited!==0)throw new Error('NEXT_BUILD_FAILED');
await cp(join(work,'.next/standalone'),release,{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(work,'.next/static'),join(release,'.next/static'),{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(work,'public'),join(release,'public'),{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(work,'migrations/postgres'),join(release,'migrations/postgres'),{recursive:true});
await mkdir(join(release,'ops/macos'),{recursive:true});
await cp(join(work,'ops/macos/clamav'),join(release,'ops/macos/clamav'),{recursive:true});
await cp(join(work,'ops/macos/install-attachment-runtime.py'),join(release,'ops/macos/install-attachment-runtime.py'));
await cp(join(work,'ops/macos/attachment-rollout.md'),join(release,'ops/macos/attachment-rollout.md'));
await cp(join(work,'scripts/legal'),join(release,'scripts/legal'),{recursive:true});
const extraction=await Bun.build({entrypoints:[join(work,'scripts/legal/extract_docx.mjs')],outdir:join(release,'scripts/legal'),target:'node',format:'esm',naming:'extract_docx.mjs'});
if(!extraction.success)throw new Error('LEGAL_EXTRACTOR_BUILD_FAILED');
const tools=['jobs','launch','provision','bootstrap-admin','bootstrap-staging-admin','doctor','minio-snapshot','migrate-files-to-minio','reconcile-attachments'];
const bundle=await Bun.build({entrypoints:tools.map(name=>join(work,'scripts/local',`${name}.ts`)),outdir:join(release,'tools'),target:'node',format:'esm',conditions:['react-server'],external:['pg'],naming:'[name].mjs',
  // Mirror Next's server-layer resolution for permission helpers used by jobs.
  plugins:[{name:'next-server-navigation',setup(build){build.onResolve({filter:/^next\/navigation$/},()=>({path:join(work,'node_modules/next/dist/client/components/navigation.react-server.js')}));}}],
});
if(!bundle.success)throw new Error(bundle.logs.map(log=>log.message).join('\n'));
const schemaHash=createHash('sha256');
for(const track of (await readdir(join(work,'migrations/postgres'))).sort()) for(const file of (await readdir(join(work,'migrations/postgres',track))).filter(f=>f.endsWith('.sql')).sort())
  schemaHash.update(track+'/'+file).update(await readFile(join(work,'migrations/postgres',track,file)));
const attachmentRuntime=JSON.parse(await readFile(join(work,'ops/macos/clamav/runtime-versions.json'),'utf8'));
const manifest={id,softwareVersion,node:'24.20.0',postgres:'18.6',attachmentRuntime,createdAt:new Date().toISOString(),schemaSha256:schemaHash.digest('hex'),lockfileSha256:createHash('sha256').update(await readFile(join(work,'bun.lock'))).digest('hex')};
await writeFile(join(release,'release.json'),JSON.stringify(manifest,null,2));
await rm(join(root,'.runtime/releases/latest'),{force:true});await symlink(release,join(root,'.runtime/releases/latest'));
console.log(`Release: ${release}`);
