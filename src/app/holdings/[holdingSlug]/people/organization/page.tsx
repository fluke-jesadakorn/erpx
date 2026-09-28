import { peopleLifecycleData } from "@/server/people-lifecycle";
import Link from "next/link";
import { PageHeader } from "@/components/ui/primitives";
import { OrganizationWorkspace } from "@/components/modules/organization-workspace";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import {
  organizationOverview,
  organizationRoster,
} from "@/server/organization";
import { appointmentOverview } from "@/server/department-authority";
import { DepartmentAuthorityWorkspace } from "@/components/modules/department-authority-workspace";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { FeaturePage } from "@/components/features/feature-page";
import { BrandAdministration } from "@/components/modules/brand-administration";
import { administrationAccess } from "@/server/administration";
import { listBrandProfiles } from "@/server/branding";

export default async function OrganizationPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{
    position?: string;
    changeId?: string;
    changeState?: string;
    changeQuery?: string;
    changePage?: string;
  }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  const { write } = await holdingResources(actor.holdingId),
    query = await searchParams;
  const authority = await appointmentOverview(write, actor);
  if (
    !["people.read", "people.manage", "iam.read", "organization.manage"].some(
      (permission) => hasPermission(actor, permission),
    )
  ) {
    if (
      !authority.canPublishDepartmentIds.length &&
      !authority.canAppointKinds.length
    )
      return (
        <FeaturePage className="content-stack">
          <PageHeader
            title="Organization access unavailable"
            description="Ask an appointed head or CEO to publish this department flow."
          />
        </FeaturePage>
      );
    const roster = await organizationRoster(write, actor.companyId);
    return (
      <FeaturePage className="content-stack" data-source="live">
        <Link href={`${base}/work`}>← Work</Link>
        <PageHeader
          title="Appointments & department flows"
          description="Publish ordered approvers for new department requests."
        />
        <DepartmentAuthorityWorkspace
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
  const access = await administrationAccess(actor);
  const organization = await organizationOverview(write, actor, query);
  return (
    <FeaturePage className="content-stack" data-source="live">
      <Link href={`${base}/people`}>← People</Link>
      <PageHeader
        title="Organization & positions"
        description="Build your team structure, appoint leaders, and publish department approval flows."
      />
      <PeopleNavigation
        base={base}
        actor={actor}
        scopeKind={actor.scope.kind}
        active="organization"
      />
      <BrandAdministration
        base={base}
        {...await listBrandProfiles(write, actor)}
        canHolding={access.owner}
        canBranches={access.manageOrganization}
        canStorefront={
          access.manageOrganization && actor.scope.kind === "holding"
        }
        canReviewPublications={
          hasPermission(actor, "records.review") ||
          hasPermission(actor, "commerce.approve")
        }
      />
      <DepartmentAuthorityWorkspace
        holdingSlug={actor.holdingSlug}
        initial={
          authority as Parameters<
            typeof DepartmentAuthorityWorkspace
          >[0]["initial"]
        }
        people={organization.people}
        positions={organization.positions}
      />
      <OrganizationWorkspace
        base={base}
        data={organization}
        lifecycle={await peopleLifecycleData(write, actor)}
        initialPositionId={query.position}
      />
    </FeaturePage>
  );
}
