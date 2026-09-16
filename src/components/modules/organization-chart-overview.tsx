"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ReactFlow, Handle, Position, Background, BackgroundVariant, useNodesInitialized, useReactFlow, useStore, ReactFlowProvider, type Node, type NodeProps, type OnNodesChange } from "@xyflow/react";
import { Expand, Minus, Plus, UnfoldVertical, FoldVertical } from "lucide-react";
import { visibleChartIds, type OrganizationGraph } from "@/lib/organization-chart";
import { layoutOrganizationChart, chartCardHeight, chartCardWidth } from "@/lib/organization-chart-layout";
import { OrganizationPersonCard, type OrganizationPersonCardProps } from "./organization-person-card";
import "@xyflow/react/dist/style.css";
import styles from "./organization-chart.module.css";

type PersonNode = Node<OrganizationPersonCardProps & Record<string, unknown>, "person">;
function PersonChartNode({ data }: NodeProps<PersonNode>) {
  return <><Handle type="target" position={Position.Top} className={styles.handle}/><OrganizationPersonCard {...data} compact/><Handle type="source" position={Position.Bottom} className={styles.handle}/></>;
}
const nodeTypes = { person: PersonChartNode };
type Props = {
  graph: OrganizationGraph;
  expanded: Set<string>;
  focusId: string | null;
  focusRequest: number;
  onDetails: (id: string) => void;
  onTeam: (id: string) => void;
  onToggle: (id: string) => void;
  onExpandAll: () => void;
  onCollapseAll: () => void;
};
function ViewportFocus({ focusId, request, layoutKey }: { focusId: string | null; request: number; layoutKey: string }) {
  const initialized = useNodesInitialized();
  const size = useStore(state => state.width + "x" + state.height);
  const { getNode, fitView, viewportInitialized } = useReactFlow();
  const previous = useRef<{ request: number; layoutKey: string; size: string } | null>(null);
  useEffect(() => {
    // Fixed-size nodes can be ready before the pan/zoom viewport has mounted.
    if (!initialized || !viewportInitialized || size.split("x").some(value => Number(value) <= 0)) return;
    const changedFocus = request > 0 && previous.current?.request !== request;
    const resized = previous.current && previous.current.size !== size;
    if ((changedFocus || (resized && request > 0)) && focusId) {
      const node = getNode(focusId);
      if (!node) return;
      void fitView({ nodes: [{ id: focusId }], padding: 0.08, maxZoom: 1, duration: 0 });
    } else if (!previous.current || previous.current.layoutKey !== layoutKey || resized) {
      void fitView({ padding: 0.18, minZoom: 0.001, maxZoom: 1, duration: 0 });
    }
    previous.current = { request, layoutKey, size };
  }, [initialized, viewportInitialized, request, focusId, layoutKey, size, getNode, fitView]);
  return null;
}
function Overview(props: Props) {
  const { graph, expanded, focusId, focusRequest, onDetails, onTeam, onToggle, onExpandAll, onCollapseAll } = props;
  const ids = useMemo(() => visibleChartIds(graph, expanded), [graph, expanded]);
  const layout = useMemo(() => layoutOrganizationChart(graph, ids), [graph, ids]);
  const { zoomIn, zoomOut, fitView } = useReactFlow();
  const [zoom, setZoom] = useState(1);
  const [measurements, setMeasurements] = useState<Record<string, { width: number; height: number }>>({});
  // Controlled nodes must retain measured dimensions so viewport initialization can finish.
  const onNodesChange = useCallback<OnNodesChange<PersonNode>>(changes => {
    const dimensions = changes.filter(change => change.type === "dimensions" && change.dimensions);
    if (!dimensions.length) return;
    setMeasurements(previous => {
      let next = previous;
      for (const change of dimensions) {
        if (change.type !== "dimensions" || !change.dimensions) continue;
        const size = change.dimensions;
        if (previous[change.id]?.width !== size.width || previous[change.id]?.height !== size.height) next = { ...next, [change.id]: size };
      }
      return next;
    });
  }, []);
  const nodes = useMemo<PersonNode[]>(() => ids.map(id => ({
    id, type: "person", position: layout.positions.get(id)!, width: chartCardWidth, height: chartCardHeight, measured: measurements[id],
    data: { entry: graph.entries.get(id)!, reports: graph.children.get(id)?.length ?? 0, focused: focusId === id, expanded: expanded.has(id), onDetails: () => onDetails(id), onTeam: () => onTeam(id), onToggle: () => onToggle(id) },
  })), [ids, layout, graph, focusId, expanded, onDetails, onTeam, onToggle, measurements]);
  const edges = useMemo(() => layout.edges.map(edge => ({ ...edge, ariaLabel: graph.entries.get(edge.source)!.person.legal_name + " manages " + graph.entries.get(edge.target)!.person.legal_name })), [layout, graph]);
  return <div className={styles.overview} data-testid="organization-overview">
    <div className={styles.canvasTools} role="group" aria-label="Overview controls">
      <div><button type="button" onClick={onExpandAll}><UnfoldVertical size={16} aria-hidden="true"/>Expand all</button><button type="button" onClick={onCollapseAll}><FoldVertical size={16} aria-hidden="true"/>Collapse</button></div>
      <div><button type="button" aria-label="Zoom out" onClick={() => void zoomOut()}><Minus size={16} aria-hidden="true"/></button><output aria-label="Chart zoom">{zoom < 0.01 ? (zoom * 100).toFixed(1) : Math.round(zoom * 100)}%</output><button type="button" aria-label="Zoom in" onClick={() => void zoomIn()}><Plus size={16} aria-hidden="true"/></button><button type="button" aria-label="Fit chart to screen" onClick={() => void fitView({ padding: .18, minZoom: 0.001, maxZoom: 1 })}><Expand size={16} aria-hidden="true"/></button></div>
    </div>
    <div className={styles.canvas}>
      <ReactFlow<PersonNode> nodes={nodes} onNodesChange={onNodesChange} edges={edges} nodeTypes={nodeTypes} minZoom={0.001} maxZoom={1.5} nodesDraggable={false} nodesConnectable={false} edgesReconnectable={false} deleteKeyCode={null} selectionOnDrag={false} panOnDrag zoomOnPinch zoomOnScroll={false} zoomOnDoubleClick={false} nodesFocusable={true} ariaLabelConfig={{ "node.a11yDescription.default": "Use Tab to view staff details or explore this team." }} edgesFocusable={false} onMove={(_, viewport) => setZoom(viewport.zoom)} defaultEdgeOptions={{ type: "smoothstep", style: { stroke: "var(--muted)", strokeWidth: 1.5 } }} preventScrolling={false} proOptions={{ hideAttribution: false }}>
        <Background variant={BackgroundVariant.Dots} gap={22} size={1} color="var(--line)"/>
        <ViewportFocus focusId={focusId} request={focusRequest} layoutKey={ids.join("|")}/>
      </ReactFlow>
    </div>
    <p className={styles.canvasHint}>Drag to explore · Pinch or use + / − to zoom · {ids.length} of {graph.entries.size} profiles shown</p>
  </div>;
}
export default function OrganizationChartOverview(props: Props) {
  return <ReactFlowProvider><Overview {...props}/></ReactFlowProvider>;
}
