"use client";
import type { ReactNode } from "react";
import { useSearchParams } from "@erpx/platform-shared-ui/lib/destination-search";
import { WorkspaceFrame, WorkspaceContent, WorkspaceContext } from "@erpx/platform-ui/components/workspace";
import { technologyHref, technologySections } from "@erpx/identity-access-ui/lib/technology-navigation";
import { routeSearchFromParams } from "@erpx/platform-shared-ui/lib/route-context";

export function TechnologySectionWorkspace({ base, view, section, children }: {
  base: string; view: keyof typeof technologySections; section: string; title: string; children: ReactNode;
}) {
  const query = useSearchParams();
  const items = technologySections[view].map(item => ({...item, href: technologyHref(base, routeSearchFromParams(query), view, item.id)}));
  return <WorkspaceFrame style={{gridTemplateColumns:"minmax(0,1fr)"}} data-testid={`technology-${view}-workspace`}>
    <WorkspaceContent><WorkspaceContext><strong>{items.find(item => item.id === section)?.title}</strong></WorkspaceContext>{children}</WorkspaceContent>
  </WorkspaceFrame>;
}
