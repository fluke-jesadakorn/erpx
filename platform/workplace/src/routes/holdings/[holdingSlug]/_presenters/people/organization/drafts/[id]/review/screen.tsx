import { redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@erpx/people-payroll-ui/lib/people-route-context";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { draftAuthority } from "@erpx/organization-server/public/organization-drafts";

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
