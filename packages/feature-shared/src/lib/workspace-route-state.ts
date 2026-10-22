"use client";

import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { canonicalUiHref } from '@erpx/feature-work_projection/lib/destination-registry';

/** Owners choose their query keys; repeated filters and return context survive UI changes. */
export function useWorkspaceRouteState() {
  const query = useSearchParams();
  const router = useRouter();
  function update(patch: Record<string, string | null>, replace = false) {
    const params = new URLSearchParams(query);
    for (const [key, value] of Object.entries(patch)) {
      if (value === null) params.delete(key);
      else params.set(key, value);
    }
    router[replace ? 'replace' : 'push'](canonicalUiHref(`${window.location.pathname}${params.size ? `?${params}` : ""}`), {scroll:false});
  }
  return { query, update };
}
