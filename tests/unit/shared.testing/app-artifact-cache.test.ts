import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, readlink, realpath, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { it, expect } from 'vitest';
import { workspaceCopyInput, copyWorkspaceSnapshot } from '../../../scripts/local/workspace-copy-input';
import { verifyAppArtifact } from '../../../scripts/local/app-artifact-cache';
import { artifactSymbolicLinks, relocateCompilerDependencyAliases } from '../../../scripts/local/standalone-links';

it('requires the exact artifact tree and treats damaged or escaping links as cache misses', async () => {
  const root = await mkdtemp(join(tmpdir(), 'erpx-artifact-')), output = join(root, 'artifact');
  try {
    await mkdir(output); await writeFile(join(output, 'server.js'), 'runtime');
    await writeFile(join(output, 'artifact.json'), '{}'); await symlink('server.js', join(output, 'entry.js'));
    const receipt = { files: { 'server.js': createHash('sha256').update('runtime').digest('hex') }, links: { 'entry.js': 'server.js' } };
    expect(await verifyAppArtifact(output, receipt)).toBe(true);
    await writeFile(join(output, 'extra.js'), 'unexpected');
    expect(await verifyAppArtifact(output, receipt)).toBe(false);
    await rm(join(output, 'extra.js')); await writeFile(join(output, 'server.js'), 'changed');
    expect(await verifyAppArtifact(output, receipt)).toBe(false);
    await writeFile(join(output, 'server.js'), 'runtime'); await rm(join(output, 'entry.js'));
    await symlink('../outside.js', join(output, 'entry.js')); await writeFile(join(root, 'outside.js'), 'runtime');
    expect(await verifyAppArtifact(output, { ...receipt, links: { 'entry.js': '../outside.js' } })).toBe(false);
    await rm(join(root, 'outside.js'));
    expect(await verifyAppArtifact(output, receipt)).toBe(false);
  } finally { await rm(root, { recursive: true, force: true }); }
});

async function compilerAliasFixture(){
  const root=await realpath(await mkdtemp(join(tmpdir(),'erpx-compiler-alias-'))),modules=join(root,'snapshot/node_modules'),compiled=join(root,'snapshot/apps/learn/.build'),output=join(root,'artifact');
  for(const path of [join(modules,'pg'),join(compiled,'node_modules'),join(output,'node_modules'),join(output,'standalone/node_modules/pg')])await mkdir(path,{recursive:true});
  const manifest=JSON.stringify({name:'pg',version:'8.23.1'});
  for(const path of [join(modules,'pg/package.json'),join(output,'standalone/node_modules/pg/package.json')])await writeFile(path,manifest);
  await symlink(join(modules,'pg'),join(compiled,'node_modules/pg-hash'),'dir');
  await symlink(join(modules,'pg'),join(output,'node_modules/pg-hash'),'dir');
  return{root,modules,compiled,output};
}
it('relocates compiler dependency aliases into the exact traced runtime package',async()=>{
  const f=await compilerAliasFixture();try{
    expect(await relocateCompilerDependencyAliases(f.compiled,f.output,f.modules)).toBe(1);
    expect(await realpath(join(f.output,'node_modules/pg-hash'))).toBe(join(f.output,'standalone/node_modules/pg'));
    expect(await readlink(join(f.output,'node_modules/pg-hash'))).toBe('../standalone/node_modules/pg');
  }finally{await rm(f.root,{recursive:true,force:true});}
});
it('rejects changed traced package identity before mutating any compiler alias',async()=>{
  const f=await compilerAliasFixture();try{
    await writeFile(join(f.output,'standalone/node_modules/pg/package.json'),JSON.stringify({name:'pg',version:'different'}));
    await expect(relocateCompilerDependencyAliases(f.compiled,f.output,f.modules)).rejects.toThrow('COMPILER_ALIAS_PACKAGE_IDENTITY_CHANGED');
    expect(await readlink(join(f.output,'node_modules/pg-hash'))).toBe(join(f.modules,'pg'));
  }finally{await rm(f.root,{recursive:true,force:true});}
});
it('enumerates aliases without descending into or mutating their foreign targets',async()=>{
  const f=await compilerAliasFixture();try{
    const foreign=join(f.root,'foreign');await mkdir(foreign);await writeFile(join(foreign,'keep'),'retain');await symlink('keep',join(foreign,'nested'));
    await rm(join(f.compiled,'node_modules/pg-hash'));await symlink(foreign,join(f.compiled,'node_modules/pg-hash'),'dir');
    await rm(join(f.output,'node_modules/pg-hash'));await symlink(foreign,join(f.output,'node_modules/pg-hash'),'dir');
    expect(await artifactSymbolicLinks(join(f.output,'node_modules'))).toEqual([join(f.output,'node_modules/pg-hash')]);
    await expect(relocateCompilerDependencyAliases(f.compiled,f.output,f.modules)).rejects.toThrow('COMPILER_ALIAS_SOURCE_OUTSIDE_SNAPSHOT');
    expect(await readlink(join(foreign,'nested'))).toBe('keep');expect(await readFile(join(foreign,'keep'),'utf8')).toBe('retain');
  }finally{await rm(f.root,{recursive:true,force:true});}
});


it('copies source while excluding private environment and generated state in nested workspaces', async () => {
  const root = await mkdtemp(join(tmpdir(), 'erpx-source-copy-')), source = join(root, 'source'), output = join(root, 'output');
  try {
    await mkdir(join(source, 'apps/finance/.cache'), { recursive: true });
    await writeFile(join(source, 'apps/finance/page.tsx'), 'export default 1;');
    await writeFile(join(source, 'apps/finance/.env.local'), 'FICTIONAL_PRIVATE_CONFIGURATION=omit');
    await writeFile(join(source, 'apps/finance/.cache/state'), 'generated');
    await symlink('page.tsx',join(source,'apps/finance/entry.tsx'));
    await copyWorkspaceSnapshot(source, output,path => workspaceCopyInput(source, path));
    expect(await readlink(join(output,'apps/finance/entry.tsx'))).toBe('page.tsx');
    await expect(copyWorkspaceSnapshot(source,output,path=>workspaceCopyInput(source,path))).rejects.toMatchObject({code:'EEXIST'});
    expect(await readFile(join(output, 'apps/finance/page.tsx'), 'utf8')).toBe('export default 1;');
    await expect(readFile(join(output, 'apps/finance/.env.local'))).rejects.toMatchObject({ code: 'ENOENT' });
    await expect(readFile(join(output, 'apps/finance/.cache/state'))).rejects.toMatchObject({ code: 'ENOENT' });
    expect(workspaceCopyInput(source, join(root, 'outside.ts'))).toBe(false);
  } finally { await rm(root, { recursive: true, force: true }); }
});
