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
for(const name of ['src','public','package.json','bun.lock','next.config.ts','tsconfig.json','next-env.d.ts','postcss.config.mjs'])
  await cp(join(root,name),join(work,name),{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(root,'node_modules'),join(work,'node_modules'),{recursive:true,mode:constants.COPYFILE_FICLONE});
// No development .env, database, fixture state or secrets enter the build tree.
const child=Bun.spawn([node,join(work,'node_modules/next/dist/bin/next'),'build'],{cwd:work,stdout:'inherit',stderr:'inherit',env:{...process.env,NODE_ENV:'production',ERPX_BUILD_DIR:'.next'}});
if(await child.exited!==0)throw new Error('NEXT_BUILD_FAILED');
await cp(join(work,'.next/standalone'),release,{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(work,'.next/static'),join(release,'.next/static'),{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(work,'public'),join(release,'public'),{recursive:true,mode:constants.COPYFILE_FICLONE});
await cp(join(root,'migrations/postgres'),join(release,'migrations/postgres'),{recursive:true});
await cp(join(root,'scripts/legal'),join(release,'scripts/legal'),{recursive:true});
const extraction=await Bun.build({entrypoints:[join(root,'scripts/legal/extract_docx.mjs')],outdir:join(release,'scripts/legal'),target:'node',format:'esm',naming:'extract_docx.mjs'});
if(!extraction.success)throw new Error('LEGAL_EXTRACTOR_BUILD_FAILED');
const tools=['jobs','launch','provision','bootstrap-admin','doctor'];
const bundle=await Bun.build({entrypoints:tools.map(name=>join(root,'scripts/local',`${name}.ts`)),outdir:join(release,'tools'),target:'node',format:'esm',conditions:['react-server'],external:['pg'],naming:'[name].mjs'});
if(!bundle.success)throw new Error(bundle.logs.map(log=>log.message).join('\n'));
const schemaHash=createHash('sha256');
for(const track of (await readdir(join(root,'migrations/postgres'))).sort()) for(const file of (await readdir(join(root,'migrations/postgres',track))).filter(f=>f.endsWith('.sql')).sort())
  schemaHash.update(track+'/'+file).update(await readFile(join(root,'migrations/postgres',track,file)));
const manifest={id,node:'24.20.0',postgres:'18.6',createdAt:new Date().toISOString(),schemaSha256:schemaHash.digest('hex'),lockfileSha256:createHash('sha256').update(await readFile(join(root,'bun.lock'))).digest('hex')};
await writeFile(join(release,'release.json'),JSON.stringify(manifest,null,2));
await rm(join(root,'.runtime/releases/latest'),{force:true});await symlink(release,join(root,'.runtime/releases/latest'));
console.log(`Release: ${release}`);
