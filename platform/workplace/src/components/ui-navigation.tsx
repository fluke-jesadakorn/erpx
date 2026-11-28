"use client";

import { useMemo, type ReactNode } from "react";
import { UiNavigationProvider, type UiNavigationBindings } from "@erpx/platform-ui/components/navigation-context";
import { isRecordTableAction } from "@erpx/records-ui/public/client";
import { useRouter } from "@erpx/platform-shared-ui/lib/destination-router";
import { routeContextHref } from "@erpx/platform-shared-ui/lib/route-context";
import { canonicalUiHref, destinationForPath, selectionForPath, workspacePathParts } from "@erpx/work-projection-ui/lib/destination-registry";
import WorkspaceLink from "@erpx/platform-workplace/components/workspace-link";

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
