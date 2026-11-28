import { redirect } from "next/navigation";
import { peopleSettingsHref, type PeopleRouteSearch } from "@erpx/people-payroll-ui/lib/people-route-context";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";

export default async function NewLeavePolicy({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  redirect(peopleSettingsHref(workspaceBase(publicScope(actor)), await searchParams, {
    section: "leave", new: "leave-policy", editor: "policy",
  }));
}
