import { redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@erpx/feature-people_payroll/lib/people-route-context";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { draftAuthority } from "@erpx/server-organization/public/organization-drafts";

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
