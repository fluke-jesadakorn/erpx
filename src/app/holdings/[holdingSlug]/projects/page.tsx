import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ServiceNavigation } from "@/components/modules/service-navigation";
import {
ProjectEditor,
StaffTaskLinks,
} from "@/components/modules/staff-work-controls";
import { ListFilters } from "@/components/ui/list-filters";
import { EmptyState,PageHeader } from "@/components/ui/primitives";
import { administrationAccess } from "@/server/administration";
import { staffPage } from "@/server/staff-page";
import Link from "next/link";
export default async function Projects({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{q?:string}>;
}) {
  const { holdingSlug } = await params,
    { actor, base, options, projects, staff } = await staffPage(holdingSlug);
  const query=await searchParams;
  const visible=projects.filter(p=>!query.q||`${p.title} ${p.description}`.toLowerCase().includes(query.q.toLowerCase()));
  const access = await administrationAccess(actor);
  return (
    <FeaturePage className="content-stack" data-source="live">
      <PageHeader
        eyebrow={actor.beta ? undefined : "Projects & workflows"}
        title={actor.beta ? "Projects" : "All projects"}
      /><WidgetGrid pageKey="projects-list">
      {actor.beta ? (
        <StaffTaskLinks base={base} />
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
      {staff && <ProjectEditor base={base} options={options} />}
      <ListFilters action={`${base}/projects`} query={query.q} placeholder="Search projects"/>
      <FeatureWidget id="projects-list-your-projects" title="Your projects"><section className="card">
        
        {!visible.length && <EmptyState title={query.q?"No matching projects":"No projects yet"} description={query.q?"Try a different search or clear filters.":"Create a project to bring its work and teammates together."}/>}
        {visible.map((p) => (
          <Link className="row" key={p.id} href={`${base}/projects/${p.id}`}>
            <div>
              <strong>{p.title}</strong>
              <p>{p.description}</p>
            </div>
          </Link>
        ))}
      </section></FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
