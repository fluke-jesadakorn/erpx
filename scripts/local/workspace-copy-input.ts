import { relative, resolve, sep } from 'node:path';
const privateDirectories = new Set(['.next', '.build', '.cache', '.turbo', '.scratch', '.runtime', '.git']);
/** Source snapshots never inherit local configuration, state or generated output. */
export function workspaceCopyInput(root: string, path: string) {
  const base = resolve(root), target = resolve(path);
  if (target !== base && !target.startsWith(base + sep)) return false;
  return !relative(base, target).split(sep).some(part => privateDirectories.has(part) || /^\.env(?:\.|$)/.test(part) || part === '.DS_Store');
}
