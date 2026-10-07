import { notFound, redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@/lib/people-route-context";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { loadPeopleMove, type PeopleMoveKind } from "@/server/modules/people_payroll/public/people-move";

export const dynamic = "force-dynamic";

export default async function PeopleMoveReviewPage({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const query = await searchParams;
  const kind = typeof query.kind === "string" ? query.kind : "";
  const id = typeof query.id === "string" ? query.id : "";
  const targetKind = typeof query.targetKind === "string" ? query.targetKind : "";
  const targetId = typeof query.targetId === "string" ? query.targetId : "";
  if (!["person", "position", "department"].includes(kind) || !id || !["position", "department", "level", "branch"].includes(targetKind) || !targetId) notFound();
  const actor = await requireWorkContext((await params).holdingSlug);
  await loadPeopleMove(actor, kind as PeopleMoveKind, id);
  redirect(peopleContextHref(workspaceBase(publicScope(actor)), query, { panel: "review" }));
}
