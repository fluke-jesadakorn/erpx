import { readlink, readdir, realpath, unlink, symlink } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";

/** Traced dependency aliases must survive moving and deleting the build tree. */
export async function relocateStandaloneLinks(standalone: string, release: string): Promise<number> {
  const sourceRoot = resolve(standalone), releaseRoot = await realpath(release);
  const canonicalSource = await realpath(standalone).catch((error: NodeJS.ErrnoException) => {
    if (error.code !== "ENOENT") throw error;
    return sourceRoot;
  });
  const inside = (file: string, root: string) => file === root || file.startsWith(root + sep);
  const entries = await readdir(releaseRoot, { recursive: true, withFileTypes: true });
  let count = 0;
  for (const entry of entries.filter(entry => entry.isSymbolicLink())) {
    const file = join(entry.parentPath, entry.name);
    const target = resolve(dirname(file), await readlink(file));
    const source = [sourceRoot, canonicalSource].find(root => inside(target, root));
    const relocated = source ? join(releaseRoot, relative(source, target)) : target;
    if (!inside(relocated, releaseRoot)) throw new Error(`STANDALONE_EXTERNAL_LINK:${relative(releaseRoot, file)}`);
    const replacement = relative(dirname(file), relocated);
    await unlink(file);
    await symlink(replacement, file);
    const actual = await realpath(file);
    if (!inside(actual, releaseRoot)) throw new Error(`STANDALONE_EXTERNAL_LINK:${relative(releaseRoot, file)}`);
    count++;
  }
  return count;
}
