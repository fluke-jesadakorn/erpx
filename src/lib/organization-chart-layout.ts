import dagre from "@dagrejs/dagre";
import type { OrganizationGraph } from "./organization-chart";

export const chartCardWidth = 256;
export const chartCardHeight = 256;

export function layoutOrganizationChart(graph: OrganizationGraph, ids: string[]) {
  const layout = new dagre.graphlib.Graph().setDefaultEdgeLabel(() => ({}));
  layout.setGraph({ rankdir: "TB", nodesep: 28, ranksep: 64, marginx: 24, marginy: 24 });
  const visible = new Set(ids);
  for (const id of ids) layout.setNode(id, { width: chartCardWidth, height: chartCardHeight });
  const edges: { id: string; source: string; target: string }[] = [];
  for (const id of ids) {
    const parent = graph.parents.get(id);
    if (!parent || !visible.has(parent)) continue;
    layout.setEdge(parent, id);
    edges.push({ id: parent + ":" + id, source: parent, target: id });
  }
  dagre.layout(layout);
  return {
    positions: new Map(ids.map(id => {
      const node = layout.node(id);
      return [id, { x: node.x - chartCardWidth / 2, y: node.y - chartCardHeight / 2 }];
    })),
    edges,
  };
}
