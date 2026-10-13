"use client";
import type { ReactNode } from "react";
import { useSearchParams } from "next/navigation";
import { WorkspaceFrame, WorkspaceNavigation, WorkspaceSections, WorkspaceContent, WorkspaceContext } from "@/components/ui/workspace";
import { technologyHref, technologySections } from "@/lib/identity_access/technology-navigation";
import { routeSearchFromParams } from "@/lib/shared/route-context";

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
