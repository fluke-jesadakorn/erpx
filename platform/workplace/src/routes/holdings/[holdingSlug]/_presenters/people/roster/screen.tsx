import { redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@erpx/people-payroll-ui/lib/people-route-context";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";

export const dynamic = "force-dynamic";

export default async function RosterPage({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  redirect(peopleContextHref(workspaceBase(publicScope(actor)), await searchParams, { panel: "schedule" }));
}
