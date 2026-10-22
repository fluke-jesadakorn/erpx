import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {PeopleOrganizationPage} from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/people-organization-page";
import { redirect } from "next/navigation";
import { peopleContextHref, type PeopleRouteSearch } from "@erpx/feature-people_payroll/lib/people-route-context";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { loadPeopleMove, type PeopleMoveKind } from "@erpx/server-people_payroll/public/people-move";

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
