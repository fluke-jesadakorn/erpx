import { redirect } from "next/navigation";
import { peopleSettingsHref, type PeopleRouteSearch } from "@/lib/people_payroll/people-route-context";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";

export default async function NewLeavePolicy({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  redirect(peopleSettingsHref(workspaceBase(publicScope(actor)), await searchParams, {
    section: "leave", new: "leave-policy", editor: "policy",
  }));
}
