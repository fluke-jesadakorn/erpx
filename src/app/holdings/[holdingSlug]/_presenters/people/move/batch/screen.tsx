import { organizationPresentationQueries } from "@/server/modules/organization/public/presentation-queries";
import { notFound, redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@/lib/people_payroll/people-route-context";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";



export const dynamic = "force-dynamic";

export default async function PeopleMoveBatchPage({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  requirePermission(actor, "people.manage");
  requirePermission(actor, "organization.manage");
  if (actor.scope.kind !== "holding" || actor.assignmentScope !== "holding" || actor.branchId || actor.departmentId) notFound();
  const presentation = await organizationPresentationQueries(actor.holdingId);
  const organization = await presentation.organizationAccessData(actor);
  if (!organization.canManage) notFound();
  redirect(peopleContextHref(workspaceBase(publicScope(actor)), await searchParams, { panel: "review", batch: "1" }));
}
