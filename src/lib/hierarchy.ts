/** Shared, source-neutral tree contract. IDs are opaque and globally namespaced. */
export type HierarchyNode = {
  id: string;
  parentId: string | null;
  kind: string;
  source: string;
  label: string;
  status: string;
  childCount: number;
};

export type DirectoryHierarchy = {
  containers: {
    externalId: string;
    parentId: string | null;
    name: string;
    kind: "ou" | "container";
  }[];
  placements: {
    kind: "identity" | "group";
    externalId: string;
    containerId: string | null;
  }[];
  groupEdges: { parentId: string; childId: string }[];
};

export const emptyDirectoryHierarchy = (): DirectoryHierarchy => ({
  containers: [],
  placements: [],
  groupEdges: [],
});

/** Reject ambiguous or incomplete topology before it can replace the last complete snapshot. */
export function validateDirectoryHierarchy(
  h: DirectoryHierarchy,
  users: string[],
  groups: string[],
) {
  function unique(ids: string[]) {
    if (ids.some((id) => !id) || new Set(ids).size !== ids.length)
      throw Error("Duplicate or missing directory identifier.");
  }
  unique(users);
  unique(groups);
  unique(h.containers.map((n) => n.externalId));
  unique(h.placements.map((n) => `${n.kind}:${n.externalId}`));
  unique(h.groupEdges.map((n) => JSON.stringify([n.parentId, n.childId])));
  const containers = new Set(h.containers.map((n) => n.externalId)),
    groupSet = new Set(groups),
    userSet = new Set(users);
  for (const n of h.containers)
    if (n.parentId !== null && !containers.has(n.parentId))
      throw Error("Missing directory container parent.");
  for (const n of h.placements)
    if (
      (n.containerId !== null && !containers.has(n.containerId)) ||
      !(n.kind === "identity" ? userSet : groupSet).has(n.externalId)
    )
      throw Error("Invalid directory placement.");
  for (const e of h.groupEdges)
    if (!groupSet.has(e.parentId) || !groupSet.has(e.childId))
      throw Error("Missing nested group.");
  function acyclic(
    ids: string[],
    edges: { parentId: string; childId: string }[],
  ) {
    const children = new Map<string, string[]>(),
      done = new Set<string>(),
      visiting = new Set<string>();
    for (const e of edges)
      children.set(e.parentId, [
        ...(children.get(e.parentId) ?? []),
        e.childId,
      ]);
    function visit(id: string, depth: number) {
      if (visiting.has(id) || depth > 64)
        throw Error(
          "Directory hierarchy contains a cycle or exceeds 64 levels.",
        );
      if (done.has(id)) return;
      visiting.add(id);
      for (const child of children.get(id) ?? []) visit(child, depth + 1);
      visiting.delete(id);
      done.add(id);
    }
    for (const id of ids) visit(id, 0);
  }
  acyclic(
    [...containers],
    h.containers.flatMap((n) =>
      n.parentId === null
        ? []
        : [{ parentId: n.parentId, childId: n.externalId }],
    ),
  );
  acyclic(groups, h.groupEdges);
}

export function descendants(nodes: HierarchyNode[], root: string) {
  const found = new Set([root]);
  for (let changed = true; changed;) {
    changed = false;
    for (const node of nodes)
      if (node.parentId && found.has(node.parentId) && !found.has(node.id)) {
        found.add(node.id);
        changed = true;
      }
  }
  return found;
}
