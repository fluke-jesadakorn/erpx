import { createHash } from 'node:crypto';
import { lstat, readFile, readdir, readlink, realpath } from 'node:fs/promises';
import { join, relative, resolve } from 'node:path';

type ArtifactInventory = { files: Record<string, string>; links: Record<string, string> };
/** A cache hit requires the complete tree, including link identities and no extras. */
export async function verifyAppArtifact(output: string, receipt: ArtifactInventory): Promise<boolean> {
  try {
    if (!(await lstat(output)).isDirectory() || !receipt.files || !receipt.links) return false;
    const physicalOutput = await realpath(output);
    const expected = new Set([...Object.keys(receipt.files), ...Object.keys(receipt.links), 'artifact.json']);
    if (expected.size !== Object.keys(receipt.files).length + Object.keys(receipt.links).length + 1) return false;
    for (const name of expected) if (!name || resolve(output, name) !== join(output, name) || !resolve(output, name).startsWith(output + '/')) return false;
    const actual = new Set<string>();
    async function visit(directory: string): Promise<boolean> {
      for (const entry of await readdir(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name), name = relative(output, path);
        if (entry.isDirectory()) { if (!await visit(path)) return false; continue; }
        if (!expected.has(name)) return false;
        actual.add(name);
        if (entry.isSymbolicLink()) {
          if (receipt.links[name] !== await readlink(path) || !(await realpath(path)).startsWith(physicalOutput + '/')) return false;
        } else if (entry.isFile()) {
          if (name !== 'artifact.json' && createHash('sha256').update(await readFile(path)).digest('hex') !== receipt.files[name]) return false;
        } else return false;
      }
      return true;
    }
    return await visit(output) && actual.size === expected.size;
  } catch { return false; }
}
