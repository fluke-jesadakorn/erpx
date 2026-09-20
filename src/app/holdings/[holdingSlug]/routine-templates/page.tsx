import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ServiceNavigation } from "@/components/modules/service-navigation";
import {
StaffTaskLinks,
TaskEditor,
} from "@/components/modules/staff-work-controls";
import { ListFilters } from "@/components/ui/list-filters";
import { EmptyState,PageHeader } from "@/components/ui/primitives";
import { administrationAccess } from "@/server/administration";
import { listTemplates } from "@/server/routines";
import { staffPage } from "@/server/staff-page";
import Link from "next/link";
export default async function Routines({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{q?:string;status?:string}>;
}) {
  const { holdingSlug } = await params,
    { actor, write, base, options, projects, staff } =
      await staffPage(holdingSlug),
    templates = await listTemplates(write, actor);
  const query=await searchParams;
  const visible=templates.filter(t=>(!query.q||t.title.toLowerCase().includes(query.q.toLowerCase()))&&(!query.status||query.status==="all"||(query.status==="published"?t.published:!t.published)));
  const access = await administrationAccess(actor);
  return (
    <FeaturePage className="content-stack" data-source="live">
      <PageHeader
        eyebrow={actor.beta ? undefined : "Projects & workflows"}
        title="Routine templates"
        description="Reusable company work. Every run has its own task and approval history."
      /><WidgetGrid pageKey="projects-routines">
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
          activeId="projects-routines"
        />
      )}
      {staff && (
        <TaskEditor
          createAs="routine"
          base={base}
          options={options}
          projects={projects}
          userId={actor.userId}
        />
      )}
      <ListFilters action={`${base}/routine-templates`} query={query.q} placeholder="Search routines" status={query.status} statuses={[["all","All statuses"],["published","Published"],["draft","Draft"]]}/>
      <FeatureWidget id="projects-routines-company-templates" title="Company templates"><section className="card">
        
        {!visible.length && <EmptyState title="No matching routines" description="Create a reusable template or adjust your filters."/>}
        {visible.map((t) => (
          <Link
            className="row"
            key={t.id}
            href={`${base}/routine-templates/${t.id}`}
          >
            <strong>{t.title}</strong>
            <span>
              {t.published ? "Published" : "Draft"} · version {t.version}
            </span>
          </Link>
        ))}
      </section></FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
