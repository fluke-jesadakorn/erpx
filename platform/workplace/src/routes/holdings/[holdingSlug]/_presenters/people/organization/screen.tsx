import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import {PeopleOrganizationPage} from "@erpx/platform-workplace/routes/holdings/[holdingSlug]/_presenters/people/people-organization-page";
import { organizationPresentationQueries } from "@erpx/organization-server/public/presentation-queries";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { PageHeader } from "@erpx/platform-ui/components/primitives";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";



import { DepartmentAuthorityWorkspace } from "@erpx/organization-ui/components/department-authority-workspace";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { peopleContextHref, type PeopleRouteSearch } from "@erpx/people-payroll-ui/lib/people-route-context";
import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
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
  const query = await searchParams;
  const canReadOrganization = ["people.read", "people.manage", "iam.read", "organization.manage"].some(
      (permission) => hasPermission(actor, permission),
    );
  // Only the appointment-only fallback still needs retained presentation
  // queries; the organization page read itself is owned by its presenter.
  if (!canReadOrganization) {
    const presentation = await organizationPresentationQueries(actor.holdingId);
    const authority = await presentation.appointmentOverview(actor);
    if (authority.canPublishDepartmentIds.length || authority.canAppointKinds.length) {
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
