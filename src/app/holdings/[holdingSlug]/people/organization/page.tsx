import { peopleLifecycleData } from "@/server/people-lifecycle";
import Link from "next/link";
import { PageHeader } from "@/components/ui/primitives";
import { OrganizationWorkspace } from "@/components/modules/organization-workspace";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { organizationOverview } from "@/server/organization";
import { workspaceBase } from "@/lib/workspace";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { FeaturePage } from "@/components/features/feature-page";
import { BrandAdministration } from "@/components/modules/brand-administration";
import { administrationAccess } from "@/server/administration";
import { listBrandProfiles } from "@/server/branding";

export default async function OrganizationPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{position?:string;changeId?:string;changeState?:string;changeQuery?:string;changePage?:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor));
  const {write}=await holdingResources(actor.holdingId),query=await searchParams;
  const access=await administrationAccess(actor);
  return <FeaturePage className="content-stack" data-source="live"><Link href={`${base}/people`}>← People</Link><PageHeader title="Organization & positions" description="Build your team structure, assign managers, and prepare staff for approvals."/><PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="organization"/><BrandAdministration base={base} {...await listBrandProfiles(write,actor)} canHolding={access.owner} canBranches={access.manageOrganization} canStorefront={access.manageOrganization&&actor.scope.kind==="holding"}/><OrganizationWorkspace base={base} data={await organizationOverview(write,actor,query)} lifecycle={await peopleLifecycleData(write,actor)} initialPositionId={query.position}/></FeaturePage>;
}
