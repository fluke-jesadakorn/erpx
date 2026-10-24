import type { WorkspaceGraph } from '../testing/workspace-graph';

const serverOwner = (file: string) => /^packages\/(server-[^/]+)\/src\//.exec(file)?.[1];
/** Runtime subpath edges, including literal dynamic imports, determine whether
 * owner implementations recursively load one another. Type contracts do not.
 * Package manifests remain complete for Bun resolution and cache invalidation. */
export function runtimeOwnerCycles(graph: Pick<WorkspaceGraph, 'runtimeImports'>) {
  const indices = new Map<string, number>(), low = new Map<string, number>();
  const stack: string[] = [], active = new Set<string>();
  const violations: { owners: string[]; files: string[] }[] = [];
  let sequence = 0;
  function visit(file: string) {
    indices.set(file, sequence); low.set(file, sequence++);
    stack.push(file); active.add(file);
    for (const dependency of graph.runtimeImports.get(file) ?? []) {
      if (!indices.has(dependency)) {
        visit(dependency);
        low.set(file, Math.min(low.get(file)!, low.get(dependency)!));
      } else if (active.has(dependency)) {
        low.set(file, Math.min(low.get(file)!, indices.get(dependency)!));
      }
    }
    if (low.get(file) !== indices.get(file)) return;
    const files: string[] = [];
    let member: string;
    do { member = stack.pop()!; active.delete(member); files.push(member); } while (member !== file);
    const owners = [...new Set(files.map(serverOwner).filter((value): value is string => !!value))].sort();
    if (owners.length > 1) violations.push({ owners, files: files.sort() });
  }
  for (const file of graph.runtimeImports.keys()) if (!indices.has(file)) visit(file);
  return violations.sort((left, right) => left.files[0].localeCompare(right.files[0]));
}
