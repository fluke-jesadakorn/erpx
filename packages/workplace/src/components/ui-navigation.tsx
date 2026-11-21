"use client";

import { useMemo, type ReactNode } from "react";
import { UiNavigationProvider, type UiNavigationBindings } from "@erpx/ui/components/navigation-context";
import { isRecordTableAction } from "@erpx/feature-records/public/client";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { routeContextHref } from "@erpx/feature-shared/lib/route-context";
import { canonicalUiHref, destinationForPath, selectionForPath, workspacePathParts } from "@erpx/feature-work_projection/lib/destination-registry";
import WorkspaceLink from "@erpx/workplace/components/workspace-link";

/** Shell composition supplies app routing without making primitives depend on owners. */
export function WorkspaceUiNavigation({ children }: { children: ReactNode }) {
  const router = useRouter();
  const bindings = useMemo<UiNavigationBindings>(() => ({
    Link: WorkspaceLink,
    isTableAction: isRecordTableAction,
    resolveFilters: (pathname, action, preserve) => {
      const parts = workspacePathParts(pathname);
      const owned = parts ? destinationForPath(parts.path) : undefined;
      return { action: owned ? pathname : action, preserve: Object.fromEntries(Object.entries(preserve).filter(([key]) => !Object.hasOwn(selectionForPath(pathname), key))) };
    },
    filterHref: routeContextHref,
    filterNavigation: {
      supports: pathname => !!workspacePathParts(pathname),
      href: canonicalUiHref,
      push: href => router.push(href, { scroll: false }),
    },
  }), [router]);
  return <UiNavigationProvider bindings={bindings}>{children}</UiNavigationProvider>;
}
