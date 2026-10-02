import { notFound, redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@/lib/people-route-context";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { requirePermission } from "@/server/iam-permissions";
import { organizationAccessData } from "@/server/organization-access";
import { holdingResources } from "@/server/runtime";

export const dynamic = "force-dynamic";

export default async function PeopleMoveBatchPage({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  requirePermission(actor, "people.manage");
  requirePermission(actor, "organization.manage");
  if (actor.scope.kind !== "holding" || actor.assignmentScope !== "holding" || actor.branchId || actor.departmentId) notFound();
  const { write } = await holdingResources(actor.holdingId);
  const organization = await organizationAccessData(write, actor);
  if (!organization.canManage) notFound();
  redirect(peopleContextHref(workspaceBase(publicScope(actor)), await searchParams, { panel: "review", batch: "1" }));
}
