"use client";
import { useRef, useState } from "react";

/** Presentation data only; owner models can satisfy this contract structurally. */
export type HierarchyNode = {
  id: string;
  parentId: string | null;
  kind: string;
  source: string;
  label: string;
  status: string;
  childCount: number;
};

/** Controlled selection/expansion, with WAI-ARIA tree keyboard navigation. */
export function HierarchyTree({
  nodes,
  selectedId,
  expandedIds,
  onSelect,
  onExpand,
}: {
  nodes: HierarchyNode[];
  selectedId: string;
  expandedIds: string[];
  onSelect: (id: string) => void;
  onExpand: (ids: string[]) => void;
}) {
  const [focused, setFocused] = useState(selectedId),
    refs = useRef(new Map<string, HTMLLIElement>());
  const expanded = new Set(expandedIds),
    visible: { node: HierarchyNode; level: number }[] = [];
  function flatten(parent: string | null, level: number) {
    for (const node of nodes.filter((n) => n.parentId === parent)) {
      visible.push({ node, level });
      if (expanded.has(node.id)) flatten(node.id, level + 1);
    }
  }
  flatten(null, 1);
  const focus = (id: string) => {
    setFocused(id);
    refs.current.get(id)?.focus();
  };
  const toggle = (id: string, open: boolean) =>
    onExpand(open ? [...expandedIds, id] : expandedIds.filter((v) => v !== id));
  return (
    <ul role="tree" aria-label="People hierarchy">
      {visible.map(({ node, level }, index) => {
        const siblings = nodes.filter((n) => n.parentId === node.parentId);
        return (
          <li
            key={node.id}
            role="treeitem"
            aria-level={level}
            aria-posinset={siblings.findIndex((n) => n.id === node.id) + 1}
            aria-setsize={siblings.length}
            aria-selected={node.id === selectedId}
            aria-expanded={node.childCount ? expanded.has(node.id) : undefined}
            tabIndex={
              node.id ===
              (visible.some((v) => v.node.id === focused)
                ? focused
                : visible.some((v) => v.node.id === selectedId)
                  ? selectedId
                  : visible[0]?.node.id)
                ? 0
                : -1
            }
            ref={(el) => {
              if (el) refs.current.set(node.id, el);
              else refs.current.delete(node.id);
            }}
            style={{ paddingInlineStart: `${(level - 1) * 16 + 10}px` }}
            onFocus={() => setFocused(node.id)}
            onClick={() => {
              focus(node.id);
              onSelect(node.id);
            }}
            onKeyDown={(e) => {
              const next =
                e.key === "ArrowDown"
                  ? visible[index + 1]?.node.id
                  : e.key === "ArrowUp"
                    ? visible[index - 1]?.node.id
                    : e.key === "Home"
                      ? visible[0]?.node.id
                      : e.key === "End"
                        ? visible.at(-1)?.node.id
                        : null;
              if (next) {
                e.preventDefault();
                focus(next);
              } else if (e.key === "ArrowRight") {
                e.preventDefault();
                if (node.childCount && !expanded.has(node.id)) toggle(node.id, true);
                else if (visible[index + 1]?.level > level) focus(visible[index + 1].node.id);
              } else if (e.key === "ArrowLeft") {
                e.preventDefault();
                if (expanded.has(node.id)) toggle(node.id, false);
                else if (node.parentId) focus(node.parentId);
              } else if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onSelect(node.id);
              } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) {
                const match = [...visible.slice(index + 1), ...visible.slice(0, index)].find((v) =>
                  v.node.label.toLowerCase().startsWith(e.key.toLowerCase()),
                );
                if (match) focus(match.node.id);
              }
            }}
          >
            <span
              aria-hidden="true"
              onClick={(e) => {
                e.stopPropagation();
                focus(node.id);
                toggle(node.id, !expanded.has(node.id));
              }}
            >
              {node.childCount ? (expanded.has(node.id) ? "▾ " : "▸ ") : "· "}
            </span>
            <span>
              {node.label}
              <small>
                {node.kind} · {node.source}
                {node.status !== "active" ? ` · ${node.status}` : ""}
              </small>
            </span>
          </li>
        );
      })}
    </ul>
  );
}
