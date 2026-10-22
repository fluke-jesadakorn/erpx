import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import {pmsListSelection} from "@erpx/server-pms/public/rules/pms-read-selection";
import { pmsWorkspace } from "../../../../../../../../../packages/server-pms/src/public/pms";
import { PmsVisualPrototype } from "@erpx/feature-pms/components/visual-prototype";
import { PmsWorkspaceView } from "@erpx/feature-pms/components/workspace";
import { pmsSections } from "@erpx/server-pms/public/rules/pms-model";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { companyFeatureCatalogue } from "@erpx/server-pms/public/hospitality-rollout";
import { pmsCrossLinks } from "@erpx/server-pms/public/rules/pms-cross-links";
import { notFound } from "next/navigation";
import { redirect } from "next/navigation";
import { frontWorkspaceHref } from "@erpx/server-work_projection/public/front-app";
export const dynamic = "force-dynamic";
export default async function PmsPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; section?: string[] }>;
  searchParams: Promise<{ q?:string;building?:string;floor?:string;typeId?:string;from?:string;to?:string;page?:string;status?:string;property?: string; guestAccount?: string; queue?: string; task?: string; night?: string;record?:string;prototype?:string;variant?:string;screen?:string;detailKind?:string;detailId?:string }>;
}) {
  const { holdingSlug, section } = await params;
  if (
    section &&
    (section.length !== 1 || !pmsSections.some(([id]) => id === section[0]))
  )
    notFound();
  const search = await searchParams,
    actor = await requireWorkContext(holdingSlug);
  const prototype = process.env.NODE_ENV !== "production" && (search.prototype === "1" || ["A","B","C"].includes(search.variant??""));
  const frontHref = await frontWorkspaceHref(actor, "pms");
  const dailySections = new Set(["dashboard", "reservations", "front-desk", "rooms", "housekeeping", "guest-accounts", "cashier", "night-audit"]);
  const requestedSection = section?.[0] ?? "dashboard";
  const reviewRoute = (requestedSection === "reservations" && search.queue === "groups") ||
    (requestedSection === "guest-accounts" && ["approval", "corrections"].includes(search.queue ?? "")) ||
    (requestedSection === "cashier" && search.queue === "approval");
  if (frontHref && dailySections.has(requestedSection) && !reviewRoute && !prototype) {
    const target = requestedSection === "dashboard" ? frontHref : frontHref + requestedSection;
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries({ q:search.q,building:search.building,floor:search.floor,typeId:search.typeId,from:search.from,to:search.to,page:search.page,status:search.status,property: search.property, guestAccount: search.guestAccount, task: search.task, night: search.night,record:search.record,queue:search.queue }))
      if (value) query.set(key, value);
    redirect(`${target}${query.size ? "?" + query : ""}`);
  }
  let data;
  try {
    data = await pmsWorkspace(actor, search.property, section?.[0] === "night-audit", false,
      requestedSection === "dashboard" ? { view: "dashboard" } : pmsListSelection(requestedSection,search));
  } catch (error) {
    return pageError(error, actor.holdingSlug);
  }
  const enabled = process.env.NODE_ENV === "production"
    ? new Set((await companyFeatureCatalogue(actor))
        .filter((entry) => ["Available", "Pilot", "Setup required"].includes(entry.state))
        .map((entry) => entry.id))
    : null;
  const frontPosHref = await frontWorkspaceHref(actor, "pos");
  if (prototype) return <PmsVisualPrototype initialSection={requestedSection} permissions={data.permissions} />;
  return (
    <PmsWorkspaceView
      key={`${actor.companyId}:${actor.branchId}:${actor.assignmentId}:${search.property ?? ""}`}
      initial={data}
      base={workspaceBase(publicScope(actor))}
      section={section?.[0] ?? "dashboard"}
      selectedGuestAccount={search.guestAccount}
      selectedRecord={search.detailKind === "pms_record" ? search.detailId : search.record}
      crossLinks={pmsCrossLinks(actor, enabled ?? undefined)}
      frontHref={frontHref ?? undefined}
      frontPosHref={frontPosHref ?? undefined}
    />
  );
}
