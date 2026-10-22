"use client";

import { useMemo, useSyncExternalStore } from "react";

export type MoveTrayItem = { engagementId: string; positionId: string | null };

export function moveTrayKey(base: string, companyId: string, actorId: string) {
  return `erpx:people-move-tray:v1:${base}:${companyId}:${actorId}`;
}

function parseMoveTray(snapshot: string): MoveTrayItem[] {
  try {
    const raw = JSON.parse(snapshot) as unknown;
    if (!Array.isArray(raw)) return [];
    const seen = new Set<string>();
    return raw.filter((value): value is MoveTrayItem => {
      if (!value || typeof value !== "object") return false;
      const item = value as Record<string, unknown>;
      if (typeof item.engagementId !== "string" || !item.engagementId || seen.has(item.engagementId)) return false;
      if (item.positionId !== null && typeof item.positionId !== "string") return false;
      seen.add(item.engagementId);
      return true;
    }).slice(0, 20);
  } catch { return []; }
}

export function readMoveTray(key: string): MoveTrayItem[] {
  return parseMoveTray(sessionStorage.getItem(key) ?? "[]");
}

export function writeMoveTray(key: string, items: MoveTrayItem[]) {
  sessionStorage.setItem(key, JSON.stringify(items));
  window.dispatchEvent(new CustomEvent("erpx:people-move-tray", { detail: key }));
}

export function useMoveTray(key: string) {
  const snapshot = useSyncExternalStore(
    onChange => {
      const changed = (event: Event) => { if (!(event instanceof CustomEvent) || event.detail === key) onChange(); };
      window.addEventListener("erpx:people-move-tray", changed);
      window.addEventListener("storage", onChange);
      return () => { window.removeEventListener("erpx:people-move-tray", changed); window.removeEventListener("storage", onChange); };
    },
    () => sessionStorage.getItem(key) ?? "[]",
    () => "[]",
  );
  return useMemo(() => parseMoveTray(snapshot), [snapshot]);
}
