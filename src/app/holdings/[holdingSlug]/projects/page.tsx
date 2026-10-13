import { staffPresentationQueries } from "@/server/modules/work_projection/public/presentation-context-queries";
import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/shared/features/feature-page";
import { ServiceNavigation } from "@/components/field_service/service-navigation";
import {
ProjectEditor,
StaffTaskLinks,
} from "@/components/work_projection/staff-work-controls";
import { ListFilters } from "@/components/ui/list-filters";
import { EmptyState,PageHeader } from "@/components/ui/primitives";
import { administrationAccess } from "@/server/modules/runtime/public/administration";

import { RecordLink as Link } from "@/components/records/record-link";
import { Card } from "@/components/ui/foundation";
export default async function Projects({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{q?:string}>;
}) {
  const { holdingSlug } = await params,
    { actor, base, options, projects, staff } = await staffPresentationQueries(holdingSlug);
  const query=await searchParams;
  const visible=projects.filter(p=>!query.q||`${p.title} ${p.description}`.toLowerCase().includes(query.q.toLowerCase()));
  const access = await administrationAccess(actor);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <PageHeader appearance="standard"
        eyebrow={actor.beta ? undefined : "Projects & workflows"}
        title={actor.beta ? "Projects" : "All projects"}
      /><WidgetGrid pageKey="projects-list">
      {actor.beta ? (
        <StaffTaskLinks appearance="standard" base={base} />
      ) : (
        <ServiceNavigation
          context={{
            workspaceBase: base,
            scopeKind: actor.scope.kind,
            role: actor.role,
            permissions: actor.iam?.permissions,
            adminSections: access.sections,
            canManageApprovals: access.manageOrganization,
          }}
          groupId="projects"
          activeId="projects-list"
        />
      )}
      {staff && <ProjectEditor appearance="standard" base={base} options={options} />}
      <ListFilters appearance="standard" action={`${base}/projects`} query={query.q} placeholder="Search projects"/>
      <FeatureWidget appearance="standard" id="projects-list-your-projects" title="Your projects"><Card as="section" appearance="standard" className="ui-panel">
        
        {!visible.length && <EmptyState title={query.q?"No matching projects":"No projects yet"} description={query.q?"Try a different search or clear filters.":"Create a project to bring its work and teammates together."}/>}
        {visible.map((p) => (
          <Link className="ui-record-item" key={p.id} href={`${base}/projects/${p.id}`}>
            <div>
              <strong>{p.title}</strong>
              <p>{p.description}</p>
            </div>
          </Link>
        ))}
      </Card></FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
