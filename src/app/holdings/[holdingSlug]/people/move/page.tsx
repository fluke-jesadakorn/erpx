import { redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@/lib/people-route-context";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { loadPeopleMove, type PeopleMoveKind } from "@/server/modules/people_payroll/public/people-move";

export const dynamic = "force-dynamic";

export default async function PeopleMovePage({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const query = await searchParams;
  const kind = typeof query.kind === "string" ? query.kind : "";
  const id = typeof query.id === "string" ? query.id : "";
  if (!id || !["person", "position", "department"].includes(kind)) redirect(`${base}/people`);
  await loadPeopleMove(actor, kind as PeopleMoveKind, id);
  redirect(peopleContextHref(base, query, { panel: "move" }));
}
