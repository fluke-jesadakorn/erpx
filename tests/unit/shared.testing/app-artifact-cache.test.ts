import { createHash } from 'node:crypto';
import { cp, mkdtemp, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { it, expect } from 'vitest';
import { workspaceCopyInput } from '../../../scripts/local/workspace-copy-input';
import { verifyAppArtifact } from '../../../scripts/local/app-artifact-cache';

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


it('copies source while excluding private environment and generated state in nested workspaces', async () => {
  const root = await mkdtemp(join(tmpdir(), 'erpx-source-copy-')), source = join(root, 'source'), output = join(root, 'output');
  try {
    await mkdir(join(source, 'apps/finance/.cache'), { recursive: true });
    await writeFile(join(source, 'apps/finance/page.tsx'), 'export default 1;');
    await writeFile(join(source, 'apps/finance/.env.local'), 'FICTIONAL_PRIVATE_CONFIGURATION=omit');
    await writeFile(join(source, 'apps/finance/.cache/state'), 'generated');
    await cp(source, output, { recursive: true, filter: path => workspaceCopyInput(source, path) });
    expect(await readFile(join(output, 'apps/finance/page.tsx'), 'utf8')).toBe('export default 1;');
    await expect(readFile(join(output, 'apps/finance/.env.local'))).rejects.toMatchObject({ code: 'ENOENT' });
    await expect(readFile(join(output, 'apps/finance/.cache/state'))).rejects.toMatchObject({ code: 'ENOENT' });
    expect(workspaceCopyInput(source, join(root, 'outside.ts'))).toBe(false);
  } finally { await rm(root, { recursive: true, force: true }); }
});
