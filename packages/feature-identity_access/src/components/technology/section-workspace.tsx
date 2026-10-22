"use client";
import type { ReactNode } from "react";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { WorkspaceFrame, WorkspaceNavigation, WorkspaceSections, WorkspaceContent, WorkspaceContext } from "@erpx/ui/components/workspace";
import { technologyHref, technologySections } from "@erpx/feature-identity_access/lib/technology-navigation";
import { routeSearchFromParams } from "@erpx/feature-shared/lib/route-context";

export function TechnologySectionWorkspace({ base, view, section, title, children }: {
  base: string; view: keyof typeof technologySections; section: string; title: string; children: ReactNode;
}) {
  const query = useSearchParams();
  const items = technologySections[view].map(item => ({...item, href: technologyHref(base, routeSearchFromParams(query), view, item.id)}));
  return <WorkspaceFrame data-testid={`technology-${view}-workspace`}>
    <WorkspaceNavigation title={title} label={items.find(item => item.id === section)?.title} aria-label={`${title} navigation`}>
      <WorkspaceSections items={items} selected={section} label={view === "resources" ? "Resource sections" : `${title} sections`}/>
    </WorkspaceNavigation>
    <WorkspaceContent><WorkspaceContext><strong>{items.find(item => item.id === section)?.title}</strong></WorkspaceContext>{children}</WorkspaceContent>
  </WorkspaceFrame>;
}
