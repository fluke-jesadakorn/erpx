import { PeopleNavigation } from "@/components/modules/people-navigation";
import { RosterWorkspace } from "@/components/people/roster-workspace";
import { PageHeader } from "@/components/ui/primitives";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { rosterData } from "@/server/people-roster";
import { workspaceBase } from "@/lib/workspace";
import { FeaturePage } from "@/components/features/feature-page";
export const dynamic = "force-dynamic";
export default async function RosterPage({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ department?: string; week?: string; view?: string; request?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug), query = await searchParams;
  const { write } = await holdingResources(actor.holdingId), base = workspaceBase(publicScope(actor));
  return <FeaturePage className="content" data-source="live">
    <PageHeader eyebrow={query.view==="mine"?"My work":"Your team"} title={query.view==="mine"?"My shifts":"Roster"} description={query.view==="mine"?"Your published schedule and shift change requests.":"Plan the week. Keep everyone in the loop."} />
    <PeopleNavigation base={base} actor={actor} active="roster" scopeKind={actor.scope.kind} />
    <RosterWorkspace base={base} initial={await rosterData(write, actor, query)} initialView={query.view} requestId={query.request} />
  </FeaturePage>;
}
