import {RoutineTemplateEditor} from '@erpx/operations-ui/components/routine-template-editor';
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { FeaturePage, FeatureWidget,WidgetGrid } from "@erpx/platform-shared-ui/components/features/feature-page";
import { ServiceNavigation } from "@erpx/field-service-ui/components/service-navigation";
import {StaffTaskLinks} from '@erpx/work-projection-ui/components/staff-work-controls';
import { ListFilters } from "@erpx/platform-ui/components/list-filters";
import { EmptyState,PageHeader } from "@erpx/platform-ui/components/primitives";
import { administrationAccess } from "@erpx/platform-runtime-server/public/administration";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { routineReadRequest } from "./routine-read-request";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {
  nativeRoutineEditorContext,
  nativeRoutineTemplateList,
  nativeRoutineTemplatesAvailable,
} from "@erpx/operations-ui/public/native-routines-read";

import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { Card } from "@erpx/platform-ui/components/foundation";
async function nativeTemplates(holdingSlug: string, actor: Awaited<ReturnType<typeof requireWorkContext>>) {
  if (!nativeRoutineTemplatesAvailable())
    throw new ApiError(503, "NATIVE_ROUTINES_UNAVAILABLE", "The native Operations routine read is unavailable.");
  const request = await routineReadRequest(holdingSlug, 'routine-templates', actor);
  return nativeRoutineTemplateList(request, holdingSlug, actor.userId);
}
async function nativeEditor(holdingSlug: string, actor: Awaited<ReturnType<typeof requireWorkContext>>) {
  const request = await routineReadRequest(holdingSlug, 'routine-template-editor-context', actor);
  return nativeRoutineEditorContext(request, holdingSlug, actor.userId);
}
export default async function Routines({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{q?:string;status?:string}>;
}) {
  const { holdingSlug } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    templates = await nativeTemplates(holdingSlug, actor),
    { projects, options, canWrite, purchases } = await nativeEditor(holdingSlug, actor);
  const query=await searchParams;
  const visible=templates.filter(t=>(!query.q||t.title.toLowerCase().includes(query.q.toLowerCase()))&&(!query.status||query.status==="all"||(query.status==="published"?t.published:!t.published)));
  const access = await administrationAccess(actor);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <PageHeader appearance="standard"
        eyebrow={actor.beta ? undefined : "Projects & workflows"}
        title="Routine templates"
        description="Reusable company work. Every run has its own task and approval history."
      /><WidgetGrid pageKey="projects-routines">
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
          activeId="projects-routines"
        />
      )}
      {canWrite && (
        <RoutineTemplateEditor appearance="standard"
          purchases={purchases??undefined}
          canWrite={canWrite}
          base={base}
          options={options}
          projects={projects}
          userId={actor.userId}
        />
      )}
      <ListFilters appearance="standard" action={canonicalUiHref(`${base}/routine-templates`)} query={query.q} placeholder="Search routines" status={query.status} statuses={[["all","All statuses"],["published","Published"],["draft","Draft"]]}/>
      <FeatureWidget appearance="standard" id="projects-routines-company-templates" title="Company templates"><Card as="section" appearance="standard" className="ui-panel">
        
        {!visible.length && <EmptyState title="No matching routines" description="Create a reusable template or adjust your filters."/>}
        {visible.map((t) => (
          <Link
            className="ui-record-item"
            key={t.id}
            href={canonicalUiHref(`${base}/routine-templates/${t.id}`)}
          >
            <strong>{t.title}</strong>
            <span>
              {t.published ? "Published" : "Draft"} · version {t.version}
            </span>
          </Link>
        ))}
      </Card></FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
