/** Actual Linux RO-mount regression probe; only disposable labeled containers supply these paths. */
import assert from 'node:assert/strict';
import {chmod,cp,mkdir,readFile,rm,symlink,unlink,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {prepareImmutableAppVendor,validateImmutableAppVendor} from '../../../scripts/testing/engineering/immutable-app-vendor';
import {runtimeSourceFingerprint} from '../../../scripts/local/source-fingerprint';

const base='/tmp/erpx-vendor-probe',nonce='a'.repeat(32),mode=process.argv[2];
if(process.platform!=='linux'||process.arch!=='arm64'||!['valid','external-top','external-nested','source-file','source-directory','source-private-alias','vendor-drift','restored'].includes(mode??''))throw Error('EXPLICIT_LINUX_VENDOR_PROBE_REQUIRED');
const roots=[join(base,'source'),join(base,'build')];
const validate=()=>validateImmutableAppVendor(roots[0]!,base,roots[1]!,nonce);
async function put(path:string,bytes:string|Buffer){await mkdir(join(path,'..'),{recursive:true});await writeFile(path,bytes);}

if(mode==='vendor-drift'){
 await assert.rejects(validate,/ENGINEERING_BUILD_SOURCE_CHANGED/);
 console.log('IMMUTABLE_VENDOR_PROBE_PASS:vendor-drift');
}else if(mode==='restored'){
 await validate();
 console.log('IMMUTABLE_VENDOR_PROBE_PASS:restored');
}else{
 await chmod(base,0o700);
 await mkdir('/tmp/outside-vendor',{recursive:true});await writeFile('/tmp/outside-vendor/index.js','external dependency\n');
 await mkdir('/tmp/outside-source',{recursive:true});await writeFile('/tmp/outside-source/index.ts','export const escaped=true;\n');
 const pdf=await readFile('/tmp/prepare-pdf-assets.ts');
 for(const tree of roots){
  const files:Record<string,string|Buffer>={
   'package.json':'{"private":true,"type":"module"}\n','bun.lock':'synthetic locked vendor fixture\n','next.config.ts':'export default {};\n','tsconfig.base.json':'{}\n','postcss.config.mjs':'export default {};\n',
   'packages/owner/package.json':'{"name":"@erpx/owner","type":"module"}\n','packages/owner/index.ts':'export const original=true;\n','public/owned.txt':'owned public bytes\n','scripts/assets/prepare-pdf-assets.ts':pdf,
   'ops/macos/install-attachment-runtime.py':'# fixture\n','ops/macos/attachment-rollout.md':'fixture\n','ops/macos/erpxctl.py':'# fixture\n','ops/macos/offsite.py':'# fixture\n','ops/macos/recovery_status.py':'# fixture\n','ops/macos/clamav/fixture.conf':'fixture\n',
  };
  for(const [path,bytes]of Object.entries(files))await put(join(tree,path),bytes);
  await mkdir(join(tree,'apps/service'),{recursive:true});await symlink('../../public',join(tree,'apps/service/public'));
  if(mode==='source-file')await symlink('/tmp/outside-source/index.ts',join(tree,'packages/owner/escaped.ts'));
  if(mode==='source-directory')await symlink('/tmp/outside-source',join(tree,'packages/owner/escaped-directory'));
  if(mode==='source-private-alias'){
   await mkdir(join(tree,'.scratch/hidden'),{recursive:true});
   await symlink('/tmp/outside-source/index.ts',join(tree,'.scratch/hidden/escaped.ts'));
   await symlink('../../.scratch/hidden',join(tree,'packages/owner/private-alias'));
  }
 }
 if(mode==='external-top'||mode==='external-nested'){
  await assert.rejects(()=>prepareImmutableAppVendor(base,nonce),/ENGINEERING_VENDOR_LINK_OUTSIDE_CASE/);
 }else if(mode==='source-file'||mode==='source-directory'||mode==='source-private-alias'){
  await assert.rejects(()=>prepareImmutableAppVendor(base,nonce),/ENGINEERING_SOURCE_LINK_OUTSIDE_CASE/);
 }else{
  const identity=await prepareImmutableAppVendor(base,nonce),accepted=await validate();
  assert.equal(accepted.vendorSha256,identity.vendorSha256);assert.equal(accepted.sourceFingerprint,identity.sourceFingerprint);
  const modules=join(roots[1]!,'node_modules');
  await assert.rejects(()=>writeFile(join(modules,'react/index.js'),'forbidden write\n'),(error:unknown)=>(error as NodeJS.ErrnoException).code==='EROFS');
  await assert.rejects(()=>validateImmutableAppVendor(roots[0]!,base,roots[1]!,'b'.repeat(32)),/ENGINEERING_BUILD_IDENTITY_INVALID/);
  // Both trees changing equally must still fail the original sealed identity.
  for(const tree of roots)await writeFile(join(tree,'packages/owner/index.ts'),'export const original=false;\n');
  await assert.rejects(validate,/ENGINEERING_BUILD_SOURCE_CHANGED/);
  for(const tree of roots)await writeFile(join(tree,'packages/owner/index.ts'),'export const original=true;\n');
  await validate();
  // Equal rendered bytes do not make changed compiler path topology equivalent.
  for(const tree of roots){
   await cp(join(tree,'public'),join(tree,'public-copy'),{recursive:true});
   await unlink(join(tree,'apps/service/public'));await symlink('../../public-copy',join(tree,'apps/service/public'));
   assert.equal(await runtimeSourceFingerprint(tree),identity.sourceFingerprint);
  }
  await assert.rejects(validate,/ENGINEERING_SOURCE_LINKS_CHANGED/);
  for(const tree of roots){await unlink(join(tree,'apps/service/public'));await symlink('../../public',join(tree,'apps/service/public'));await rm(join(tree,'public-copy'),{recursive:true});}
  await validate();
 }
 console.log('IMMUTABLE_VENDOR_PROBE_PASS:'+mode);
}
