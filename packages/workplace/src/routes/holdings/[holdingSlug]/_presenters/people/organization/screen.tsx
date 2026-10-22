import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {PeopleOrganizationPage} from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/people-organization-page";
import { organizationPresentationQueries } from "@erpx/server-organization/public/presentation-queries";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { PageHeader } from "@erpx/ui/components/primitives";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";



import { DepartmentAuthorityWorkspace } from "@erpx/feature-organization/components/department-authority-workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { peopleContextHref, type PeopleRouteSearch } from "@erpx/feature-people_payroll/lib/people-route-context";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { forbidden, redirect } from "next/navigation";

export default async function OrganizationPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  const presentation = await organizationPresentationQueries(actor.holdingId),
    query = await searchParams;
  const authority = await presentation.appointmentOverview(actor);
  const canReadOrganization = ["people.read", "people.manage", "iam.read", "organization.manage"].some(
      (permission) => hasPermission(actor, permission),
    );
  if (!canReadOrganization && (authority.canPublishDepartmentIds.length || authority.canAppointKinds.length)) {
    // Appointed department heads still use their existing focused approval tool.
    // The main People panel cannot replace this workflow until authority-specific
    // access has been moved there.
    const roster = await presentation.organizationRoster(actor.companyId);
    return (
      <FeaturePage className="ui-page-content" data-source="live">
        <Link href={canonicalUiHref(`${base}/work`)}>← Work</Link>
        <PageHeader appearance="standard"
          title="Appointments & department flows"
          description="Publish ordered approvers for new department requests."
        />
        <DepartmentAuthorityWorkspace appearance="standard"
          holdingSlug={actor.holdingSlug}
          initial={
            authority as Parameters<
              typeof DepartmentAuthorityWorkspace
            >[0]["initial"]
          }
          people={roster.people}
          positions={roster.positions}
        />
      </FeaturePage>
    );
  }
  if (!canReadOrganization) {
    forbidden();
  }
  const destination: Record<string, string | null> = {};
  if (!query.company && !query.position && !query.branch && !query.department && !query.person) destination.company = actor.companyId;
  if (!query.position && query.view === "positions") destination.tab = "positions";
  if (!query.position && query.view === "people") destination.tab = "people";
  if (!query.position && query.view === "templates") {
    destination.panel = "settings";
    destination.section = "templates";
  }
  if (!query.position && !query.tab && (query.view === "spending" || query.view === "legacy")) {
    destination.tab = "settings";
    if (query.view === "spending") destination.section = "spending";
  }
  if (query.changeId || query.changeState || query.changeQuery || query.changePage) {
    destination.position = null;
    destination.section = null;
    destination.company = actor.companyId;
    destination.panel = "history";
  }
  const _anchor = typeof query.changeId === "string" && query.changeId
    ? `#organization-change-${encodeURIComponent(query.changeId)}`
    : query.changeState || query.changeQuery || query.changePage
      ? "#organization-change-queue"
      : "";
  return <PeopleOrganizationPage actor={actor} search={Object.fromEntries(Object.entries({...query,...destination}).filter((entry):entry is [string,string]=>typeof entry[1]==="string"))}/>;
}
