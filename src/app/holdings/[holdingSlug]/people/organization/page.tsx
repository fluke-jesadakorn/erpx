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

export default async function OrganizationPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{position?:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor));
  const {write}=await holdingResources(actor.holdingId);
  return <FeaturePage className="content-stack" data-source="live"><Link href={`${base}/people`}>← People</Link><PageHeader title="Organization & positions" description="Build your team structure, assign managers, and prepare staff for approvals."/><PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="organization"/><OrganizationWorkspace base={base} data={await organizationOverview(write,actor)} lifecycle={await peopleLifecycleData(write,actor)} initialPositionId={(await searchParams).position}/></FeaturePage>;
}
