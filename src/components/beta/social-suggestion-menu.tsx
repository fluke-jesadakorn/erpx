"use client";
import { Button } from "@/components/ui/foundation";

import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import type { Suggestion } from "@/server/modules/collaboration/public/rules/social-text";
import styles from "./social-text.module.css";
import { type UiAppearance } from "@/components/ui/foundation";

export type SuggestionAnchor = {
  caret: { left: number; top: number; bottom: number };
  bounds: { left: number; right: number; width: number };
};

export function textareaSuggestionAnchor(
  node: HTMLTextAreaElement,
  caret: number,
): SuggestionAnchor {
  const rect = node.getBoundingClientRect();
  const computed = window.getComputedStyle(node);
  const mirror = document.createElement("div");
  const copied = [
    "boxSizing",
    "fontFamily",
    "fontSize",
    "fontStyle",
    "fontWeight",
    "fontVariant",
    "letterSpacing",
    "lineHeight",
    "textAlign",
    "textIndent",
    "textTransform",
    "paddingTop",
    "paddingRight",
    "paddingBottom",
    "paddingLeft",
    "borderTopWidth",
    "borderRightWidth",
    "borderBottomWidth",
    "borderLeftWidth",
  ] as const;
  for (const property of copied) mirror.style[property] = computed[property];
  Object.assign(mirror.style, {
    position: "fixed",
    left: `${rect.left}px`,
    top: `${rect.top}px`,
    width: `${rect.width}px`,
    height: "auto",
    minHeight: "0",
    maxHeight: "none",
    overflow: "hidden",
    visibility: "hidden",
    whiteSpace: "pre-wrap",
    overflowWrap: "break-word",
    wordWrap: "break-word",
    pointerEvents: "none",
    zIndex: "-1",
  });
  mirror.textContent = node.value.slice(0, caret);
  const marker = document.createElement("span");
  marker.textContent = "\u200b";
  mirror.append(marker);
  document.body.append(mirror);
  const markerRect = marker.getBoundingClientRect();
  const lineHeight =
    Number.parseFloat(computed.lineHeight) || Number.parseFloat(computed.fontSize) * 1.4 || 20;
  const anchor = {
    caret: {
      left: markerRect.left - node.scrollLeft,
      top: markerRect.top - node.scrollTop,
      bottom: markerRect.top - node.scrollTop + lineHeight,
    },
    bounds: { left: rect.left, right: rect.right, width: rect.width },
  };
  mirror.remove();
  return anchor;
}

export function SocialSuggestionMenu({ appearance = "legacy",
  open,
  listId,
  trigger,
  results,
  selectedIndex,
  status,
  getAnchor,
  onChoose,
  onHoverIndex,
}: ({
  open: boolean;
  listId: string;
  trigger: "@" | "#";
  results: Suggestion[];
  selectedIndex: number;
  status: string;
  getAnchor: () => SuggestionAnchor | null;
  onChoose: (suggestion: Suggestion) => void;
  onHoverIndex: (index: number) => void;
}) & { appearance?: UiAppearance }) {
  const menu = useRef<HTMLDivElement>(null);
  const active = useRef<HTMLButtonElement>(null);
  const [style, setStyle] = useState<CSSProperties | null>(null);
  const updatePosition = useCallback(() => {
    const anchor = getAnchor();
    if (!anchor) return;
    const viewport = window.visualViewport;
    const viewportLeft = viewport?.offsetLeft ?? 0;
    const viewportTop = viewport?.offsetTop ?? 0;
    const viewportWidth = viewport?.width ?? document.documentElement.clientWidth;
    const viewportHeight = viewport?.height ?? window.innerHeight;
    const menuWidth = Math.min(Math.max(anchor.bounds.width, 280), 560, viewportWidth - 16);
    const left = Math.max(
      viewportLeft + 8,
      Math.min(anchor.caret.left, viewportLeft + viewportWidth - menuWidth - 8),
    );
    const above = Math.max(0, anchor.caret.top - viewportTop - 16);
    const below = Math.max(0, viewportTop + viewportHeight - anchor.caret.bottom - 16);
    const height = menu.current?.getBoundingClientRect().height ?? 240;
    const placeAbove = above >= Math.min(height, 160) || above >= below;
    const available = placeAbove ? above : below;
    const maxHeight = Math.max(80, Math.min(360, available - 8));
    const visibleHeight = Math.min(height, maxHeight);
    const top = placeAbove
      ? Math.max(viewportTop + 8, anchor.caret.top - visibleHeight - 8)
      : Math.min(anchor.caret.bottom + 8, viewportTop + viewportHeight - visibleHeight - 8);
    setStyle({ position: "fixed", left, top, width: menuWidth, maxHeight, zIndex: 2600 });
  }, [getAnchor]);

  useEffect(() => {
    if (!open) return;
    updatePosition();
    const frame = requestAnimationFrame(updatePosition);
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    window.visualViewport?.addEventListener("resize", updatePosition);
    window.visualViewport?.addEventListener("scroll", updatePosition);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
      window.visualViewport?.removeEventListener("resize", updatePosition);
      window.visualViewport?.removeEventListener("scroll", updatePosition);
    };
  }, [open, updatePosition, results, status]);
  useEffect(() => {
    active.current?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex, results]);

  if (!open || !style || typeof document === "undefined") return null;
  return createPortal(
    <div data-ui-appearance={appearance} ref={menu} className={styles.suggestions} style={style}>
      <div data-ui-appearance={appearance} className={styles.caption}>
        {trigger === "@" ? "Mention people, AI agents, or records" : "Add a hashtag"}
      </div>
      <div data-ui-appearance={appearance}
        id={listId}
        role="listbox"
        aria-label={trigger === "@" ? "People, AI agents, and records" : "Hashtags"}
      >
        {results.map((item, index) => (
          <div data-ui-appearance={appearance} key={`${item.kind}:${item.targetId}`}>
            {(index === 0 || results[index - 1].kind !== item.kind) && (
              <div data-ui-appearance={appearance} className={styles.group}>
                {
                  { person: "People", agent: "AI agents", record: "Records", tag: "Hashtags" }[
                    item.kind
                  ]
                }
              </div>
            )}
            <Button appearance={appearance}
              variant="plain"
              ref={index === selectedIndex ? active : null}
              type="button"
              role="option"
              id={`${listId}-${index}`}
              aria-selected={index === selectedIndex}
              className={index === selectedIndex ? styles.active : undefined}
              onMouseDown={(event) => event.preventDefault()}
              onMouseMove={() => onHoverIndex(index)}
              onClick={() => onChoose(item)}
            >
              <strong data-ui-appearance={appearance}>
                {item.kind === "tag" ? "#" : "@"}
                {item.label}
              </strong>
              <small data-ui-appearance={appearance}>{item.detail}</small>
            </Button>
          </div>
        ))}
      </div>
      <p data-ui-appearance={appearance} role="status">{status || "↑ ↓ to choose · Enter to insert · Esc to close"}</p>
    </div>,
    document.body,
  );
}
