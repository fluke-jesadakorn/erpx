import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pmsWorkspace } from "@/server/pms";
import { PmsWorkspaceView } from "@/components/pms/workspace";
import { pmsSections } from "@/lib/pms-model";
import { pageError } from "@/server/page-error";
import { companyFeatureCatalogue } from "@/server/hospitality-rollout";
import { pmsCrossLinks } from "@/lib/pms-cross-links";
import { notFound } from "next/navigation";
import { redirect } from "next/navigation";
import { frontWorkspaceHref } from "@/server/front-app";
export const dynamic = "force-dynamic";
export default async function PmsPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; section?: string[] }>;
  searchParams: Promise<{ property?: string; folio?: string; queue?: string; task?: string; night?: string;record?:string }>;
}) {
  const { holdingSlug, section } = await params;
  if (
    section &&
    (section.length !== 1 || !pmsSections.some(([id]) => id === section[0]))
  )
    notFound();
  const search = await searchParams,
    actor = await requireWorkContext(holdingSlug);
  const frontHref = await frontWorkspaceHref(actor, "pms");
  const dailySections = new Set(["dashboard", "reservations", "front-desk", "rooms", "housekeeping", "folios", "cashier", "night-audit"]);
  const requestedSection = section?.[0] ?? "dashboard";
  const reviewRoute = (requestedSection === "folios" && ["approval", "corrections"].includes(search.queue ?? "")) ||
    (requestedSection === "cashier" && search.queue === "approval");
  if (frontHref && dailySections.has(requestedSection) && !reviewRoute) {
    const target = requestedSection === "dashboard" ? frontHref : frontHref + requestedSection;
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries({ property: search.property, folio: search.folio, task: search.task, night: search.night,record:search.record,queue:search.queue }))
      if (value) query.set(key, value);
    redirect(`${target}${query.size ? "?" + query : ""}`);
  }
  let data;
  try {
    data = await pmsWorkspace(actor, search.property, section?.[0] === "night-audit", false,
      requestedSection === "dashboard" ? { view: "dashboard" } : undefined);
  } catch (error) {
    return pageError(error, actor.holdingSlug);
  }
  const enabled = process.env.NODE_ENV === "production"
    ? new Set((await companyFeatureCatalogue(actor))
        .filter((entry) => ["Available", "Pilot", "Setup required"].includes(entry.state))
        .map((entry) => entry.id))
    : null;
  const frontPosHref = await frontWorkspaceHref(actor, "pos");
  return (
    <PmsWorkspaceView
      initial={data}
      base={workspaceBase(publicScope(actor))}
      section={section?.[0] ?? "dashboard"}
      selectedFolio={search.folio}
      selectedRecord={search.record}
      crossLinks={pmsCrossLinks(actor, enabled ?? undefined)}
      frontHref={frontHref ?? undefined}
      frontPosHref={frontPosHref ?? undefined}
    />
  );
}
