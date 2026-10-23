import type { WorkspaceGraph } from '../testing/workspace-graph';

const leaves = [
  'packages/server-identity_access/src/public/iam-service-reads.ts',
  'packages/server-identity_access/src/public/iam-grants.ts',
  'packages/server-identity_access/src/public/directory-lookup.ts',
  'packages/server-identity_access/src/public/directory-synchronization.ts',
  'packages/server-runtime/src/adapters/runtime/filesystem.ts',
  'packages/server-runtime/src/adapters/runtime/minio.ts',
  'packages/server-pms/src/public/feature-availability.ts',
  'packages/server-platform_administration/src/public/release-predicate.ts',
];
const commandCompositions = new Set([
  'packages/server-identity_access/src/adapters/iam-service.ts',
  'packages/server-identity_access/src/adapters/iam-directories.ts',
  'packages/server-workflow/src/adapters/commands.ts',
  'packages/server-legal/src/adapters/retention-execution.ts',
  'packages/server-identity_access/src/adapters/authorization.ts',
]);
/** These read/primitive entry points must not regain the business command graph. */
export function runtimeLeafViolations(graph: Pick<WorkspaceGraph, 'runtimeImports'>) {
  const violations: { source: string; target: string }[] = [];
  for (const source of leaves) {
    const pending = [source], visited = new Set<string>();
    for (let index = 0; index < pending.length; index++) {
      const file = pending[index];
      if (visited.has(file)) continue;
      visited.add(file);
      if (commandCompositions.has(file)) violations.push({ source, target: file });
      for (const dependency of graph.runtimeImports.get(file) ?? []) pending.push(dependency);
    }
  }
  return violations;
}
