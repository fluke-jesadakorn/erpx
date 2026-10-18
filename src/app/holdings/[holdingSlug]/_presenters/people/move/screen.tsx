import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {PeopleOrganizationPage} from "@/app/holdings/[holdingSlug]/_presenters/people/people-organization-page";
import { redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@/lib/people_payroll/people-route-context";
import { workspaceBase } from "@/lib/work_projection/workspace";
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
  if (!id || !["person", "position", "department"].includes(kind)) redirect(canonicalUiHref(`${base}/people`));
  await loadPeopleMove(actor, kind as PeopleMoveKind, id);
  return <PeopleOrganizationPage actor={actor} search={{...query,panel:"move"}}/>;
}
