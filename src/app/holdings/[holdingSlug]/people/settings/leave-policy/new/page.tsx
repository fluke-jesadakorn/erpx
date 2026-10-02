import { redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@/lib/people-route-context";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";

export default async function NewLeavePolicy({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  redirect(peopleContextHref(workspaceBase(publicScope(actor)), await searchParams, {
    panel: "settings", section: "leave", new: "leave-policy", editor: "policy",
  }));
}
