import Link from "next/link";
import { FeaturePage } from "@/components/features/feature-page";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { LeavePolicyWizard } from "@/components/self-service/leave-policy-wizard";
import { PageHeader } from "@/components/ui/primitives";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { ApiError } from "@/server/http";
import { leaveAdministration } from "@/server/leave-self-service";

export default async function NewLeavePolicy({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const slug = (await params).holdingSlug;
  const actor = await requireWorkContext(slug);
  const base = workspaceBase(publicScope(actor));
  let data: Awaited<ReturnType<typeof leaveAdministration>> | null = null;
  try { data = await leaveAdministration(actor); }
  catch (error) { if (!(error instanceof ApiError && error.status === 403)) throw error; }
  return <FeaturePage className="content-stack" data-source="live">
    <PageHeader eyebrow="People · Leave & overtime" title="Create leave policy" description="Create one policy version after reviewing its workdays and entitlement rules." />
    <PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="settings" />
    <Link href={`${base}/people/settings?section=leave`}>← Leave & overtime settings</Link>
    {data ? <LeavePolicyWizard base={base} types={data.types} /> : <p>HR setup or permission is required to create leave policies.</p>}
  </FeaturePage>;
}
