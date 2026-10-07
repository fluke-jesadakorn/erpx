import { redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@/lib/people-route-context";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { draftAuthority } from "@/server/modules/organization/public/organization-drafts";

export const dynamic = "force-dynamic";

export default async function Page({ params, searchParams }: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  draftAuthority(actor);
  redirect(peopleContextHref(workspaceBase(publicScope(actor)), await searchParams, { draft: id, panel: "review" }));
}
