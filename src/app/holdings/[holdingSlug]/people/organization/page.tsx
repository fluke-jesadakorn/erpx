import Link from "next/link";
import { PageHeader } from "@/components/ui/primitives";
import { OrganizationWorkspace } from "@/components/modules/organization-workspace";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { organizationOverview } from "@/server/organization";
import { workspaceBase } from "@/lib/workspace";
import { PeopleNavigation } from "@/components/modules/people-navigation";

export default async function OrganizationPage({params}:{params:Promise<{holdingSlug:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor));
  const {write}=await holdingResources(actor.holdingId);
  return <main className="content-stack" data-source="live"><Link href={`${base}/people`}>← People</Link><PageHeader title="Organization" description="Build your team structure, assign managers, and prepare staff for approvals."/><PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="organization"/><OrganizationWorkspace base={base} data={await organizationOverview(write,actor)}/></main>;
}
