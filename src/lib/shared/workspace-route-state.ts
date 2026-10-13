"use client";

import { useSearchParams } from "next/navigation";

/** Owners choose their query keys; repeated filters and return context survive UI changes. */
export function useWorkspaceRouteState() {
  const query = useSearchParams();
  function update(patch: Record<string, string | null>, replace = false) {
    const params = new URLSearchParams(query);
    for (const [key, value] of Object.entries(patch)) {
      if (value === null) params.delete(key);
      else params.set(key, value);
    }
    window.history[replace ? "replaceState" : "pushState"](null, "", `${window.location.pathname}${params.size ? `?${params}` : ""}`);
  }
  return { query, update };
}
