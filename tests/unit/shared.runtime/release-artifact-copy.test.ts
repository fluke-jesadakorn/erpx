import {expect,it} from 'vitest';
import {mkdir,mkdtemp,readFile,readlink,realpath,rename,rm,symlink,writeFile} from 'node:fs/promises';
import {join,resolve,sep} from 'node:path';
import {copyReleaseArtifact} from '../../../scripts/local/copy-release-artifact';

it('keeps nested standalone package symlinks usable after the original release is relocated',async()=>{
 await mkdir(resolve('.runtime/tests'),{recursive:true});
 const area=await mkdtemp(resolve('.runtime/tests/release-copy-')),source=join(area,'original'),copy=join(area,'isolated');
 try{
  await mkdir(join(source,'node_modules/@scope'),{recursive:true});
  await mkdir(join(source,'packages/example'),{recursive:true});await mkdir(join(source,'shared'));
  const bytes='export const value = "Original standalone release bytes";\n';
  await writeFile(join(source,'shared/entry.mjs'),bytes);
  await symlink('../../packages/example',join(source,'node_modules/@scope/example'));
  await symlink('../../shared/entry.mjs',join(source,'packages/example/entry.mjs'));
  await copyReleaseArtifact(source,copy);
  await rename(source,join(area,'original-preserved'));
  const entry=join(copy,'node_modules/@scope/example/entry.mjs');
  expect(await readFile(entry,'utf8')).toBe(bytes);
  expect(await readlink(join(copy,'node_modules/@scope/example'))).toBe('../../packages/example');
  expect(await readlink(join(copy,'packages/example/entry.mjs'))).toBe('../../shared/entry.mjs');
  expect((await realpath(entry)).startsWith(copy+sep)).toBe(true);
 }finally{await rm(area,{recursive:true,force:true});}
});
